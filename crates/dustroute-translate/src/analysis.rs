//! Physical-first analysis facade shared by CLI, MCP, and optimizers.
//!
//! The types in this module deliberately expose every abstraction boundary. A
//! caller can stop at physical evidence without accepting a logical guess.

use std::collections::{BTreeMap, BTreeSet, VecDeque};

use serde::{Deserialize, Serialize};

use crate::{
    BlockKind, InferredTerminal, InferredTruthTable, MinecraftSnapshot, Pos, RegionBounds,
    ReverseRequest, ReverseResult, Scenario, ScenarioAction, ScenarioCapability,
    ScenarioDifference, ScenarioExpectation, ScenarioRun, ScenarioTrace, Translator,
    TruthTableComparison, World, compare_scenario_traces, compare_truth_tables,
    inferred_input_driver, run_scenario, world_from_snapshot,
};

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

#[must_use]
pub fn derive_local_logic(result: &ReverseResult) -> LogicalRole {
    let Some(table) = &result.truth_table else {
        return LogicalRole {
            classification: FunctionalClassification::Unknown,
            output_functions: Vec::new(),
            input_count: result.analysis.inputs.len(),
            output_count: result.analysis.outputs.len(),
            basis: "physical_evidence_only".to_owned(),
            reason: Some(
                result
                    .truth_table_error
                    .as_ref()
                    .map(ToString::to_string)
                    .unwrap_or_else(|| "truth-table inference was not requested".to_owned()),
            ),
        };
    };
    classify_truth_table(table)
}

#[must_use]
pub fn classify_focused_role(result: &ReverseResult, target: Pos) -> FocusedRole {
    let physical_component = result
        .analysis
        .scene
        .component_at(target)
        .map(|component| component.id);
    let component = result
        .analysis
        .components
        .iter()
        .find(|component| component.positions.contains(&target));
    let Some(component) = component else {
        return FocusedRole {
            position: target,
            physical_component,
            signal_component: None,
            incoming_components: BTreeSet::new(),
            outgoing_components: BTreeSet::new(),
            role: LocalSignalRole::SupportOrUnresolved,
        };
    };
    let is_input = result
        .analysis
        .inputs
        .iter()
        .any(|item| item.component == component.id);
    let is_output = result
        .analysis
        .outputs
        .iter()
        .any(|item| item.component == component.id);
    let role = if is_input {
        LocalSignalRole::InputBoundary
    } else if is_output {
        LocalSignalRole::OutputBoundary
    } else if component.incoming.len() > 1 {
        LocalSignalRole::SignalMerge
    } else if component.outgoing.len() > 1 {
        LocalSignalRole::SignalBranch
    } else if component.incoming.contains(&component.id)
        || component.outgoing.contains(&component.id)
    {
        LocalSignalRole::FeedbackPath
    } else {
        LocalSignalRole::IntermediatePath
    };
    FocusedRole {
        position: target,
        physical_component,
        signal_component: Some(component.id),
        incoming_components: component.incoming.clone(),
        outgoing_components: component.outgoing.clone(),
        role,
    }
}

#[must_use]
pub fn classify_truth_table(table: &InferredTruthTable) -> LogicalRole {
    let columns = (0..table.outputs.len())
        .map(|index| {
            table
                .rows
                .iter()
                .map(|row| row.outputs[index])
                .collect::<Vec<_>>()
        })
        .collect::<Vec<_>>();
    let functions = columns
        .iter()
        .map(|column| classify_boolean_column(column))
        .collect::<Vec<_>>();
    let classification = match (table.inputs.len(), functions.as_slice()) {
        (
            2,
            [BooleanFunction::Xor, BooleanFunction::And]
            | [BooleanFunction::And, BooleanFunction::Xor],
        ) => FunctionalClassification::HalfAdder,
        (3, functions) if functions.len() == 2 => {
            let parity = columns
                .iter()
                .any(|column| column == &[false, true, true, false, true, false, false, true]);
            let majority = columns
                .iter()
                .any(|column| column == &[false, false, false, true, false, true, true, true]);
            if parity && majority {
                FunctionalClassification::FullAdder
            } else {
                FunctionalClassification::Unclassified
            }
        }
        (_, [function]) => function_to_classification(*function),
        _ => FunctionalClassification::Unclassified,
    };
    LogicalRole {
        classification,
        output_functions: functions,
        input_count: table.inputs.len(),
        output_count: table.outputs.len(),
        basis: "inferred_truth_table".to_owned(),
        reason: None,
    }
}

#[must_use]
pub fn propose_scenarios(
    snapshot: &MinecraftSnapshot,
    analysis: &PhysicalAnalysis,
) -> Vec<Scenario> {
    // An Observer is not a steady-state source: a lever transition is visible
    // as a one-redstone-tick pulse at its back. Choose the pulse capability for
    // generated scenarios so MCP callers do not receive a scenario that is
    // rejected merely because an Observer is present.
    let required_capability = world_from_snapshot(snapshot)
        .ok()
        .filter(|world| {
            world
                .iter()
                .any(|(_, block)| block.kind == BlockKind::Observer)
        })
        .map_or(ScenarioCapability::SteadyPower, |_| {
            ScenarioCapability::ObserverPulse
        });
    analysis
        .reverse
        .analysis
        .inputs
        .iter()
        .enumerate()
        .filter_map(|(index, input)| {
            Some(Scenario {
                label: format!("toggle inferred input {index}"),
                initial: snapshot.clone(),
                actions: inferred_input_actions(snapshot, &analysis.reverse.analysis, input)?,
                observe: analysis
                    .reverse
                    .analysis
                    .outputs
                    .iter()
                    .map(|output| output.anchor)
                    .collect(),
                duration_redstone_ticks: 10,
                required_capabilities: vec![required_capability],
                expectation: ScenarioExpectation::default(),
            })
        })
        .collect()
}

fn inferred_input_actions(
    snapshot: &MinecraftSnapshot,
    analysis: &crate::RegionAnalysis,
    input: &crate::InferredTerminal,
) -> Option<Vec<ScenarioAction>> {
    let Ok(world) = world_from_snapshot(snapshot) else {
        return None;
    };
    let Ok(driver) = inferred_input_driver(&world, analysis, input) else {
        return None;
    };
    Some(match driver {
        crate::InferredInputDriver::Lever(position) => vec![
            ScenarioAction::SetLeverState {
                redstone_tick: 1,
                position,
                powered: true,
            },
            ScenarioAction::SetLeverState {
                redstone_tick: 5,
                position,
                powered: false,
            },
        ],
        crate::InferredInputDriver::Button(position) => vec![
            ScenarioAction::PressButton {
                redstone_tick: 1,
                position,
            },
            ScenarioAction::ReleaseButton {
                redstone_tick: 5,
                position,
            },
        ],
        crate::InferredInputDriver::PressurePlate(position) => vec![
            ScenarioAction::SetPressurePlateLevel {
                redstone_tick: 1,
                position,
                level: 15,
            },
            ScenarioAction::SetPressurePlateLevel {
                redstone_tick: 5,
                position,
                level: 0,
            },
        ],
        crate::InferredInputDriver::External(position) => vec![
            ScenarioAction::SetExternalPower {
                redstone_tick: 1,
                position,
                powered: true,
            },
            ScenarioAction::SetExternalPower {
                redstone_tick: 5,
                position,
                powered: false,
            },
        ],
    })
}

pub fn simulate_scenario(scenario: &Scenario) -> Result<ScenarioRun, String> {
    run_scenario(scenario)
}

#[must_use]
pub fn compare_live_trace(
    expected: &ScenarioTrace,
    actual: &ScenarioTrace,
) -> Vec<ScenarioDifference> {
    compare_scenario_traces(expected, actual)
}

#[must_use]
pub fn explain_signal_path(analysis: &PhysicalAnalysis, from: Pos, to: Pos) -> SignalPath {
    let scene = &analysis.reverse.analysis.scene;
    let Some(source) = scene.component_at(from).map(|component| component.id) else {
        return missing_path(from, to, "source is not a modeled circuit component");
    };
    let Some(sink) = scene.component_at(to).map(|component| component.id) else {
        return missing_path(from, to, "destination is not a modeled circuit component");
    };
    let mut queue = VecDeque::from([source]);
    let mut previous = BTreeMap::new();
    let mut visited = BTreeSet::from([source]);
    while let Some(current) = queue.pop_front() {
        if current == sink {
            break;
        }
        for connection in scene
            .connections
            .iter()
            .filter(|edge| edge.source.component == current)
        {
            let next = connection.sink.component;
            if visited.insert(next) {
                previous.insert(next, (current, connection.transfer));
                queue.push_back(next);
            }
        }
    }
    if !visited.contains(&sink) {
        return missing_path(from, to, "no directed signal path was found");
    }
    let mut components = vec![sink];
    let mut transfers = Vec::new();
    let mut current = sink;
    while current != source {
        let (parent, transfer) = previous[&current];
        components.push(parent);
        transfers.push(transfer);
        current = parent;
    }
    components.reverse();
    transfers.reverse();
    let positions = components
        .iter()
        .filter_map(|id| scene.components.iter().find(|c| c.id == *id).map(|c| c.pos))
        .collect::<Vec<_>>();
    SignalPath {
        complete: scene.observation.is_complete(),
        explanation: format!(
            "directed path through {} physical components",
            positions.len()
        ),
        positions,
        transfers,
    }
}

/// Builds a small explanation around `target` while retaining the full
/// directed path evidence needed by an LLM to reason about local roles.  Path
/// enumeration is deliberately capped; callers should request a larger
/// region explicitly rather than making a gaze query unbounded.
#[must_use]
pub fn explain_focused_component(
    analysis: &PhysicalAnalysis,
    target: Pos,
    analysis_complete: bool,
) -> FocusedExplanation {
    const MAX_TERMINALS: usize = 16;
    const MAX_PATHS: usize = 8;
    let scene = &analysis.reverse.analysis.scene;
    let role = classify_focused_role(&analysis.reverse, target);
    let component_positions = scene
        .components
        .iter()
        .map(|component| (component.id, component.pos))
        .collect::<BTreeMap<_, _>>();
    let physical_component = scene.component_at(target).map(|component| component.id);
    let (block, observed_name, observed_properties) = scene
        .component_at(target)
        .map(|component| {
            (
                Some(component.block.kind),
                component.block.observed_name.clone(),
                component.block.observed_properties.clone(),
            )
        })
        .unwrap_or((None, None, BTreeMap::new()));
    let adjacent = |incoming: bool| {
        scene
            .connections
            .iter()
            .filter(|connection| {
                physical_component.is_some_and(|component| {
                    if incoming {
                        connection.sink.component == component
                    } else {
                        connection.source.component == component
                    }
                })
            })
            .filter_map(|connection| {
                let source_position = component_positions.get(&connection.source.component)?;
                let sink_position = component_positions.get(&connection.sink.component)?;
                Some(FocusedConnection {
                    source_component: connection.source.component,
                    source_position: *source_position,
                    sink_component: connection.sink.component,
                    sink_position: *sink_position,
                    transfer: connection.transfer,
                    confidence: connection.confidence,
                })
            })
            .collect::<Vec<_>>()
    };
    let input_candidates = analysis
        .reverse
        .analysis
        .inputs
        .iter()
        .take(MAX_TERMINALS)
        .cloned()
        .collect::<Vec<_>>();
    let output_candidates = analysis
        .reverse
        .analysis
        .outputs
        .iter()
        .take(MAX_TERMINALS)
        .cloned()
        .collect::<Vec<_>>();
    let paths_from_inputs = input_candidates
        .iter()
        .take(MAX_PATHS)
        .map(|input| FocusedPath {
            endpoint: input.anchor,
            direction: "input_to_focus".to_owned(),
            path: explain_signal_path(analysis, input.anchor, target),
        })
        .collect::<Vec<_>>();
    let paths_to_outputs = output_candidates
        .iter()
        .take(MAX_PATHS)
        .map(|output| FocusedPath {
            endpoint: output.anchor,
            direction: "focus_to_output".to_owned(),
            path: explain_signal_path(analysis, target, output.anchor),
        })
        .collect::<Vec<_>>();
    let timing = scene.temporal_assessment();
    let temporal_devices = analysis
        .hierarchy
        .temporal
        .behavior
        .devices
        .iter()
        .filter(|device| {
            physical_component.is_some_and(|component| {
                device.component == component
                    || role.incoming_components.contains(&device.component.0)
                    || role.outgoing_components.contains(&device.component.0)
            })
        })
        .take(MAX_TERMINALS)
        .cloned()
        .collect::<Vec<_>>();
    let observation_complete = analysis_complete && scene.observation.is_complete();
    let mut caveats = Vec::new();
    if !observation_complete {
        caveats.push(
            "the focused explanation is bounded by an incomplete or open observation".to_owned(),
        );
    }
    if timing.requirement != dustroute_physical::TemporalRequirement::SteadyStateSafe {
        caveats.push(format!(
            "timing requires {:?}; same-tick order is evidence, not a vanilla scheduler proof",
            timing.requirement
        ));
    }
    if input_candidates.is_empty() {
        caveats.push("no mapped input terminal was found in the observed region".to_owned());
    }
    if output_candidates.is_empty() {
        caveats.push(
            "no mapped observable output terminal was found in the observed region".to_owned(),
        );
    }
    FocusedExplanation {
        position: target,
        block,
        observed_name,
        observed_properties,
        physical_component,
        role,
        incoming: adjacent(true),
        outgoing: adjacent(false),
        input_candidates,
        output_candidates,
        paths_from_inputs,
        paths_to_outputs,
        timing,
        temporal_devices,
        observation_complete,
        caveats,
    }
}

/// Produces the same physical-first shape for the hierarchical fast path.
/// Flat terminal inference is intentionally omitted for large regions, so the
/// returned candidate/path arrays remain empty and the caveat says why.
#[must_use]
pub fn explain_focused_scene(
    scene: &dustroute_physical::PhysicalScene,
    hierarchy: &dustroute_ir::HierarchicalIr,
    target: Pos,
    analysis_complete: bool,
) -> FocusedExplanation {
    let physical_component = scene.component_at(target).map(|component| component.id);
    let component_positions = scene
        .components
        .iter()
        .map(|component| (component.id, component.pos))
        .collect::<BTreeMap<_, _>>();
    let (block, observed_name, observed_properties) = scene
        .component_at(target)
        .map(|component| {
            (
                Some(component.block.kind),
                component.block.observed_name.clone(),
                component.block.observed_properties.clone(),
            )
        })
        .unwrap_or((None, None, BTreeMap::new()));
    let (incoming_components, outgoing_components) = physical_component
        .map(|component| {
            let incoming = scene
                .connections
                .iter()
                .filter(|edge| edge.sink.component == component)
                .map(|edge| edge.source.component.0)
                .collect::<BTreeSet<_>>();
            let outgoing = scene
                .connections
                .iter()
                .filter(|edge| edge.source.component == component)
                .map(|edge| edge.sink.component.0)
                .collect::<BTreeSet<_>>();
            (incoming, outgoing)
        })
        .unwrap_or_default();
    let has_feedback = physical_component.is_some_and(|component| {
        scene
            .connections
            .iter()
            .any(|edge| edge.source.component == component && edge.sink.component == component)
    });
    let role_kind = if physical_component.is_none() {
        LocalSignalRole::SupportOrUnresolved
    } else if incoming_components.len() > 1 {
        LocalSignalRole::SignalMerge
    } else if outgoing_components.len() > 1 {
        LocalSignalRole::SignalBranch
    } else if has_feedback {
        LocalSignalRole::FeedbackPath
    } else if !incoming_components.is_empty() || !outgoing_components.is_empty() {
        LocalSignalRole::IntermediatePath
    } else {
        LocalSignalRole::SupportOrUnresolved
    };
    let role = FocusedRole {
        position: target,
        physical_component,
        signal_component: physical_component.map(|component| component.0),
        incoming_components,
        outgoing_components,
        role: role_kind,
    };
    let connections = |incoming: bool| {
        scene
            .connections
            .iter()
            .filter(|edge| {
                physical_component.is_some_and(|component| {
                    if incoming {
                        edge.sink.component == component
                    } else {
                        edge.source.component == component
                    }
                })
            })
            .filter_map(|edge| {
                Some(FocusedConnection {
                    source_component: edge.source.component,
                    source_position: *component_positions.get(&edge.source.component)?,
                    sink_component: edge.sink.component,
                    sink_position: *component_positions.get(&edge.sink.component)?,
                    transfer: edge.transfer,
                    confidence: edge.confidence,
                })
            })
            .collect::<Vec<_>>()
    };
    let timing = scene.temporal_assessment();
    let temporal_devices = hierarchy
        .temporal
        .behavior
        .devices
        .iter()
        .filter(|device| {
            physical_component.is_some_and(|component| {
                device.component == component
                    || role.incoming_components.contains(&device.component.0)
                    || role.outgoing_components.contains(&device.component.0)
            })
        })
        .take(16)
        .cloned()
        .collect::<Vec<_>>();
    let observation_complete = analysis_complete && scene.observation.is_complete();
    let mut caveats = vec![
        "flat terminal and path inference was skipped for this hierarchical region".to_owned(),
    ];
    if !observation_complete {
        caveats.push(
            "the focused explanation is bounded by an incomplete or open observation".to_owned(),
        );
    }
    if timing.requirement != dustroute_physical::TemporalRequirement::SteadyStateSafe {
        caveats.push(format!(
            "timing requires {:?}; same-tick order is evidence, not a vanilla scheduler proof",
            timing.requirement
        ));
    }
    FocusedExplanation {
        position: target,
        block,
        observed_name,
        observed_properties,
        physical_component,
        role,
        incoming: connections(true),
        outgoing: connections(false),
        input_candidates: Vec::new(),
        output_candidates: Vec::new(),
        paths_from_inputs: Vec::new(),
        paths_to_outputs: Vec::new(),
        timing,
        temporal_devices,
        observation_complete,
        caveats,
    }
}

#[must_use]
pub fn verify_semantic_equivalence(
    expected: &PhysicalAnalysis,
    actual: &PhysicalAnalysis,
) -> SemanticEquivalence {
    match (&expected.reverse.truth_table, &actual.reverse.truth_table) {
        (Some(expected), Some(actual)) => {
            let comparison = compare_truth_tables(expected, actual);
            SemanticEquivalence {
                equivalent: comparison.comparable && comparison.fitness_penalty == 0,
                reason: if comparison.comparable && comparison.fitness_penalty == 0 {
                    "truth tables are identical".to_owned()
                } else {
                    "truth tables differ or have incompatible terminals".to_owned()
                },
                comparison: Some(comparison),
            }
        }
        _ => SemanticEquivalence {
            equivalent: false,
            comparison: None,
            reason: "both analyses require inferred truth tables".to_owned(),
        },
    }
}

fn classify_boolean_column(values: &[bool]) -> BooleanFunction {
    match values {
        [false, true] => BooleanFunction::Buffer,
        [true, false] => BooleanFunction::Not,
        [false, false, false, true] => BooleanFunction::And,
        [false, true, true, true] => BooleanFunction::Or,
        [false, true, true, false] => BooleanFunction::Xor,
        [true, true, true, false] => BooleanFunction::Nand,
        [true, false, false, false] => BooleanFunction::Nor,
        [true, false, false, true] => BooleanFunction::Xnor,
        _ => BooleanFunction::Unclassified,
    }
}

const fn function_to_classification(function: BooleanFunction) -> FunctionalClassification {
    match function {
        BooleanFunction::Buffer => FunctionalClassification::Buffer,
        BooleanFunction::Not => FunctionalClassification::Not,
        BooleanFunction::And => FunctionalClassification::And,
        BooleanFunction::Or => FunctionalClassification::Or,
        BooleanFunction::Xor => FunctionalClassification::Xor,
        BooleanFunction::Nand => FunctionalClassification::Nand,
        BooleanFunction::Nor => FunctionalClassification::Nor,
        BooleanFunction::Xnor => FunctionalClassification::Xnor,
        BooleanFunction::Unclassified => FunctionalClassification::Unclassified,
    }
}

fn missing_path(from: Pos, to: Pos, reason: &str) -> SignalPath {
    SignalPath {
        positions: vec![from, to],
        transfers: Vec::new(),
        complete: false,
        explanation: reason.to_owned(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{ForwardOptions, MinecraftSnapshotBlock, half_adder};

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

    #[test]
    fn recognizes_primitive_boolean_columns() {
        assert_eq!(
            classify_boolean_column(&[false, false, false, true]),
            BooleanFunction::And
        );
        assert_eq!(
            classify_boolean_column(&[false, true, true, true]),
            BooleanFunction::Or
        );
        assert_eq!(
            classify_boolean_column(&[false, true, true, false]),
            BooleanFunction::Xor
        );
        assert_eq!(
            classify_boolean_column(&[true, false]),
            BooleanFunction::Not
        );
        assert_eq!(
            classify_boolean_column(&[false, false, true, false]),
            BooleanFunction::Unclassified
        );
    }
}
