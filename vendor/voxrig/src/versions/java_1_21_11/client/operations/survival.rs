//! Own-player observation and a deliberately bounded stationary standing model.
mod attributes;
use super::super::super::wire::velocity;
use super::geometry::GeometryView;
use super::*;
use crate::diagnostic_projection::diagnostic_record;
use crate::versions::java_1_21_11::client::players::{self, PlayerPose};
use std::collections::BTreeMap;

#[cfg(test)]
mod tests;

/// Distinguishes a native new-world default from an actual received update.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, serde::Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum ValueBasis {
    /// Native client initialization, not an independent server observation.
    NativeReset,
    /// Native update applied at this receive boundary.
    Received {
        /// Connection-local packet ordinal.
        sequence: u64,
    },
}
/// A projected native attribute and its basis.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, serde::Deserialize)]
pub struct AttributeValue {
    /// Value after native modifier order and attribute-specific clamping.
    pub value: f64,
    /// Initialization or receive evidence.
    pub basis: ValueBasis,
}
/// Last received health packet; no health is inferred from game mode.
#[derive(Clone, Debug, PartialEq, Serialize, serde::Deserialize)]
pub struct PlayerHealth {
    /// Native health (zero means dead).
    pub health: f32,
    /// Food level, 0..20.
    pub food: i32,
    /// Received saturation.
    pub saturation: f32,
    /// Packet ordinal.
    pub receive_sequence: u64,
}
/// A velocity sample; elapsed time does not simulate gravity or friction.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, serde::Deserialize)]
pub struct VelocitySample {
    /// Blocks per game tick, resolved from a native packet.
    pub value: [f64; 3],
    /// Packet ordinal.
    pub receive_sequence: u64,
}
/// Last received effect update. Duration is not a current remaining duration.
#[derive(Clone, Debug, PartialEq, Serialize, serde::Deserialize)]
pub struct ReceivedEffect {
    /// Native status-effect registry identifier.
    pub effect_id: i32,
    /// Native amplifier, 0..255.
    pub amplifier: i32,
    /// Duration at receipt; -1 is infinite.
    pub duration_at_receipt: i32,
    /// Ambient/particles/icon flags.
    pub flags: u8,
    /// Packet ordinal.
    pub receive_sequence: u64,
}
/// A packet whose player-motion consequences are outside this stationary model.
#[derive(Clone, Debug, PartialEq, Serialize, serde::Deserialize)]
pub struct MotionInterruption {
    /// Native packet ID, retained for diagnosis rather than silently ignored.
    pub packet_id: i32,
    /// Connection-local receive boundary.
    pub receive_sequence: u64,
}
/// Own-player client projection. It does not assert a complete server snapshot.
#[derive(Clone, Debug, Default, PartialEq, Serialize, serde::Deserialize)]
pub struct LocalPlayerState {
    /// Identity from this world's login, never from a nearby player's spawn.
    pub entity_id: Option<i32>,
    /// Native initialized/received posture; unknown serializers invalidate it.
    pub pose: Option<PlayerPose>,
    /// Evidence for the pose; None when unavailable.
    pub pose_basis: Option<ValueBasis>,
    /// Native standing scale.
    pub scale: Option<AttributeValue>,
    /// Native block-break-speed attribute.
    pub block_break_speed: Option<AttributeValue>,
    /// Native mining-efficiency attribute, not a tool speed.
    pub mining_efficiency: Option<AttributeValue>,
    /// Native submerged-mining-speed attribute, not evidence of submersion.
    pub submerged_mining_speed: Option<AttributeValue>,
    /// Native movement-speed attribute, including received modifiers.
    pub movement_speed: Option<AttributeValue>,
    /// Native gravity; negative values are preserved, not silently made normal.
    pub gravity: Option<AttributeValue>,
    /// Native jump-strength attribute; not an assertion that jumping is allowed.
    pub jump_strength: Option<AttributeValue>,
    /// Maximum native stepping height, before geometric checks.
    pub step_height: Option<AttributeValue>,
    /// Native movement-efficiency attribute.
    pub movement_efficiency: Option<AttributeValue>,
    /// Native sneaking-speed attribute.
    pub sneaking_speed: Option<AttributeValue>,
    /// Native safe-fall distance; does not establish a safe path.
    pub safe_fall_distance: Option<AttributeValue>,
    /// Native fall-damage multiplier; no damage prediction is implied.
    pub fall_damage_multiplier: Option<AttributeValue>,
    /// Last resolved velocity; None for unsupported rotated relative updates.
    pub velocity: Option<VelocitySample>,
    /// Unsupported impulse/vehicle context; requires a fresh world baseline.
    pub motion_interruption: Option<MotionInterruption>,
    /// Last received health; unavailable before the packet and after a world reset.
    pub health: Option<PlayerHealth>,
    /// Effect updates not yet removed by a packet. No local expiration is invented.
    pub effect_updates: BTreeMap<i32, ReceivedEffect>,
    /// False: vanilla effect packets have no complete-list fence in this projection.
    /// An empty map must not authorize assumptions about absence for mining.
    pub effects_complete: bool,
}

// Audited against the game's native registry/default attribute container.
pub(super) const DRY_CUBES: &[&str] = &[
    "minecraft:stone",
    "minecraft:dirt",
    "minecraft:grass_block",
    "minecraft:cobblestone",
    "minecraft:oak_planks",
    "minecraft:spruce_planks",
    "minecraft:quartz_block",
    "minecraft:smooth_quartz",
    "minecraft:white_concrete",
    "minecraft:glass",
    "minecraft:andesite",
    "minecraft:granite",
];
impl LocalPlayerState {
    pub(in crate::versions::java_1_21_11::client) fn spawned(entity_id: i32) -> Self {
        let mut player = Self {
            entity_id: Some(entity_id),
            pose: Some(PlayerPose::Standing),
            pose_basis: Some(ValueBasis::NativeReset),
            ..Self::default()
        };
        attributes::initialize(&mut player);
        player
    }
    pub(super) fn reset_world(&self) -> Self {
        self.entity_id.map(Self::spawned).unwrap_or_default()
    }
}

pub(super) fn receive(state: &mut State, id: i32, payload: &[u8]) -> anyhow::Result<bool> {
    use ids::play_clientbound as input;
    if matches!(id, input::EXPLOSION | input::VEHICLE_MOVE) {
        // These known packets previously had no advertised projection. Do not
        // pretend their ignored complex payload establishes stationary motion.
        interrupt(state, id);
        return Ok(true);
    }
    if id == input::SET_PASSENGERS {
        let mut r = Reader::new(payload);
        let vehicle = r.varint()?;
        if vehicle < 0 {
            bail!("invalid vehicle entity ID");
        }
        let mut own = false;
        for _ in 0..r.count(1024)? {
            let passenger = r.varint()?;
            if passenger < 0 {
                bail!("invalid passenger entity ID");
            }
            own |= Some(passenger) == state.operations.local_player.entity_id;
        }
        r.end()?;
        if own {
            interrupt(state, id);
        }
        return Ok(true);
    }
    if !matches!(
        id,
        input::UPDATE_HEALTH
            | input::ENTITY_VELOCITY
            | input::ENTITY_METADATA
            | input::ENTITY_UPDATE_ATTRIBUTES
            | input::ENTITY_EFFECT
            | input::REMOVE_ENTITY_EFFECT
    ) {
        return Ok(false);
    }
    let mut r = Reader::new(payload);
    let mut next = state.operations.local_player.clone();
    if id != input::UPDATE_HEALTH && Some(r.varint()?) != next.entity_id {
        return Ok(false); // Let the remote-player receiver inspect its own entities.
    }
    let sequence = state.sequence;
    match id {
        input::UPDATE_HEALTH => {
            let health = r.f32()?;
            let food = r.varint()?;
            let saturation = r.f32()?;
            if health < 0.0 || !(0..=20).contains(&food) || saturation < 0.0 {
                bail!("invalid player health");
            }
            next.health = Some(PlayerHealth {
                health,
                food,
                saturation,
                receive_sequence: sequence,
            });
        }
        input::ENTITY_VELOCITY => {
            next.velocity = Some(VelocitySample {
                value: velocity(&mut r)?,
                receive_sequence: sequence,
            });
        }
        input::ENTITY_METADATA => {
            let update = players::read_pose(&mut r, &mut next.pose)?;
            if !update.supported {
                next.pose = None;
                next.pose_basis = None;
                state.operations.local_player = next;
                return Ok(true); // Unsupported serializer deliberately invalidates posture.
            }
            if update.received {
                next.pose_basis = Some(ValueBasis::Received { sequence });
            }
        }
        input::ENTITY_UPDATE_ATTRIBUTES => {
            let values = players::read_attributes(&mut r)?;
            attributes::received(&mut next, &values, sequence);
        }
        input::ENTITY_EFFECT | input::REMOVE_ENTITY_EFFECT => {
            let effect_id = r.varint()?;
            if !(0..=65535).contains(&effect_id) {
                bail!("invalid effect ID");
            }
            if id == input::REMOVE_ENTITY_EFFECT {
                next.effect_updates.remove(&effect_id);
            } else {
                let amplifier = r.varint()?;
                let duration_at_receipt = r.varint()?;
                let flags = r.u8()?;
                if !(0..=255).contains(&amplifier) || duration_at_receipt < -1 || flags & !7 != 0 {
                    bail!("invalid status effect");
                }
                if next.effect_updates.len() >= 256 && !next.effect_updates.contains_key(&effect_id)
                {
                    bail!("own-player effect limit exceeded");
                }
                next.effect_updates.insert(
                    effect_id,
                    ReceivedEffect {
                        effect_id,
                        amplifier,
                        duration_at_receipt,
                        flags,
                        receive_sequence: sequence,
                    },
                );
            }
        }
        _ => unreachable!(),
    }
    r.end()?;
    state.operations.local_player = next;
    Ok(true)
}

fn interrupt(state: &mut State, packet_id: i32) {
    state
        .motion
        .invalidate(state.sequence, "unmodeled player motion");
    state.operations.local_player.velocity = None;
    state.operations.local_player.motion_interruption = Some(MotionInterruption {
        packet_id,
        receive_sequence: state.sequence,
    });
}

diagnostic_record! {
    /// A derived stationary standing context; never a server ground acknowledgement.
    #[derive(Clone, Debug, Serialize)]
    pub struct StandingContext => RecordedStandingContext {
        /// Connection identity.
        pub connection_id: u64,
        /// Received packet boundary shared with the geometry read.
        pub receive_sequence: u64,
        /// Current reconstructed client frame, not server time.
        pub client_tick: u64,
        /// Received-world revision.
        pub world_revision: u64,
        /// Current dimension.
        pub dimension: String,
        /// Explicit received or predicted-and-observed basis. Neither proves server rest.
        pub position_basis: StandingPositionBasis,
        /// Feet position with the basis above. Locally submitted flight is refused.
        pub position: [f64; 3],
        /// Native standing eye position, using float dimensions.
        pub eye_position: [f64; 3],
        /// Native unscaled standing body bounds, minimum XYZ then maximum XYZ.
        pub bounds: [f64; 6],
        /// Derived downward contact with admitted static geometry.
        pub on_ground: bool,
        /// Solid cells establishing the contact; empty if not supported.
        pub support: Vec<[i32; 3]>,
        /// False after all admitted surrounding cells establish a dry context.
        pub submerged: bool,
        /// Own-player projection at the same boundary, including mining attributes.
        pub player: LocalPlayerState,
    }
    diagnostic_serde {}
}

impl Operations {
    /// Inspect stationary normal-size standing geometry. Unsupported fluid,
    /// posture, motion, moving blocks or unavailable geometry reject the query.
    pub async fn standing_context(&self) -> Result<StandingContext> {
        let mut state = self.bot.session.state.lock().await;
        self.ready(&state)?;
        let tick = self.bot.session.started.elapsed().as_millis() as u64 / 50;
        context(&mut state, self.bot.session.id, tick)
    }
}

pub(super) fn look_flags(state: &mut State, tick: u64) -> Result<u8> {
    if state.operations.game_mode == Some(GameMode::Survival) {
        Ok(u8::from(context(state, 0, tick)?.on_ground))
    } else {
        Ok(0)
    } // Existing creative flight control has no standing contract.
}

fn unavailable(message: impl std::fmt::Display) -> Error {
    Error::new(ErrorKind::State, anyhow::anyhow!("{message}"))
}
struct StandingRegion {
    bounds: [f64; 6],
    min: [i32; 3],
    max: [i32; 3],
}
fn standing_region(position: [f64; 3]) -> StandingRegion {
    let half = f64::from(0.6f32) / 2.0;
    let bounds = [
        position[0] - half,
        position[1],
        position[2] - half,
        position[0] + half,
        position[1] + f64::from(1.8f32),
        position[2] + half,
    ];
    // One-cell halo also refuses unsupported protruding/context-dependent neighbors.
    let min = [
        bounds[0].floor() as i32 - 1,
        bounds[1].floor() as i32 - 1,
        bounds[2].floor() as i32 - 1,
    ];
    let max = [
        bounds[3].floor() as i32 + 1,
        bounds[4].floor() as i32 + 1,
        bounds[5].floor() as i32 + 1,
    ];
    StandingRegion { bounds, min, max }
}

#[cfg(test)]
pub(super) fn standing_intersects(position: [f64; 3], cell: [i32; 3]) -> bool {
    let bounds = standing_region(position).bounds;
    (0..3).all(|i| bounds[i] < f64::from(cell[i] + 1) && bounds[i + 3] > f64::from(cell[i]))
}

// Readiness covers every cell that context() will inspect, including neighbor
// chunks at chunk edges. Unknown cells wait; unsupported received cells still
// fail the subsequent context validation. No timeout infers missing geometry.
pub(super) fn standing_baselines_received(state: &State) -> Result<bool> {
    let Some(position) = state.position else {
        return Ok(false);
    };
    validate_pose(position, state.rotation)?;
    let Some((_, height)) = &state.world.dimension else {
        return Ok(false);
    };
    let StandingRegion { min, max, .. } = standing_region(position);
    if min[1] < height.min_y || max[1] >= height.min_y + height.height {
        return Err(unavailable(
            "standing context crosses the observed dimension bounds",
        ));
    }
    Ok((min[0]..=max[0]).all(|x| {
        (min[1]..=max[1]).all(|y| (min[2]..=max[2]).all(|z| state.world.block([x, y, z]).is_some()))
    }))
}

pub(super) fn context(state: &mut State, connection_id: u64, tick: u64) -> Result<StandingContext> {
    let basis = super::movement::standing_basis(state)?;
    context_with_basis(state, connection_id, tick, basis)
}
pub(super) fn context_with_basis(
    state: &mut State,
    connection_id: u64,
    tick: u64,
    position_basis: StandingPositionBasis,
) -> Result<StandingContext> {
    let player = &state.operations.local_player;
    if let Some(interruption) = &player.motion_interruption {
        return Err(unavailable(format!(
            "unsupported player motion packet {} at receive sequence {}; fresh world baseline required",
            interruption.packet_id, interruption.receive_sequence
        )));
    }
    if player.entity_id.is_none()
        || player.pose != Some(PlayerPose::Standing)
        || player.scale.map(|v| v.value) != Some(1.0)
    {
        return Err(unavailable(
            "stationary context requires known normal-size standing posture",
        ));
    }
    if state.operations.requested_flying || state.operations.abilities.is_some_and(|a| a & 2 != 0) {
        return Err(unavailable("stationary context refuses active flight"));
    }
    if player.health.as_ref().is_some_and(|h| h.health <= 0.0) {
        return Err(unavailable("player is dead"));
    }
    let position = state
        .position
        .ok_or_else(|| unavailable("player position unavailable"))?;
    validate_pose(position, state.rotation)?;
    state.reconstruction.advance(&state.world, tick);
    if state.reconstruction.issue.is_some() || !state.reconstruction.recovery_chunks.is_empty() {
        return Err(unavailable("client reconstruction incomplete"));
    }
    let geometry = standing_geometry(state, position, position_basis.geometry_reserve())?;
    if matches!(position_basis, StandingPositionBasis::Predicted { .. })
        && geometry.support.is_empty()
    {
        return Err(unavailable(
            "predicted standing lost its currently received floor support",
        ));
    }
    Ok(StandingContext {
        connection_id,
        receive_sequence: state.sequence,
        client_tick: state.reconstruction.tick,
        world_revision: state.world.revision,
        dimension: state.world.dimension.as_ref().unwrap().0.clone(),
        position,
        position_basis,
        eye_position: [position[0], position[1] + f64::from(1.62f32), position[2]],
        bounds: geometry.bounds,
        on_ground: !geometry.support.is_empty(),
        support: geometry.support,
        submerged: false,
        player: player.clone(),
    })
}
pub(super) struct StandingGeometry {
    pub bounds: [f64; 6],
    pub support: Vec<[i32; 3]>,
}
// Pure geometry shared by prospective endpoints and actual standing admission.
pub(super) fn standing_geometry(
    state: &impl GeometryView,
    position: [f64; 3],
    error: [f64; 3],
) -> Result<StandingGeometry> {
    validate_pose(position, [0.0; 2])?;
    let StandingRegion {
        mut bounds,
        min,
        max,
    } = standing_region(position);
    for axis in [0, 2] {
        bounds[axis] -= error[axis];
        bounds[axis + 3] += error[axis];
    }
    let mut support = Vec::new();
    for x in min[0]..=max[0] {
        for y in min[1]..=max[1] {
            for z in min[2]..=max[2] {
                let p = [x, y, z];
                let block = state.block(p)?;
                match block.name.as_str() {
                    "minecraft:air" | "minecraft:cave_air" | "minecraft:void_air" => {}
                    name if DRY_CUBES.contains(&name) => {
                        let cube = [
                            f64::from(x),
                            f64::from(y),
                            f64::from(z),
                            f64::from(x + 1),
                            f64::from(y + 1),
                            f64::from(z + 1),
                        ];
                        let horizontal = bounds[0] < cube[3]
                            && bounds[3] > cube[0]
                            && bounds[2] < cube[5]
                            && bounds[5] > cube[2];
                        if horizontal && bounds[1] < cube[4] && bounds[4] > cube[1] {
                            return Err(unavailable(format!(
                                "standing body intersects solid geometry at {p:?}"
                            )));
                        }
                        let gap = bounds[1] - cube[4];
                        // Native VoxelShape axis probing shrinks the other two axes by 1e-7.
                        let probe_horizontal = bounds[0] + 1e-7 < cube[3]
                            && bounds[3] - 1e-7 > cube[0]
                            && bounds[2] + 1e-7 < cube[5]
                            && bounds[5] - 1e-7 > cube[2];
                        if probe_horizontal
                            && (0.0..1e-7).contains(&gap)
                            && (error == [0.0; 3]
                                || (bounds[0] + 2.0 * error[0] + 1e-7 < cube[3]
                                    && bounds[3] - 2.0 * error[0] - 1e-7 > cube[0]
                                    && bounds[2] + 2.0 * error[2] + 1e-7 < cube[5]
                                    && bounds[5] - 2.0 * error[2] - 1e-7 > cube[2]))
                        {
                            support.push(p);
                        }
                    }
                    _ => {
                        return Err(Error::new(
                            ErrorKind::Unsupported,
                            anyhow::anyhow!(
                                "unsupported standing geometry {} at {p:?}",
                                block.name
                            ),
                        ));
                    }
                }
            }
        }
    }
    Ok(StandingGeometry { bounds, support })
}

// Conservative post-motion reach/occlusion check. For admitted full cubes the
// swept ray is contained in this axis-aligned corridor. Refusing any intervening
// solid in the corridor deliberately avoids treating corner samples as proof
// that the continuum between them is unobstructed.
pub(super) fn uncertain_target(
    state: &State,
    standing: &StandingContext,
    hit: &super::super::raycast::BlockHit,
) -> Result<()> {
    uncertain_target_in(
        state,
        standing.eye_position,
        standing.position_basis.geometry_reserve(),
        state.rotation,
        hit,
    )
}
pub(super) fn uncertain_target_in(
    state: &impl GeometryView,
    eye_position: [f64; 3],
    error: [f64; 3],
    rotation: [f32; 2],
    hit: &super::super::raycast::BlockHit,
) -> Result<()> {
    if error == [0.0; 3] {
        return Ok(());
    }
    if !DRY_CUBES.contains(&hit.state.name.as_str()) {
        return Err(unavailable(
            "uncertain target requires an admitted full cube",
        ));
    }
    super::super::raycast::uncertain_cube_hit_in(eye_position, error, rotation, hit, |p| {
        state.block(p).map_err(anyhow::Error::from)
    })
}
