use std::collections::BTreeMap;

use dustroute_physical::{BlockKind, ComponentId, PhysicalScene, Pos, TransferKind};
use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
enum TemporalNodeKind {
    Source,
    Wire,
    Inverter,
    Delay,
    Comparator,
    Actuator,
    Conductor,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
struct TemporalNode {
    pub component: ComponentId,
    pub kind: TemporalNodeKind,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
struct TemporalDependency {
    pub source: ComponentId,
    pub sink: ComponentId,
}

#[derive(Clone, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
struct TemporalDependencyGraph {
    pub nodes: Vec<TemporalNode>,
    pub edges: Vec<TemporalDependency>,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum TemporalSemantics {
    DelayedForward,
    DelayedInvert,
    CompareOrSubtract,
    MechanicalActuation,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct TemporalDevice {
    pub component: ComponentId,
    pub physical_position: Pos,
    pub semantics: TemporalSemantics,
    pub minimum_delay_ticks: u8,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct BehaviorEvent {
    pub tick: u64,
    pub component: ComponentId,
    pub powered: bool,
}

#[derive(Clone, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
pub struct BehaviorTrace {
    pub label: String,
    pub events: Vec<BehaviorEvent>,
    pub stable: bool,
}

#[derive(Clone, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
pub struct BehaviorIr {
    pub devices: Vec<TemporalDevice>,
    pub patterns: Vec<BehaviorPattern>,
    pub traces: Vec<BehaviorTrace>,
    pub physical_origins: BTreeMap<ComponentId, Pos>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum BehaviorPattern {
    DelayedPath,
    ClockCandidate {
        feedback_components: Vec<ComponentId>,
    },
    LatchCandidate {
        feedback_components: Vec<ComponentId>,
    },
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum TemporalEvidence {
    VerifiedPhysicalConnection {
        source: ComponentId,
        sink: ComponentId,
    },
    DeviceSemantics {
        component: ComponentId,
        block: BlockKind,
    },
    TruthTableInference,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct TemporalAnalysis {
    pub behavior: BehaviorIr,
    pub confidence_percent: u8,
    pub evidence: Vec<TemporalEvidence>,
}

impl TemporalAnalysis {
    #[must_use]
    pub fn from_scene(scene: &PhysicalScene) -> Self {
        let origins: BTreeMap<_, _> = scene.components.iter().map(|c| (c.id, c.pos)).collect();
        let nodes = scene
            .components
            .iter()
            .map(|c| TemporalNode {
                component: c.id,
                kind: signal_kind(c.block.kind),
            })
            .collect();
        let edges = scene
            .connections
            .iter()
            .filter(|edge| edge.transfer != TransferKind::StructuralSupport)
            .map(|edge| TemporalDependency {
                source: edge.source.component,
                sink: edge.sink.component,
            })
            .collect();
        let evidence = scene
            .connections
            .iter()
            .map(|edge| TemporalEvidence::VerifiedPhysicalConnection {
                source: edge.source.component,
                sink: edge.sink.component,
            })
            .chain(scene.components.iter().filter_map(|c| {
                temporal_semantics(c.block.kind).map(|_| TemporalEvidence::DeviceSemantics {
                    component: c.id,
                    block: c.block.kind,
                })
            }))
            .collect();
        let devices = scene
            .components
            .iter()
            .filter_map(|c| {
                temporal_semantics(c.block.kind).map(|semantics| TemporalDevice {
                    component: c.id,
                    physical_position: c.pos,
                    semantics,
                    minimum_delay_ticks: component_delay(c.block.kind, c.block.delay),
                })
            })
            .collect::<Vec<_>>();
        let dependencies = TemporalDependencyGraph { nodes, edges };
        let behavior = BehaviorIr {
            patterns: classify_behavior_patterns(&dependencies, &devices),
            devices,
            traces: Vec::new(),
            physical_origins: origins.clone(),
        };
        Self {
            behavior,
            confidence_percent: 100,
            evidence,
        }
    }
}

fn classify_behavior_patterns(
    signal: &TemporalDependencyGraph,
    devices: &[TemporalDevice],
) -> Vec<BehaviorPattern> {
    let adjacency: BTreeMap<ComponentId, Vec<ComponentId>> =
        signal
            .edges
            .iter()
            .fold(BTreeMap::new(), |mut adjacency, edge| {
                adjacency.entry(edge.source).or_default().push(edge.sink);
                adjacency
            });
    let mut feedback = signal
        .edges
        .iter()
        .filter(|edge| path_exists(&adjacency, edge.sink, edge.source))
        .flat_map(|edge| [edge.source, edge.sink])
        .collect::<Vec<_>>();
    feedback.sort_unstable();
    feedback.dedup();
    if feedback.is_empty() {
        return (!devices.is_empty())
            .then_some(BehaviorPattern::DelayedPath)
            .into_iter()
            .collect();
    }
    let kinds: BTreeMap<_, _> = signal
        .nodes
        .iter()
        .map(|node| (node.component, node.kind))
        .collect();
    if feedback
        .iter()
        .any(|component| kinds.get(component) == Some(&TemporalNodeKind::Inverter))
    {
        vec![BehaviorPattern::ClockCandidate {
            feedback_components: feedback,
        }]
    } else {
        vec![BehaviorPattern::LatchCandidate {
            feedback_components: feedback,
        }]
    }
}

fn path_exists(
    adjacency: &BTreeMap<ComponentId, Vec<ComponentId>>,
    start: ComponentId,
    target: ComponentId,
) -> bool {
    let mut pending = vec![start];
    let mut seen = std::collections::BTreeSet::new();
    while let Some(current) = pending.pop() {
        if current == target {
            return true;
        }
        if seen.insert(current) {
            pending.extend(adjacency.get(&current).into_iter().flatten().copied());
        }
    }
    false
}

const fn signal_kind(kind: BlockKind) -> TemporalNodeKind {
    match kind {
        BlockKind::Lever | BlockKind::RedstoneBlock => TemporalNodeKind::Source,
        BlockKind::RedstoneWire => TemporalNodeKind::Wire,
        BlockKind::RedstoneTorch => TemporalNodeKind::Inverter,
        BlockKind::Repeater => TemporalNodeKind::Delay,
        BlockKind::Comparator => TemporalNodeKind::Comparator,
        BlockKind::Piston => TemporalNodeKind::Actuator,
        BlockKind::Air | BlockKind::Solid | BlockKind::Transparent => TemporalNodeKind::Conductor,
    }
}

fn component_delay(kind: BlockKind, delay: Option<u8>) -> u8 {
    match kind {
        BlockKind::Repeater => delay.unwrap_or(1),
        BlockKind::RedstoneTorch | BlockKind::Piston => 1,
        _ => 0,
    }
}

const fn temporal_semantics(kind: BlockKind) -> Option<TemporalSemantics> {
    match kind {
        BlockKind::Repeater => Some(TemporalSemantics::DelayedForward),
        BlockKind::RedstoneTorch => Some(TemporalSemantics::DelayedInvert),
        BlockKind::Comparator => Some(TemporalSemantics::CompareOrSubtract),
        BlockKind::Piston => Some(TemporalSemantics::MechanicalActuation),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use dustroute_physical::{
        Block, ConnectionKind, Observation, PhysicalComponent, PhysicalConnection, SceneBounds,
        VerifiedTopology,
    };

    fn analyze(topology: &VerifiedTopology) -> TemporalAnalysis {
        let scene = PhysicalScene::from_unvalidated_topology(
            Observation::complete(
                "test",
                SceneBounds::new(Pos::new(-16, -16, -16), Pos::new(16, 128, 16)),
            ),
            topology,
        );
        TemporalAnalysis::from_scene(&scene)
    }

    #[test]
    fn projects_delay_and_physical_traceability() {
        let mut repeater = Block::new(BlockKind::Repeater);
        repeater.delay = Some(3);
        let topology = VerifiedTopology::from_parts(
            vec![
                PhysicalComponent {
                    id: ComponentId(0),
                    pos: Pos::new(0, 64, 0),
                    block: Block::new(BlockKind::Lever),
                },
                PhysicalComponent {
                    id: ComponentId(1),
                    pos: Pos::new(1, 64, 0),
                    block: repeater,
                },
            ],
            [PhysicalConnection {
                source: ComponentId(0),
                sink: ComponentId(1),
                kind: ConnectionKind::DirectSource,
            }],
        );
        let projection = analyze(&topology);
        assert_eq!(projection.behavior.devices[0].minimum_delay_ticks, 3);
        assert_eq!(
            projection.behavior.patterns,
            vec![BehaviorPattern::DelayedPath]
        );
        assert_eq!(
            projection.behavior.physical_origins[&ComponentId(1)],
            Pos::new(1, 64, 0)
        );
    }

    #[test]
    fn identifies_inverting_feedback_as_clock_candidate() {
        let topology = VerifiedTopology::from_parts(
            vec![
                PhysicalComponent {
                    id: ComponentId(0),
                    pos: Pos::new(0, 64, 0),
                    block: Block::new(BlockKind::RedstoneTorch),
                },
                PhysicalComponent {
                    id: ComponentId(1),
                    pos: Pos::new(1, 64, 0),
                    block: Block::new(BlockKind::RedstoneWire),
                },
                PhysicalComponent {
                    id: ComponentId(2),
                    pos: Pos::new(1, 63, 0),
                    block: Block::new(BlockKind::RedstoneWire),
                },
                PhysicalComponent {
                    id: ComponentId(3),
                    pos: Pos::new(0, 63, 0),
                    block: Block::new(BlockKind::Solid),
                },
            ],
            [
                PhysicalConnection {
                    source: ComponentId(0),
                    sink: ComponentId(1),
                    kind: ConnectionKind::DirectSource,
                },
                PhysicalConnection {
                    source: ComponentId(1),
                    sink: ComponentId(2),
                    kind: ConnectionKind::Dust,
                },
                PhysicalConnection {
                    source: ComponentId(2),
                    sink: ComponentId(3),
                    kind: ConnectionKind::WeakPower,
                },
                PhysicalConnection {
                    source: ComponentId(3),
                    sink: ComponentId(0),
                    kind: ConnectionKind::Control,
                },
            ],
        );
        let projection = analyze(&topology);
        assert!(matches!(
            projection.behavior.patterns[0],
            BehaviorPattern::ClockCandidate { .. }
        ));
    }
}
