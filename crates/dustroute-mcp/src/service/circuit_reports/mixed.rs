//! Owned graph diagnostics. These records cannot restore a model or an observation.
use crate::operations::mutation::{Success, UnrecordedFailure};
use crate::snapshot_content::{ContentId, ValidationKey};
use dustroute_ir::{MixedEdge, MixedNodeId, MixedNodeKind, RecognitionStatus};
use dustroute_physical::{BlockKind, ComponentId, Confidence, Pos};
use dustroute_translate::world_reverse::RegionBounds;
use serde::Serialize;
use std::collections::BTreeMap;

#[derive(Debug, Serialize)]
pub(crate) struct CircuitIrResponse {
    pub ok: Success,
    pub analysis_mode: &'static str,
    pub circuit_id: uuid::Uuid,
    pub content_id: ContentId,
    pub analysis_id: ValidationKey,
    pub analysis_id_schema: &'static str,
    pub mutation_performed: bool,
    pub target: Option<Pos>,
    pub bounds: RegionBounds,
    pub analysis_complete: bool,
    pub expansion: super::super::circuit_capture::ExpansionEvidence,
    pub mixed_ir: MixedIrReport,
    pub guidance: &'static str,
}
#[derive(Debug, Serialize)]
pub(crate) struct IrIdentityMismatch {
    #[serde(flatten)]
    pub refusal: UnrecordedFailure,
    pub current_analysis_id: ValidationKey,
    pub analysis_id_schema: &'static str,
    pub retryable: bool,
}
#[derive(Debug, Serialize)]
pub(crate) struct IrNodeUnavailable {
    #[serde(flatten)]
    pub refusal: UnrecordedFailure,
    pub available_node_count: usize,
}
#[derive(Debug, Serialize)]
pub(crate) struct MixedIrReport {
    physical_component_count: usize,
    recognized_component_count: usize,
    unresolved_component_count: usize,
    node_count: usize,
    edge_count: usize,
    nodes: Vec<NodeSummary>,
    edges: Vec<MixedEdge>,
    expanded_node: Option<ExpandedNode>,
}
#[derive(Debug, Serialize)]
struct NodeSummary {
    id: MixedNodeId,
    kind: MixedNodeKind,
    recognition: RecognitionStatus,
    confidence: Confidence,
    component_count: usize,
    bounds: OptionalBounds,
    expandable: bool,
}
#[derive(Debug, Serialize)]
struct OptionalBounds {
    min: Option<Pos>,
    max: Option<Pos>,
}
#[derive(Debug, Serialize)]
struct ExpandedNode {
    id: MixedNodeId,
    kind: MixedNodeKind,
    recognition: RecognitionStatus,
    confidence: Confidence,
    components: Vec<ComponentSummary>,
    incoming: Vec<MixedEdge>,
    outgoing: Vec<MixedEdge>,
}
#[derive(Debug, Serialize)]
struct ComponentSummary {
    id: ComponentId,
    position: Pos,
    block: BlockKind,
    observed_name: Option<String>,
    observed_properties: BTreeMap<String, String>,
}

pub(in super::super) fn mixed_ir_report(
    hierarchy: &dustroute_ir::HierarchicalIr,
    expanded_node_id: Option<usize>,
) -> Result<MixedIrReport, String> {
    let scene = &hierarchy.physical_graph.value.scene;
    let mixed = dustroute_ir::build_mixed_ir(hierarchy);
    let nodes = mixed
        .nodes
        .iter()
        .map(|node| {
            let positions = node
                .physical_components
                .iter()
                .filter_map(|component| scene.components.get(component.0).map(|item| item.pos))
                .collect::<Vec<_>>();
            let min = positions.iter().copied().reduce(|left, right| Pos {
                x: left.x.min(right.x),
                y: left.y.min(right.y),
                z: left.z.min(right.z),
            });
            let max = positions.iter().copied().reduce(|left, right| Pos {
                x: left.x.max(right.x),
                y: left.y.max(right.y),
                z: left.z.max(right.z),
            });
            NodeSummary {
                id: node.id,
                kind: node.kind.clone(),
                recognition: node.recognition,
                confidence: node.confidence,
                component_count: node.physical_components.len(),
                bounds: OptionalBounds { min, max },
                expandable: node.expandable,
            }
        })
        .collect();
    let expanded_node = if let Some(id) = expanded_node_id {
        let node = mixed
            .nodes
            .get(id)
            .filter(|node| node.id.0 == id)
            .ok_or_else(|| format!("mixed IR node {id} does not exist"))?;
        Some(ExpandedNode {
            id: node.id,
            kind: node.kind.clone(),
            recognition: node.recognition,
            confidence: node.confidence,
            components: node
                .physical_components
                .iter()
                .filter_map(|component| scene.components.get(component.0))
                .map(|component| ComponentSummary {
                    id: component.id,
                    position: component.pos,
                    block: component.block.kind,
                    observed_name: component.block.observed_name.clone(),
                    observed_properties: component.block.observed_properties.clone(),
                })
                .collect(),
            incoming: mixed
                .edges
                .iter()
                .filter(|edge| edge.sink == node.id)
                .cloned()
                .collect(),
            outgoing: mixed
                .edges
                .iter()
                .filter(|edge| edge.source == node.id)
                .cloned()
                .collect(),
        })
    } else {
        None
    };
    Ok(MixedIrReport {
        physical_component_count: mixed.physical_component_count,
        recognized_component_count: mixed.recognized_component_count,
        unresolved_component_count: mixed.unresolved_component_count,
        node_count: mixed.nodes.len(),
        edge_count: mixed.edges.len(),
        nodes,
        edges: mixed.edges,
        expanded_node,
    })
}
