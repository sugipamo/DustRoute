//! Received remote-player tracking, without entity physics or render interpolation.
use super::{Reader, ids};
use anyhow::{Context, bail};
use serde::Serialize;
use std::collections::{BTreeMap, BTreeSet};

#[cfg(test)]
mod tests;

/// Packet-observed players in this dimension, not a list of online accounts.
#[derive(Clone, Debug, Serialize)]
pub struct PlayerObservations {
    /// Connection-local identity.
    pub connection_id: u64,
    /// Receive boundary shared by all records.
    pub receive_sequence: u64,
    /// Dimension containing these entities.
    pub dimension: String,
    /// Only spawned entities with a received profile are exposed.
    pub players: Vec<ObservedPlayer>,
}

/// Latest received player coordinates; no server-current-time guarantee.
#[derive(Clone, Debug, Serialize)]
pub struct ObservedPlayer {
    /// Name from the native profile packet.
    pub name: String,
    /// Native UUID bytes, independent of reusable entity IDs.
    pub uuid: [u8; 16],
    /// Connection-local entity identifier.
    pub entity_id: i32,
    /// Feet position, with native relative-position quantization.
    pub position: [f64; 3],
    /// Head yaw and pitch in native degrees, without visual interpolation.
    pub rotation: [f32; 2],
    /// Known native pose, absent after unsupported metadata.
    pub pose: Option<PlayerPose>,
    /// Received scale attribute (native default is one).
    pub scale: f32,
    /// Absent for unsupported poses/metadata; never guessed as standing.
    pub eye_position: Option<[f64; 3]>,
    /// Last packet affecting this entity's spatial observation.
    pub receive_sequence: u64,
}

/// Native poses relevant to a player's viewpoint.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum PlayerPose {
    /// Standing.
    Standing,
    /// Fall flying.
    Gliding,
    /// Sleeping; bed orientation is not reconstructed here.
    Sleeping,
    /// Swimming.
    Swimming,
    /// Riptide spin.
    SpinAttack,
    /// Sneaking.
    Crouching,
    /// Other poses are retained but do not produce a viewpoint.
    Unsupported {
        /// Native enum ID.
        id: i32,
    },
}
impl PlayerPose {
    pub(super) fn decode(id: i32) -> Self {
        match id {
            0 => Self::Standing,
            1 => Self::Gliding,
            2 => Self::Sleeping,
            3 => Self::Swimming,
            4 => Self::SpinAttack,
            5 => Self::Crouching,
            id => Self::Unsupported { id },
        }
    }
    fn eye_height(self) -> Option<f32> {
        match self {
            Self::Standing => Some(1.62),
            Self::Gliding | Self::Swimming | Self::SpinAttack => Some(0.4),
            Self::Crouching => Some(1.27),
            Self::Sleeping | Self::Unsupported { .. } => None,
        }
    }
}

#[derive(Clone)]
struct Entity {
    uuid: [u8; 16],
    position: [f64; 3],
    rotation: [f32; 2],
    pose: Option<PlayerPose>,
    scale: f32,
    sequence: u64,
}

#[derive(Default)]
pub(super) struct PlayerTracker {
    profiles: BTreeMap<[u8; 16], String>,
    entities: BTreeMap<i32, Entity>,
}
impl PlayerTracker {
    pub fn reset_world(&mut self) {
        // Player-info entries survive dimension changes; world entities do not.
        self.entities.clear();
    }
    pub fn observations(&self) -> Vec<ObservedPlayer> {
        self.entities
            .iter()
            .filter_map(|(&id, entity)| {
                let name = self.profiles.get(&entity.uuid)?.clone();
                let eye_position = entity.pose.and_then(PlayerPose::eye_height).map(|height| {
                    let mut p = entity.position;
                    p[1] += f64::from(height * entity.scale);
                    p
                });
                Some(ObservedPlayer {
                    name,
                    uuid: entity.uuid,
                    entity_id: id,
                    position: entity.position,
                    rotation: entity.rotation,
                    pose: entity.pose,
                    scale: entity.scale,
                    eye_position,
                    receive_sequence: entity.sequence,
                })
            })
            .collect()
    }
    pub fn receive(&mut self, id: i32, bytes: &[u8], sequence: u64) -> anyhow::Result<bool> {
        use ids::play_clientbound as p;
        let mut r = Reader::new(bytes);
        match id {
            p::PLAYER_INFO => {
                let flags = r.u8()?;
                let mut additions = Vec::new();
                for _ in 0..r.count(1024)? {
                    let uuid = r.take(16)?.try_into()?;
                    if flags & 1 != 0 {
                        let name = r.string()?;
                        if name.is_empty() || name.len() > 64 {
                            bail!("invalid player profile name");
                        }
                        for _ in 0..r.count(64)? {
                            r.string()?;
                            r.string()?;
                            if r.bool()? {
                                r.string()?;
                            }
                        }
                        additions.push((uuid, name));
                    }
                    if flags & 2 != 0 && r.bool()? {
                        r.take(24)?; // Session UUID and key expiry.
                        r.byte_array(65536)?;
                        r.byte_array(65536)?;
                    }
                    for bit in [4, 8, 16] {
                        if flags & bit != 0 {
                            r.varint()?;
                        }
                    }
                    if flags & 32 != 0 && r.bool()? {
                        r.skip_nbt()?;
                    }
                    // Native action order differs from the bit order.
                    if flags & 128 != 0 {
                        r.varint()?;
                    }
                    if flags & 64 != 0 {
                        r.bool()?;
                    }
                }
                r.end()?;
                let new_count = additions
                    .iter()
                    .map(|a| a.0)
                    .collect::<BTreeSet<_>>()
                    .iter()
                    .filter(|u| !self.profiles.contains_key(*u))
                    .count();
                if self.profiles.len() + new_count > 1024 {
                    bail!("player profile limit exceeded");
                }
                self.profiles.extend(additions);
            }
            p::PLAYER_REMOVE => {
                let mut removed = Vec::new();
                for _ in 0..r.count(1024)? {
                    removed.push(<[u8; 16]>::try_from(r.take(16)?)?);
                }
                r.end()?;
                for uuid in removed {
                    self.profiles.remove(&uuid);
                    self.entities.retain(|_, entity| entity.uuid != uuid);
                }
            }
            p::SPAWN_ENTITY => {
                let entity_id = r.varint()?;
                let uuid = r.take(16)?.try_into()?;
                let kind = r.varint()?;
                if kind != ids::PLAYER_ENTITY_TYPE {
                    self.entities.remove(&entity_id);
                    return Ok(true);
                }
                let position = position(&mut r)?;
                let velocity = r.u8()?;
                if velocity != 0 {
                    r.take(5)?;
                    if velocity & 4 != 0 {
                        r.varint()?;
                    }
                }
                let pitch = angle(&mut r)?;
                angle(&mut r)?; // Body yaw; gaze uses head yaw.
                let yaw = angle(&mut r)?;
                r.varint()?;
                r.end()?;
                if self.entities.len() >= 1024 && !self.entities.contains_key(&entity_id) {
                    bail!("tracked player limit exceeded");
                }
                self.entities.insert(
                    entity_id,
                    Entity {
                        uuid,
                        position,
                        rotation: [yaw, pitch],
                        pose: Some(PlayerPose::Standing),
                        scale: 1.0,
                        sequence,
                    },
                );
            }
            p::ENTITY_DESTROY => {
                let mut removed = Vec::new();
                for _ in 0..r.count(65536)? {
                    removed.push(r.varint()?);
                }
                r.end()?;
                for entity_id in removed {
                    self.entities.remove(&entity_id);
                }
            }
            p::REL_ENTITY_MOVE
            | p::ENTITY_MOVE_LOOK
            | p::ENTITY_LOOK
            | p::ENTITY_TELEPORT
            | p::SYNC_ENTITY_POSITION
            | p::ENTITY_HEAD_ROTATION
            | p::ENTITY_METADATA
            | p::ENTITY_UPDATE_ATTRIBUTES => {
                let entity_id = r.varint()?;
                let Some(previous) = self.entities.get(&entity_id) else {
                    return Ok(true);
                };
                // Commit only after the complete supported packet is validated.
                let mut entity = previous.clone();
                match id {
                    p::REL_ENTITY_MOVE | p::ENTITY_MOVE_LOOK | p::ENTITY_LOOK => {
                        if id != p::ENTITY_LOOK {
                            for coordinate in &mut entity.position {
                                let delta = i16::from_be_bytes(r.take(2)?.try_into()?);
                                if delta != 0 {
                                    // TrackedPosition.pack uses Java Math.round, including negative ties.
                                    *coordinate = ((*coordinate * 4096.0 + 0.5).floor()
                                        + f64::from(delta))
                                        / 4096.0;
                                }
                            }
                        }
                        if id != p::REL_ENTITY_MOVE {
                            entity.rotation = [angle(&mut r)?, angle(&mut r)?];
                        }
                        r.bool()?;
                    }
                    p::ENTITY_TELEPORT => {
                        entity.position = position(&mut r)?;
                        entity.rotation = [angle(&mut r)?, angle(&mut r)?];
                        r.bool()?;
                    }
                    p::SYNC_ENTITY_POSITION => {
                        entity.position = position(&mut r)?;
                        for _ in 0..3 {
                            r.f64()?;
                        }
                        entity.rotation = [r.f32()?, r.f32()?];
                        r.bool()?;
                    }
                    p::ENTITY_HEAD_ROTATION => {
                        entity.rotation[0] = angle(&mut r)?;
                    }
                    p::ENTITY_METADATA => {
                        if !read_pose(&mut r, &mut entity.pose)?.supported {
                            // An unknown payload cannot be skipped safely. Preserve position,
                            // but invalidate the viewpoint until an explicit supported pose arrives.
                            entity.pose = None;
                            entity.sequence = sequence;
                            self.entities.insert(entity_id, entity);
                            return Ok(true);
                        }
                    }
                    p::ENTITY_UPDATE_ATTRIBUTES => {
                        if let Some(value) = read_attributes(&mut r)?.get(&ids::SCALE_ATTRIBUTE) {
                            entity.scale = value.clamp(0.0625, 16.0) as f32;
                        }
                    }
                    _ => unreachable!(),
                }
                r.end()?;
                entity.sequence = sequence;
                self.entities.insert(entity_id, entity);
            }
            _ => return Ok(false),
        }
        Ok(true)
    }
}

fn angle(r: &mut Reader<'_>) -> anyhow::Result<f32> {
    Ok(f32::from(r.u8()? as i8) * 360.0 / 256.0)
}
fn position(r: &mut Reader<'_>) -> anyhow::Result<[f64; 3]> {
    let position = [r.f64()?, r.f64()?, r.f64()?];
    if position.iter().any(|v| v.abs() > 33_554_432.0) {
        bail!("player coordinate out of bounds");
    }
    Ok(position)
}
/// Native modifier order, shared by own and remote player observations.
pub(super) fn read_attributes(r: &mut Reader<'_>) -> anyhow::Result<BTreeMap<i32, f64>> {
    let mut values = BTreeMap::new();
    for _ in 0..r.count(1024)? {
        let key = r.varint()?;
        if key < 0 || values.contains_key(&key) {
            bail!("invalid/duplicate player attribute");
        }
        let base = r.f64()?;
        let mut additions = 0.0;
        let mut base_factors = Vec::new();
        let mut total_factors = Vec::new();
        let mut modifiers = BTreeSet::new();
        for _ in 0..r.count(1024)? {
            if !modifiers.insert(r.string()?) {
                bail!("duplicate attribute modifier");
            }
            let amount = r.f64()?;
            match r.u8()? {
                0 => additions += amount,
                1 => base_factors.push(amount),
                2 => total_factors.push(amount),
                _ => bail!("unknown attribute operation"),
            }
        }
        let adjusted = base + additions;
        let mut value = adjusted;
        for amount in base_factors {
            value += adjusted * amount;
        }
        for amount in total_factors {
            value *= 1.0 + amount;
        }
        if !value.is_finite() {
            bail!("non-finite player attribute");
        }
        values.insert(key, value);
    }
    Ok(values)
}
pub(super) struct PoseUpdate {
    pub supported: bool,
    pub received: bool,
}
pub(super) fn read_pose(
    r: &mut Reader<'_>,
    pose: &mut Option<PlayerPose>,
) -> anyhow::Result<PoseUpdate> {
    let mut seen = BTreeSet::new();
    let mut received = false;
    loop {
        let key = r.u8()?;
        if key == 255 {
            return Ok(PoseUpdate {
                supported: true,
                received,
            });
        }
        if !seen.insert(key) {
            bail!("duplicate player metadata index");
        }
        let kind = r.varint()?;
        if key == ids::PLAYER_POSE_METADATA {
            if kind != 20 {
                bail!("incorrect player pose serializer");
            }
            *pose = Some(PlayerPose::decode(r.varint()?));
            received = true;
        } else if !skip_metadata(r, kind)? {
            return Ok(PoseUpdate {
                supported: false,
                received,
            });
        }
    }
}
fn skip_metadata(r: &mut Reader<'_>, kind: i32) -> anyhow::Result<bool> {
    match kind {
        0 => {
            r.take(1)?;
        }
        1 | 12 | 14 | 15 | 19..=28 | 31..=34 | 38 => {
            r.varint()?;
        }
        2 => {
            let mut done = false;
            for i in 0..10 {
                let byte = r.u8()?;
                if i == 9 && byte > 1 {
                    bail!("metadata varlong overflow");
                }
                if byte & 128 == 0 {
                    done = true;
                    break;
                }
            }
            if !done {
                bail!("metadata varlong too long");
            }
        }
        3 => {
            r.f32()?;
        }
        4 => {
            r.string()?;
        }
        5 => {
            r.skip_nbt()?;
        }
        6 => {
            if r.bool()? {
                r.skip_nbt()?;
            }
        }
        8 => {
            r.bool()?;
        }
        9 | 35 => {
            for _ in 0..3 {
                r.f32()?;
            }
        }
        10 => {
            r.take(8)?;
        }
        11 => {
            if r.bool()? {
                r.take(8)?;
            }
        }
        13 => {
            if r.bool()? {
                r.take(16)?;
            }
        }
        16 => {
            if !skip_effect_particle(r)? {
                return Ok(false);
            }
        }
        17 => {
            for _ in 0..r.count(1024)? {
                if !skip_effect_particle(r)? {
                    return Ok(false);
                }
            }
        }
        18 => {
            for _ in 0..3 {
                r.varint()?;
            }
        }
        29 => {
            if r.bool()? {
                r.string()?;
                r.take(8)?;
            }
        }
        36 => {
            for _ in 0..4 {
                r.f32()?;
            }
        }
        // Slot, painting and profile serializers need their own bounded codecs.
        _ => return Ok(false),
    }
    Ok(true)
}
fn skip_effect_particle(r: &mut Reader<'_>) -> anyhow::Result<bool> {
    // The native player's active potion-effect particles use entity_effect (ARGB).
    // Other particle payloads are not inferred from their names or lengths.
    if r.varint()? != ids::ENTITY_EFFECT_PARTICLE {
        return Ok(false);
    }
    r.take(4).context("truncated entity-effect particle")?;
    Ok(true)
}
