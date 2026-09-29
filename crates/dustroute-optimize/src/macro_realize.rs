//! Macro realization: source-bound planning, routing, materialization and
//! independent structural, steady-state and temporal verification.
use dustroute_library::assembly::Assembly;
use dustroute_library::blueprint::BlueprintCatalog;
use dustroute_physical::{Facing, PhysicalPatch, Pos, World};
use dustroute_translate::{PlacedCell, TruthTableComparison};
use std::sync::Arc;
mod boundary;
mod materialize;
mod ownership;
mod planning;
mod routing;
mod steady;
mod structure;
mod transitions;

pub use boundary::extract_cell_boundary;
pub use boundary::extract_model_boundary;
pub use boundary::extract_model_boundary_with_context;
pub use materialize::materialize_macro_replacement;
pub use materialize::materialize_macro_replacement_in_assembly;
pub use materialize::materialize_macro_replacement_in_known_regions;
pub(crate) use planning::layout_revision;
pub use planning::plan_blueprint_replacement;
pub use planning::plan_macro_replacement;
pub use planning::plan_macro_replacement_in_catalog;
pub use planning::plan_macro_replacement_with_reserved;
pub use planning::resolve_blueprint_layout;
pub use planning::resolve_builtin_layout;
pub use steady::verify_macro_steady_state;
pub use structure::validate_macro_structure;
use transitions::transition_edges;
pub use transitions::verify_boundary_strengths;
pub use transitions::verify_macro_transitions;
pub use transitions::verify_world_transitions;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum MacroBoundaryDirection {
    Input,
    Output,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct MacroBoundaryPort {
    pub observed_index: usize,
    pub name: String,
    pub position: Pos,
    pub direction: MacroBoundaryDirection,
    pub facing: Option<Facing>,
    pub driver_position: Option<Pos>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct MacroPortRoute {
    pub boundary: MacroBoundaryPort,
    pub candidate_port: String,
    pub candidate_position: Pos,
    /// Inclusive, axis-aligned route skeleton. It is deliberately not a block
    /// patch until support, strength and neighbouring-net checks have passed.
    pub path: Vec<Pos>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ContextualVerificationState {
    Pending,
    Passed,
    Failed,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct MacroRealizationVerification {
    pub structural: ContextualVerificationState,
    pub steady_state: ContextualVerificationState,
    pub transitions: ContextualVerificationState,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct MacroReplacementPlan {
    pub component_id: String,
    pub placed: PlacedCell,
    pub routes: Vec<MacroPortRoute>,
    pub verification: MacroRealizationVerification,
    pub automatic_apply_allowed: bool,
    pub total_route_length: usize,
    /// Retains contracts and immutable identities while geometry is routed.
    source_catalog: Arc<BlueprintCatalog>,
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct MacroStructuralReport {
    pub candidate_collisions: Vec<Pos>,
    pub route_collisions: Vec<Pos>,
    pub route_cross_net_contacts: Vec<(usize, usize, Pos, Pos)>,
    pub candidate_support_issues: Vec<Pos>,
    /// Supports that a later materializer may add if their positions are free.
    pub required_route_supports: Vec<Pos>,
    pub blocked_route_supports: Vec<Pos>,
}

impl MacroStructuralReport {
    #[must_use]
    pub fn valid(&self) -> bool {
        self.candidate_collisions.is_empty()
            && self.route_collisions.is_empty()
            && self.route_cross_net_contacts.is_empty()
            && self.candidate_support_issues.is_empty()
            && self.blocked_route_supports.is_empty()
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct MaterializedMacroReplacement {
    pub world: World,
    pub patch: PhysicalPatch,
    pub added_supports: Vec<Pos>,
    pub inserted_repeaters: Vec<Pos>,
    /// Reviewed initial state; behavior and adoption remain separate decisions.
    pub assembly: Assembly,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct MacroSteadyStateReport {
    pub state: ContextualVerificationState,
    pub comparison: Option<TruthTableComparison>,
    /// Expected boundary index to newly inferred terminal index.
    pub input_mapping: Vec<usize>,
    pub output_mapping: Vec<usize>,
    pub differing_assignments: Vec<Vec<bool>>,
    pub reason: Option<String>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct MacroTransitionCase {
    pub from: Vec<bool>,
    pub to: Vec<bool>,
    pub original_outputs: Vec<Vec<bool>>,
    pub candidate_outputs: Vec<Vec<bool>>,
    pub equivalent: bool,
    pub first_difference_tick: Option<usize>,
}

/// One observed output edge in a macro transition case. The sampled output
/// arrays remain available for compatibility, while this view makes the
/// state-changing edges and their elapsed sample intervals explicit.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct MacroTransitionEdge {
    pub at_tick: usize,
    pub from: Vec<bool>,
    pub to: Vec<bool>,
    pub elapsed_from_previous: Option<usize>,
}

impl MacroTransitionCase {
    /// Extracts state-changing edges from a sampled output trace. The first
    /// edge has no predecessor; subsequent intervals are measured in the
    /// trace's sampling unit (currently redstone ticks).
    #[must_use]
    pub fn original_transition_edges(&self) -> Vec<MacroTransitionEdge> {
        transition_edges(&self.original_outputs)
    }

    #[must_use]
    pub fn candidate_transition_edges(&self) -> Vec<MacroTransitionEdge> {
        transition_edges(&self.candidate_outputs)
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct MacroTransitionReport {
    pub state: ContextualVerificationState,
    pub cases: Vec<MacroTransitionCase>,
    pub differing_cases: usize,
    pub reason: Option<String>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum MacroRealizationError {
    UnsupportedLayoutReference(String),
    MissingObservedPort {
        direction: MacroBoundaryDirection,
        index: usize,
    },
    MissingCandidatePort(String),
    NoPlacement,
    Blueprint(String),
    StructurallyInvalid(Box<MacroStructuralReport>),
    NoRepeaterSite {
        route: usize,
        after_steps: usize,
    },
}
#[cfg(test)]
mod tests {
    use super::routing::manhattan_path;
    use super::*;
    use dustroute_physical::{Block, BlockKind};
    use dustroute_translate::RotationY;
    use std::collections::BTreeSet;

    use crate::{ObservedMacroMetrics, find_builtin_verified_macro_replacements};
    use dustroute_translate::{RegionBounds, analyze_world_region, derive_functional_network};

    #[test]
    fn plans_compact_xor_against_fixed_baseline_ports_without_authorizing_apply() {
        let baseline = dustroute_translate::compiled_xor_cell().unwrap();
        let (low, high) = baseline.world.bounds().unwrap();
        let analysis = analyze_world_region(&baseline.world, RegionBounds::new(low, high));
        let model = derive_functional_network(&baseline.world, &analysis, 8, 64).unwrap();
        let candidate = find_builtin_verified_macro_replacements(
            &model,
            "java",
            "1.21.11",
            ObservedMacroMetrics::from_world(&baseline.world),
        )
        .remove(0);

        let boundary = extract_model_boundary_with_context(&model, &baseline.world, &analysis);
        let reserved = boundary
            .iter()
            .filter_map(|port| port.driver_position)
            .collect();
        let plan = plan_macro_replacement_with_reserved(&candidate, &boundary, &reserved).unwrap();

        assert_eq!(plan.routes.len(), 3);
        assert!(
            plan.routes
                .iter()
                .all(|route| route.path.first() == Some(&route.candidate_position))
        );
        assert!(
            plan.routes
                .iter()
                .all(|route| route.path.last() == Some(&route.boundary.position))
        );
        assert!(!plan.automatic_apply_allowed);
        assert_eq!(
            plan.verification.transitions,
            ContextualVerificationState::Pending
        );

        let boundary_positions = boundary
            .iter()
            .map(|port| port.position)
            .collect::<BTreeSet<_>>();
        let replaceable = baseline
            .world
            .positions()
            .filter(|pos| !boundary_positions.contains(pos))
            .collect();
        let report = validate_macro_structure(&plan, &baseline.world, &replaceable);
        assert!(report.candidate_collisions.is_empty());
        assert!(report.blocked_route_supports.is_empty());
        // The compatibility facade lacks explicit air coverage above the old
        // bounding box. It cannot certify the replacement's rising wires.
        let missing =
            materialize_macro_replacement(&plan, &baseline.world, &replaceable, 14).unwrap_err();
        assert!(
            matches!(missing, MacroRealizationError::Blueprint(ref detail) if detail.contains("UnknownWireConnection"))
        );
        // This synthetic fixture declares the full working volume as known;
        // real callers must pass actual scan coverage, not derive new evidence.
        let known =
            dustroute_translate::Region::new(Pos::new(-100, -10, -100), Pos::new(100, 20, 100));
        let materialized = materialize_macro_replacement_in_known_regions(
            &plan,
            &baseline.world,
            &[known],
            &replaceable,
            14,
        )
        .unwrap();
        assert_eq!(materialized.assembly.known_regions, vec![known]);
        let steady = verify_macro_steady_state(
            &model.truth_table,
            &baseline.world,
            &materialized.world,
            8,
            64,
        );
        assert_eq!(
            steady.state,
            ContextualVerificationState::Passed,
            "{steady:#?}"
        );
        assert_eq!(steady.comparison.as_ref().unwrap().differing_rows, 0);
        assert!(steady.differing_assignments.is_empty());
        let transitions = verify_macro_transitions(
            &model.truth_table,
            &baseline.world,
            &materialized.world,
            64,
            16,
            4,
        );
        assert_eq!(transitions.state, ContextualVerificationState::Failed);
        assert_eq!(transitions.differing_cases, 12);
        assert!(
            transitions
                .cases
                .iter()
                .all(|case| { case.original_outputs.last() == case.candidate_outputs.last() })
        );
        let swap = transitions
            .cases
            .iter()
            .find(|case| case.from == [true, false] && case.to == [false, true])
            .unwrap();
        assert_eq!(swap.first_difference_tick, Some(10));
        assert_eq!(
            swap.original_outputs
                .iter()
                .filter(|value| value == &&vec![false])
                .count(),
            1
        );
        assert_eq!(
            swap.candidate_outputs
                .iter()
                .filter(|value| value == &&vec![false])
                .count(),
            2
        );
    }

    #[test]
    fn structural_validation_rejects_immutable_obstacles_and_cross_net_contacts() {
        let baseline = dustroute_translate::compiled_xor_cell().unwrap();
        let (low, high) = baseline.world.bounds().unwrap();
        let analysis = analyze_world_region(&baseline.world, RegionBounds::new(low, high));
        let model = derive_functional_network(&baseline.world, &analysis, 8, 64).unwrap();
        let candidate = find_builtin_verified_macro_replacements(
            &model,
            "java",
            "1.21.11",
            ObservedMacroMetrics::from_world(&baseline.world),
        )
        .remove(0);
        let mut plan =
            plan_macro_replacement(&candidate, &extract_cell_boundary(&baseline)).unwrap();
        let (collision, candidate_block) = plan.placed.blocks().next().unwrap();
        let mut observed = World::new();
        let obstacle = if candidate_block.kind == BlockKind::Solid {
            BlockKind::Transparent
        } else {
            BlockKind::Solid
        };
        observed.set(collision, dustroute_physical::Block::new(obstacle));
        plan.routes[1].path = plan.routes[0].path.clone();

        let report = validate_macro_structure(&plan, &observed, &BTreeSet::new());
        assert!(report.candidate_collisions.contains(&collision));
        assert!(!report.route_cross_net_contacts.is_empty());
    }

    #[test]
    fn materializes_supported_wire_refreshes_strength_and_round_trips_patch() {
        let placed = PlacedCell {
            cell: dustroute_translate::terminal_cell("source"),
            origin: Pos::new(0, 0, 0),
            rotation: RotationY::R0,
        };
        let boundary = MacroBoundaryPort {
            observed_index: 0,
            name: "out".into(),
            position: Pos::new(20, 1, 0),
            direction: MacroBoundaryDirection::Output,
            facing: None,
            driver_position: None,
        };
        let plan = MacroReplacementPlan {
            component_id: "test.long-route".into(),
            source_catalog: Arc::new(
                dustroute_library::builtin_blueprints::builtin_blueprints().clone(),
            ),
            placed,
            routes: vec![MacroPortRoute {
                boundary: boundary.clone(),
                candidate_port: "out".into(),
                candidate_position: Pos::new(0, 1, 0),
                path: manhattan_path(Pos::new(0, 1, 0), boundary.position),
            }],
            verification: MacroRealizationVerification {
                structural: ContextualVerificationState::Pending,
                steady_state: ContextualVerificationState::Pending,
                transitions: ContextualVerificationState::Pending,
            },
            automatic_apply_allowed: false,
            total_route_length: 20,
        };
        let mut observed = World::new();
        observed.set(Pos::new(20, 0, 0), Block::new(BlockKind::Solid));
        observed.place(BlockKind::RedstoneWire, boundary.position);

        let result = materialize_macro_replacement(&plan, &observed, &BTreeSet::new(), 14).unwrap();

        assert_eq!(result.inserted_repeaters, [Pos::new(14, 1, 0)]);
        assert_eq!(
            result.world.kind_at(Pos::new(14, 1, 0)),
            BlockKind::Repeater
        );
        assert!(result.world.support_issues().is_empty());
        let restored = result.patch.inverse().apply_virtual(&result.world).unwrap();
        assert_eq!(restored, observed);
    }

    #[test]
    fn steady_state_verification_rebinds_terminals_by_boundary_component() {
        let cell = dustroute_translate::compact_compiled_xor_cell().unwrap();
        let (low, high) = cell.world.bounds().unwrap();
        let analysis = analyze_world_region(&cell.world, RegionBounds::new(low, high));
        let expected = derive_functional_network(&cell.world, &analysis, 8, 64)
            .unwrap()
            .truth_table;

        let report = verify_macro_steady_state(&expected, &cell.world, &cell.world, 8, 64);

        assert_eq!(report.state, ContextualVerificationState::Passed);
        assert_eq!(report.comparison.unwrap().differing_bits, 0);
        assert_eq!(report.input_mapping.len(), 2);
        assert_eq!(report.output_mapping.len(), 1);
    }

    #[test]
    fn transition_verification_covers_single_and_multi_input_changes() {
        let cell = dustroute_translate::compact_compiled_xor_cell().unwrap();
        let (low, high) = cell.world.bounds().unwrap();
        let analysis = analyze_world_region(&cell.world, RegionBounds::new(low, high));
        let expected = derive_functional_network(&cell.world, &analysis, 8, 64)
            .unwrap()
            .truth_table;

        let report = verify_macro_transitions(&expected, &cell.world, &cell.world, 64, 16, 4);

        assert_eq!(report.state, ContextualVerificationState::Passed);
        assert_eq!(report.cases.len(), 12);
        assert_eq!(report.differing_cases, 0);
        assert!(
            report
                .cases
                .iter()
                .any(|case| { case.from == [true, false] && case.to == [false, true] })
        );
        let case = report
            .cases
            .iter()
            .find(|case| case.from == [false, false] && case.to == [true, false])
            .expect("single-input transition case");
        let edges = case.original_transition_edges();
        assert!(!edges.is_empty());
        assert_eq!(edges[0].elapsed_from_previous, None);
        assert!(edges.windows(2).all(|pair| {
            pair[1].at_tick > pair[0].at_tick
                && pair[1].elapsed_from_previous == Some(pair[1].at_tick - pair[0].at_tick)
        }));
    }
}
