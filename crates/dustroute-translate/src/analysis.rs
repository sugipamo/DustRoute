//! Physical-first analysis facade shared by CLI, MCP, and optimizers.
//!
//! The types in this module deliberately expose every abstraction boundary. A
//! caller can stop at physical evidence without accepting a logical guess.

use std::collections::{BTreeMap, BTreeSet, VecDeque};

use serde::{Deserialize, Serialize};

use crate::{
    api::ReverseRequest, api::ReverseResult, api::Translator, scenario::Scenario,
    scenario::ScenarioAction, scenario::ScenarioCapability, scenario::ScenarioDifference,
    scenario::ScenarioExpectation, scenario::ScenarioRun, scenario::ScenarioTrace,
    scenario::compare_scenario_traces, scenario::run_scenario, snapshot::MinecraftSnapshot,
    snapshot::world_from_snapshot, world::BlockKind, world::Pos, world::World,
    world_reverse::InferredTerminal, world_reverse::InferredTruthTable,
    world_reverse::RegionBounds, world_reverse::TruthTableComparison,
    world_reverse::compare_truth_tables, world_reverse::inferred_input_driver,
};

mod equivalence;
mod explanation;
mod roles;
mod scenarios;
pub use equivalence::verify_semantic_equivalence;
pub use explanation::{explain_focused_component, explain_focused_scene, explain_signal_path};
pub use roles::{classify_focused_role, classify_truth_table, derive_local_logic};
pub use scenarios::{compare_live_trace, propose_scenarios, simulate_scenario};

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum BooleanFunction {
    Buffer,
    Not,
    And,
    Or,
    Xor,
    Nand,
    Nor,
    Xnor,
    Unclassified,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum FunctionalClassification {
    Buffer,
    Not,
    And,
    Or,
    Xor,
    Nand,
    Nor,
    Xnor,
    HalfAdder,
    FullAdder,
    Unclassified,
    Unknown,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct LogicalRole {
    pub classification: FunctionalClassification,
    pub output_functions: Vec<BooleanFunction>,
    pub input_count: usize,
    pub output_count: usize,
    pub basis: String,
    pub reason: Option<String>,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum LocalSignalRole {
    InputBoundary,
    OutputBoundary,
    SignalMerge,
    SignalBranch,
    FeedbackPath,
    IntermediatePath,
    SupportOrUnresolved,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct FocusedRole {
    pub position: Pos,
    pub physical_component: Option<dustroute_physical::ComponentId>,
    pub signal_component: Option<usize>,
    pub incoming_components: BTreeSet<usize>,
    pub outgoing_components: BTreeSet<usize>,
    pub role: LocalSignalRole,
}

/// Result of the complete staged translation. `reverse` is retained as a
/// compatibility view while `hierarchy` is the canonical stage-by-stage view.
#[derive(Clone, Debug)]
pub struct PhysicalAnalysis {
    pub bounds: RegionBounds,
    pub reverse: ReverseResult,
    pub hierarchy: dustroute_ir::HierarchicalIr,
    pub logical_role: LogicalRole,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct SignalPath {
    pub positions: Vec<Pos>,
    pub transfers: Vec<dustroute_physical::TransferKind>,
    pub complete: bool,
    pub explanation: String,
}

/// A directed physical edge adjacent to the focused component.  This is
/// intentionally presentation-neutral so MCP and CLI clients can explain the
/// same evidence without rebuilding graph details themselves.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct FocusedConnection {
    pub source_component: dustroute_physical::ComponentId,
    pub source_position: Pos,
    pub sink_component: dustroute_physical::ComponentId,
    pub sink_position: Pos,
    pub transfer: dustroute_physical::TransferKind,
    pub confidence: dustroute_physical::Confidence,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct FocusedPath {
    pub endpoint: Pos,
    pub direction: String,
    pub path: SignalPath,
}

/// Bounded, physical-first explanation for a gaze target.  The candidate
/// terminals and paths are evidence, not a claim that the entire circuit has
/// been semantically identified.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct FocusedExplanation {
    pub position: Pos,
    pub block: Option<BlockKind>,
    pub observed_name: Option<String>,
    pub observed_properties: BTreeMap<String, String>,
    pub physical_component: Option<dustroute_physical::ComponentId>,
    pub role: FocusedRole,
    pub incoming: Vec<FocusedConnection>,
    pub outgoing: Vec<FocusedConnection>,
    pub input_candidates: Vec<InferredTerminal>,
    pub output_candidates: Vec<InferredTerminal>,
    pub paths_from_inputs: Vec<FocusedPath>,
    pub paths_to_outputs: Vec<FocusedPath>,
    pub timing: dustroute_physical::TemporalAssessment,
    pub temporal_devices: Vec<dustroute_ir::TemporalDevice>,
    pub observation_complete: bool,
    pub caveats: Vec<String>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SemanticEquivalence {
    pub equivalent: bool,
    pub comparison: Option<TruthTableComparison>,
    pub reason: String,
}

#[must_use]
pub fn analyze_physical_region(world: &World, request: ReverseRequest) -> PhysicalAnalysis {
    let reverse = Translator.reverse(world, request);
    let mut hierarchy = dustroute_ir::hierarchy_from_views(
        &reverse.analysis.scene,
        reverse.gate_view.clone(),
        reverse.expression_view.clone(),
        reverse.functional_view.clone(),
    );
    hierarchy.temporal = reverse.temporal.clone();
    let logical_role = derive_local_logic(&reverse);
    PhysicalAnalysis {
        bounds: request.bounds,
        reverse,
        hierarchy,
        logical_role,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{api::ForwardOptions, circuits::half_adder, snapshot::MinecraftSnapshotBlock};

    #[test]
    fn facade_preserves_stages_and_classifies_half_adder() {
        let forward = Translator
            .forward(&half_adder(), ForwardOptions::default())
            .unwrap();
        let (min, max) = forward.compiled.world.bounds().unwrap();
        let analysis = analyze_physical_region(
            &forward.compiled.world,
            ReverseRequest::new(RegionBounds::new(
                min.offset(-1, -1, -1),
                max.offset(1, 1, 1),
            ))
            .with_truth_table(8),
        );
        assert_eq!(
            analysis.logical_role.classification,
            FunctionalClassification::HalfAdder
        );
        assert!(
            !analysis
                .hierarchy
                .physical_graph
                .value
                .scene
                .components
                .is_empty()
        );
        assert!(!analysis.hierarchy.cell_graph.value.cells.gates.is_empty());
    }

    #[test]
    fn equivalence_requires_matching_truth_tables() {
        let forward = Translator
            .forward(&half_adder(), ForwardOptions::default())
            .unwrap();
        let (min, max) = forward.compiled.world.bounds().unwrap();
        let request = ReverseRequest::new(RegionBounds::new(
            min.offset(-1, -1, -1),
            max.offset(1, 1, 1),
        ))
        .with_truth_table(8);
        let left = analyze_physical_region(&forward.compiled.world, request);
        let right = analyze_physical_region(&forward.compiled.world, request);
        assert!(verify_semantic_equivalence(&left, &right).equivalent);
    }

    #[test]
    fn explains_a_directed_input_to_output_path() {
        let forward = Translator
            .forward(&half_adder(), ForwardOptions::default())
            .unwrap();
        let (min, max) = forward.compiled.world.bounds().unwrap();
        let analysis = analyze_physical_region(
            &forward.compiled.world,
            ReverseRequest::new(RegionBounds::new(
                min.offset(-1, -1, -1),
                max.offset(1, 1, 1),
            )),
        );
        let scene = &analysis.reverse.analysis.scene;
        let edge = scene
            .connections
            .first()
            .expect("compiled circuit should have a directed connection");
        let from = scene
            .components
            .iter()
            .find(|component| component.id == edge.source.component)
            .unwrap()
            .pos;
        let to = scene
            .components
            .iter()
            .find(|component| component.id == edge.sink.component)
            .unwrap()
            .pos;
        let path = explain_signal_path(&analysis, from, to);
        assert!(path.positions.len() >= 2);
        assert_eq!(path.transfers.len() + 1, path.positions.len());
    }

    #[test]
    fn focused_explanation_keeps_terminals_paths_and_timing_evidence() {
        let forward = Translator
            .forward(&half_adder(), ForwardOptions::default())
            .unwrap();
        let (min, max) = forward.compiled.world.bounds().unwrap();
        let analysis = analyze_physical_region(
            &forward.compiled.world,
            ReverseRequest::new(RegionBounds::new(
                min.offset(-1, -1, -1),
                max.offset(1, 1, 1),
            )),
        );
        let target = analysis.reverse.analysis.inputs[0].anchor;
        let explanation = explain_focused_component(&analysis, target, true);
        assert_eq!(explanation.position, target);
        assert_eq!(explanation.role.role, LocalSignalRole::InputBoundary);
        assert!(!explanation.input_candidates.is_empty());
        assert!(!explanation.output_candidates.is_empty());
        assert_eq!(explanation.paths_from_inputs[0].path.positions[0], target);
        assert!(explanation.observation_complete);
    }

    #[test]
    fn hierarchical_focused_explanation_is_explicit_about_skipped_terminals() {
        let forward = Translator
            .forward(&half_adder(), ForwardOptions::default())
            .unwrap();
        let (min, max) = forward.compiled.world.bounds().unwrap();
        let analysis = analyze_physical_region(
            &forward.compiled.world,
            ReverseRequest::new(RegionBounds::new(
                min.offset(-1, -1, -1),
                max.offset(1, 1, 1),
            )),
        );
        let target = analysis.reverse.analysis.scene.components[0].pos;
        let explanation = explain_focused_scene(
            &analysis.reverse.analysis.scene,
            &analysis.hierarchy,
            target,
            true,
        );
        assert!(explanation.input_candidates.is_empty());
        assert!(explanation.output_candidates.is_empty());
        assert!(
            explanation
                .caveats
                .iter()
                .any(|caveat| caveat.contains("flat terminal"))
        );
        assert!(explanation.observation_complete);
    }

    #[test]
    fn live_comparison_is_the_shared_scenario_comparator() {
        let expected = ScenarioTrace {
            duration_redstone_ticks: 2,
            final_powered: BTreeMap::from([(Pos::new(1, 0, 0), true)]),
            ..ScenarioTrace::default()
        };
        let actual = ScenarioTrace {
            duration_redstone_ticks: 2,
            final_powered: BTreeMap::from([(Pos::new(1, 0, 0), false)]),
            ..ScenarioTrace::default()
        };
        assert!(matches!(
            compare_live_trace(&expected, &actual).as_slice(),
            [ScenarioDifference::FinalPowered { .. }]
        ));
    }

    #[test]
    fn generated_scenarios_use_observer_pulse_capability() {
        let snapshot = MinecraftSnapshot {
            min: Pos::new(0, 0, 0),
            max: Pos::new(2, 1, 0),
            blocks: vec![
                MinecraftSnapshotBlock {
                    pos: Pos::new(0, 0, 0),
                    name: "minecraft:stone".into(),
                    properties: BTreeMap::new(),
                },
                MinecraftSnapshotBlock {
                    pos: Pos::new(1, 0, 0),
                    name: "minecraft:stone".into(),
                    properties: BTreeMap::new(),
                },
                MinecraftSnapshotBlock {
                    pos: Pos::new(2, 0, 0),
                    name: "minecraft:stone".into(),
                    properties: BTreeMap::new(),
                },
                MinecraftSnapshotBlock {
                    pos: Pos::new(0, 1, 0),
                    name: "minecraft:lever".into(),
                    properties: BTreeMap::from([
                        ("face".into(), "floor".into()),
                        ("facing".into(), "east".into()),
                        ("powered".into(), "false".into()),
                    ]),
                },
                MinecraftSnapshotBlock {
                    pos: Pos::new(1, 1, 0),
                    name: "minecraft:observer".into(),
                    properties: BTreeMap::from([
                        ("facing".into(), "west".into()),
                        ("powered".into(), "false".into()),
                    ]),
                },
                MinecraftSnapshotBlock {
                    pos: Pos::new(2, 1, 0),
                    name: "minecraft:redstone_wire".into(),
                    properties: BTreeMap::new(),
                },
            ],
        };
        let world = world_from_snapshot(&snapshot).unwrap();
        let analysis = analyze_physical_region(
            &world,
            ReverseRequest::new(RegionBounds::new(snapshot.min, snapshot.max)),
        );
        let scenarios = propose_scenarios(&snapshot, &analysis);
        assert_eq!(scenarios.len(), 1);
        assert_eq!(
            scenarios[0].required_capabilities,
            vec![ScenarioCapability::ObserverPulse]
        );
    }
}
