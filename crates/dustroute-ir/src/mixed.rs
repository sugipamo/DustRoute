use std::collections::{BTreeMap, BTreeSet, VecDeque};

use dustroute_physical::{BlockKind, ComponentId, Confidence, PhysicalScene, TransferKind};
use serde::{Deserialize, Serialize};

use crate::{HierarchicalIr, RecognitionStatus, RecognizedGateKind};

#[derive(Clone, Copy, Debug, Deserialize, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize)]
pub struct MixedNodeId(pub usize);

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(tag = "representation", rename_all = "snake_case")]
pub enum MixedNodeKind {
    LogicGate { kind: RecognizedGateKind },
    TimedCell { kind: RecognizedGateKind },
    PhysicalRegion,
    Boundary { direction: BoundaryDirection },
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum BoundaryDirection {
    Input,
    Output,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct MixedNode {
    pub id: MixedNodeId,
    pub kind: MixedNodeKind,
    pub physical_components: BTreeSet<ComponentId>,
    pub confidence: Confidence,
    pub recognition: RecognitionStatus,
    pub expandable: bool,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct MixedEdge {
    pub source: MixedNodeId,
    pub sink: MixedNodeId,
    pub physical_kind: TransferKind,
}

#[derive(Clone, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
pub struct MixedIr {
    pub nodes: Vec<MixedNode>,
    pub edges: Vec<MixedEdge>,
    pub physical_component_count: usize,
    pub recognized_component_count: usize,
    pub unresolved_component_count: usize,
}

fn timed_gate(scene: &PhysicalScene, components: &BTreeSet<ComponentId>) -> bool {
    components.iter().any(|component| {
        scene.components.get(component.0).is_some_and(|component| {
            matches!(
                component.block.kind,
                BlockKind::Repeater | BlockKind::Comparator
            )
        })
    })
}

fn unresolved_regions(
    scene: &PhysicalScene,
    unresolved: &BTreeSet<ComponentId>,
) -> Vec<BTreeSet<ComponentId>> {
    let mut adjacency = BTreeMap::<ComponentId, BTreeSet<ComponentId>>::new();
    for connection in &scene.connections {
        if unresolved.contains(&connection.source.component)
            && unresolved.contains(&connection.sink.component)
        {
            adjacency
                .entry(connection.source.component)
                .or_default()
                .insert(connection.sink.component);
            adjacency
                .entry(connection.sink.component)
                .or_default()
                .insert(connection.source.component);
        }
    }
    let mut remaining = unresolved.clone();
    let mut regions = Vec::new();
    while let Some(start) = remaining.pop_first() {
        let mut region = BTreeSet::from([start]);
        let mut queue = VecDeque::from([start]);
        while let Some(component) = queue.pop_front() {
            for neighbor in adjacency.get(&component).into_iter().flatten() {
                if remaining.remove(neighbor) {
                    region.insert(*neighbor);
                    queue.push_back(*neighbor);
                }
            }
        }
        regions.push(region);
    }
    regions
}

#[must_use]
pub fn build_mixed_ir(hierarchy: &HierarchicalIr) -> MixedIr {
    let scene = &hierarchy.physical_graph.value.scene;
    let mut nodes = Vec::new();
    let mut owner = BTreeMap::<ComponentId, MixedNodeId>::new();
    for gate in &hierarchy.cell_graph.value.cells.gates {
        let id = MixedNodeId(nodes.len());
        let kind = if timed_gate(scene, &gate.physical_components) {
            MixedNodeKind::TimedCell { kind: gate.kind }
        } else {
            MixedNodeKind::LogicGate { kind: gate.kind }
        };
        nodes.push(MixedNode {
            id,
            kind,
            physical_components: gate.physical_components.clone(),
            confidence: gate.confidence,
            recognition: gate.status,
            expandable: true,
        });
        for component in &gate.physical_components {
            owner.entry(*component).or_insert(id);
        }
    }
    let unresolved = scene
        .components
        .iter()
        .map(|component| component.id)
        .filter(|component| !owner.contains_key(component))
        .collect::<BTreeSet<_>>();
    for region in unresolved_regions(scene, &unresolved) {
        let id = MixedNodeId(nodes.len());
        nodes.push(MixedNode {
            id,
            kind: MixedNodeKind::PhysicalRegion,
            physical_components: region.clone(),
            confidence: Confidence::Certain,
            recognition: RecognitionStatus::Partial,
            expandable: true,
        });
        for component in region {
            owner.insert(component, id);
        }
    }
    let mut edges = Vec::new();
    for connection in &scene.connections {
        let (Some(source), Some(sink)) = (
            owner.get(&connection.source.component),
            owner.get(&connection.sink.component),
        ) else {
            continue;
        };
        let edge = MixedEdge {
            source: *source,
            sink: *sink,
            physical_kind: connection.transfer,
        };
        if source != sink && !edges.contains(&edge) {
            edges.push(edge);
        }
    }
    MixedIr {
        nodes,
        edges,
        physical_component_count: scene.components.len(),
        recognized_component_count: owner.len().saturating_sub(unresolved.len()),
        unresolved_component_count: unresolved.len(),
    }
}

#[cfg(test)]
mod tests {
    use dustroute_physical::{
        Block, BlockKind, ComponentId, ConnectionKind, Observation, PhysicalComponent,
        PhysicalConnection, PhysicalScene, Pos, SceneBounds, VerifiedTopology,
    };

    use crate::{build_cell_graph, build_mixed_ir, build_physical_graph, build_physical_snapshot};

    #[test]
    fn unresolved_physical_components_are_compacted_without_loss() {
        let mut components = Vec::new();
        let mut connections = Vec::new();
        for x in 0..64 {
            components.push(PhysicalComponent {
                id: ComponentId(x),
                pos: Pos::new(x as i32, 0, 0),
                block: Block::new(BlockKind::RedstoneWire),
            });
            if x > 0 {
                connections.push(PhysicalConnection {
                    source: ComponentId(x - 1),
                    sink: ComponentId(x),
                    kind: ConnectionKind::Dust,
                });
            }
        }
        let topology = VerifiedTopology::from_parts(components, connections);
        let scene = PhysicalScene::from_unvalidated_topology(
            Observation::complete(
                "minecraft:overworld",
                SceneBounds::new(Pos::new(0, 0, 0), Pos::new(63, 0, 0)),
            ),
            &topology,
        );
        let snapshot = build_physical_snapshot(&scene);
        let physical = build_physical_graph(&snapshot);
        let cells = build_cell_graph(&physical);
        let hierarchy = crate::derive_hierarchy(&scene);
        assert_eq!(cells.value.cells.gates.len(), 0);
        let mixed = build_mixed_ir(&hierarchy);
        assert_eq!(mixed.nodes.len(), 1);
        assert_eq!(mixed.nodes[0].physical_components.len(), 64);
        assert_eq!(mixed.physical_component_count, 64);
    }
}
