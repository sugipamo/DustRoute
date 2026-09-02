use std::collections::BTreeMap;
use std::error::Error;
use std::fmt::{Display, Formatter};

use serde::{Deserialize, Serialize};

use crate::wire::update_wire_shapes;
use crate::world::{
    Block, BlockKind, Facing, ObservationClassification, Pos, WireConnection, World,
};

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
    let world = world_from_snapshot(&snapshot)?;
    Ok((snapshot, world))
}

pub fn world_from_snapshot(snapshot: &MinecraftSnapshot) -> Result<World, SnapshotError> {
    let mut world = World::new();
    for record in &snapshot.blocks {
        world.set(record.pos, block_from_record(record)?);
    }
    update_wire_shapes(&mut world);
    Ok(world)
}

fn block_from_record(record: &MinecraftSnapshotBlock) -> Result<Block, SnapshotError> {
    let short_name = record
        .name
        .strip_prefix("minecraft:")
        .unwrap_or(&record.name);
    let (kind, observation_classification) = match short_name {
        "air" | "cave_air" | "void_air" => (BlockKind::Air, ObservationClassification::Exact),
        "redstone_wire" => (BlockKind::RedstoneWire, ObservationClassification::Exact),
        "redstone_torch" | "redstone_wall_torch" => {
            (BlockKind::RedstoneTorch, ObservationClassification::Exact)
        }
        "repeater" => (BlockKind::Repeater, ObservationClassification::Exact),
        "comparator" => (BlockKind::Comparator, ObservationClassification::Exact),
        "lever" => (BlockKind::Lever, ObservationClassification::Exact),
        name if name.ends_with("_button") => (BlockKind::Button, ObservationClassification::Exact),
        name if name.ends_with("_pressure_plate") => {
            (BlockKind::PressurePlate, ObservationClassification::Exact)
        }
        "redstone_lamp" => (BlockKind::RedstoneLamp, ObservationClassification::Exact),
        "redstone_block" => (BlockKind::RedstoneBlock, ObservationClassification::Exact),
        "observer" => (BlockKind::Observer, ObservationClassification::Exact),
        "piston" | "sticky_piston" => (BlockKind::Piston, ObservationClassification::Exact),
        "glass" | "tinted_glass" => (BlockKind::Transparent, ObservationClassification::Exact),
        name if name.ends_with("_slab") || name.ends_with("_stairs") => {
            (BlockKind::Transparent, ObservationClassification::Exact)
        }
        "stone" | "dirt" | "grass_block" | "bedrock" | "cobblestone" | "deepslate" => {
            (BlockKind::Solid, ObservationClassification::Exact)
        }
        _ => (BlockKind::Solid, ObservationClassification::Coarse),
    };
    let mut block = Block::new(kind);
    block.observed_name = Some(record.name.clone());
    block.observed_properties = record.properties.clone();
    block.observation_classification = observation_classification;
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
        BlockKind::Button => {
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
        BlockKind::PressurePlate => {
            block.powered = property_bool(record, "powered").or_else(|| {
                record
                    .properties
                    .get("power")
                    .and_then(|value| value.parse::<u8>().ok())
                    .map(|power| power > 0)
            });
            block.power_level = record
                .properties
                .get("power")
                .and_then(|value| value.parse().ok());
            block.support_offset = Some(Pos::new(0, -1, 0));
        }
        BlockKind::RedstoneLamp => block.powered = property_bool(record, "lit"),
        BlockKind::Observer => {
            // Minecraft's `facing` is the observation/front direction. The
            // internal directional convention points toward output/back.
            block.facing = Some(property_facing(record)?.opposite());
            block.powered = property_bool(record, "powered");
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
    use dustroute_minecraft::CapabilityLevel;

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

    #[test]
    fn preserves_unknown_block_identity_and_all_states() {
        let json = r#"{
            "min":{"x":0,"y":0,"z":0},
            "max":{"x":0,"y":0,"z":0},
            "blocks":[
              {"pos":{"x":0,"y":0,"z":0},"name":"minecraft:target","properties":{"power":"7","custom":"kept"}}
            ]
        }"#;
        let (_, world) = world_from_snapshot_json(json).unwrap();
        let block = world.get(Pos::new(0, 0, 0)).unwrap();
        assert_eq!(block.kind, BlockKind::Solid);
        assert_eq!(block.observed_name.as_deref(), Some("minecraft:target"));
        assert_eq!(block.observed_properties["power"], "7");
        assert_eq!(block.observed_properties["custom"], "kept");
    }

    #[test]
    fn imports_slab_state_as_exact_physical_evidence() {
        let record = MinecraftSnapshotBlock {
            pos: Pos::new(0, 0, 0),
            name: "minecraft:stone_slab".to_owned(),
            properties: BTreeMap::from([("type".to_owned(), "top".to_owned())]),
        };
        let block = block_from_record(&record).unwrap();
        assert_eq!(block.kind, BlockKind::Transparent);
        assert_eq!(
            block.observation_classification,
            ObservationClassification::Exact
        );
        assert!(block.redstone_traits().supports_dust_on_top);
    }

    #[test]
    fn imports_button_plate_and_lamp_as_supported_io() {
        let button = block_from_record(&MinecraftSnapshotBlock {
            pos: Pos::new(0, 0, 0),
            name: "minecraft:stone_button".to_owned(),
            properties: BTreeMap::from([
                ("face".to_owned(), "wall".to_owned()),
                ("facing".to_owned(), "east".to_owned()),
                ("powered".to_owned(), "true".to_owned()),
            ]),
        })
        .unwrap();
        assert_eq!(button.kind, BlockKind::Button);
        assert_eq!(button.powered, Some(true));

        let plate = block_from_record(&MinecraftSnapshotBlock {
            pos: Pos::new(1, 0, 0),
            name: "minecraft:heavy_weighted_pressure_plate".to_owned(),
            properties: BTreeMap::from([("power".to_owned(), "7".to_owned())]),
        })
        .unwrap();
        assert_eq!(plate.kind, BlockKind::PressurePlate);
        assert_eq!(plate.powered, Some(true));
        assert_eq!(plate.power_level, Some(7));

        let lamp = block_from_record(&MinecraftSnapshotBlock {
            pos: Pos::new(2, 0, 0),
            name: "minecraft:redstone_lamp".to_owned(),
            properties: BTreeMap::from([("lit".to_owned(), "true".to_owned())]),
        })
        .unwrap();
        assert_eq!(lamp.kind, BlockKind::RedstoneLamp);
        assert_eq!(lamp.powered, Some(true));
    }

    #[test]
    fn imports_observer_front_as_internal_back_and_keeps_power_state() {
        let observer = block_from_record(&MinecraftSnapshotBlock {
            pos: Pos::new(1, 1, 0),
            name: "minecraft:observer".to_owned(),
            properties: BTreeMap::from([
                ("facing".to_owned(), "west".to_owned()),
                ("powered".to_owned(), "true".to_owned()),
            ]),
        })
        .unwrap();
        assert_eq!(observer.kind, BlockKind::Observer);
        assert_eq!(observer.facing, Some(Facing::East));
        assert_eq!(observer.powered, Some(true));
        assert_eq!(observer.capabilities().temporal, CapabilityLevel::Full);
    }
}
