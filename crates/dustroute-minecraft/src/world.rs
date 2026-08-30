//! Minecraft block and world primitives, independent from DustRoute IRs.

use std::collections::BTreeMap;
use std::error::Error;
use std::fmt::{Display, Formatter};

use serde::{Deserialize, Serialize};

#[derive(
    Clone, Copy, Debug, Default, Deserialize, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize,
)]
pub struct Pos {
    pub x: i32,
    pub y: i32,
    pub z: i32,
}

impl Pos {
    #[must_use]
    pub const fn new(x: i32, y: i32, z: i32) -> Self {
        Self { x, y, z }
    }

    #[must_use]
    pub const fn offset(self, dx: i32, dy: i32, dz: i32) -> Self {
        Self::new(self.x + dx, self.y + dy, self.z + dz)
    }
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, Hash, PartialEq, Serialize)]
pub enum BlockKind {
    Air,
    Solid,
    Transparent,
    RedstoneWire,
    RedstoneTorch,
    Repeater,
    Comparator,
    Lever,
    RedstoneBlock,
    Piston,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize)]
pub enum Facing {
    North,
    East,
    South,
    West,
    Up,
    Down,
}

impl Facing {
    #[must_use]
    pub const fn horizontal_offset(self) -> Option<Pos> {
        match self {
            Self::North => Some(Pos::new(0, 0, -1)),
            Self::East => Some(Pos::new(1, 0, 0)),
            Self::South => Some(Pos::new(0, 0, 1)),
            Self::West => Some(Pos::new(-1, 0, 0)),
            Self::Up | Self::Down => None,
        }
    }

    #[must_use]
    pub const fn opposite(self) -> Self {
        match self {
            Self::North => Self::South,
            Self::East => Self::West,
            Self::South => Self::North,
            Self::West => Self::East,
            Self::Up => Self::Down,
            Self::Down => Self::Up,
        }
    }
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, Hash, PartialEq, Serialize)]
pub enum WireConnection {
    None,
    Side,
    Up,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct BlockProperties {
    pub supports_components: bool,
    pub receives_weak_power: bool,
    pub receives_strong_power: bool,
    pub repeater_reads_block_power: bool,
    pub strong_power_drives_dust: bool,
}

impl BlockProperties {
    pub(crate) const fn support_only(supports_components: bool) -> Self {
        Self {
            supports_components,
            receives_weak_power: false,
            receives_strong_power: false,
            repeater_reads_block_power: false,
            strong_power_drives_dust: false,
        }
    }

    #[must_use]
    pub const fn can_be_powered(self) -> bool {
        self.receives_weak_power || self.receives_strong_power
    }
}

impl BlockKind {
    /// Whether this block is an explicit redstone circuit element rather than
    /// structural support that may incidentally carry power.
    #[must_use]
    pub const fn is_redstone_related(self) -> bool {
        matches!(
            self,
            Self::RedstoneWire
                | Self::RedstoneTorch
                | Self::Repeater
                | Self::Comparator
                | Self::Lever
                | Self::RedstoneBlock
                | Self::Piston
        )
    }

    #[must_use]
    pub const fn properties(self) -> BlockProperties {
        crate::blocks::behavior_profile(self).properties
    }
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct Block {
    pub kind: BlockKind,
    pub facing: Option<Facing>,
    pub powered: Option<bool>,
    /// Observed analog redstone strength, when the block state exposes one.
    pub power_level: Option<u8>,
    pub delay: Option<u8>,
    pub support_offset: Option<Pos>,
    pub wire_connections: Option<BTreeMap<Facing, WireConnection>>,
}

impl Block {
    #[must_use]
    pub const fn new(kind: BlockKind) -> Self {
        Self {
            kind,
            facing: None,
            powered: None,
            power_level: None,
            delay: None,
            support_offset: None,
            wire_connections: None,
        }
    }

    #[must_use]
    pub fn support_pos(&self, pos: Pos) -> Option<Pos> {
        self.support_offset
            .map(|offset| pos.offset(offset.x, offset.y, offset.z))
    }
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct World {
    blocks: BTreeMap<Pos, Block>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SupportError(pub Vec<(Pos, BlockKind, Option<Pos>)>);

impl Display for SupportError {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        write!(f, "{} component(s) have invalid support", self.0.len())
    }
}

impl Error for SupportError {}

impl World {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    pub fn set(&mut self, pos: Pos, block: Block) {
        if block.kind == BlockKind::Air {
            self.blocks.remove(&pos);
        } else {
            self.blocks.insert(pos, block);
        }
    }

    pub fn place(&mut self, kind: BlockKind, pos: Pos) -> &mut Block {
        let mut block = Block::new(kind);
        if matches!(
            kind,
            BlockKind::RedstoneWire | BlockKind::Repeater | BlockKind::Comparator
        ) {
            block.support_offset = Some(Pos::new(0, -1, 0));
        }
        self.blocks.insert(pos, block);
        self.blocks.get_mut(&pos).expect("block was just inserted")
    }

    #[must_use]
    pub fn get(&self, pos: Pos) -> Option<&Block> {
        self.blocks.get(&pos)
    }

    #[must_use]
    pub fn kind_at(&self, pos: Pos) -> BlockKind {
        self.get(pos).map_or(BlockKind::Air, |block| block.kind)
    }

    pub fn remove(&mut self, pos: Pos) -> Option<Block> {
        self.blocks.remove(&pos)
    }

    pub fn iter(&self) -> impl Iterator<Item = (&Pos, &Block)> {
        self.blocks.iter()
    }

    pub fn positions(&self) -> impl Iterator<Item = Pos> + '_ {
        self.blocks.keys().copied()
    }

    pub fn fill(&mut self, a: Pos, b: Pos, block: Block) {
        for x in a.x.min(b.x)..=a.x.max(b.x) {
            for y in a.y.min(b.y)..=a.y.max(b.y) {
                for z in a.z.min(b.z)..=a.z.max(b.z) {
                    self.set(Pos::new(x, y, z), block.clone());
                }
            }
        }
    }

    #[must_use]
    pub fn bounds(&self) -> Option<(Pos, Pos)> {
        let mut positions = self.blocks.keys();
        let first = *positions.next()?;
        let (mut low, mut high) = (first, first);
        for pos in positions {
            low = Pos::new(low.x.min(pos.x), low.y.min(pos.y), low.z.min(pos.z));
            high = Pos::new(high.x.max(pos.x), high.y.max(pos.y), high.z.max(pos.z));
        }
        Some((low, high))
    }

    #[must_use]
    pub fn support_issues(&self) -> Vec<(Pos, BlockKind, Option<Pos>)> {
        self.blocks
            .iter()
            .filter_map(|(pos, block)| {
                let requires = matches!(
                    block.kind,
                    BlockKind::RedstoneWire
                        | BlockKind::RedstoneTorch
                        | BlockKind::Repeater
                        | BlockKind::Comparator
                        | BlockKind::Lever
                );
                if !requires {
                    return None;
                }
                let support = block.support_pos(*pos);
                let valid =
                    support.is_some_and(|at| self.kind_at(at).properties().supports_components);
                (!valid).then_some((*pos, block.kind, support))
            })
            .collect()
    }

    pub fn validate_supports(&self) -> Result<(), SupportError> {
        let issues = self.support_issues();
        if issues.is_empty() {
            Ok(())
        } else {
            Err(SupportError(issues))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn air_is_not_stored_and_bounds_are_deterministic() {
        let mut world = World::new();
        world.set(Pos::new(3, 2, -1), Block::new(BlockKind::Solid));
        world.set(Pos::new(-2, 5, 4), Block::new(BlockKind::Solid));
        world.set(Pos::new(3, 2, -1), Block::new(BlockKind::Air));
        assert_eq!(
            world.bounds(),
            Some((Pos::new(-2, 5, 4), Pos::new(-2, 5, 4)))
        );
    }

    #[test]
    fn validates_component_support() {
        let mut world = World::new();
        world.set(Pos::new(0, 0, 0), Block::new(BlockKind::Solid));
        world.place(BlockKind::RedstoneWire, Pos::new(0, 1, 0));
        assert!(world.validate_supports().is_ok());
        world.remove(Pos::new(0, 0, 0));
        assert!(world.validate_supports().is_err());
    }
}
