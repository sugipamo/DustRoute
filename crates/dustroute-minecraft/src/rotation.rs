//! Quarter turns shared by placed cells and immutable blueprint instances.

use crate::{Block, Facing, Pos};
use serde::{Deserialize, Serialize};

#[derive(
    Clone, Copy, Debug, Default, Deserialize, Eq, Hash, PartialEq, Serialize, schemars::JsonSchema,
)]
#[serde(rename_all = "snake_case")]
#[repr(u8)]
pub enum RotationY {
    #[default]
    R0 = 0,
    R90 = 1,
    R180 = 2,
    R270 = 3,
}

impl RotationY {
    #[must_use]
    pub const fn is_identity(&self) -> bool {
        matches!(self, Self::R0)
    }

    #[must_use]
    pub const fn then(self, inner: Self) -> Self {
        match (self as u8 + inner as u8) % 4 {
            0 => Self::R0,
            1 => Self::R90,
            2 => Self::R180,
            _ => Self::R270,
        }
    }

    #[must_use]
    pub const fn inverse(self) -> Self {
        match self {
            Self::R90 => Self::R270,
            Self::R270 => Self::R90,
            other => other,
        }
    }

    /// Checked counterpart for coordinates loaded from archives.
    #[must_use]
    pub fn checked_pos(self, pos: Pos) -> Option<Pos> {
        Some(match self {
            Self::R0 => pos,
            Self::R90 => Pos::new(pos.z.checked_neg()?, pos.y, pos.x),
            Self::R180 => Pos::new(pos.x.checked_neg()?, pos.y, pos.z.checked_neg()?),
            Self::R270 => Pos::new(pos.z, pos.y, pos.x.checked_neg()?),
        })
    }

    #[must_use]
    pub fn checked_block(self, block: &Block) -> Option<Block> {
        // Check arbitrary saved support offsets before the legacy projection.
        if let Some(offset) = block.support_offset {
            self.checked_pos(offset)?;
        }
        Some(self.block(block))
    }

    #[must_use]
    pub const fn pos(self, pos: Pos) -> Pos {
        match self {
            Self::R0 => pos,
            Self::R90 => Pos::new(-pos.z, pos.y, pos.x),
            Self::R180 => Pos::new(-pos.x, pos.y, -pos.z),
            Self::R270 => Pos::new(pos.z, pos.y, -pos.x),
        }
    }

    #[must_use]
    pub fn facing(self, facing: Facing) -> Facing {
        if matches!(facing, Facing::Up | Facing::Down) {
            return facing;
        }
        let index = match facing {
            Facing::North => 0,
            Facing::East => 1,
            Facing::South => 2,
            Facing::West => 3,
            Facing::Up | Facing::Down => unreachable!(),
        };
        [Facing::North, Facing::East, Facing::South, Facing::West]
            [(index + usize::from(self as u8)) % 4]
    }

    #[must_use]
    pub fn block(self, block: &Block) -> Block {
        let mut result = block.clone();
        result.facing = result.facing.map(|facing| self.facing(facing));
        result.support_offset = result.support_offset.map(|offset| self.pos(offset));
        if let Some(head) = &mut result.piston_head {
            head.facing = self.facing(head.facing);
        }
        result.wire_connections = result.wire_connections.as_ref().map(|connections| {
            connections
                .iter()
                .map(|(facing, connection)| (self.facing(*facing), *connection))
                .collect()
        });
        // Preserve the native state alongside the modeled fields for the
        // directional properties used by supported redstone layouts.
        result.observed_properties = block
            .observed_properties
            .iter()
            .map(|(key, value)| {
                let key = horizontal(key)
                    .map(|facing| name(self.facing(facing)).to_owned())
                    .unwrap_or_else(|| key.clone());
                let value = if key == "facing" {
                    horizontal(value)
                        .map(|facing| name(self.facing(facing)).to_owned())
                        .unwrap_or_else(|| value.clone())
                } else if key == "axis" && matches!(self, Self::R90 | Self::R270) {
                    match value.as_str() {
                        "x" => "z".into(),
                        "z" => "x".into(),
                        _ => value.clone(),
                    }
                } else {
                    value.clone()
                };
                (key, value)
            })
            .collect();
        result
    }
}

fn horizontal(value: &str) -> Option<Facing> {
    match value {
        "north" => Some(Facing::North),
        "east" => Some(Facing::East),
        "south" => Some(Facing::South),
        "west" => Some(Facing::West),
        _ => None,
    }
}

fn name(facing: Facing) -> &'static str {
    match facing {
        Facing::North => "north",
        Facing::East => "east",
        Facing::South => "south",
        Facing::West => "west",
        Facing::Up => "up",
        Facing::Down => "down",
    }
}
