//! Tracked entity state, metadata, spawn parsing, and entity raycasts.

use crate::versions::java_1_16_1::{
    interaction::BlockPos,
    inventory::{ItemStack, read_slot},
    physics::Vec3,
    protocol::{get_string, get_varint},
    registry::entity_name,
    world::skip_nbt,
};
use anyhow::{Context, Result, bail};
use byteorder::{BigEndian, ReadBytesExt};
use std::{collections::HashMap, io::Cursor};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[non_exhaustive]
/// Possible values represented by `EntityKind`.
pub enum EntityKind {
    /// The `Object` variant.
    Object,
    /// The `Living` variant.
    Living,
    /// The `Player` variant.
    Player,
    /// The `ExperienceOrb` variant.
    ExperienceOrb,
    /// The `Painting` variant.
    Painting,
}

#[derive(Clone, Debug, PartialEq)]
/// State and protocol data represented by `EntityState`.
pub struct EntityState {
    /// The `entity_id` value.
    pub entity_id: i32,
    /// The `uuid` value.
    pub uuid: Option<[u8; 16]>,
    /// The `kind` value.
    pub kind: EntityKind,
    /// The `type_id` value.
    pub type_id: Option<i32>,
    /// The `type_name` value.
    pub type_name: Option<&'static str>,
    /// The `position` value.
    pub position: Vec3,
    /// The `velocity` value.
    pub velocity: Vec3,
    /// The `yaw` value.
    pub yaw: f32,
    /// The `pitch` value.
    pub pitch: f32,
    /// The `head_yaw` value.
    pub head_yaw: f32,
    /// The `on_ground` value.
    pub on_ground: bool,
    /// The `object_data` value.
    pub object_data: Option<i32>,
    /// The `experience_count` value.
    pub experience_count: Option<i16>,
    /// The `painting_motive` value.
    pub painting_motive: Option<i32>,
    /// The `painting_direction` value.
    pub painting_direction: Option<u8>,
    /// The `metadata` value.
    pub metadata: HashMap<u8, MetadataValue>,
    /// The `equipment` value.
    pub equipment: HashMap<i8, Option<ItemStack>>,
    /// The `passengers` value.
    pub passengers: Vec<i32>,
    /// The `attached_to` value.
    pub attached_to: Option<i32>,
}

#[derive(Clone, Debug, PartialEq)]
/// State and protocol data represented by `EntityRaycastHit`.
pub struct EntityRaycastHit {
    /// The `entity` value.
    pub entity: EntityState,
    /// The `point` value.
    pub point: Vec3,
    /// The `distance` value.
    pub distance: f64,
}

#[derive(Clone, Debug, PartialEq)]
/// Possible values represented by `MetadataValue`.
pub enum MetadataValue {
    /// Documentation for this public variant.
    Byte(i8),
    /// Documentation for this public variant.
    VarInt(i32),
    /// Documentation for this public variant.
    Float(f32),
    /// Documentation for this public variant.
    String(String),
    /// Documentation for this public variant.
    OptionalString(Option<String>),
    /// Documentation for this public variant.
    Slot(Option<ItemStack>),
    /// Documentation for this public variant.
    Bool(bool),
    /// The `Rotation` variant.
    Rotation {
        /// The `x` value carried by this variant.
        x: f32,
        /// The `y` value carried by this variant.
        y: f32,
        /// The `z` value carried by this variant.
        z: f32,
    },
    /// Documentation for this public variant.
    Position(BlockPos),
    /// Documentation for this public variant.
    OptionalPosition(Option<BlockPos>),
    /// Documentation for this public variant.
    OptionalUuid(Option<[u8; 16]>),
    /// Documentation for this public variant.
    Nbt(Vec<u8>),
    /// The `Particle` variant.
    Particle {
        /// The `id` value carried by this variant.
        id: i32,
        /// The `data` value carried by this variant.
        data: ParticleData,
    },
    /// The `Villager` variant.
    Villager {
        /// The `kind` value carried by this variant.
        kind: i32,
        /// The `profession` value carried by this variant.
        profession: i32,
        /// The `level` value carried by this variant.
        level: i32,
    },
    /// Documentation for this public variant.
    OptionalVarInt(Option<i32>),
}

#[derive(Clone, Debug, PartialEq)]
/// Possible values represented by `ParticleData`.
pub enum ParticleData {
    /// The `None` variant.
    None,
    /// Documentation for this public variant.
    BlockState(i32),
    /// The `Dust` variant.
    Dust {
        /// The `red` value carried by this variant.
        red: f32,
        /// The `green` value carried by this variant.
        green: f32,
        /// The `blue` value carried by this variant.
        blue: f32,
        /// The `scale` value carried by this variant.
        scale: f32,
    },
    /// Documentation for this public variant.
    Item(Option<ItemStack>),
}

#[derive(Clone, Debug, Default, PartialEq)]
/// State and protocol data represented by `EntityTracker`.
pub struct EntityTracker {
    /// The `entities` value.
    pub entities: HashMap<i32, EntityState>,
}

impl EntityTracker {
    /// Performs the `observe` operation.
    pub fn observe(&self, origin: Vec3, radius: f64) -> Vec<EntityState> {
        let radius_squared = radius.max(0.0).powi(2);
        self.entities
            .values()
            .filter(|entity| {
                let dx = entity.position.x - origin.x;
                let dy = entity.position.y - origin.y;
                let dz = entity.position.z - origin.z;
                dx * dx + dy * dy + dz * dz <= radius_squared
            })
            .cloned()
            .collect()
    }

    /// Performs the `raycast` operation.
    pub fn raycast(
        &self,
        origin: Vec3,
        direction: Vec3,
        max_distance: f64,
        excluded_entity: Option<i32>,
    ) -> Option<EntityRaycastHit> {
        let length =
            (direction.x * direction.x + direction.y * direction.y + direction.z * direction.z)
                .sqrt();
        if !length.is_finite()
            || length <= 1.0e-12
            || !max_distance.is_finite()
            || !(0.0..=1024.0).contains(&max_distance)
        {
            return None;
        }
        let direction = Vec3 {
            x: direction.x / length,
            y: direction.y / length,
            z: direction.z / length,
        };
        self.entities
            .values()
            .filter(|entity| Some(entity.entity_id) != excluded_entity)
            .filter_map(|entity| {
                let (width, height) = entity
                    .type_id
                    .and_then(crate::versions::java_1_16_1::registry::entity_dimensions)
                    .unwrap_or((0.6, 1.8));
                let half = width / 2.0;
                let aabb = crate::versions::java_1_16_1::Aabb {
                    min_x: entity.position.x - half,
                    min_y: entity.position.y,
                    min_z: entity.position.z - half,
                    max_x: entity.position.x + half,
                    max_y: entity.position.y + height,
                    max_z: entity.position.z + half,
                };
                ray_aabb_distance(origin, direction, aabb)
                    .filter(|distance| *distance <= max_distance)
                    .map(|distance| EntityRaycastHit {
                        entity: entity.clone(),
                        point: Vec3 {
                            x: origin.x + direction.x * distance,
                            y: origin.y + direction.y * distance,
                            z: origin.z + direction.z * distance,
                        },
                        distance,
                    })
            })
            .min_by(|a, b| a.distance.total_cmp(&b.distance))
    }
}

fn ray_aabb_distance(
    origin: Vec3,
    direction: Vec3,
    aabb: crate::versions::java_1_16_1::Aabb,
) -> Option<f64> {
    let mut near = f64::NEG_INFINITY;
    let mut far = f64::INFINITY;
    for (start, delta, min, max) in [
        (origin.x, direction.x, aabb.min_x, aabb.max_x),
        (origin.y, direction.y, aabb.min_y, aabb.max_y),
        (origin.z, direction.z, aabb.min_z, aabb.max_z),
    ] {
        if delta.abs() <= 1.0e-12 {
            if start < min || start > max {
                return None;
            }
            continue;
        }
        let first = (min - start) / delta;
        let last = (max - start) / delta;
        near = near.max(first.min(last));
        far = far.min(first.max(last));
        if near > far {
            return None;
        }
    }
    if far < 0.0 { None } else { Some(near.max(0.0)) }
}

fn angle(value: i8) -> f32 {
    f32::from(value) * 360.0 / 256.0
}

fn velocity(value: i16) -> f64 {
    f64::from(value) / 8000.0
}

fn read_uuid(rest: &mut &[u8]) -> Result<[u8; 16]> {
    if rest.len() < 16 {
        bail!("truncated UUID");
    }
    let mut uuid = [0; 16];
    uuid.copy_from_slice(&rest[..16]);
    *rest = &rest[16..];
    Ok(uuid)
}

fn base_entity(
    entity_id: i32,
    uuid: Option<[u8; 16]>,
    kind: EntityKind,
    type_id: Option<i32>,
    position: Vec3,
) -> EntityState {
    EntityState {
        entity_id,
        uuid,
        kind,
        type_id,
        type_name: type_id.and_then(entity_name),
        position,
        velocity: Vec3::default(),
        yaw: 0.0,
        pitch: 0.0,
        head_yaw: 0.0,
        on_ground: false,
        object_data: None,
        experience_count: None,
        painting_motive: None,
        painting_direction: None,
        metadata: HashMap::new(),
        equipment: HashMap::new(),
        passengers: Vec::new(),
        attached_to: None,
    }
}

pub(crate) fn parse_spawn_object(payload: &[u8]) -> Result<EntityState> {
    let mut rest = payload;
    let entity_id = get_varint(&mut rest)?;
    let uuid = read_uuid(&mut rest)?;
    let type_id = get_varint(&mut rest)?;
    let mut c = Cursor::new(rest);
    let position = Vec3 {
        x: c.read_f64::<BigEndian>()?,
        y: c.read_f64::<BigEndian>()?,
        z: c.read_f64::<BigEndian>()?,
    };
    let pitch = angle(c.read_i8()?);
    let yaw = angle(c.read_i8()?);
    let object_data = c.read_i32::<BigEndian>()?;
    let motion = Vec3 {
        x: velocity(c.read_i16::<BigEndian>()?),
        y: velocity(c.read_i16::<BigEndian>()?),
        z: velocity(c.read_i16::<BigEndian>()?),
    };
    let mut entity = base_entity(
        entity_id,
        Some(uuid),
        EntityKind::Object,
        Some(type_id),
        position,
    );
    entity.pitch = pitch;
    entity.yaw = yaw;
    entity.object_data = Some(object_data);
    entity.velocity = motion;
    Ok(entity)
}

pub(crate) fn parse_spawn_living(payload: &[u8]) -> Result<EntityState> {
    let mut rest = payload;
    let entity_id = get_varint(&mut rest)?;
    let uuid = read_uuid(&mut rest)?;
    let type_id = get_varint(&mut rest)?;
    let mut c = Cursor::new(rest);
    let position = Vec3 {
        x: c.read_f64::<BigEndian>()?,
        y: c.read_f64::<BigEndian>()?,
        z: c.read_f64::<BigEndian>()?,
    };
    let yaw = angle(c.read_i8()?);
    let pitch = angle(c.read_i8()?);
    let head_yaw = angle(c.read_i8()?);
    let motion = Vec3 {
        x: velocity(c.read_i16::<BigEndian>()?),
        y: velocity(c.read_i16::<BigEndian>()?),
        z: velocity(c.read_i16::<BigEndian>()?),
    };
    let mut entity = base_entity(
        entity_id,
        Some(uuid),
        EntityKind::Living,
        Some(type_id),
        position,
    );
    entity.yaw = yaw;
    entity.pitch = pitch;
    entity.head_yaw = head_yaw;
    entity.velocity = motion;
    Ok(entity)
}

pub(crate) fn parse_spawn_player(payload: &[u8]) -> Result<EntityState> {
    let mut rest = payload;
    let entity_id = get_varint(&mut rest)?;
    let uuid = read_uuid(&mut rest)?;
    let mut c = Cursor::new(rest);
    let position = Vec3 {
        x: c.read_f64::<BigEndian>()?,
        y: c.read_f64::<BigEndian>()?,
        z: c.read_f64::<BigEndian>()?,
    };
    let mut entity = base_entity(entity_id, Some(uuid), EntityKind::Player, None, position);
    entity.type_name = Some("player");
    entity.yaw = angle(c.read_i8()?);
    entity.pitch = angle(c.read_i8()?);
    Ok(entity)
}

pub(crate) fn parse_spawn_orb(payload: &[u8]) -> Result<EntityState> {
    let mut rest = payload;
    let entity_id = get_varint(&mut rest)?;
    let mut c = Cursor::new(rest);
    let position = Vec3 {
        x: c.read_f64::<BigEndian>()?,
        y: c.read_f64::<BigEndian>()?,
        z: c.read_f64::<BigEndian>()?,
    };
    let count = c.read_i16::<BigEndian>()?;
    let mut entity = base_entity(entity_id, None, EntityKind::ExperienceOrb, None, position);
    entity.type_name = Some("experience_orb");
    entity.experience_count = Some(count);
    Ok(entity)
}

pub(crate) fn parse_spawn_painting(payload: &[u8]) -> Result<EntityState> {
    let mut rest = payload;
    let entity_id = get_varint(&mut rest)?;
    let uuid = read_uuid(&mut rest)?;
    let motive = get_varint(&mut rest)?;
    let position = BlockPos::unpack(take_u64(&mut rest)?);
    let direction = *rest.first().context("missing painting direction")?;
    if direction > 3 {
        bail!("invalid painting direction {direction}");
    }
    let mut entity = base_entity(
        entity_id,
        Some(uuid),
        EntityKind::Painting,
        None,
        Vec3 {
            x: f64::from(position.x),
            y: f64::from(position.y),
            z: f64::from(position.z),
        },
    );
    entity.type_name = Some("painting");
    entity.painting_motive = Some(motive);
    entity.painting_direction = Some(direction);
    Ok(entity)
}

pub(crate) fn apply_relative(entity: &mut EntityState, payload: &[u8], look: bool) -> Result<()> {
    let mut rest = payload;
    let id = get_varint(&mut rest)?;
    if id != entity.entity_id {
        bail!("relative move entity ID mismatch");
    }
    let mut c = Cursor::new(rest);
    entity.position.x += f64::from(c.read_i16::<BigEndian>()?) / 4096.0;
    entity.position.y += f64::from(c.read_i16::<BigEndian>()?) / 4096.0;
    entity.position.z += f64::from(c.read_i16::<BigEndian>()?) / 4096.0;
    if look {
        entity.yaw = angle(c.read_i8()?);
        entity.pitch = angle(c.read_i8()?);
    }
    entity.on_ground = c.read_u8()? != 0;
    Ok(())
}

pub(crate) fn parse_metadata(payload: &[u8]) -> Result<(i32, HashMap<u8, MetadataValue>)> {
    let mut rest = payload;
    let entity_id = get_varint(&mut rest)?;
    let mut values = HashMap::new();
    loop {
        let key = *rest.first().context("truncated metadata")?;
        rest = &rest[1..];
        if key == 0xff {
            break;
        }
        let kind = get_varint(&mut rest)?;
        let value = read_metadata_value(kind, &mut rest)?;
        values.insert(key, value);
    }
    Ok((entity_id, values))
}

fn read_metadata_value(kind: i32, rest: &mut &[u8]) -> Result<MetadataValue> {
    Ok(match kind {
        0 => MetadataValue::Byte(take_i8(rest)?),
        1 | 11 | 13 | 18 => MetadataValue::VarInt(get_varint(rest)?),
        2 => MetadataValue::Float(take_f32(rest)?),
        3 | 4 => MetadataValue::String(get_string(rest)?),
        5 => MetadataValue::OptionalString(if take_bool(rest)? {
            Some(get_string(rest)?)
        } else {
            None
        }),
        6 => MetadataValue::Slot(read_slot(rest)?),
        7 => MetadataValue::Bool(take_bool(rest)?),
        8 => MetadataValue::Rotation {
            x: take_f32(rest)?,
            y: take_f32(rest)?,
            z: take_f32(rest)?,
        },
        9 => MetadataValue::Position(BlockPos::unpack(take_u64(rest)?)),
        10 => MetadataValue::OptionalPosition(if take_bool(rest)? {
            Some(BlockPos::unpack(take_u64(rest)?))
        } else {
            None
        }),
        12 => MetadataValue::OptionalUuid(if take_bool(rest)? {
            Some(read_uuid(rest)?)
        } else {
            None
        }),
        14 => {
            let original = *rest;
            let mut cursor = Cursor::new(original);
            skip_nbt(&mut cursor)?;
            let length = cursor.position() as usize;
            *rest = &original[length..];
            MetadataValue::Nbt(original[..length].to_vec())
        }
        15 => {
            let id = get_varint(rest)?;
            let data = match id {
                3 | 23 => ParticleData::BlockState(get_varint(rest)?),
                14 => ParticleData::Dust {
                    red: take_f32(rest)?,
                    green: take_f32(rest)?,
                    blue: take_f32(rest)?,
                    scale: take_f32(rest)?,
                },
                34 => ParticleData::Item(read_slot(rest)?),
                _ => ParticleData::None,
            };
            MetadataValue::Particle { id, data }
        }
        16 => MetadataValue::Villager {
            kind: get_varint(rest)?,
            profession: get_varint(rest)?,
            level: get_varint(rest)?,
        },
        17 => {
            let value = get_varint(rest)?;
            MetadataValue::OptionalVarInt((value != 0).then_some(value - 1))
        }
        _ => bail!("unknown metadata type {kind}"),
    })
}

fn take_i8(rest: &mut &[u8]) -> Result<i8> {
    let value = *rest.first().context("truncated i8")? as i8;
    *rest = &rest[1..];
    Ok(value)
}
fn take_bool(rest: &mut &[u8]) -> Result<bool> {
    Ok(take_i8(rest)? != 0)
}
fn take_f32(rest: &mut &[u8]) -> Result<f32> {
    let mut c = Cursor::new(*rest);
    let value = c.read_f32::<BigEndian>()?;
    *rest = &rest[c.position() as usize..];
    Ok(value)
}
fn take_u64(rest: &mut &[u8]) -> Result<u64> {
    let mut c = Cursor::new(*rest);
    let value = c.read_u64::<BigEndian>()?;
    *rest = &rest[c.position() as usize..];
    Ok(value)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::versions::java_1_16_1::protocol::put_varint;

    #[test]
    fn metadata_keeps_raw_typed_values() {
        let mut payload = Vec::new();
        put_varint(&mut payload, 7);
        payload.push(0);
        put_varint(&mut payload, 0);
        payload.push(5);
        payload.push(7);
        put_varint(&mut payload, 7);
        payload.push(1);
        payload.push(0xff);
        let (id, values) = parse_metadata(&payload).unwrap();
        assert_eq!(id, 7);
        assert_eq!(values.get(&0), Some(&MetadataValue::Byte(5)));
        assert_eq!(values.get(&7), Some(&MetadataValue::Bool(true)));
    }

    #[test]
    fn entity_raycast_uses_registry_dimensions_and_nearest_hit() {
        let mut tracker = EntityTracker::default();
        tracker.entities.insert(
            1,
            base_entity(
                1,
                None,
                EntityKind::Living,
                Some(101),
                Vec3 {
                    x: 0.0,
                    y: 0.0,
                    z: 4.0,
                },
            ),
        );
        tracker.entities.insert(
            2,
            base_entity(
                2,
                None,
                EntityKind::Living,
                Some(101),
                Vec3 {
                    x: 0.0,
                    y: 0.0,
                    z: 7.0,
                },
            ),
        );
        let hit = tracker
            .raycast(
                Vec3 {
                    x: 0.0,
                    y: 1.0,
                    z: 0.0,
                },
                Vec3 {
                    x: 0.0,
                    y: 0.0,
                    z: 1.0,
                },
                8.0,
                None,
            )
            .unwrap();
        assert_eq!(hit.entity.entity_id, 1);
        assert!(hit.distance < 4.0);
        assert!(
            tracker
                .raycast(Vec3::default(), Vec3::default(), 8.0, None)
                .is_none()
        );
    }
}
