use std::collections::BTreeMap;

use dustroute_physical::{BlockKind, ComponentId, ConnectionKind, PhysicalCircuit, Pos};
use serde::{Deserialize, Serialize};

use crate::LogicDag;

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum AbstractionLevel {
    Physical,
    Signal,
    Logic,
    Behavior,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum SignalNodeKind {
    Source,
    Wire,
    Inverter,
    Delay,
    Comparator,
    Actuator,
    Conductor,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct SignalNode {
    pub component: ComponentId,
    pub physical_position: Pos,
    pub kind: SignalNodeKind,
    pub delay_ticks: u8,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct SignalEdge {
    pub source: ComponentId,
    pub sink: ComponentId,
    pub physical_kind: ConnectionKind,
    pub delay_ticks: u8,
}

#[derive(Clone, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
pub struct SignalIr {
    pub nodes: Vec<SignalNode>,
    pub edges: Vec<SignalEdge>,
    pub physical_origins: BTreeMap<ComponentId, Pos>,
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
pub enum ProjectionEvidence {
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

#[derive(Clone, Debug)]
pub enum IrProjection {
    Physical(PhysicalCircuit),
    Signal(SignalIr),
    Logic(LogicDag),
    Behavior(BehaviorIr),
}

impl IrProjection {
    #[must_use]
    pub const fn level(&self) -> AbstractionLevel {
        match self {
            Self::Physical(_) => AbstractionLevel::Physical,
            Self::Signal(_) => AbstractionLevel::Signal,
            Self::Logic(_) => AbstractionLevel::Logic,
            Self::Behavior(_) => AbstractionLevel::Behavior,
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ProjectionError {
    Unsupported {
        from: AbstractionLevel,
        to: AbstractionLevel,
    },
    TemporalCircuitRequiresBehavior {
        components: Vec<ComponentId>,
    },
}

impl std::fmt::Display for ProjectionError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Unsupported { from, to } => write!(
                formatter,
                "projection from {from:?} to {to:?} is not implemented"
            ),
            Self::TemporalCircuitRequiresBehavior { components } => write!(
                formatter,
                "{} temporal component(s) require Behavior IR",
                components.len()
            ),
        }
    }
}
impl std::error::Error for ProjectionError {}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PhysicalProjection {
    pub signal: SignalIr,
    pub behavior: BehaviorIr,
    pub confidence_percent: u8,
    pub evidence: Vec<ProjectionEvidence>,
}

impl PhysicalProjection {
    #[must_use]
    pub fn from_physical(physical: &PhysicalCircuit) -> Self {
        let origins: BTreeMap<_, _> = physical.components.iter().map(|c| (c.id, c.pos)).collect();
        let by_id: BTreeMap<_, _> = physical.components.iter().map(|c| (c.id, c)).collect();
        let nodes = physical
            .components
            .iter()
            .map(|c| SignalNode {
                component: c.id,
                physical_position: c.pos,
                kind: signal_kind(c.block.kind),
                delay_ticks: component_delay(c.block.kind, c.block.delay),
            })
            .collect();
        let edges = physical
            .connections
            .iter()
            .map(|edge| SignalEdge {
                source: edge.source,
                sink: edge.sink,
                physical_kind: edge.kind,
                delay_ticks: by_id
                    .get(&edge.source)
                    .map_or(0, |c| component_delay(c.block.kind, c.block.delay)),
            })
            .collect();
        let evidence = physical
            .connections
            .iter()
            .map(|edge| ProjectionEvidence::VerifiedPhysicalConnection {
                source: edge.source,
                sink: edge.sink,
            })
            .chain(physical.components.iter().filter_map(|c| {
                temporal_semantics(c.block.kind).map(|_| ProjectionEvidence::DeviceSemantics {
                    component: c.id,
                    block: c.block.kind,
                })
            }))
            .collect();
        let devices = physical
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
        let signal = SignalIr {
            nodes,
            edges,
            physical_origins: origins.clone(),
        };
        let behavior = BehaviorIr {
            patterns: classify_behavior_patterns(&signal, &devices),
            devices,
            traces: Vec::new(),
            physical_origins: origins.clone(),
        };
        Self {
            signal,
            behavior,
            confidence_percent: 100,
            evidence,
        }
    }

    pub fn require_combinational(&self) -> Result<&SignalIr, ProjectionError> {
        if self.behavior.devices.is_empty() {
            Ok(&self.signal)
        } else {
            Err(ProjectionError::TemporalCircuitRequiresBehavior {
                components: self
                    .behavior
                    .devices
                    .iter()
                    .map(|device| device.component)
                    .collect(),
            })
        }
    }
}

fn classify_behavior_patterns(
    signal: &SignalIr,
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
        .any(|component| kinds.get(component) == Some(&SignalNodeKind::Inverter))
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

const fn signal_kind(kind: BlockKind) -> SignalNodeKind {
    match kind {
        BlockKind::Lever | BlockKind::RedstoneBlock => SignalNodeKind::Source,
        BlockKind::RedstoneWire => SignalNodeKind::Wire,
        BlockKind::RedstoneTorch => SignalNodeKind::Inverter,
        BlockKind::Repeater => SignalNodeKind::Delay,
        BlockKind::Comparator => SignalNodeKind::Comparator,
        BlockKind::Piston => SignalNodeKind::Actuator,
        BlockKind::Air | BlockKind::Solid | BlockKind::Transparent => SignalNodeKind::Conductor,
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
    use dustroute_physical::{Block, PhysicalComponent, PhysicalConnection};

    #[test]
    fn projects_delay_and_physical_traceability() {
        let mut repeater = Block::new(BlockKind::Repeater);
        repeater.delay = Some(3);
        let physical = PhysicalCircuit::from_parts(
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
        let projection = PhysicalProjection::from_physical(&physical);
        assert_eq!(projection.behavior.devices[0].minimum_delay_ticks, 3);
        assert_eq!(
            projection.behavior.patterns,
            vec![BehaviorPattern::DelayedPath]
        );
        assert!(matches!(
            projection.require_combinational(),
            Err(ProjectionError::TemporalCircuitRequiresBehavior { .. })
        ));
        assert_eq!(
            projection.signal.physical_origins[&ComponentId(1)],
            Pos::new(1, 64, 0)
        );
    }

    #[test]
    fn identifies_inverting_feedback_as_clock_candidate() {
        let physical = PhysicalCircuit::from_parts(
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
            ],
            [
                PhysicalConnection {
                    source: ComponentId(0),
                    sink: ComponentId(1),
                    kind: ConnectionKind::DirectSource,
                },
                PhysicalConnection {
                    source: ComponentId(1),
                    sink: ComponentId(0),
                    kind: ConnectionKind::Control,
                },
            ],
        );
        let projection = PhysicalProjection::from_physical(&physical);
        assert!(matches!(
            projection.behavior.patterns[0],
            BehaviorPattern::ClockCandidate { .. }
        ));
    }
}
