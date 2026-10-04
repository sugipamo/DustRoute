//! Health, hunger, effects, attributes, game mode, and respawn state.

use crate::versions::java_1_16_1::protocol::{get_string, get_varint};
use crate::versions::java_1_16_1::world::skip_nbt;
use anyhow::{Result, bail};
use byteorder::{BigEndian, ReadBytesExt};
use std::{collections::HashMap, io::Cursor};

#[derive(Clone, Copy, Debug, Default, PartialEq)]
/// State and protocol data represented by `Vitals`.
pub struct Vitals {
    /// The `health` value.
    pub health: f32,
    /// The `food` value.
    pub food: i32,
    /// The `saturation` value.
    pub saturation: f32,
}

#[derive(Clone, Copy, Debug, Default, PartialEq)]
/// State and protocol data represented by `Experience`.
pub struct Experience {
    /// The `progress` value.
    pub progress: f32,
    /// The `level` value.
    pub level: i32,
    /// The `total` value.
    pub total: i32,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
/// State and protocol data represented by `Difficulty`.
pub struct Difficulty {
    /// The `id` value.
    pub id: u8,
    /// The `locked` value.
    pub locked: bool,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
/// State and protocol data represented by `SpawnPosition`.
pub struct SpawnPosition {
    /// The `x` value.
    pub x: i32,
    /// The `y` value.
    pub y: i32,
    /// The `z` value.
    pub z: i32,
}

#[derive(Clone, Debug, PartialEq)]
/// State and protocol data represented by `StatusEffect`.
pub struct StatusEffect {
    /// The `id` value.
    pub id: i8,
    /// The `amplifier` value.
    pub amplifier: i8,
    /// The `duration_ticks` value.
    pub duration_ticks: i32,
    /// The `flags` value.
    pub flags: i8,
}

#[derive(Clone, Debug, PartialEq)]
/// State and protocol data represented by `AttributeModifier`.
pub struct AttributeModifier {
    /// The `uuid` value.
    pub uuid: [u8; 16],
    /// The `amount` value.
    pub amount: f64,
    /// The `operation` value.
    pub operation: i8,
}

#[derive(Clone, Debug, PartialEq)]
/// State and protocol data represented by `Attribute`.
pub struct Attribute {
    /// The `key` value.
    pub key: String,
    /// The `base` value.
    pub base: f64,
    /// The `modifiers` value.
    pub modifiers: Vec<AttributeModifier>,
}

impl Attribute {
    /// Performs the `value` operation.
    pub fn value(&self) -> f64 {
        let additions = self
            .modifiers
            .iter()
            .filter(|m| m.operation == 0)
            .map(|m| m.amount)
            .sum::<f64>();
        let base = self.base + additions;
        let multiply_base = self
            .modifiers
            .iter()
            .filter(|m| m.operation == 1)
            .map(|m| m.amount)
            .sum::<f64>();
        self.modifiers
            .iter()
            .filter(|m| m.operation == 2)
            .fold(base + self.base * multiply_base, |value, m| {
                value * (1.0 + m.amount)
            })
    }
}

#[derive(Clone, Debug, Default, PartialEq)]
/// State and protocol data represented by `SurvivalState`.
pub struct SurvivalState {
    /// The `vitals` value.
    pub vitals: Option<Vitals>,
    /// The `experience` value.
    pub experience: Experience,
    /// The `difficulty` value.
    pub difficulty: Option<Difficulty>,
    /// The `game_mode` value.
    pub game_mode: Option<u8>,
    /// The `previous_game_mode` value.
    pub previous_game_mode: Option<u8>,
    /// The `world_age` value.
    pub world_age: i64,
    /// The `time_of_day` value.
    pub time_of_day: i64,
    /// The `raining` value.
    pub raining: Option<bool>,
    /// The `rain_level` value.
    pub rain_level: Option<f32>,
    /// The `thunder_level` value.
    pub thunder_level: Option<f32>,
    /// The `spawn_position` value.
    pub spawn_position: Option<SpawnPosition>,
    /// The `dimension` value.
    pub dimension: Option<String>,
    /// The `world_name` value.
    pub world_name: Option<String>,
    /// The `effects` value.
    pub effects: HashMap<i8, StatusEffect>,
    /// The `attributes` value.
    pub attributes: HashMap<String, Attribute>,
    /// The `flying_allowed` value.
    pub flying_allowed: bool,
    /// The `flying` value.
    pub flying: bool,
    /// The `invulnerable` value.
    pub invulnerable: bool,
    /// The `creative_mode` value.
    pub creative_mode: bool,
    /// The `flying_speed` value.
    pub flying_speed: f32,
    /// The `walking_speed` value.
    pub walking_speed: f32,
    /// The `item_cooldowns` value.
    pub item_cooldowns: HashMap<i32, i32>,
}

#[derive(Clone, Debug, PartialEq)]
/// State and protocol data represented by `GameStateChange`.
pub struct GameStateChange {
    /// The `reason` value.
    pub reason: u8,
    /// The `value` value.
    pub value: f32,
}

#[derive(Clone, Debug, PartialEq)]
/// State and protocol data represented by `RespawnState`.
pub struct RespawnState {
    /// The `dimension` value.
    pub dimension: String,
    /// The `world_name` value.
    pub world_name: String,
    /// The `hashed_seed` value.
    pub hashed_seed: i64,
    /// The `game_mode` value.
    pub game_mode: u8,
    /// The `previous_game_mode` value.
    pub previous_game_mode: u8,
    /// The `debug` value.
    pub debug: bool,
    /// The `flat` value.
    pub flat: bool,
    /// The `copy_metadata` value.
    pub copy_metadata: bool,
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct JoinState {
    pub entity_id: i32,
    pub game_mode: u8,
    pub previous_game_mode: u8,
    pub dimension: String,
    pub world_name: String,
}

#[derive(Clone, Debug, PartialEq)]
/// Possible values represented by `CombatEvent`.
pub enum CombatEvent {
    /// The `Enter` variant.
    Enter,
    /// The `End` variant.
    End {
        /// The `duration_ticks` value carried by this variant.
        duration_ticks: i32,
        /// The `entity_id` value carried by this variant.
        entity_id: i32,
    },
    /// The `Death` variant.
    Death {
        /// The `player_id` value carried by this variant.
        player_id: i32,
        /// The `entity_id` value carried by this variant.
        entity_id: i32,
        /// The `message` value carried by this variant.
        message: crate::text_component::ProtocolText,
    },
    /// The `Unknown` variant.
    Unknown {
        /// The `id` value carried by this variant.
        id: i32,
    },
}

pub(crate) fn parse_vitals(payload: &[u8]) -> Result<Vitals> {
    let mut cursor = Cursor::new(payload);
    let health = cursor.read_f32::<BigEndian>()?;
    let mut rest = &payload[cursor.position() as usize..];
    let food = get_varint(&mut rest)?;
    let mut cursor = Cursor::new(rest);
    let saturation = cursor.read_f32::<BigEndian>()?;
    Ok(Vitals {
        health,
        food,
        saturation,
    })
}

pub(crate) fn parse_experience(payload: &[u8]) -> Result<Experience> {
    let mut cursor = Cursor::new(payload);
    let progress = cursor.read_f32::<BigEndian>()?;
    let mut rest = &payload[cursor.position() as usize..];
    Ok(Experience {
        progress,
        level: get_varint(&mut rest)?,
        total: get_varint(&mut rest)?,
    })
}

pub(crate) fn parse_effect(payload: &[u8]) -> Result<(i32, StatusEffect)> {
    let mut rest = payload;
    let entity_id = get_varint(&mut rest)?;
    if rest.len() < 2 {
        bail!("truncated entity effect");
    }
    let id = rest[0] as i8;
    let amplifier = rest[1] as i8;
    rest = &rest[2..];
    let duration_ticks = get_varint(&mut rest)?;
    let flags = *rest
        .first()
        .ok_or_else(|| anyhow::anyhow!("missing effect flags"))? as i8;
    Ok((
        entity_id,
        StatusEffect {
            id,
            amplifier,
            duration_ticks,
            flags,
        },
    ))
}

pub(crate) fn parse_attributes(payload: &[u8]) -> Result<(i32, Vec<Attribute>)> {
    let mut rest = payload;
    let entity_id = get_varint(&mut rest)?;
    let mut cursor = Cursor::new(rest);
    let count = cursor.read_i32::<BigEndian>()?;
    if !(0..=1024).contains(&count) {
        bail!("invalid attribute count {count}");
    }
    rest = &rest[cursor.position() as usize..];
    let mut attributes = Vec::with_capacity(count as usize);
    for _ in 0..count {
        let key = get_string(&mut rest)?;
        let mut cursor = Cursor::new(rest);
        let base = cursor.read_f64::<BigEndian>()?;
        rest = &rest[cursor.position() as usize..];
        let modifier_count = get_varint(&mut rest)?;
        if !(0..=1024).contains(&modifier_count) {
            bail!("invalid modifier count {modifier_count}");
        }
        let mut modifiers = Vec::with_capacity(modifier_count as usize);
        for _ in 0..modifier_count {
            if rest.len() < 16 {
                bail!("truncated attribute UUID");
            }
            let mut uuid = [0; 16];
            uuid.copy_from_slice(&rest[..16]);
            rest = &rest[16..];
            let mut cursor = Cursor::new(rest);
            let amount = cursor.read_f64::<BigEndian>()?;
            let operation = cursor.read_i8()?;
            rest = &rest[cursor.position() as usize..];
            modifiers.push(AttributeModifier {
                uuid,
                amount,
                operation,
            });
        }
        attributes.push(Attribute {
            key,
            base,
            modifiers,
        });
    }
    Ok((entity_id, attributes))
}

pub(crate) fn parse_respawn(payload: &[u8]) -> Result<RespawnState> {
    let mut rest = payload;
    let dimension = get_string(&mut rest)?;
    let world_name = get_string(&mut rest)?;
    let mut cursor = Cursor::new(rest);
    let hashed_seed = cursor.read_i64::<BigEndian>()?;
    let game_mode = cursor.read_u8()?;
    let previous_game_mode = cursor.read_u8()?;
    let debug = cursor.read_u8()? != 0;
    let flat = cursor.read_u8()? != 0;
    let copy_metadata = cursor.read_u8()? != 0;
    Ok(RespawnState {
        dimension,
        world_name,
        hashed_seed,
        game_mode,
        previous_game_mode,
        debug,
        flat,
        copy_metadata,
    })
}

pub(crate) fn parse_join(payload: &[u8]) -> Result<JoinState> {
    let mut cursor = Cursor::new(payload);
    let entity_id = cursor.read_i32::<BigEndian>()?;
    let game_mode = cursor.read_u8()?;
    let previous_game_mode = cursor.read_u8()?;
    let mut rest = &payload[cursor.position() as usize..];
    let world_count = get_varint(&mut rest)?;
    if !(0..=1024).contains(&world_count) {
        bail!("invalid world count {world_count}");
    }
    for _ in 0..world_count {
        let _ = get_string(&mut rest)?;
    }
    let mut cursor = Cursor::new(rest);
    skip_nbt(&mut cursor)?;
    rest = &rest[cursor.position() as usize..];
    let dimension = get_string(&mut rest)?;
    let world_name = get_string(&mut rest)?;
    Ok(JoinState {
        entity_id,
        game_mode,
        previous_game_mode,
        dimension,
        world_name,
    })
}

pub(crate) fn parse_combat_event(payload: &[u8]) -> Result<CombatEvent> {
    let mut rest = payload;
    let event = get_varint(&mut rest)?;
    Ok(match event {
        0 => CombatEvent::Enter,
        1 => {
            let duration_ticks = get_varint(&mut rest)?;
            let mut cursor = Cursor::new(rest);
            CombatEvent::End {
                duration_ticks,
                entity_id: cursor.read_i32::<BigEndian>()?,
            }
        }
        2 => {
            let player_id = get_varint(&mut rest)?;
            let mut cursor = Cursor::new(rest);
            let entity_id = cursor.read_i32::<BigEndian>()?;
            rest = &rest[cursor.position() as usize..];
            CombatEvent::Death {
                player_id,
                entity_id,
                message: crate::text_component::ProtocolText::received(get_string(&mut rest)?),
            }
        }
        id => CombatEvent::Unknown { id },
    })
}

pub(crate) fn unpack_position(value: u64) -> SpawnPosition {
    let x = ((value as i64) >> 38) as i32;
    let y = ((value & 0xfff) as i32) << 20 >> 20;
    let z = (((value >> 12) & 0x3ff_ffff) as i32) << 6 >> 6;
    SpawnPosition { x, y, z }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::versions::java_1_16_1::protocol::{put_string, put_varint};
    use byteorder::WriteBytesExt;

    #[test]
    fn parses_vitals_without_losing_protocol_values() {
        let mut payload = Vec::new();
        payload.write_f32::<BigEndian>(17.5).unwrap();
        put_varint(&mut payload, 13);
        payload.write_f32::<BigEndian>(3.25).unwrap();
        let value = parse_vitals(&payload).unwrap();
        assert_eq!(value.health, 17.5);
        assert_eq!(value.food, 13);
        assert_eq!(value.saturation, 3.25);
    }

    #[test]
    fn attribute_operations_match_vanilla_order() {
        let attribute = Attribute {
            key: "generic.movement_speed".into(),
            base: 0.1,
            modifiers: vec![
                AttributeModifier {
                    uuid: [0; 16],
                    amount: 0.05,
                    operation: 0,
                },
                AttributeModifier {
                    uuid: [1; 16],
                    amount: 0.5,
                    operation: 1,
                },
                AttributeModifier {
                    uuid: [2; 16],
                    amount: 0.2,
                    operation: 2,
                },
            ],
        };
        assert!((attribute.value() - 0.24).abs() < 1.0e-12);
    }

    #[test]
    fn parses_death_combat_event_as_raw_json() {
        let mut payload = Vec::new();
        put_varint(&mut payload, 2);
        put_varint(&mut payload, 7);
        payload.write_i32::<BigEndian>(42).unwrap();
        put_string(&mut payload, r#"{\"text\":\"fell\"}"#);
        assert_eq!(
            parse_combat_event(&payload).unwrap(),
            CombatEvent::Death {
                player_id: 7,
                entity_id: 42,
                message: crate::text_component::ProtocolText::received(
                    r#"{\"text\":\"fell\"}"#.into()
                ),
            }
        );
    }
}
