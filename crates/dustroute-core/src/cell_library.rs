use std::collections::BTreeMap;

use crate::cells::{
    PhysicalCell, PortKind, and_cell, nand_cell, not_cell, not_top_cell, or_buffered_cell,
};
use crate::logic::GateKind;
use crate::sim::RedstoneTickSimulator;
use crate::wire::update_wire_shapes;
use crate::world::{BlockKind, Facing, Pos};

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CellVerification {
    pub valid: bool,
    pub cases: Vec<(Vec<bool>, bool, bool)>,
}

fn expected(kind: GateKind, inputs: &[bool]) -> bool {
    match kind {
        GateKind::Not => !inputs[0],
        GateKind::And => inputs.iter().all(|value| *value),
        GateKind::Or => inputs.iter().any(|value| *value),
        GateKind::Nand => !inputs.iter().all(|value| *value),
        _ => false,
    }
}

fn drive_input(world: &mut crate::world::World, port: &crate::cells::InputPort, value: bool) {
    let facing = port.facing.unwrap_or(Facing::West);
    let delta = facing.horizontal_offset().unwrap_or(Pos::new(-1, 0, 0));
    let driver = port.pos.offset(delta.x, delta.y, delta.z);
    match port.kind {
        PortKind::Wire if value => {
            world.set(driver, crate::world::Block::new(BlockKind::RedstoneBlock));
        }
        PortKind::Wire => {
            world.remove(driver);
        }
        PortKind::BlockPower => {
            let lever = world.place(BlockKind::Lever, driver);
            lever.powered = Some(value);
            lever.facing = Some(facing.opposite());
            lever.support_offset = Some(Pos::new(-delta.x, -delta.y, -delta.z));
        }
    }
}

#[must_use]
pub fn verify_cell(kind: GateKind, cell: &PhysicalCell) -> CellVerification {
    if cell.outputs.len() != 1 || cell.inputs.len() > usize::BITS as usize {
        return CellVerification {
            valid: false,
            cases: Vec::new(),
        };
    }
    let mut cases = Vec::new();
    for bits in 0..(1_usize << cell.inputs.len()) {
        let inputs: Vec<_> = (0..cell.inputs.len())
            .map(|index| bits & (1 << index) != 0)
            .collect();
        let mut world = cell.world.clone();
        for (port, value) in cell.inputs.iter().zip(&inputs) {
            drive_input(&mut world, port, *value);
        }
        update_wire_shapes(&mut world);
        let actual = RedstoneTickSimulator::new(world)
            .and_then(|mut simulator| simulator.settle_ticks(8))
            .is_ok_and(|state| state.strength(cell.outputs[0].pos) > 0);
        let wanted = expected(kind, &inputs);
        cases.push((inputs, wanted, actual));
    }
    CellVerification {
        valid: cases.iter().all(|(_, wanted, actual)| wanted == actual),
        cases,
    }
}

#[derive(Clone, Debug, Default)]
pub struct CellLibrary {
    candidates: BTreeMap<GateKind, Vec<PhysicalCell>>,
}

impl CellLibrary {
    pub fn add(&mut self, kind: GateKind, cell: PhysicalCell) {
        self.candidates.entry(kind).or_default().push(cell);
    }

    #[must_use]
    pub fn candidates_for(&self, kind: GateKind) -> &[PhysicalCell] {
        self.candidates.get(&kind).map_or(&[], Vec::as_slice)
    }

    #[must_use]
    pub fn verified_for(&self, kind: GateKind) -> Vec<(&PhysicalCell, CellVerification)> {
        self.candidates_for(kind)
            .iter()
            .filter_map(|cell| {
                let verification = verify_cell(kind, cell);
                verification.valid.then_some((cell, verification))
            })
            .collect()
    }

    #[must_use]
    pub fn choose(&self, kind: GateKind) -> Option<&PhysicalCell> {
        self.candidates_for(kind)
            .iter()
            .find(|cell| verify_cell(kind, cell).valid)
    }
}

#[must_use]
pub fn default_cell_library() -> CellLibrary {
    let mut library = CellLibrary::default();
    library.add(GateKind::Not, not_top_cell());
    library.add(GateKind::Not, not_cell());
    library.add(GateKind::And, and_cell());
    library.add(GateKind::Or, or_buffered_cell());
    library.add(GateKind::Nand, nand_cell());
    library
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn verified_library_covers_primitive_logic_cells() {
        let library = default_cell_library();
        assert!(library.candidates_for(GateKind::Not).len() >= 2);
        for kind in [GateKind::Not, GateKind::And, GateKind::Or, GateKind::Nand] {
            assert!(
                !library.verified_for(kind).is_empty(),
                "missing verified {kind:?}"
            );
        }
        assert_eq!(library.choose(GateKind::Not).unwrap().name, "not_torch_top");
    }
}
