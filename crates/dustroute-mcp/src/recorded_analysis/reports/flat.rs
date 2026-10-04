use super::common::{CapabilityReport, SignalLiveness, capability_report, signal_liveness};
use super::truth::TruthDiagnostics;
use crate::operations::mutation::Success;
use dustroute_ir::{
    Expr, ExpressionView, FunctionalView, GateView, IrCompleteness, IrDiagnostic,
    SteadyStateProjection, TemporalDevice, TimingAssessment, TransientAssessment, UnresolvedItem,
};
use dustroute_physical::{BlockKind, GapCandidate, Observation, Pos};
use dustroute_translate::analysis::LogicalRole;
use dustroute_translate::api::{ReverseResult, TruthTableSemantics};
use dustroute_translate::world_reverse::{
    InferredTerminal, InterfaceEvidence, RegionBounds, TerminalConfidence, TruthTableRow,
};
use serde::{Serialize, Serializer};
use std::collections::BTreeSet;

#[derive(Clone, Debug, PartialEq, Serialize)]
pub(crate) struct ReverseAnalysisReport {
    ok: Success,
    bounds: RegionBounds,
    redstone_blocks: usize,
    physical: PhysicalReport,
    signal_liveness: SignalLiveness,
    stages: Stages,
    gate_view: GateView,
    expression_view: ExpressionView,
    functional_view: FunctionalView,
    physical_function_model: Option<FunctionModel>,
    functional_validity: TimingAssessment,
    behavior_ir: BehaviorReport,
    inputs: Vec<Terminal>,
    interface_evidence: InterfaceEvidence,
    unsupported_observed_blocks: Vec<UnsupportedBlock>,
    outputs: Vec<Terminal>,
    expressions: Vec<ExpressionText>,
    logical_role: LogicalRole,
    truth_table_semantics: TruthTableSemantics,
    truth_table: Option<Vec<TruthTableRow>>,
    #[serde(flatten)]
    truth_diagnostics: TruthDiagnostics,
    diagnostics: Diagnostics,
}
#[derive(Clone, Debug, PartialEq, Serialize)]
struct PhysicalReport {
    components: usize,
    verified_connections: usize,
    physical_traversal_groups: usize,
    connected_fragments: usize,
    nearby_gap_candidates: Vec<GapCandidate>,
    observation: Observation,
    analysis_complete: bool,
    block_capabilities: CapabilityReport,
}
#[derive(Clone, Debug, PartialEq, Serialize)]
struct Stages {
    observed_world: ObservedWorld,
    physical_scene: PhysicalStage,
    electrical_network: ElectricalStage,
    timed_behavior: TimedStage,
    local_logic: LocalStage,
    functional_candidates: FunctionalStage,
}
#[derive(Clone, Debug, PartialEq, Serialize)]
struct ObservedWorld {
    bounds: RegionBounds,
    redstone_blocks: usize,
}
#[derive(Clone, Debug, PartialEq, Serialize)]
struct PhysicalStage {
    completeness: IrCompleteness,
    components: usize,
    diagnostics: Vec<IrDiagnostic>,
}
#[derive(Clone, Debug, PartialEq, Serialize)]
struct ElectricalStage {
    completeness: IrCompleteness,
    directed_connections: usize,
    unresolved: Vec<UnresolvedItem>,
}
#[derive(Clone, Debug, PartialEq, Serialize)]
struct TimedStage {
    timing: TimingAssessment,
    devices: usize,
    traces: usize,
}
#[derive(Clone, Debug, PartialEq, Serialize)]
struct LocalStage {
    completeness: IrCompleteness,
    cells: GateView,
    expressions: ExpressionView,
}
#[derive(Clone, Debug, PartialEq, Serialize)]
struct FunctionalStage {
    completeness: IrCompleteness,
    functions: FunctionalView,
    unresolved: Vec<UnresolvedItem>,
}
#[derive(Clone, Debug, PartialEq, Serialize)]
struct FunctionModel {
    output_functions: Vec<OutputFunction>,
    shared_physical_components: Vec<Influence>,
    interpretation: &'static str,
}
#[derive(Clone, Debug, PartialEq, Serialize)]
struct OutputFunction {
    output_index: usize,
    position: Pos,
    expression: ExpressionText,
    truth_column: Vec<bool>,
}
#[derive(Clone, Debug, PartialEq, Serialize)]
struct Influence {
    component: usize,
    positions: BTreeSet<Pos>,
    input_dependencies: BTreeSet<usize>,
    output_dependencies: BTreeSet<usize>,
}
#[derive(Clone, Debug, PartialEq, Serialize)]
struct BehaviorReport {
    temporal_devices: Vec<TemporalDevice>,
    trace_count: usize,
    timing: TimingAssessment,
    timed_nodes: usize,
    timed_edges: usize,
    steady_state_projection: SteadyStateProjection,
    transient_assessment: Transients,
}
#[derive(Clone, Debug, PartialEq, Serialize)]
struct Transients {
    status: TransientStatus,
    assessments: Vec<TransientAssessment>,
    guidance: &'static str,
}
#[derive(Clone, Copy, Debug, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
enum TransientStatus {
    NotSimulated,
    ObservedInitialStateOnly,
}
#[derive(Clone, Debug, PartialEq)]
struct ExpressionText(Expr);
impl Serialize for ExpressionText {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.collect_str(&self.0)
    }
}
#[derive(Clone, Debug, PartialEq, Serialize)]
struct Terminal {
    position: Pos,
    component: usize,
    confidence: TerminalLabel,
}
#[derive(Clone, Copy, Debug, PartialEq)]
struct TerminalLabel(TerminalConfidence);
impl Serialize for TerminalLabel {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(match self.0 {
            TerminalConfidence::Certain => "certain",
            TerminalConfidence::Likely => "likely",
        })
    }
}
impl From<&InferredTerminal> for Terminal {
    fn from(t: &InferredTerminal) -> Self {
        Self {
            position: t.anchor,
            component: t.component,
            confidence: TerminalLabel(t.confidence),
        }
    }
}
#[derive(Clone, Debug, PartialEq, Serialize)]
struct UnsupportedBlock {
    position: Pos,
    block: BlockKind,
}
#[derive(Clone, Debug, PartialEq, Serialize)]
struct Diagnostics {
    signal_islands: usize,
    isolated_redstone: usize,
    unreachable_components: usize,
    components_without_output_path: usize,
    invalid_supports: usize,
    non_controllable_torches: usize,
}

pub(crate) fn reverse_report(
    bounds: RegionBounds,
    translated: &ReverseResult,
) -> ReverseAnalysisReport {
    let mut hierarchy = dustroute_ir::hierarchy_from_views(
        &translated.analysis.scene,
        translated.gate_view.clone(),
        translated.expression_view.clone(),
        translated.functional_view.clone(),
    );
    hierarchy.temporal = translated.temporal.clone();
    let scene = &translated.analysis.scene;
    ReverseAnalysisReport {
        ok: Success,
        bounds,
        redstone_blocks: translated.analysis.redstone_blocks.len(),
        physical: PhysicalReport {
            components: scene.components.len(),
            verified_connections: scene.connections.len(),
            physical_traversal_groups: scene.physical_traversal_groups().len(),
            connected_fragments: scene.fragments.len(),
            nearby_gap_candidates: scene.gap_candidates(2),
            observation: scene.observation.clone(),
            analysis_complete: scene.observation.is_complete(),
            block_capabilities: capability_report(scene),
        },
        signal_liveness: signal_liveness(scene, None),
        stages: Stages {
            observed_world: ObservedWorld { bounds, redstone_blocks: translated.analysis.redstone_blocks.len() },
            physical_scene: PhysicalStage {
                completeness: hierarchy.physical_snapshot.completeness,
                components: hierarchy.physical_snapshot.value.scene.components.len(),
                diagnostics: hierarchy.physical_snapshot.diagnostics,
            },
            electrical_network: ElectricalStage {
                completeness: hierarchy.physical_graph.completeness,
                directed_connections: hierarchy.physical_graph.value.scene.connections.len(),
                unresolved: hierarchy.physical_graph.unresolved,
            },
            timed_behavior: TimedStage {
                timing: hierarchy.temporal.timing.clone(),
                devices: hierarchy.temporal.behavior.devices.len(),
                traces: hierarchy.temporal.behavior.traces.len(),
            },
            local_logic: LocalStage {
                completeness: hierarchy.logic_graph.completeness,
                cells: hierarchy.cell_graph.value.cells,
                expressions: hierarchy.logic_graph.value.expressions,
            },
            functional_candidates: FunctionalStage {
                completeness: hierarchy.functional_graph.completeness,
                functions: hierarchy.functional_graph.value.functions,
                unresolved: hierarchy.functional_graph.unresolved,
            },
        },
        gate_view: translated.gate_view.clone(),
        expression_view: translated.expression_view.clone(),
        functional_view: translated.functional_view.clone(),
        physical_function_model: translated.functional_network.as_ref().map(|model| FunctionModel {
            output_functions: model.output_functions.iter().map(|output| OutputFunction {
                output_index: output.output_index,
                position: output.terminal.anchor,
                expression: ExpressionText(output.expression.clone()),
                truth_column: output.truth_column.clone(),
            }).collect(),
            shared_physical_components: model.physical_influences.iter()
                .filter(|influence| influence.shared_role).map(|influence| Influence {
                    component: influence.component,
                    positions: influence.positions.clone(),
                    input_dependencies: influence.input_dependencies.clone(),
                    output_dependencies: influence.output_dependencies.clone(),
                }).collect(),
            interpretation: "Output functions are derived from the shared physical network. Physical components are not assigned exclusive gate identities.",
        }),
        functional_validity: translated.temporal.timing.clone(),
        behavior_ir: BehaviorReport {
            temporal_devices: translated.temporal.behavior.devices.clone(),
            trace_count: translated.temporal.behavior.traces.len(),
            timing: translated.temporal.timing.clone(),
            timed_nodes: translated.temporal.timed_circuit.nodes.len(),
            timed_edges: translated.temporal.timed_circuit.edges.len(),
            steady_state_projection: translated.temporal.steady_state.clone(),
            transient_assessment: Transients {
                status: if translated.temporal.behavior.traces.is_empty() {
                    TransientStatus::NotSimulated
                } else {
                    TransientStatus::ObservedInitialStateOnly
                },
                assessments: translated.temporal.transients.clone(),
                guidance: "hazard_candidate means a measured transient has no registered intent; hazard_confirmed requires an explicit signal contract. Initial-state settling does not cover every input transition.",
            },
        },
        inputs: translated.analysis.inputs.iter().map(Terminal::from).collect(),
        interface_evidence: translated.analysis.interface.clone(),
        unsupported_observed_blocks: translated.analysis.unsupported.iter().map(|(position,block)| UnsupportedBlock {
            position: *position,
            block: *block,
        }).collect(),
        outputs: translated.analysis.outputs.iter().map(Terminal::from).collect(),
        expressions: translated.expressions.iter().cloned().map(ExpressionText).collect(),
        logical_role: dustroute_translate::analysis::derive_local_logic(translated),
        truth_table_semantics: translated.truth_table_semantics,
        truth_table: translated.truth_table.as_ref().map(|table| table.rows.clone()),
        truth_diagnostics: TruthDiagnostics::new(translated),
        diagnostics: Diagnostics {
            signal_islands: translated.analysis.diagnostics.signal_islands.len(),
            isolated_redstone: translated.analysis.diagnostics.isolated_redstone.len(),
            unreachable_components: translated.analysis.diagnostics.unreachable_from_inputs.len(),
            components_without_output_path: translated.analysis.diagnostics.cannot_reach_outputs.len(),
            invalid_supports: translated.analysis.diagnostics.invalid_supports.len(),
            non_controllable_torches: translated.analysis.diagnostics.non_controllable_torches.len(),
        },
    }
}
