use super::{
    MAX_FLAT_ANALYSIS_COMPONENTS,
    common::{
        CapabilityReport, FocusedHierarchy, SignalLiveness, capability_report, signal_liveness,
    },
};
use crate::operations::mutation::Success;
use crate::recorded_analysis::RecordedExpansion;
use dustroute_ir::{
    FunctionalView, HierarchicalIr, IrCompleteness, IrDiagnostic, MixedEdge, MixedNode,
    MixedNodeKind, TimingAssessment, TransientFinding, UnresolvedItem,
};
use dustroute_physical::Pos;
use dustroute_translate::{analysis::FocusedExplanation, world_reverse::RegionBounds};
use serde::Serialize;
use std::collections::BTreeMap;

#[derive(Clone, Debug, PartialEq, Serialize)]
pub(crate) struct HierarchicalAnalysisReport {
    ok: Success,
    analysis_mode: &'static str,
    bounds: RegionBounds,
    analysis_complete: bool,
    focused_component: Option<FocusedHierarchy>,
    focused_explanation: Option<FocusedExplanation>,
    expansion: RecordedExpansion,
    block_capabilities: CapabilityReport,
    signal_liveness: SignalLiveness,
    stages: Stages,
    temporal: Temporal,
    truth_table: (),
    truth_table_status: SkippedStatus,
    truth_table_skip: TruthSkip,
    truth_table_skipped: &'static str,
}
#[derive(Clone, Debug, PartialEq, Serialize)]
struct Stages {
    physical_snapshot: SnapshotStage,
    physical_graph: GraphStage,
    cell_graph: CellStage,
    logic_graph: LogicStage,
    mixed_ir: MixedStage,
    functional_graph: FunctionStage,
}
#[derive(Clone, Debug, PartialEq, Serialize)]
struct SnapshotStage {
    completeness: IrCompleteness,
    components: usize,
    diagnostic_count: usize,
    diagnostics: Vec<IrDiagnostic>,
    diagnostics_truncated: bool,
}
#[derive(Clone, Debug, PartialEq, Serialize)]
struct GraphStage {
    completeness: IrCompleteness,
    directed_connections: usize,
    physical_traversal_groups: usize,
    fragments: usize,
    unresolved: Vec<UnresolvedItem>,
}
#[derive(Clone, Debug, PartialEq, Serialize)]
struct CellStage {
    completeness: IrCompleteness,
    cell_count: usize,
    unresolved_component_count: usize,
    detail: &'static str,
}
#[derive(Clone, Debug, PartialEq, Serialize)]
struct LogicStage {
    completeness: IrCompleteness,
    expression_count: usize,
    detail: &'static str,
}
#[derive(Clone, Debug, PartialEq, Serialize)]
struct MixedStage {
    physical_component_count: usize,
    recognized_component_count: usize,
    unresolved_component_count: usize,
    node_count: usize,
    edge_count: usize,
    representation_counts: BTreeMap<Representation, usize>,
    nodes: Vec<MixedNode>,
    edges: Vec<MixedEdge>,
}
#[derive(Clone, Copy, Debug, Eq, PartialEq, Ord, PartialOrd, Serialize)]
#[serde(rename_all = "snake_case")]
enum Representation {
    LogicGate,
    TimedCell,
    PhysicalRegion,
    Boundary,
}
#[derive(Clone, Debug, PartialEq, Serialize)]
struct FunctionStage {
    completeness: IrCompleteness,
    functions: FunctionalView,
    validity: TimingAssessment,
}
#[derive(Clone, Debug, PartialEq, Serialize)]
struct Temporal {
    timing: TimingAssessment,
    timed_nodes: usize,
    timed_edges: usize,
    steady_state_retained_components: usize,
    steady_state_compressed_components: usize,
    steady_state_edges: usize,
    transient_assessment: TransientSummary,
}
#[derive(Clone, Debug, PartialEq, Serialize)]
struct TransientSummary {
    status: &'static str,
    findings: Vec<TransientFinding>,
    guidance: &'static str,
}
#[derive(Clone, Copy, Debug, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
enum SkippedStatus {
    SkippedLargeCircuit,
}
#[derive(Clone, Debug, PartialEq, Serialize)]
struct TruthSkip {
    code: &'static str,
    component_count: Option<usize>,
    threshold: usize,
    guidance: &'static str,
}

pub(crate) fn hierarchical_report(
    bounds: RegionBounds,
    hierarchy: &HierarchicalIr,
    focused: Option<FocusedHierarchy>,
    expansion: RecordedExpansion,
    focus: Option<Pos>,
) -> HierarchicalAnalysisReport {
    let scene = &hierarchy.physical_graph.value.scene;
    let mixed = dustroute_ir::build_mixed_ir(hierarchy);
    let mut counts = BTreeMap::new();
    for node in &mixed.nodes {
        let representation = match node.kind {
            MixedNodeKind::LogicGate { .. } => Representation::LogicGate,
            MixedNodeKind::TimedCell { .. } => Representation::TimedCell,
            MixedNodeKind::PhysicalRegion => Representation::PhysicalRegion,
            MixedNodeKind::Boundary { .. } => Representation::Boundary,
        };
        *counts.entry(representation).or_insert(0_usize) += 1;
    }
    let complete = !expansion.limit_reached() && scene.observation.is_complete();
    HierarchicalAnalysisReport {
        ok: Success,
        analysis_mode: "hierarchical_local_first",
        bounds,
        analysis_complete: !expansion.limit_reached(),
        focused_component: focused,
        focused_explanation: focus.map(|target| {
            dustroute_translate::analysis::explain_focused_scene(scene, hierarchy, target, complete)
        }),
        block_capabilities: capability_report(scene),
        signal_liveness: signal_liveness(scene, focus),
        stages: Stages {
            physical_snapshot: SnapshotStage {
                completeness: hierarchy.physical_snapshot.completeness,
                components: scene.components.len(),
                diagnostic_count: hierarchy.physical_snapshot.diagnostics.len(),
                diagnostics: hierarchy
                    .physical_snapshot
                    .diagnostics
                    .iter()
                    .take(16)
                    .cloned()
                    .collect(),
                diagnostics_truncated: hierarchy.physical_snapshot.diagnostics.len() > 16,
            },
            physical_graph: GraphStage {
                completeness: hierarchy.physical_graph.completeness,
                directed_connections: scene.connections.len(),
                physical_traversal_groups: scene.physical_traversal_groups().len(),
                fragments: scene.fragments.len(),
                unresolved: hierarchy.physical_graph.unresolved.clone(),
            },
            cell_graph: CellStage {
                completeness: hierarchy.cell_graph.completeness,
                cell_count: hierarchy.cell_graph.value.cells.gates.len(),
                unresolved_component_count: hierarchy.cell_graph.unresolved.len(),
                detail: "represented by mixed_ir node references; recursive cell payload omitted",
            },
            logic_graph: LogicStage {
                completeness: hierarchy.logic_graph.completeness,
                expression_count: hierarchy.logic_graph.value.expressions.expressions.len(),
                detail: "recursive expressions omitted; follow mixed_ir edges by node id",
            },
            mixed_ir: MixedStage {
                physical_component_count: mixed.physical_component_count,
                recognized_component_count: mixed.recognized_component_count,
                unresolved_component_count: mixed.unresolved_component_count,
                node_count: mixed.nodes.len(),
                edge_count: mixed.edges.len(),
                representation_counts: counts,
                nodes: mixed.nodes,
                edges: mixed.edges,
            },
            functional_graph: FunctionStage {
                completeness: hierarchy.functional_graph.completeness,
                functions: hierarchy.functional_graph.value.functions.clone(),
                validity: hierarchy.temporal.timing.clone(),
            },
        },
        temporal: Temporal {
            timing: hierarchy.temporal.timing.clone(),
            timed_nodes: hierarchy.temporal.timed_circuit.nodes.len(),
            timed_edges: hierarchy.temporal.timed_circuit.edges.len(),
            steady_state_retained_components: hierarchy
                .temporal
                .steady_state
                .retained_components
                .len(),
            steady_state_compressed_components: hierarchy
                .temporal
                .steady_state
                .compressed_components
                .len(),
            steady_state_edges: hierarchy.temporal.steady_state.edges.len(),
            transient_assessment: TransientSummary {
                status: "not_simulated",
                findings: vec![],
                guidance: "timing risk is structural only; run transition scenarios before claiming that a pulse was observed",
            },
        },
        truth_table: (),
        truth_table_status: SkippedStatus::SkippedLargeCircuit,
        truth_table_skip: TruthSkip {
            code: "flat_analysis_component_threshold",
            component_count: expansion.components_loaded(),
            threshold: MAX_FLAT_ANALYSIS_COMPONENTS,
            guidance: "set include_truth_table=true to request bounded exhaustive simulation",
        },
        truth_table_skipped: "large circuits use local cells and hierarchical summaries instead of a flat whole-circuit truth table",
        expansion,
    }
}
