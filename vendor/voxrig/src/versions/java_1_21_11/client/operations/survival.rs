//! Own-player observation and a deliberately bounded stationary standing model.
use super::*;
use crate::versions::java_1_21_11::client::players::{self, PlayerPose};
use std::collections::BTreeMap;

#[cfg(test)]
mod tests;

/// Distinguishes a native new-world default from an actual received update.
#[derive(Clone, Copy, Debug, PartialEq, Serialize)]
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
#[derive(Clone, Copy, Debug, PartialEq, Serialize)]
pub struct AttributeValue {
    /// Value after native modifier order and attribute-specific clamping.
    pub value: f64,
    /// Initialization or receive evidence.
    pub basis: ValueBasis,
}
/// Last received health packet; no health is inferred from game mode.
#[derive(Clone, Debug, PartialEq, Serialize)]
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
#[derive(Clone, Copy, Debug, PartialEq, Serialize)]
pub struct VelocitySample {
    /// Blocks per game tick, resolved from a native packet.
    pub value: [f64; 3],
    /// Packet ordinal.
    pub receive_sequence: u64,
}
/// Last received effect update. Duration is not a current remaining duration.
#[derive(Clone, Debug, PartialEq, Serialize)]
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
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct MotionInterruption {
    /// Native packet ID, retained for diagnosis rather than silently ignored.
    pub packet_id: i32,
    /// Connection-local receive boundary.
    pub receive_sequence: u64,
}
/// Own-player client projection. It does not assert a complete server snapshot.
#[derive(Clone, Debug, Default, PartialEq, Serialize)]
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
const DRY_CUBES: &[&str] = &[
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
const ATTRIBUTES: [(i32, f64, f64, f64); 4] = [
    (ids::SCALE_ATTRIBUTE, 1.0, 0.0625, 16.0),
    (5, 1.0, 0.0, 1024.0),
    (20, 0.0, 0.0, 1024.0),
    (29, 0.2, 0.0, 20.0),
];
impl LocalPlayerState {
    pub(in crate::versions::java_1_21_11::client) fn spawned(entity_id: i32) -> Self {
        let value = |index: usize| {
            Some(AttributeValue {
                value: ATTRIBUTES[index].1,
                basis: ValueBasis::NativeReset,
            })
        };
        Self {
            entity_id: Some(entity_id),
            pose: Some(PlayerPose::Standing),
            pose_basis: Some(ValueBasis::NativeReset),
            scale: value(0),
            block_break_speed: value(1),
            mining_efficiency: value(2),
            submerged_mining_speed: value(3),
            ..Self::default()
        }
    }
    pub(super) fn reset_world(&self) -> Self {
        self.entity_id.map(Self::spawned).unwrap_or_default()
    }
    pub(in crate::versions::java_1_21_11::client) fn correct_velocity(
        &mut self,
        delta: [f64; 3],
        flags: u32,
        sequence: u64,
    ) -> anyhow::Result<()> {
        if delta.iter().any(|v| !v.is_finite()) {
            bail!("non-finite correction velocity");
        }
        let relative = flags & 224;
        let previous = self.velocity.map(|v| v.value);
        // ROTATE_DELTA requires native angle-table rotation of prior velocity.
        // Retain explicit uncertainty instead of applying an approximate rotation.
        let rotated_unknown = flags & 256 != 0 && relative != 0 && previous != Some([0.0; 3]);
        let value = if rotated_unknown || (relative != 0 && previous.is_none()) {
            None
        } else {
            let mut velocity = delta;
            for (axis, value) in velocity.iter_mut().enumerate() {
                if flags & (32 << axis) != 0 {
                    *value += previous.expect("checked baseline")[axis];
                }
            }
            if velocity.iter().any(|v| !v.is_finite()) {
                bail!("non-finite resolved velocity");
            }
            Some(VelocitySample {
                value: velocity,
                receive_sequence: sequence,
            })
        };
        self.velocity = value;
        Ok(())
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
            for (index, (key, _, min, max)) in ATTRIBUTES.iter().enumerate() {
                if let Some(value) = values.get(key) {
                    let sample = Some(AttributeValue {
                        value: value.clamp(*min, *max),
                        basis: ValueBasis::Received { sequence },
                    });
                    match index {
                        0 => next.scale = sample,
                        1 => next.block_break_speed = sample,
                        2 => next.mining_efficiency = sample,
                        3 => next.submerged_mining_speed = sample,
                        _ => unreachable!(),
                    }
                }
            }
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
    state.operations.local_player.velocity = None;
    state.operations.local_player.motion_interruption = Some(MotionInterruption {
        packet_id,
        receive_sequence: state.sequence,
    });
}

// Native packed vector codec. No old 1.16.1 i16 velocity interpretation.
fn velocity(r: &mut Reader<'_>) -> anyhow::Result<[f64; 3]> {
    let first = r.u8()?;
    if first == 0 {
        return Ok([0.0; 3]);
    }
    let second = r.u8()?;
    let packed = (u64::from(r.u32()?) << 16) | (u64::from(second) << 8) | u64::from(first);
    let mut scale = u64::from(first & 3);
    if first & 4 != 0 {
        scale |= u64::from(r.varint()? as u32) << 2;
    }
    Ok(std::array::from_fn(|axis| {
        let bits = ((packed >> (3 + axis * 15)) & 32767).min(32766);
        (bits as f64 * 2.0 / 32766.0 - 1.0) * scale as f64
    }))
}

/// A derived stationary standing context; never a server ground acknowledgement.
#[derive(Clone, Debug, Serialize)]
pub struct StandingContext {
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
    /// Latest received feet position. Locally submitted flight is refused.
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
fn context(state: &mut State, connection_id: u64, tick: u64) -> Result<StandingContext> {
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
    if !state.operations.position_from_server || player.velocity.map(|v| v.value) != Some([0.0; 3])
    {
        return Err(unavailable(
            "stationary context requires a received position and zero resolved velocity",
        ));
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
    let (dimension, height) = state
        .world
        .dimension
        .as_ref()
        .ok_or_else(|| unavailable("dimension unavailable"))?;
    let half = f64::from(0.6f32) / 2.0;
    let bounds = [
        position[0] - half,
        position[1],
        position[2] - half,
        position[0] + half,
        position[1] + f64::from(1.8f32),
        position[2] + half,
    ];
    let mut support = Vec::new();
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
    for x in min[0]..=max[0] {
        for y in min[1]..=max[1] {
            for z in min[2]..=max[2] {
                let p = [x, y, z];
                if y < height.min_y || y >= height.min_y + height.height {
                    return Err(unavailable(
                        "standing context crosses the observed dimension bounds",
                    ));
                }
                let cell = state.reconstruction.cell(&state.world, p);
                if cell.moving.is_some() {
                    return Err(unavailable(format!("moving standing geometry at {p:?}")));
                }
                let block = cell.state.ok_or_else(|| {
                    unavailable(format!("standing geometry unavailable at {p:?}"))
                })?;
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
                        if probe_horizontal && (0.0..1e-7).contains(&gap) {
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
    Ok(StandingContext {
        connection_id,
        receive_sequence: state.sequence,
        client_tick: state.reconstruction.tick,
        world_revision: state.world.revision,
        dimension: dimension.clone(),
        position,
        eye_position: [position[0], position[1] + f64::from(1.62f32), position[2]],
        bounds,
        on_ground: !support.is_empty(),
        support,
        submerged: false,
        player: player.clone(),
    })
}
