use std::collections::BTreeMap;

use dustroute_library::blueprint::{
    BlueprintCatalog, BlueprintError, BlueprintRevisionId, ClassificationRevisionId,
};
use dustroute_library::builtin_blueprints::*;

use crate::cells::{PhysicalCell, PortKind};
use crate::ir::logic::GateKind;
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
        GateKind::Xor => inputs[0] ^ inputs[1],
        _ => false,
    }
}

pub(crate) fn drive_input(
    world: &mut crate::world::World,
    port: &crate::cells::InputPort,
    value: bool,
) {
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
    verify_cell_with_settle_ticks(kind, cell, 8)
}

#[must_use]
pub fn verify_cell_with_settle_ticks(
    kind: GateKind,
    cell: &PhysicalCell,
    settle_ticks: usize,
) -> CellVerification {
    let input_count = match kind {
        GateKind::Not => 1,
        GateKind::And | GateKind::Or | GateKind::Nand | GateKind::Xor => 2,
        _ => {
            return CellVerification {
                valid: false,
                cases: Vec::new(),
            };
        }
    };
    if cell.outputs.len() != 1 || cell.inputs.len() != input_count {
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
        let Ok(state) = RedstoneTickSimulator::new(world)
            .and_then(|mut simulator| simulator.settle_ticks(settle_ticks))
        else {
            return CellVerification {
                valid: false,
                cases,
            };
        };
        let actual = state.strength(cell.outputs[0].pos) > 0;
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
    verification_ticks: BTreeMap<
        (
            Option<dustroute_library::blueprint::BlueprintRevisionId>,
            String,
        ),
        usize,
    >,
    /// Immutable source snapshot for the geometry projections in this library.
    /// Raw authoring libraries have no catalog and cannot certify contracts.
    catalog: Option<BlueprintCatalog>,
    unavailable: Vec<(BlueprintRevisionId, BlueprintError)>,
}

impl CellLibrary {
    /// Discovers concrete revisions by classification. Geometry is only a
    /// routing proposal; callers must review the composed Assembly against
    /// `catalog()` before accepting it. Logical simulation remains separate.
    pub fn from_blueprints(
        catalog: &BlueprintCatalog,
        classifications: &BTreeMap<GateKind, ClassificationRevisionId>,
        settle_ticks: usize,
    ) -> Result<Self, BlueprintError> {
        let mut library = Self {
            catalog: Some(catalog.clone()),
            ..Self::default()
        };
        for (kind, classification) in classifications {
            if catalog.classification(classification).is_none() {
                return Err(BlueprintError::UnknownClassification(
                    classification.clone(),
                ));
            }
            for revision in catalog.candidates(classification) {
                match crate::blueprint::blueprint_cell_for_routing(catalog, &revision.id) {
                    Ok(cell) => library.add_with_settle_ticks(*kind, cell, settle_ticks),
                    Err(error) => library.unavailable.push((revision.id.clone(), error)),
                }
            }
            if let Some(candidates) = library.candidates.get_mut(kind) {
                // Stable size ordering is a search heuristic, never trust or
                // a mutable latest-version policy.
                candidates.sort_by_key(|cell| {
                    let volume = cell.world.bounds().map_or(0, |(min, max)| {
                        (i64::from(max.x) - i64::from(min.x) + 1)
                            .saturating_mul(i64::from(max.y) - i64::from(min.y) + 1)
                            .saturating_mul(i64::from(max.z) - i64::from(min.z) + 1)
                    });
                    (
                        cell.world.iter().count(),
                        volume,
                        cell.source_revision.clone(),
                    )
                });
            }
        }
        Ok(library)
    }

    #[must_use]
    pub fn catalog(&self) -> Option<&BlueprintCatalog> {
        self.catalog.as_ref()
    }

    /// Unsupported layouts stay visible as diagnostics, not silently trusted
    /// geometry with discarded requirements.
    #[must_use]
    pub fn unavailable(&self) -> &[(BlueprintRevisionId, BlueprintError)] {
        &self.unavailable
    }

    pub fn add(&mut self, kind: GateKind, cell: PhysicalCell) {
        self.add_with_settle_ticks(kind, cell, 8);
    }

    pub fn add_with_settle_ticks(
        &mut self,
        kind: GateKind,
        cell: PhysicalCell,
        settle_ticks: usize,
    ) {
        self.verification_ticks.insert(
            (cell.source_revision.clone(), cell.name.clone()),
            settle_ticks,
        );
        self.candidates.entry(kind).or_default().push(cell);
    }

    pub fn settle_ticks_for(&self, cell: &PhysicalCell) -> usize {
        self.verification_ticks
            .get(&(cell.source_revision.clone(), cell.name.clone()))
            .copied()
            .unwrap_or(8)
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
                let verification =
                    verify_cell_with_settle_ticks(kind, cell, self.settle_ticks_for(cell));
                verification.valid.then_some((cell, verification))
            })
            .collect()
    }

    #[must_use]
    pub fn choose(&self, kind: GateKind) -> Option<&PhysicalCell> {
        self.candidates_for(kind).iter().find(|cell| {
            verify_cell_with_settle_ticks(kind, cell, self.settle_ticks_for(cell)).valid
        })
    }
}

#[must_use]
pub fn default_cell_library() -> CellLibrary {
    CellLibrary::from_blueprints(builtin_blueprints(), &logic_classifications(), 64)
        .expect("built-in classification bindings exist")
}

/// Adapter from the existing Boolean DAG to interpretation labels. These
/// bindings do not assign logical meaning to connection types.
#[must_use]
pub fn logic_classifications() -> BTreeMap<GateKind, ClassificationRevisionId> {
    [
        (GateKind::Not, NOT_CLASSIFICATION_REVISION),
        (GateKind::And, AND_CLASSIFICATION_REVISION),
        (GateKind::Or, OR_CLASSIFICATION_REVISION),
        (GateKind::Nand, NAND_CLASSIFICATION_REVISION),
        (GateKind::Xor, XOR_CLASSIFICATION_REVISION),
    ]
    .into_iter()
    .map(|(kind, id)| {
        (
            kind,
            ClassificationRevisionId::new(id).expect("built-in ID"),
        )
    })
    .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn verified_library_covers_primitive_logic_cells() {
        let library = default_cell_library();
        assert!(library.candidates_for(GateKind::Not).len() >= 2);
        for kind in [
            GateKind::Not,
            GateKind::And,
            GateKind::Or,
            GateKind::Nand,
            GateKind::Xor,
        ] {
            let candidates = library
                .candidates_for(kind)
                .iter()
                .map(|cell| (&cell.name, verify_cell(kind, cell)))
                .collect::<Vec<_>>();
            assert!(
                !library.verified_for(kind).is_empty(),
                "missing verified {kind:?}: {candidates:?}"
            );
        }
        assert_eq!(library.choose(GateKind::Not).unwrap().name, "not_torch_top");
    }

    #[test]
    fn rejected_upstream_xor_is_not_selected_by_the_physical_library() {
        let cell = crate::cells::external_xor_cell();
        let verification = verify_cell(GateKind::Xor, &cell);
        assert!(!verification.valid);
        assert_eq!(
            verification.cases,
            vec![
                (vec![false, false], false, false),
                (vec![true, false], true, false),
                (vec![false, true], true, false),
                (vec![true, true], false, false),
            ]
        );
        assert_eq!(
            default_cell_library().choose(GateKind::Xor).unwrap().name,
            "dustroute.xor.compact_compiled.1_21_11"
        );
    }

    #[test]
    fn compiled_baseline_xor_has_the_xor_truth_table() {
        let cell = crate::cells::compiled_xor_cell().unwrap();
        let verification = verify_cell_with_settle_ticks(GateKind::Xor, &cell, 64);
        assert!(verification.valid, "{:?}", verification.cases);
    }

    #[test]
    fn compact_compiled_xor_has_the_xor_truth_table() {
        let cell = crate::cells::compact_compiled_xor_cell().unwrap();
        let verification = verify_cell_with_settle_ticks(GateKind::Xor, &cell, 64);
        assert!(verification.valid, "{:?}", verification.cases);
    }
}
