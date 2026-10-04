//! Fixed synthetic geometry. The table uses Minecraft types and explicitly
//! retains unknown state and absent versus empty wire observations. Building
//! records is not compilation, execution, adoption, or placement proof.
use crate::blueprint::PositionedBlock;
use dustroute_minecraft::{Block, BlockKind, Facing, Pos, WireConnection};

#[derive(Clone, Copy)]
pub(crate) struct Cell {
    position: Pos,
    kind: BlockKind,
    facing: Option<Facing>,
    powered: Option<bool>,
    delay: Option<u8>,
    support: Option<Pos>,
    wires: Option<&'static [(Facing, WireConnection)]>,
}

pub(crate) const fn cell(position: Pos, kind: BlockKind) -> Cell {
    Cell {
        position,
        kind,
        facing: None,
        powered: None,
        delay: None,
        support: None,
        wires: None,
    }
}

impl Cell {
    pub(crate) const fn facing(mut self, facing: Facing) -> Self {
        self.facing = Some(facing);
        self
    }
    pub(crate) const fn powered(mut self, powered: bool) -> Self {
        self.powered = Some(powered);
        self
    }
    pub(crate) const fn delay(mut self, delay: u8) -> Self {
        self.delay = Some(delay);
        self
    }
    pub(crate) const fn support(mut self, support: Pos) -> Self {
        self.support = Some(support);
        self
    }
    pub(crate) const fn wires(mut self, wires: &'static [(Facing, WireConnection)]) -> Self {
        self.wires = Some(wires);
        self
    }
    pub(crate) fn positioned(self) -> PositionedBlock {
        let mut block = Block::new(self.kind);
        block.facing = self.facing;
        block.powered = self.powered;
        block.delay = self.delay;
        block.support_offset = self.support;
        block.wire_connections = self.wires.map(|wires| wires.iter().copied().collect());
        PositionedBlock {
            position: self.position,
            block,
        }
    }
}
