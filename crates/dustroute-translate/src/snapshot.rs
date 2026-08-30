use std::collections::BTreeMap;
use std::error::Error;
use std::fmt::{Display, Formatter};

use serde::{Deserialize, Serialize};

use crate::wire::update_wire_shapes;
use crate::world::{Block, BlockKind, Facing, Pos, WireConnection, World};

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct MinecraftSnapshot {
    pub min: Pos,
    pub max: Pos,
    pub blocks: Vec<MinecraftSnapshotBlock>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct MinecraftSnapshotBlock {
    pub pos: Pos,
    pub name: String,
    #[serde(default)]
    pub properties: BTreeMap<String, String>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum SnapshotError {
    Json(String),
    InvalidFacing { pos: Pos, value: String },
}

impl Display for SnapshotError {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Json(message) => write!(f, "invalid Minecraft snapshot JSON: {message}"),
            Self::InvalidFacing { pos, value } => {
                write!(f, "invalid facing {value:?} at {pos:?}")
            }
        }
    }
}

impl Error for SnapshotError {}

pub fn world_from_snapshot_json(json: &str) -> Result<(MinecraftSnapshot, World), SnapshotError> {
    let snapshot: MinecraftSnapshot =
        serde_json::from_str(json).map_err(|error| SnapshotError::Json(error.to_string()))?;
    let mut world = World::new();
    for record in &snapshot.blocks {
        world.set(record.pos, block_from_record(record)?);
    }
    update_wire_shapes(&mut world);
    Ok((snapshot, world))
}

fn block_from_record(record: &MinecraftSnapshotBlock) -> Result<Block, SnapshotError> {
    let short_name = record
        .name
        .strip_prefix("minecraft:")
        .unwrap_or(&record.name);
    let kind = match short_name {
        "air" | "cave_air" | "void_air" => BlockKind::Air,
        "redstone_wire" => BlockKind::RedstoneWire,
        "redstone_torch" | "redstone_wall_torch" => BlockKind::RedstoneTorch,
        "repeater" => BlockKind::Repeater,
        "comparator" => BlockKind::Comparator,
        "lever" => BlockKind::Lever,
        "redstone_block" => BlockKind::RedstoneBlock,
        "piston" | "sticky_piston" => BlockKind::Piston,
        "glass" | "tinted_glass" => BlockKind::Transparent,
        _ => BlockKind::Solid,
    };
    let mut block = Block::new(kind);
    match kind {
        BlockKind::RedstoneWire => {
            block.power_level = record
                .properties
                .get("power")
                .and_then(|value| value.parse().ok());
            block.support_offset = Some(Pos::new(0, -1, 0));
            block.wire_connections = Some(
                [Facing::North, Facing::East, Facing::South, Facing::West]
                    .into_iter()
                    .map(|facing| {
                        let value = record.properties.get(facing_name(facing)).map_or(
                            WireConnection::None,
                            |value| match value.as_str() {
                                "up" => WireConnection::Up,
                                "side" => WireConnection::Side,
                                _ => WireConnection::None,
                            },
                        );
                        (facing, value)
                    })
                    .collect(),
            );
        }
        BlockKind::Repeater | BlockKind::Comparator => {
            block.support_offset = Some(Pos::new(0, -1, 0));
            block.facing = Some(property_facing(record)?.opposite());
            block.powered = property_bool(record, "powered");
            block.delay = record
                .properties
                .get("delay")
                .and_then(|value| value.parse().ok());
        }
        BlockKind::RedstoneTorch => {
            block.powered = property_bool(record, "lit");
            block.support_offset = if short_name == "redstone_wall_torch" {
                let outward = property_facing(record)?;
                let delta = outward.opposite().horizontal_offset().expect("horizontal");
                Some(delta)
            } else {
                Some(Pos::new(0, -1, 0))
            };
        }
        BlockKind::Lever => {
            let facing = property_facing(record)?;
            block.powered = property_bool(record, "powered");
            block.facing = Some(facing);
            block.support_offset = match record.properties.get("face").map(String::as_str) {
                Some("ceiling") => Some(Pos::new(0, 1, 0)),
                Some("wall") => {
                    let delta = facing.opposite().horizontal_offset().expect("horizontal");
                    Some(delta)
                }
                _ => Some(Pos::new(0, -1, 0)),
            };
        }
        BlockKind::Piston => block.facing = Some(property_facing(record)?),
        _ => {}
    }
    Ok(block)
}

fn property_bool(record: &MinecraftSnapshotBlock, name: &str) -> Option<bool> {
    record
        .properties
        .get(name)
        .and_then(|value| value.parse().ok())
}

fn property_facing(record: &MinecraftSnapshotBlock) -> Result<Facing, SnapshotError> {
    let value = record
        .properties
        .get("facing")
        .map(String::as_str)
        .unwrap_or("north");
    match value {
        "north" => Ok(Facing::North),
        "east" => Ok(Facing::East),
        "south" => Ok(Facing::South),
        "west" => Ok(Facing::West),
        "up" => Ok(Facing::Up),
        "down" => Ok(Facing::Down),
        _ => Err(SnapshotError::InvalidFacing {
            pos: record.pos,
            value: value.into(),
        }),
    }
}

const fn facing_name(facing: Facing) -> &'static str {
    match facing {
        Facing::North => "north",
        Facing::East => "east",
        Facing::South => "south",
        Facing::West => "west",
        Facing::Up => "up",
        Facing::Down => "down",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn imports_java_repeater_direction_into_internal_direction() {
        let json = r#"{
            "min":{"x":0,"y":0,"z":0},
            "max":{"x":2,"y":1,"z":0},
            "blocks":[
              {"pos":{"x":0,"y":0,"z":0},"name":"minecraft:stone"},
              {"pos":{"x":1,"y":0,"z":0},"name":"minecraft:stone"},
              {"pos":{"x":2,"y":0,"z":0},"name":"minecraft:stone"},
              {"pos":{"x":0,"y":1,"z":0},"name":"minecraft:redstone_wire","properties":{"east":"side"}},
              {"pos":{"x":1,"y":1,"z":0},"name":"minecraft:repeater","properties":{"facing":"west","delay":"1","powered":"false"}},
              {"pos":{"x":2,"y":1,"z":0},"name":"minecraft:redstone_wire","properties":{"west":"side"}}
            ]
        }"#;
        let (_, world) = world_from_snapshot_json(json).unwrap();
        assert_eq!(
            world.get(Pos::new(1, 1, 0)).unwrap().facing,
            Some(Facing::East)
        );
    }
}
