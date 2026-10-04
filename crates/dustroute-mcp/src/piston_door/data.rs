//! Fixed observed layout. The independent capture is kept as a test fixture;
//! production builds this snapshot from Rust values, without a JSON decoder.
use dustroute_translate::{
    snapshot::{MinecraftSnapshot, MinecraftSnapshotBlock},
    world::{Facing, Pos, WireConnection},
};
use std::collections::BTreeMap;

#[derive(Clone, Copy)]
struct Cell {
    pos: Pos,
    state: State,
}
#[derive(Clone, Copy)]
enum State {
    Stone,
    FloorLeverNorth {
        powered: bool,
    },
    StickyPiston {
        facing: Facing,
        extended: bool,
    },
    StickyHead {
        facing: Facing,
    },
    Wire {
        power: u8,
        arms: [WireConnection; 4],
    },
}
fn facing_name(facing: Facing) -> &'static str {
    match facing {
        Facing::North => "north",
        Facing::East => "east",
        Facing::South => "south",
        Facing::West => "west",
        Facing::Up => "up",
        Facing::Down => "down",
    }
}
fn connection_name(connection: WireConnection) -> &'static str {
    match connection {
        WireConnection::None => "none",
        WireConnection::Side => "side",
        WireConnection::Up => "up",
    }
}
fn block(cell: &Cell) -> MinecraftSnapshotBlock {
    let mut properties = BTreeMap::new();
    let name = match cell.state {
        State::Stone => "minecraft:stone",
        State::FloorLeverNorth { powered } => {
            properties.insert("face".into(), "floor".into());
            properties.insert("facing".into(), "north".into());
            properties.insert("powered".into(), powered.to_string());
            "minecraft:lever"
        }
        State::StickyPiston { facing, extended } => {
            properties.insert("facing".into(), facing_name(facing).into());
            properties.insert("extended".into(), extended.to_string());
            "minecraft:sticky_piston"
        }
        State::StickyHead { facing } => {
            properties.insert("facing".into(), facing_name(facing).into());
            properties.insert("short".into(), "false".into());
            properties.insert("type".into(), "sticky".into());
            "minecraft:piston_head"
        }
        State::Wire { power, arms } => {
            properties.insert("power".into(), power.to_string());
            for (key, arm) in ["north", "east", "south", "west"].into_iter().zip(arms) {
                properties.insert(key.into(), connection_name(arm).into());
            }
            "minecraft:redstone_wire"
        }
    };
    MinecraftSnapshotBlock {
        pos: cell.pos,
        name: name.into(),
        properties,
    }
}
fn snapshot(cells: &[Cell]) -> MinecraftSnapshot {
    MinecraftSnapshot {
        min: Pos::new(-2, -1, -3),
        max: Pos::new(6, 2, 5),
        blocks: cells.iter().map(block).collect(),
    }
}
pub(super) fn open() -> MinecraftSnapshot {
    snapshot(OPEN)
}
pub(super) fn closed() -> MinecraftSnapshot {
    snapshot(CLOSED)
}

const OPEN: &[Cell] = &[
    Cell {
        pos: Pos::new(-1, -1, 0),
        state: State::Stone,
    },
    Cell {
        pos: Pos::new(0, -1, 0),
        state: State::Stone,
    },
    Cell {
        pos: Pos::new(0, -1, 1),
        state: State::Stone,
    },
    Cell {
        pos: Pos::new(0, -1, 2),
        state: State::Stone,
    },
    Cell {
        pos: Pos::new(0, -1, 3),
        state: State::Stone,
    },
    Cell {
        pos: Pos::new(0, 0, 0),
        state: State::StickyPiston {
            facing: Facing::East,
            extended: false,
        },
    },
    Cell {
        pos: Pos::new(0, 0, 1),
        state: State::Wire {
            power: 0,
            arms: [
                WireConnection::Side,
                WireConnection::None,
                WireConnection::Side,
                WireConnection::None,
            ],
        },
    },
    Cell {
        pos: Pos::new(0, 0, 2),
        state: State::Wire {
            power: 0,
            arms: [
                WireConnection::Side,
                WireConnection::None,
                WireConnection::Side,
                WireConnection::None,
            ],
        },
    },
    Cell {
        pos: Pos::new(0, 0, 3),
        state: State::Wire {
            power: 0,
            arms: [
                WireConnection::Side,
                WireConnection::Side,
                WireConnection::None,
                WireConnection::None,
            ],
        },
    },
    Cell {
        pos: Pos::new(1, -1, 0),
        state: State::Stone,
    },
    Cell {
        pos: Pos::new(1, -1, 3),
        state: State::Stone,
    },
    Cell {
        pos: Pos::new(1, 0, -2),
        state: State::Stone,
    },
    Cell {
        pos: Pos::new(1, 0, -1),
        state: State::Stone,
    },
    Cell {
        pos: Pos::new(1, 0, 0),
        state: State::Stone,
    },
    Cell {
        pos: Pos::new(1, 0, 1),
        state: State::Stone,
    },
    Cell {
        pos: Pos::new(1, 0, 2),
        state: State::Stone,
    },
    Cell {
        pos: Pos::new(1, 0, 3),
        state: State::Wire {
            power: 0,
            arms: [
                WireConnection::None,
                WireConnection::Side,
                WireConnection::None,
                WireConnection::Side,
            ],
        },
    },
    Cell {
        pos: Pos::new(1, 1, -2),
        state: State::Stone,
    },
    Cell {
        pos: Pos::new(1, 1, -1),
        state: State::Stone,
    },
    Cell {
        pos: Pos::new(1, 1, 1),
        state: State::Stone,
    },
    Cell {
        pos: Pos::new(1, 1, 2),
        state: State::Stone,
    },
    Cell {
        pos: Pos::new(2, -1, -2),
        state: State::Stone,
    },
    Cell {
        pos: Pos::new(2, -1, -1),
        state: State::Stone,
    },
    Cell {
        pos: Pos::new(2, -1, 0),
        state: State::Stone,
    },
    Cell {
        pos: Pos::new(2, -1, 1),
        state: State::Stone,
    },
    Cell {
        pos: Pos::new(2, -1, 2),
        state: State::Stone,
    },
    Cell {
        pos: Pos::new(2, -1, 3),
        state: State::Stone,
    },
    Cell {
        pos: Pos::new(2, -1, 4),
        state: State::Stone,
    },
    Cell {
        pos: Pos::new(2, 0, 3),
        state: State::Wire {
            power: 0,
            arms: [
                WireConnection::None,
                WireConnection::Side,
                WireConnection::Side,
                WireConnection::Side,
            ],
        },
    },
    Cell {
        pos: Pos::new(2, 0, 4),
        state: State::FloorLeverNorth { powered: false },
    },
    Cell {
        pos: Pos::new(2, 2, -2),
        state: State::Stone,
    },
    Cell {
        pos: Pos::new(2, 2, -1),
        state: State::Stone,
    },
    Cell {
        pos: Pos::new(2, 2, 0),
        state: State::Stone,
    },
    Cell {
        pos: Pos::new(2, 2, 1),
        state: State::Stone,
    },
    Cell {
        pos: Pos::new(2, 2, 2),
        state: State::Stone,
    },
    Cell {
        pos: Pos::new(3, -1, 3),
        state: State::Stone,
    },
    Cell {
        pos: Pos::new(3, 0, -2),
        state: State::Stone,
    },
    Cell {
        pos: Pos::new(3, 0, -1),
        state: State::Stone,
    },
    Cell {
        pos: Pos::new(3, 0, 0),
        state: State::Stone,
    },
    Cell {
        pos: Pos::new(3, 0, 1),
        state: State::Stone,
    },
    Cell {
        pos: Pos::new(3, 0, 2),
        state: State::Stone,
    },
    Cell {
        pos: Pos::new(3, 0, 3),
        state: State::Wire {
            power: 0,
            arms: [
                WireConnection::None,
                WireConnection::Side,
                WireConnection::None,
                WireConnection::Side,
            ],
        },
    },
    Cell {
        pos: Pos::new(3, 1, -2),
        state: State::Stone,
    },
    Cell {
        pos: Pos::new(3, 1, -1),
        state: State::Stone,
    },
    Cell {
        pos: Pos::new(3, 1, 0),
        state: State::Stone,
    },
    Cell {
        pos: Pos::new(3, 1, 1),
        state: State::Stone,
    },
    Cell {
        pos: Pos::new(3, 1, 2),
        state: State::Stone,
    },
    Cell {
        pos: Pos::new(4, -1, 2),
        state: State::Stone,
    },
    Cell {
        pos: Pos::new(4, -1, 3),
        state: State::Stone,
    },
    Cell {
        pos: Pos::new(4, 0, 1),
        state: State::Stone,
    },
    Cell {
        pos: Pos::new(4, 0, 2),
        state: State::Wire {
            power: 0,
            arms: [
                WireConnection::Up,
                WireConnection::None,
                WireConnection::Side,
                WireConnection::None,
            ],
        },
    },
    Cell {
        pos: Pos::new(4, 0, 3),
        state: State::Wire {
            power: 0,
            arms: [
                WireConnection::Side,
                WireConnection::None,
                WireConnection::None,
                WireConnection::Side,
            ],
        },
    },
    Cell {
        pos: Pos::new(4, 1, 0),
        state: State::StickyPiston {
            facing: Facing::West,
            extended: false,
        },
    },
    Cell {
        pos: Pos::new(4, 1, 1),
        state: State::Wire {
            power: 0,
            arms: [
                WireConnection::Side,
                WireConnection::None,
                WireConnection::Side,
                WireConnection::None,
            ],
        },
    },
    Cell {
        pos: Pos::new(5, 0, 0),
        state: State::Stone,
    },
];

const CLOSED: &[Cell] = &[
    Cell {
        pos: Pos::new(-1, -1, 0),
        state: State::Stone,
    },
    Cell {
        pos: Pos::new(0, -1, 0),
        state: State::Stone,
    },
    Cell {
        pos: Pos::new(0, -1, 1),
        state: State::Stone,
    },
    Cell {
        pos: Pos::new(0, -1, 2),
        state: State::Stone,
    },
    Cell {
        pos: Pos::new(0, -1, 3),
        state: State::Stone,
    },
    Cell {
        pos: Pos::new(0, 0, 0),
        state: State::StickyPiston {
            facing: Facing::East,
            extended: true,
        },
    },
    Cell {
        pos: Pos::new(0, 0, 1),
        state: State::Wire {
            power: 11,
            arms: [
                WireConnection::Side,
                WireConnection::None,
                WireConnection::Side,
                WireConnection::None,
            ],
        },
    },
    Cell {
        pos: Pos::new(0, 0, 2),
        state: State::Wire {
            power: 12,
            arms: [
                WireConnection::Side,
                WireConnection::None,
                WireConnection::Side,
                WireConnection::None,
            ],
        },
    },
    Cell {
        pos: Pos::new(0, 0, 3),
        state: State::Wire {
            power: 13,
            arms: [
                WireConnection::Side,
                WireConnection::Side,
                WireConnection::None,
                WireConnection::None,
            ],
        },
    },
    Cell {
        pos: Pos::new(1, -1, 0),
        state: State::Stone,
    },
    Cell {
        pos: Pos::new(1, -1, 3),
        state: State::Stone,
    },
    Cell {
        pos: Pos::new(1, 0, -2),
        state: State::Stone,
    },
    Cell {
        pos: Pos::new(1, 0, -1),
        state: State::Stone,
    },
    Cell {
        pos: Pos::new(1, 0, 0),
        state: State::StickyHead {
            facing: Facing::East,
        },
    },
    Cell {
        pos: Pos::new(1, 0, 1),
        state: State::Stone,
    },
    Cell {
        pos: Pos::new(1, 0, 2),
        state: State::Stone,
    },
    Cell {
        pos: Pos::new(1, 0, 3),
        state: State::Wire {
            power: 14,
            arms: [
                WireConnection::None,
                WireConnection::Side,
                WireConnection::None,
                WireConnection::Side,
            ],
        },
    },
    Cell {
        pos: Pos::new(1, 1, -2),
        state: State::Stone,
    },
    Cell {
        pos: Pos::new(1, 1, -1),
        state: State::Stone,
    },
    Cell {
        pos: Pos::new(1, 1, 1),
        state: State::Stone,
    },
    Cell {
        pos: Pos::new(1, 1, 2),
        state: State::Stone,
    },
    Cell {
        pos: Pos::new(2, -1, -2),
        state: State::Stone,
    },
    Cell {
        pos: Pos::new(2, -1, -1),
        state: State::Stone,
    },
    Cell {
        pos: Pos::new(2, -1, 0),
        state: State::Stone,
    },
    Cell {
        pos: Pos::new(2, -1, 1),
        state: State::Stone,
    },
    Cell {
        pos: Pos::new(2, -1, 2),
        state: State::Stone,
    },
    Cell {
        pos: Pos::new(2, -1, 3),
        state: State::Stone,
    },
    Cell {
        pos: Pos::new(2, -1, 4),
        state: State::Stone,
    },
    Cell {
        pos: Pos::new(2, 0, 0),
        state: State::Stone,
    },
    Cell {
        pos: Pos::new(2, 0, 3),
        state: State::Wire {
            power: 15,
            arms: [
                WireConnection::None,
                WireConnection::Side,
                WireConnection::Side,
                WireConnection::Side,
            ],
        },
    },
    Cell {
        pos: Pos::new(2, 0, 4),
        state: State::FloorLeverNorth { powered: true },
    },
    Cell {
        pos: Pos::new(2, 1, 0),
        state: State::Stone,
    },
    Cell {
        pos: Pos::new(2, 2, -2),
        state: State::Stone,
    },
    Cell {
        pos: Pos::new(2, 2, -1),
        state: State::Stone,
    },
    Cell {
        pos: Pos::new(2, 2, 0),
        state: State::Stone,
    },
    Cell {
        pos: Pos::new(2, 2, 1),
        state: State::Stone,
    },
    Cell {
        pos: Pos::new(2, 2, 2),
        state: State::Stone,
    },
    Cell {
        pos: Pos::new(3, -1, 3),
        state: State::Stone,
    },
    Cell {
        pos: Pos::new(3, 0, -2),
        state: State::Stone,
    },
    Cell {
        pos: Pos::new(3, 0, -1),
        state: State::Stone,
    },
    Cell {
        pos: Pos::new(3, 0, 0),
        state: State::Stone,
    },
    Cell {
        pos: Pos::new(3, 0, 1),
        state: State::Stone,
    },
    Cell {
        pos: Pos::new(3, 0, 2),
        state: State::Stone,
    },
    Cell {
        pos: Pos::new(3, 0, 3),
        state: State::Wire {
            power: 14,
            arms: [
                WireConnection::None,
                WireConnection::Side,
                WireConnection::None,
                WireConnection::Side,
            ],
        },
    },
    Cell {
        pos: Pos::new(3, 1, -2),
        state: State::Stone,
    },
    Cell {
        pos: Pos::new(3, 1, -1),
        state: State::Stone,
    },
    Cell {
        pos: Pos::new(3, 1, 0),
        state: State::StickyHead {
            facing: Facing::West,
        },
    },
    Cell {
        pos: Pos::new(3, 1, 1),
        state: State::Stone,
    },
    Cell {
        pos: Pos::new(3, 1, 2),
        state: State::Stone,
    },
    Cell {
        pos: Pos::new(4, -1, 2),
        state: State::Stone,
    },
    Cell {
        pos: Pos::new(4, -1, 3),
        state: State::Stone,
    },
    Cell {
        pos: Pos::new(4, 0, 1),
        state: State::Stone,
    },
    Cell {
        pos: Pos::new(4, 0, 2),
        state: State::Wire {
            power: 12,
            arms: [
                WireConnection::Up,
                WireConnection::None,
                WireConnection::Side,
                WireConnection::None,
            ],
        },
    },
    Cell {
        pos: Pos::new(4, 0, 3),
        state: State::Wire {
            power: 13,
            arms: [
                WireConnection::Side,
                WireConnection::None,
                WireConnection::None,
                WireConnection::Side,
            ],
        },
    },
    Cell {
        pos: Pos::new(4, 1, 0),
        state: State::StickyPiston {
            facing: Facing::West,
            extended: true,
        },
    },
    Cell {
        pos: Pos::new(4, 1, 1),
        state: State::Wire {
            power: 11,
            arms: [
                WireConnection::Side,
                WireConnection::None,
                WireConnection::Side,
                WireConnection::None,
            ],
        },
    },
    Cell {
        pos: Pos::new(5, 0, 0),
        state: State::Stone,
    },
];
