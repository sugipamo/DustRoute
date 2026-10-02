//! Bounded dry-cube prediction and separately observed native controls.
mod control;
use super::*;
use crate::versions::java_1_21_11::math::trig;
pub(super) use control::standing_basis;
pub use control::{StandingPositionBasis, SurvivalMotionRecord, SurvivalMotionStatus};

/// Digital walking input for one predicted native game tick, without sprint/sneak.
#[derive(Clone, Copy, Debug, Default, Serialize)]
pub struct SurvivalInput {
    /// -1 backwards, 0 released, 1 forwards.
    pub forward: i8,
    /// -1 right, 0 released, 1 left.
    pub strafe: i8,
    /// Jump key state, including native repeat cooldown.
    pub jump: bool,
}
/// A simulated player frame, never a received pose or permission to build.
#[derive(Clone, Debug, Serialize)]
pub struct PredictedMotionFrame {
    /// Tick count from the preview's initial context, not server time.
    pub tick: u16,
    /// Predicted feet position.
    pub position: [f64; 3],
    /// Simulated next-tick velocity, including gravity while resting on a floor.
    pub velocity: [f64; 3],
    /// Predicted downward collision.
    pub on_ground: bool,
    /// Predicted X/Z obstruction.
    pub horizontal_collision: bool,
    /// No displacement with released controls and predicted floor contact.
    pub resting: bool,
}
/// Read-only simulation against one received world snapshot. Not a reusable plan.
#[derive(Clone, Debug, Serialize)]
pub struct SurvivalMovementPreview {
    /// Received starting posture, attributes and world revision.
    pub initial: StandingContext,
    /// Body yaw used for input; no look packet was sent.
    pub yaw: f32,
    /// Predicted frames. The world itself is not advanced into the future.
    pub frames: Vec<PredictedMotionFrame>,
}
impl Operations {
    /// Preview at most 120 dry-cube walking/jump ticks. Uses native default motion
    /// attributes, normal posture and no received effect updates. This is a
    /// prediction from the projection, not evidence that effects are absent on
    /// the server, that motion occurred, or that future geometry will stay fixed.
    pub async fn preview_survival_motion(
        &self,
        yaw: f32,
        inputs: &[SurvivalInput],
    ) -> Result<SurvivalMovementPreview> {
        let mut state = self.bot.session.state.lock().await;
        self.ready(&state)?;
        preview(
            &mut state,
            self.bot.session.id,
            self.bot.session.started.elapsed().as_millis() as u64 / 50,
            yaw,
            inputs,
        )
    }
}
fn preview(
    state: &mut State,
    connection_id: u64,
    tick: u64,
    yaw: f32,
    inputs: &[SurvivalInput],
) -> Result<SurvivalMovementPreview> {
    validate_pose([0.0; 3], [yaw, 0.0])?;
    if inputs.is_empty()
        || inputs.len() > 120
        || inputs
            .iter()
            .any(|i| !(-1..=1).contains(&i.forward) || !(-1..=1).contains(&i.strafe))
    {
        return Err(invalid("motion requires 1..120 bounded digital inputs"));
    }
    if state.operations.game_mode != Some(GameMode::Survival) {
        return Err(invalid("survival mode required"));
    }
    let initial = survival::context(state, connection_id, tick)?;
    let p = &initial.player;
    if !initial.on_ground
        || !p.effect_updates.is_empty()
        || p.movement_speed.map(|v| v.value) != Some(f64::from(0.1f32))
        || p.gravity.map(|v| v.value) != Some(0.08)
        || p.jump_strength.map(|v| v.value) != Some(f64::from(0.42f32))
        || p.step_height.map(|v| v.value) != Some(0.6)
    {
        return Err(invalid(
            "dry motion preview requires grounded native default movement attributes and no received effects",
        ));
    }
    let mut model = Model::from_context(&initial);
    let mut frames = Vec::with_capacity(inputs.len());
    for input in inputs {
        let proposed = model.intent(*input, yaw);
        if proposed.iter().any(|v| v.abs() > 1.0) {
            return Err(invalid("motion exceeds bounded dry preview step"));
        }
        let geometry = geometry(state, model.frame.position, proposed)?;
        model.advance(*input, proposed, &geometry);
        if model.frame.position[1] < initial.position[1] - 3.0 {
            return Err(invalid("preview falls outside bounded construction height"));
        }
        frames.push(model.frame.clone());
    }
    Ok(SurvivalMovementPreview {
        initial,
        yaw,
        frames,
    })
}

fn body(p: [f64; 3]) -> [f64; 6] {
    let half = f64::from(0.6f32) / 2.0;
    [
        p[0] - half,
        p[1],
        p[2] - half,
        p[0] + half,
        p[1] + f64::from(1.8f32),
        p[2] + half,
    ]
}
fn geometry(state: &State, p: [f64; 3], motion: [f64; 3]) -> Result<Vec<[f64; 6]>> {
    let a = body(p);
    let b = body(std::array::from_fn(|i| p[i] + motion[i]));
    let min: [i32; 3] = std::array::from_fn(|i| a[i].min(b[i]).floor() as i32 - 1);
    let max: [i32; 3] = std::array::from_fn(|i| a[i + 3].max(b[i + 3]).floor() as i32 + 1);
    let mut boxes = Vec::new();
    let (_, height) = state
        .world
        .dimension
        .as_ref()
        .context("dimension unavailable")?;
    for x in min[0]..=max[0] {
        for y in min[1]..=max[1] {
            for z in min[2]..=max[2] {
                if y < height.min_y || y >= height.min_y + height.height {
                    return Err(invalid("motion leaves observed dimension"));
                }
                let p = [x, y, z];
                let cell = state.reconstruction.cell(&state.world, p);
                if cell.moving.is_some() {
                    return Err(invalid("moving geometry in preview sweep"));
                }
                let block = cell.state.context("motion geometry unavailable")?;
                match block.name.as_str() {
                    "minecraft:air" | "minecraft:cave_air" | "minecraft:void_air" => {}
                    name if survival::DRY_CUBES.contains(&name) => boxes.push([
                        x as f64,
                        y as f64,
                        z as f64,
                        (x + 1) as f64,
                        (y + 1) as f64,
                        (z + 1) as f64,
                    ]),
                    _ => {
                        return Err(Error::new(
                            ErrorKind::Unsupported,
                            anyhow::anyhow!("unsupported motion geometry {} at {p:?}", block.name),
                        ));
                    }
                }
            }
        }
    }
    Ok(boxes)
}
fn acceleration(input: SurvivalInput, yaw: f32, speed: f32) -> [f64; 3] {
    let (mut x, mut z) = (f32::from(input.strafe), f32::from(input.forward));
    let length = (x * x + z * z).sqrt();
    if length == 0.0 {
        return [0.0; 3];
    }
    x = (x / length) * 0.98f32;
    z = (z / length) * 0.98f32;
    let length = (x * x + z * z).sqrt();
    let (nx, nz) = (x * (1.0 / length), z * (1.0 / length));
    let ratio = nx.abs().min(nz.abs()) / nx.abs().max(nz.abs());
    let magnitude = (length * (1.0 + ratio * ratio).sqrt()).min(1.0);
    let (mut x, mut z) = (f64::from(nx * magnitude), f64::from(nz * magnitude));
    let length = x * x + z * z;
    if length < 1e-7 {
        return [0.0; 3];
    }
    if length > 1.0 {
        let inverse = 1.0 / length.sqrt();
        x *= inverse;
        z *= inverse;
    }
    x *= f64::from(speed);
    z *= f64::from(speed);
    let angle = yaw * (std::f32::consts::PI / 180.0);
    let (s, c) = (f64::from(trig(angle, false)), f64::from(trig(angle, true)));
    [x * c - z * s, 0.0, z * c + x * s]
}
fn collide(mut bounds: [f64; 6], motion: [f64; 3], boxes: &[[f64; 6]]) -> [f64; 3] {
    let mut result = [0.0; 3];
    let order = if motion[0].abs() < motion[2].abs() {
        [1, 2, 0]
    } else {
        [1, 0, 2]
    };
    for axis in order {
        let mut distance = motion[axis];
        if distance == 0.0 {
            continue;
        }
        for cube in boxes {
            if distance.abs() < 1e-7 {
                distance = 0.0;
                break;
            }
            if (0..3).any(|i| {
                i != axis && (bounds[i] + 1e-7 >= cube[i + 3] || bounds[i + 3] - 1e-7 <= cube[i])
            }) {
                continue;
            }
            let ahead = cube[axis] - bounds[axis + 3];
            let behind = cube[axis + 3] - bounds[axis];
            if distance > 0.0 && ahead >= -1e-7 {
                distance = distance.min(ahead);
            } else if distance < 0.0 && behind <= 1e-7 {
                distance = distance.max(behind);
            }
        }
        result[axis] = distance;
        bounds[axis] += distance;
        bounds[axis + 3] += distance;
    }
    result
}
// Native Entity.adjustMovementForCollisions step search, restricted to the
// already admitted full cubes and native 0.6f step height.
fn collide_with_step(
    bounds: [f64; 6],
    motion: [f64; 3],
    boxes: &[[f64; 6]],
    on_ground: bool,
) -> [f64; 3] {
    let adjusted = collide(bounds, motion, boxes);
    let downward = motion[1] < 0.0 && motion[1] != adjusted[1];
    if !(downward || on_ground) || (motion[0] == adjusted[0] && motion[2] == adjusted[2]) {
        return adjusted;
    }
    let mut base = bounds;
    if downward {
        base[1] += adjusted[1];
        base[4] += adjusted[1];
    }
    let mut scan = base;
    scan[0] += motion[0].min(0.0);
    scan[3] += motion[0].max(0.0);
    scan[2] += motion[2].min(0.0);
    scan[5] += motion[2].max(0.0);
    scan[4] += f64::from(0.6f32);
    if !downward {
        scan[1] -= f64::from(1e-5f32);
    }
    let mut heights: Vec<f32> = boxes
        .iter()
        .filter(|b| (0..3).all(|i| scan[i] < b[i + 3] && scan[i + 3] > b[i]))
        .flat_map(|b| [b[1], b[4]])
        .map(|y| (y - base[1]) as f32)
        .filter(|h| *h >= 0.0 && *h <= 0.6f32 && *h != adjusted[1] as f32)
        .collect();
    heights.sort_by(f32::total_cmp);
    heights.dedup();
    for height in heights {
        let mut candidate = collide(base, [motion[0], f64::from(height), motion[2]], boxes);
        if candidate[0] * candidate[0] + candidate[2] * candidate[2]
            > adjusted[0] * adjusted[0] + adjusted[2] * adjusted[2]
        {
            candidate[1] -= bounds[1] - base[1];
            return candidate;
        }
    }
    adjusted
}

struct Model {
    frame: PredictedMotionFrame,
    jump_cooldown: u8,
}
impl Model {
    fn from_context(context: &StandingContext) -> Self {
        let mut model = Self::new(context.position);
        if let StandingPositionBasis::PredictedAndObserved { predicted, .. } =
            &context.position_basis
        {
            model.frame.velocity = predicted.velocity;
        }
        model
    }
    fn new(position: [f64; 3]) -> Self {
        Self {
            frame: PredictedMotionFrame {
                tick: 0,
                position,
                velocity: [0.0; 3],
                on_ground: true,
                horizontal_collision: false,
                resting: true,
            },
            jump_cooldown: 0,
        }
    }
    fn intent(&mut self, input: SurvivalInput, yaw: f32) -> [f64; 3] {
        self.jump_cooldown = self.jump_cooldown.saturating_sub(1);
        let mut v = self.frame.velocity;
        if v[0] * v[0] + v[2] * v[2] < 9e-6 {
            v[0] = 0.0;
            v[2] = 0.0;
        }
        if v[1].abs() < 0.003 {
            v[1] = 0.0;
        }
        if input.jump && self.frame.on_ground && self.jump_cooldown == 0 {
            v[1] = v[1].max(f64::from(0.42f32));
            self.jump_cooldown = 10;
        } else if !input.jump {
            self.jump_cooldown = 0;
        }
        // Preserve native float evaluation, even when this material simplifies the ratio.
        #[allow(clippy::eq_op)]
        let speed = if self.frame.on_ground {
            0.1f32 * (0.21600002f32 / (0.6f32 * 0.6f32 * 0.6f32))
        } else {
            0.02f32
        };
        let a = acceleration(input, yaw, speed);
        std::array::from_fn(|i| v[i] + a[i])
    }
    fn advance(&mut self, input: SurvivalInput, proposed: [f64; 3], boxes: &[[f64; 6]]) {
        let adjusted = collide_with_step(
            body(self.frame.position),
            proposed,
            boxes,
            self.frame.on_ground,
        );
        let friction = if self.frame.on_ground {
            0.6f32 * 0.91f32
        } else {
            0.91f32
        };
        let mut velocity = proposed;
        for axis in 0..3 {
            if if axis == 1 {
                proposed[axis] != adjusted[axis]
            } else {
                (proposed[axis] - adjusted[axis]).abs() >= 1e-5
            } {
                velocity[axis] = 0.0;
            }
        }
        self.frame.tick += 1;
        let length2 = adjusted.iter().map(|v| v * v).sum::<f64>();
        let move_position =
            length2 > 1e-7 || proposed.iter().map(|v| v * v).sum::<f64>() - length2 < 1e-7;
        if move_position {
            self.frame.position = std::array::from_fn(|i| self.frame.position[i] + adjusted[i]);
        }
        self.frame.velocity = [
            velocity[0] * f64::from(friction),
            (velocity[1] - 0.08) * f64::from(0.98f32),
            velocity[2] * f64::from(friction),
        ];
        self.frame.on_ground = proposed[1] < 0.0 && proposed[1] != adjusted[1];
        self.frame.horizontal_collision =
            (proposed[0] - adjusted[0]).abs() >= 1e-5 || (proposed[2] - adjusted[2]).abs() >= 1e-5;
        self.frame.resting = self.frame.on_ground
            && (!move_position || adjusted == [0.0; 3])
            && input.forward == 0
            && input.strafe == 0
            && !input.jump;
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn input_and_collision_primitives_match_native_game_methods() {
        let fixture: serde_json::Value = serde_json::from_str(include_str!(
            "../../../../../data/java_1_21_11/dry_movement.json"
        ))
        .unwrap();
        assert_eq!(
            fixture["materials"].as_object().unwrap().len(),
            survival::DRY_CUBES.len()
        );
        for material in survival::DRY_CUBES {
            let v = &fixture["materials"][material];
            assert_eq!(v[0].as_f64().unwrap(), f64::from(0.6f32));
            assert_eq!(v[1].as_f64().unwrap(), 1.0);
            assert_eq!(v[2].as_f64().unwrap(), 1.0);
        }
        for c in fixture["inputs"].as_array().unwrap() {
            let input = SurvivalInput {
                strafe: c["strafe"].as_i64().unwrap() as i8,
                forward: c["forward"].as_i64().unwrap() as i8,
                jump: false,
            };
            let actual = acceleration(
                input,
                c["yaw"].as_f64().unwrap() as f32,
                c["speed"].as_f64().unwrap() as f32,
            );
            for (i, v) in actual.iter().enumerate() {
                assert!(
                    (v - c["expected"][i].as_f64().unwrap()).abs() < 1e-10,
                    "input {c}"
                );
            }
        }
        for c in fixture["collisions"].as_array().unwrap() {
            let actual = collide(
                body(serde_json::from_value(c["position"].clone()).unwrap()),
                serde_json::from_value(c["motion"].clone()).unwrap(),
                &serde_json::from_value::<Vec<[f64; 6]>>(c["boxes"].clone()).unwrap(),
            );
            for (i, v) in actual.iter().enumerate() {
                assert!(
                    (v - c["expected"][i].as_f64().unwrap()).abs() < 1e-10,
                    "collision {c}: {actual:?}"
                );
            }
        }
    }
    #[test]
    fn jump_lands_and_released_walking_brakes_with_gravity_retained() {
        let floor = [[-20.0, 0.0, -20.0, 20.0, 1.0, 20.0]];
        let mut model = Model::new([0.5, 1.0, 0.5]);
        let mut peak = 1.0f64;
        for tick in 0..35 {
            let input = SurvivalInput {
                forward: i8::from(tick < 5),
                jump: tick == 0,
                ..Default::default()
            };
            let delta = model.intent(input, 0.0);
            model.advance(input, delta, &floor);
            peak = peak.max(model.frame.position[1]);
        }
        assert!(peak > 2.2 && peak < 2.3);
        assert_eq!(model.frame.position[1], 1.0);
        assert!(model.frame.resting);
        assert!(model.frame.position[2] > 0.5);
        assert_eq!(model.frame.velocity, [0.0, -0.08 * f64::from(0.98f32), 0.0]);
    }
}
