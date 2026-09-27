use crate::logic::GateKind;
use crate::world::{Block, Facing, Pos, World};

#[derive(Clone, Copy, Debug, Default, Eq, Hash, PartialEq)]
pub enum PortKind {
    #[default]
    Wire,
    BlockPower,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct InputPort {
    pub name: String,
    pub pos: Pos,
    pub kind: PortKind,
    pub facing: Option<Facing>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct OutputPort {
    pub name: String,
    pub pos: Pos,
    pub kind: PortKind,
    pub facing: Option<Facing>,
}

pub use dustroute_minecraft::RotationY;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PhysicalCell {
    /// Exact source selected from a catalog; metadata alone is never proof.
    pub source_revision: Option<dustroute_library::blueprint::BlueprintRevisionId>,
    pub name: String,
    pub world: World,
    pub inputs: Vec<InputPort>,
    pub outputs: Vec<OutputPort>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PlacedCell {
    pub cell: PhysicalCell,
    pub origin: Pos,
    pub rotation: RotationY,
}

impl PlacedCell {
    fn transform_pos(&self, pos: Pos) -> Pos {
        let rotated = self.rotation.pos(pos);
        rotated.offset(self.origin.x, self.origin.y, self.origin.z)
    }

    #[must_use]
    pub fn input_port(&self, name: &str) -> Option<InputPort> {
        self.cell
            .inputs
            .iter()
            .find(|port| port.name == name)
            .map(|port| InputPort {
                name: port.name.clone(),
                pos: self.transform_pos(port.pos),
                kind: port.kind,
                facing: port.facing.map(|facing| self.rotation.facing(facing)),
            })
    }

    #[must_use]
    pub fn output_port(&self, name: &str) -> Option<OutputPort> {
        self.cell
            .outputs
            .iter()
            .find(|port| port.name == name)
            .map(|port| OutputPort {
                name: port.name.clone(),
                pos: self.transform_pos(port.pos),
                kind: port.kind,
                facing: port.facing.map(|facing| self.rotation.facing(facing)),
            })
    }

    pub fn blocks(&self) -> impl Iterator<Item = (Pos, Block)> + '_ {
        self.cell
            .world
            .iter()
            .map(|(pos, block)| (self.transform_pos(*pos), self.rotation.block(block)))
    }
}

// These compatibility functions resolve frozen blueprint revisions. Authoring
// recipes live in cell_generators and are invoked explicitly by the generator.
use dustroute_library::builtin_blueprints::{
    AND_REVISION, BUFFER_REVISION, EXTERNAL_XOR_REVISION, NAND_REVISION, NOT_SIDE_REVISION,
    NOT_TOP_REVISION, OR_REVISION, TERMINAL_REVISION, XOR_COMPACT_REVISION, XOR_REVISION,
};

pub use crate::cell_generators::compiled_xor_cell_with_config;

#[must_use]
pub fn terminal_cell(name: &str) -> PhysicalCell {
    let mut cell = crate::blueprint::builtin_cell(TERMINAL_REVISION);
    cell.name = name.into();
    cell
}

#[must_use]
pub fn buffered_boundary_cell(name: &str) -> PhysicalCell {
    let mut cell = crate::blueprint::builtin_cell(BUFFER_REVISION);
    cell.name = name.into();
    cell
}

#[must_use]
pub fn not_cell() -> PhysicalCell {
    crate::blueprint::builtin_cell(NOT_SIDE_REVISION)
}
#[must_use]
pub fn not_top_cell() -> PhysicalCell {
    crate::blueprint::builtin_cell(NOT_TOP_REVISION)
}
#[must_use]
pub fn and_cell() -> PhysicalCell {
    crate::blueprint::builtin_cell(AND_REVISION)
}
#[must_use]
pub fn nand_cell() -> PhysicalCell {
    crate::blueprint::builtin_cell(NAND_REVISION)
}
#[must_use]
pub fn or_buffered_cell() -> PhysicalCell {
    crate::blueprint::builtin_cell(OR_REVISION)
}
#[must_use]
pub fn external_xor_cell() -> PhysicalCell {
    crate::blueprint::builtin_cell(EXTERNAL_XOR_REVISION)
}

pub fn compiled_xor_cell() -> Result<PhysicalCell, String> {
    crate::blueprint::builtin_cell_result(XOR_REVISION).map_err(|error| error.to_string())
}
pub fn compact_compiled_xor_cell() -> Result<PhysicalCell, String> {
    crate::blueprint::builtin_cell_result(XOR_COMPACT_REVISION).map_err(|error| error.to_string())
}

#[must_use]
pub fn baseline_cell_for(kind: GateKind) -> Option<PhysicalCell> {
    match kind {
        GateKind::Input => Some(buffered_boundary_cell("input_buffer")),
        GateKind::Output => Some(buffered_boundary_cell("output")),
        GateKind::Not => Some(not_top_cell()),
        GateKind::And => Some(and_cell()),
        GateKind::Or => Some(or_buffered_cell()),
        GateKind::Nand => Some(nand_cell()),
        GateKind::Xor => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::world::BlockKind;

    #[test]
    fn rotates_ports_blocks_and_support_offsets() {
        let placed = PlacedCell {
            cell: not_cell(),
            origin: Pos::new(10, 3, 20),
            rotation: RotationY::R90,
        };
        let input = placed.input_port("a").unwrap();
        assert_eq!(input.pos, Pos::new(10, 3, 20));
        assert_eq!(input.facing, Some(Facing::North));
        let torch = placed
            .blocks()
            .find(|(_, block)| block.kind == BlockKind::RedstoneTorch)
            .unwrap();
        assert_eq!(torch.0, Pos::new(10, 3, 21));
        assert_eq!(torch.1.support_offset, Some(Pos::new(0, 0, -1)));
    }
}
