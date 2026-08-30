use std::collections::{BTreeMap, BTreeSet};

use dustroute_physical::{
    CapabilityLevel, CapabilityStage, ComponentId, PhysicalDiagnostic, PhysicalScene, Pos,
};
use serde::{Deserialize, Serialize};

use crate::{
    ExpressionView, FunctionalView, GateView, RecognitionStatus, TemporalAnalysis,
    classify_function, derive_expressions, recognize_gates,
};

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum IrStage {
    PhysicalSnapshot,
    PhysicalGraph,
    CellGraph,
    LogicGraph,
    FunctionalGraph,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum IrCompleteness {
    Complete,
    Partial,
    OpenBoundary,
    LimitReached,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum DiagnosticSeverity {
    Information,
    Warning,
    Error,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct IrDiagnostic {
    pub stage: IrStage,
    pub severity: DiagnosticSeverity,
    pub code: String,
    pub message: String,
    pub physical_components: BTreeSet<ComponentId>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct UnresolvedItem {
    pub stage: IrStage,
    pub entity: String,
    pub reason: String,
    pub physical_components: BTreeSet<ComponentId>,
}

#[derive(Clone, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
pub struct ProvenanceMap {
    /// Stable, stage-qualified entity name to canonical physical component IDs.
    pub physical_components_by_entity: BTreeMap<String, BTreeSet<ComponentId>>,
    /// Canonical physical component ID to its observed Minecraft position.
    pub physical_positions: BTreeMap<ComponentId, Pos>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct TransformResult<T> {
    pub value: T,
    pub completeness: IrCompleteness,
    pub diagnostics: Vec<IrDiagnostic>,
    pub unresolved: Vec<UnresolvedItem>,
    pub provenance: ProvenanceMap,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct PhysicalSnapshot {
    /// The canonical observed scene. No logical inference is introduced here.
    pub scene: PhysicalScene,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct PhysicalGraph {
    /// Port-level, directed physical connectivity remains authoritative.
    pub scene: PhysicalScene,
}

#[derive(Clone, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
pub struct CellGraph {
    pub cells: GateView,
}

#[derive(Clone, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
pub struct LogicGraph {
    pub expressions: ExpressionView,
}

#[derive(Clone, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
pub struct FunctionalGraph {
    pub functions: FunctionalView,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct HierarchicalIr {
    pub physical_snapshot: TransformResult<PhysicalSnapshot>,
    pub physical_graph: TransformResult<PhysicalGraph>,
    pub cell_graph: TransformResult<CellGraph>,
    pub logic_graph: TransformResult<LogicGraph>,
    pub functional_graph: TransformResult<FunctionalGraph>,
    /// Orthogonal timing view retained alongside every abstraction stage.
    pub temporal: TemporalAnalysis,
}

#[must_use]
pub fn build_physical_snapshot(scene: &PhysicalScene) -> TransformResult<PhysicalSnapshot> {
    let provenance = physical_provenance(scene);
    let (completeness, diagnostics) = physical_completeness(scene, IrStage::PhysicalSnapshot);
    TransformResult {
        value: PhysicalSnapshot {
            scene: scene.clone(),
        },
        completeness,
        diagnostics,
        unresolved: Vec::new(),
        provenance,
    }
}

#[must_use]
pub fn build_physical_graph(
    snapshot: &TransformResult<PhysicalSnapshot>,
) -> TransformResult<PhysicalGraph> {
    let mut scene = snapshot.value.scene.clone();
    for component in &mut scene.components {
        component.support = None;
    }
    scene.connections.retain(|connection| {
        connection.transfer != dustroute_physical::TransferKind::StructuralSupport
    });
    let mut unresolved = Vec::new();
    for diagnostic in &scene.diagnostics {
        if let PhysicalDiagnostic::AmbiguousConnection { components, reason } = diagnostic {
            unresolved.push(UnresolvedItem {
                stage: IrStage::PhysicalGraph,
                entity: format!("connection:{}:{}", components[0].0, components[1].0),
                reason: reason.clone(),
                physical_components: components.iter().copied().collect(),
            });
        }
    }
    let capability_issues = scene
        .capability_report()
        .issues
        .into_iter()
        .filter(|issue| {
            matches!(
                issue.stage,
                CapabilityStage::PhysicalClassification | CapabilityStage::Connectivity
            )
        })
        .collect::<Vec<_>>();
    unresolved.extend(capability_issues.iter().map(|issue| UnresolvedItem {
        stage: IrStage::PhysicalGraph,
        entity: format!("component:{}", issue.component.0),
        reason: format!(
            "{:?} capability is {:?} for {}",
            issue.stage,
            issue.level,
            issue.observed_name.as_deref().unwrap_or("synthetic block")
        ),
        physical_components: BTreeSet::from([issue.component]),
    }));
    let mut diagnostics = snapshot.diagnostics.clone();
    diagnostics.extend(capability_issues.iter().map(|issue| IrDiagnostic {
        stage: IrStage::PhysicalGraph,
        severity: DiagnosticSeverity::Warning,
        code: "block_capability_limited".to_owned(),
        message: format!(
            "{} at {:?} has {:?} {:?} support",
            issue.observed_name.as_deref().unwrap_or("synthetic block"),
            issue.position,
            issue.level,
            issue.stage
        ),
        physical_components: BTreeSet::from([issue.component]),
    }));
    TransformResult {
        value: PhysicalGraph { scene },
        completeness: if unresolved.is_empty() {
            snapshot.completeness
        } else {
            IrCompleteness::Partial
        },
        diagnostics,
        unresolved,
        provenance: snapshot.provenance.clone(),
    }
}

#[must_use]
pub fn build_cell_graph(physical: &TransformResult<PhysicalGraph>) -> TransformResult<CellGraph> {
    let cells = recognize_gates(&physical.value.scene);
    let mut result = cell_result(physical, cells);
    add_steady_state_capabilities(&physical.value.scene, &mut result);
    result
}

fn cell_result(
    physical: &TransformResult<PhysicalGraph>,
    cells: GateView,
) -> TransformResult<CellGraph> {
    let unresolved = cells
        .unresolved_components
        .iter()
        .map(|component| UnresolvedItem {
            stage: IrStage::CellGraph,
            entity: format!("component:{}", component.0),
            reason: "no supported local cell pattern covers this physical component".to_owned(),
            physical_components: BTreeSet::from([*component]),
        })
        .collect::<Vec<_>>();
    let mut provenance = physical.provenance.clone();
    for cell in &cells.gates {
        provenance.physical_components_by_entity.insert(
            format!("cell:{}", cell.id.0),
            cell.physical_components.clone(),
        );
    }
    TransformResult {
        completeness: derived_completeness(
            physical.completeness,
            !unresolved.is_empty(),
            cells
                .gates
                .iter()
                .any(|cell| cell.status != RecognitionStatus::Complete),
        ),
        diagnostics: physical.diagnostics.clone(),
        unresolved,
        provenance,
        value: CellGraph { cells },
    }
}

#[must_use]
pub fn build_logic_graph(
    physical: &TransformResult<PhysicalGraph>,
    cells: &TransformResult<CellGraph>,
) -> TransformResult<LogicGraph> {
    let expressions = derive_expressions(&physical.value.scene, &cells.value.cells);
    let mut result = logic_result(cells, expressions);
    add_temporal_capabilities(&physical.value.scene, &mut result);
    result
}

fn add_steady_state_capabilities(scene: &PhysicalScene, result: &mut TransformResult<CellGraph>) {
    let issues = scene
        .capability_report()
        .issues
        .into_iter()
        .filter(|issue| {
            issue.stage == CapabilityStage::SteadyState
                && issue.level == CapabilityLevel::Unsupported
        });
    for issue in issues {
        result.completeness = derived_completeness(result.completeness, true, false);
        result.unresolved.push(UnresolvedItem {
            stage: IrStage::CellGraph,
            entity: format!("component:{}", issue.component.0),
            reason: format!(
                "steady-state semantics are unsupported for {}",
                issue.observed_name.as_deref().unwrap_or("synthetic block")
            ),
            physical_components: BTreeSet::from([issue.component]),
        });
        result.diagnostics.push(IrDiagnostic {
            stage: IrStage::CellGraph,
            severity: DiagnosticSeverity::Warning,
            code: "steady_state_semantics_unsupported".to_owned(),
            message: format!(
                "steady-state semantics are unavailable at {:?}",
                issue.position
            ),
            physical_components: BTreeSet::from([issue.component]),
        });
    }
}

fn add_temporal_capabilities(scene: &PhysicalScene, result: &mut TransformResult<LogicGraph>) {
    for issue in scene
        .capability_report()
        .issues
        .into_iter()
        .filter(|issue| issue.stage == CapabilityStage::Temporal)
    {
        result.diagnostics.push(IrDiagnostic {
            stage: IrStage::LogicGraph,
            severity: DiagnosticSeverity::Information,
            code: match issue.level {
                CapabilityLevel::Partial => "temporal_semantics_partial",
                CapabilityLevel::Unsupported => "temporal_semantics_unsupported",
                CapabilityLevel::Full | CapabilityLevel::NotApplicable => continue,
            }
            .to_owned(),
            message: format!(
                "temporal semantics are {:?} for {} at {:?}; steady-state interpretation remains separate",
                issue.level,
                issue.observed_name.as_deref().unwrap_or("synthetic block"),
                issue.position
            ),
            physical_components: BTreeSet::from([issue.component]),
        });
    }
}

fn logic_result(
    cells: &TransformResult<CellGraph>,
    expressions: ExpressionView,
) -> TransformResult<LogicGraph> {
    let unresolved = expressions
        .expressions
        .iter()
        .filter(|expression| expression.status != RecognitionStatus::Complete)
        .map(|expression| UnresolvedItem {
            stage: IrStage::LogicGraph,
            entity: format!("expression:{}", expression.id.0),
            reason: "the source cell is partial, conflicting, or boundary-limited".to_owned(),
            physical_components: expression.physical_components.clone(),
        })
        .collect::<Vec<_>>();
    let mut provenance = cells.provenance.clone();
    for expression in &expressions.expressions {
        provenance.physical_components_by_entity.insert(
            format!("logic:{}", expression.id.0),
            expression.physical_components.clone(),
        );
    }
    TransformResult {
        completeness: derived_completeness(cells.completeness, !unresolved.is_empty(), false),
        diagnostics: cells.diagnostics.clone(),
        unresolved,
        provenance,
        value: LogicGraph { expressions },
    }
}

#[must_use]
pub fn build_functional_graph(
    cells: &TransformResult<CellGraph>,
    logic: &TransformResult<LogicGraph>,
) -> TransformResult<FunctionalGraph> {
    const MAX_EAGER_FUNCTIONAL_CELLS: usize = 32;
    if cells.value.cells.gates.len() > MAX_EAGER_FUNCTIONAL_CELLS {
        let unresolved = vec![UnresolvedItem {
            stage: IrStage::FunctionalGraph,
            entity: "whole_observed_circuit".to_owned(),
            reason: format!(
                "{} local cells exceed the eager functional-classification limit of {}; classify selected subgraphs instead",
                cells.value.cells.gates.len(),
                MAX_EAGER_FUNCTIONAL_CELLS
            ),
            physical_components: cells
                .value
                .cells
                .gates
                .iter()
                .flat_map(|cell| cell.physical_components.iter().copied())
                .collect(),
        }];
        return TransformResult {
            value: FunctionalGraph::default(),
            completeness: match logic.completeness {
                IrCompleteness::OpenBoundary => IrCompleteness::OpenBoundary,
                IrCompleteness::LimitReached => IrCompleteness::LimitReached,
                _ => IrCompleteness::Partial,
            },
            diagnostics: logic.diagnostics.clone(),
            unresolved,
            provenance: logic.provenance.clone(),
        };
    }
    let functions = classify_function(&cells.value.cells, &logic.value.expressions);
    functional_result(cells, logic, functions)
}

fn functional_result(
    cells: &TransformResult<CellGraph>,
    logic: &TransformResult<LogicGraph>,
    functions: FunctionalView,
) -> TransformResult<FunctionalGraph> {
    let unresolved = functions
        .candidates
        .iter()
        .enumerate()
        .filter(|(_, candidate)| candidate.status != RecognitionStatus::Complete)
        .map(|(index, candidate)| UnresolvedItem {
            stage: IrStage::FunctionalGraph,
            entity: format!("function:{index}"),
            reason: candidate
                .missing_features
                .iter()
                .chain(&candidate.conflicts)
                .cloned()
                .collect::<Vec<_>>()
                .join("; "),
            physical_components: candidate
                .covered_gates
                .iter()
                .filter_map(|gate| cells.value.cells.gates.get(gate.0))
                .flat_map(|cell| cell.physical_components.iter().copied())
                .collect(),
        })
        .collect::<Vec<_>>();
    let mut provenance = logic.provenance.clone();
    for (index, candidate) in functions.candidates.iter().enumerate() {
        let components = candidate
            .covered_gates
            .iter()
            .filter_map(|gate| cells.value.cells.gates.get(gate.0))
            .flat_map(|cell| cell.physical_components.iter().copied())
            .collect();
        provenance
            .physical_components_by_entity
            .insert(format!("function:{index}"), components);
    }
    TransformResult {
        completeness: derived_completeness(
            logic.completeness,
            functions.candidates.is_empty() || !unresolved.is_empty(),
            false,
        ),
        diagnostics: logic.diagnostics.clone(),
        unresolved,
        provenance,
        value: FunctionalGraph { functions },
    }
}

#[must_use]
pub fn derive_hierarchy(scene: &PhysicalScene) -> HierarchicalIr {
    let physical_snapshot = build_physical_snapshot(scene);
    let physical_graph = build_physical_graph(&physical_snapshot);
    let cell_graph = build_cell_graph(&physical_graph);
    let logic_graph = build_logic_graph(&physical_graph, &cell_graph);
    let functional_graph = build_functional_graph(&cell_graph, &logic_graph);
    HierarchicalIr {
        physical_snapshot,
        physical_graph,
        cell_graph,
        logic_graph,
        functional_graph,
        temporal: TemporalAnalysis::from_scene(scene),
    }
}

/// Builds the typed hierarchy from views already produced by a translator.
/// This avoids repeating gate recognition and expression derivation on callers
/// that also perform truth-table enrichment.
#[must_use]
pub fn hierarchy_from_views(
    scene: &PhysicalScene,
    cells: GateView,
    expressions: ExpressionView,
    functions: FunctionalView,
) -> HierarchicalIr {
    let physical_snapshot = build_physical_snapshot(scene);
    let physical_graph = build_physical_graph(&physical_snapshot);
    let mut cell_graph = cell_result(&physical_graph, cells);
    add_steady_state_capabilities(&physical_graph.value.scene, &mut cell_graph);
    let mut logic_graph = logic_result(&cell_graph, expressions);
    add_temporal_capabilities(&physical_graph.value.scene, &mut logic_graph);
    let functional_graph = functional_result(&cell_graph, &logic_graph, functions);
    HierarchicalIr {
        physical_snapshot,
        physical_graph,
        cell_graph,
        logic_graph,
        functional_graph,
        temporal: TemporalAnalysis::from_scene(scene),
    }
}

fn physical_provenance(scene: &PhysicalScene) -> ProvenanceMap {
    ProvenanceMap {
        physical_components_by_entity: scene
            .components
            .iter()
            .map(|component| {
                (
                    format!("physical:{}", component.id.0),
                    BTreeSet::from([component.id]),
                )
            })
            .collect(),
        physical_positions: scene
            .components
            .iter()
            .map(|component| (component.id, component.pos))
            .collect(),
    }
}

fn physical_completeness(
    scene: &PhysicalScene,
    stage: IrStage,
) -> (IrCompleteness, Vec<IrDiagnostic>) {
    let diagnostics: Vec<IrDiagnostic> = scene
        .diagnostics
        .iter()
        .map(|diagnostic| IrDiagnostic {
            stage,
            severity: DiagnosticSeverity::Warning,
            code: match diagnostic {
                PhysicalDiagnostic::OpenObservationBoundary { .. } => "open_observation_boundary",
                PhysicalDiagnostic::InvalidSupport { .. } => "invalid_support",
                PhysicalDiagnostic::AmbiguousConnection { .. } => "ambiguous_connection",
            }
            .to_owned(),
            message: format!("{diagnostic:?}"),
            physical_components: match diagnostic {
                PhysicalDiagnostic::OpenObservationBoundary { .. } => BTreeSet::new(),
                PhysicalDiagnostic::InvalidSupport { component, .. } => {
                    BTreeSet::from([*component])
                }
                PhysicalDiagnostic::AmbiguousConnection { components, .. } => {
                    components.iter().copied().collect()
                }
            },
        })
        .collect();
    let completeness = if !scene.observation.is_complete() {
        IrCompleteness::OpenBoundary
    } else if diagnostics.is_empty() {
        IrCompleteness::Complete
    } else {
        IrCompleteness::Partial
    };
    (completeness, diagnostics)
}

const fn derived_completeness(
    upstream: IrCompleteness,
    has_unresolved: bool,
    has_incomplete_entity: bool,
) -> IrCompleteness {
    match upstream {
        IrCompleteness::OpenBoundary => IrCompleteness::OpenBoundary,
        IrCompleteness::LimitReached => IrCompleteness::LimitReached,
        IrCompleteness::Partial => IrCompleteness::Partial,
        IrCompleteness::Complete if has_unresolved || has_incomplete_entity => {
            IrCompleteness::Partial
        }
        IrCompleteness::Complete => IrCompleteness::Complete,
    }
}

#[cfg(test)]
mod tests {
    use dustroute_physical::{
        Block, BlockKind, ComponentId, Facing, FrontierReason, Observation, ObservationFrontier,
        PhysicalComponent, PhysicalScene, SceneBounds, VerifiedTopology,
    };

    use super::*;

    #[test]
    fn hierarchy_preserves_physical_origin_through_local_logic() {
        let mut torch = Block::new(BlockKind::RedstoneTorch);
        torch.facing = Some(Facing::Up);
        torch.support_offset = Some(Pos::new(0, -1, 0));
        let topology = VerifiedTopology::from_parts(
            vec![
                PhysicalComponent {
                    id: ComponentId(0),
                    pos: Pos::new(0, 0, 0),
                    block: Block::new(BlockKind::Solid),
                },
                PhysicalComponent {
                    id: ComponentId(1),
                    pos: Pos::new(0, 1, 0),
                    block: torch,
                },
            ],
            [],
        );
        let scene = PhysicalScene::from_unvalidated_topology(
            Observation::complete(
                "minecraft:overworld",
                SceneBounds::new(Pos::new(0, 0, 0), Pos::new(0, 1, 0)),
            ),
            &topology,
        );
        let hierarchy = derive_hierarchy(&scene);
        assert_eq!(
            hierarchy.physical_graph.completeness,
            IrCompleteness::Complete
        );
        assert!(
            hierarchy.physical_snapshot.value.scene.components[1]
                .support
                .is_some()
        );
        assert!(
            hierarchy.physical_graph.value.scene.components[1]
                .support
                .is_none()
        );
        assert_eq!(hierarchy.cell_graph.value.cells.gates.len(), 1);
        assert!(
            hierarchy
                .logic_graph
                .provenance
                .physical_components_by_entity
                .contains_key("logic:0")
        );
    }

    #[test]
    fn broken_or_boundary_limited_circuit_remains_a_partial_result() {
        let topology = VerifiedTopology::from_parts(
            vec![PhysicalComponent {
                id: ComponentId(0),
                pos: Pos::new(0, 1, 0),
                block: Block::new(BlockKind::RedstoneWire),
            }],
            [],
        );
        let observation = Observation {
            dimension: "minecraft:overworld".to_owned(),
            regions: Vec::new(),
            frontier: vec![ObservationFrontier {
                position: Pos::new(0, 1, 0),
                direction: Facing::East,
                reason: FrontierReason::ScanLimitReached,
            }],
        };
        let hierarchy = derive_hierarchy(&PhysicalScene::from_unvalidated_topology(
            observation,
            &topology,
        ));
        assert_eq!(
            hierarchy.physical_snapshot.completeness,
            IrCompleteness::OpenBoundary
        );
        assert_eq!(
            hierarchy.cell_graph.completeness,
            IrCompleteness::OpenBoundary
        );
        assert!(!hierarchy.cell_graph.unresolved.is_empty());
    }

    #[test]
    fn invalid_physical_support_prevents_a_complete_claim() {
        let mut wire = Block::new(BlockKind::RedstoneWire);
        wire.support_offset = Some(Pos::new(0, -1, 0));
        let topology = VerifiedTopology::from_parts(
            vec![PhysicalComponent {
                id: ComponentId(0),
                pos: Pos::new(0, 1, 0),
                block: wire,
            }],
            [],
        );
        let scene = PhysicalScene::from_unvalidated_topology(
            Observation::complete(
                "minecraft:overworld",
                SceneBounds::new(Pos::new(0, 0, 0), Pos::new(0, 1, 0)),
            ),
            &topology,
        );
        let hierarchy = derive_hierarchy(&scene);
        assert_eq!(
            hierarchy.physical_snapshot.completeness,
            IrCompleteness::Partial
        );
        assert_eq!(hierarchy.physical_snapshot.diagnostics.len(), 1);
    }

    #[test]
    fn unsupported_semantics_are_reported_at_their_actual_stage() {
        let mut piston = Block::new(BlockKind::Piston);
        piston.observed_name = Some("minecraft:piston".to_owned());
        let topology = VerifiedTopology::from_parts(
            vec![PhysicalComponent {
                id: ComponentId(0),
                pos: Pos::new(0, 1, 0),
                block: piston,
            }],
            [],
        );
        let scene = PhysicalScene::from_unvalidated_topology(
            Observation::complete(
                "minecraft:overworld",
                SceneBounds::new(Pos::new(0, 0, 0), Pos::new(0, 2, 0)),
            ),
            &topology,
        );
        let hierarchy = derive_hierarchy(&scene);
        assert_eq!(
            hierarchy.physical_graph.completeness,
            IrCompleteness::Partial
        );
        assert!(
            hierarchy
                .physical_graph
                .diagnostics
                .iter()
                .any(|diagnostic| { diagnostic.code == "block_capability_limited" })
        );
        assert!(
            hierarchy
                .cell_graph
                .diagnostics
                .iter()
                .any(|diagnostic| { diagnostic.code == "steady_state_semantics_unsupported" })
        );
        assert!(
            hierarchy
                .logic_graph
                .diagnostics
                .iter()
                .any(|diagnostic| { diagnostic.code == "temporal_semantics_unsupported" })
        );
    }
}
