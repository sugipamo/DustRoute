use std::collections::{BTreeMap, BTreeSet, VecDeque};

use dustroute_physical::{
    BlockKind, ComponentId, ConnectionKind, PhysicalScene, Pos, TransferKind,
};
use serde::{Deserialize, Serialize};

use crate::TransitionTrace;

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum TemporalNodeKind {
    Source,
    Wire,
    Inverter,
    Delay,
    Comparator,
    Observer,
    Actuator,
    Conductor,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct TemporalNode {
    pub component: ComponentId,
    pub kind: TemporalNodeKind,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
struct TemporalDependency {
    pub source: ComponentId,
    pub sink: ComponentId,
    pub kind: ConnectionKind,
}

#[derive(Clone, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
struct TemporalDependencyGraph {
    pub nodes: Vec<TemporalNode>,
    pub edges: Vec<TemporalDependency>,
}

#[derive(Clone, Copy, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
pub struct DelayRange {
    /// Minecraft redstone ticks (two game ticks each).
    pub minimum_redstone_ticks: u32,
    /// Minecraft redstone ticks (two game ticks each).
    pub maximum_redstone_ticks: u32,
}

impl DelayRange {
    const fn add(self, other: Self) -> Self {
        Self {
            minimum_redstone_ticks: self
                .minimum_redstone_ticks
                .saturating_add(other.minimum_redstone_ticks),
            maximum_redstone_ticks: self
                .maximum_redstone_ticks
                .saturating_add(other.maximum_redstone_ticks),
        }
    }
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum EdgeBehavior {
    Immediate,
    DelayedForward,
    DelayedInvert,
    Analog,
    Pulse,
    Mechanical,
    OrderSensitive,
}

/// A transition delay expressed in the scheduler's base unit rather than as
/// an integer redstone-tick convenience value.  `SameGameTick` is an ordered
/// zero-game-tick transition; its exact position is carried by
/// [`BehaviorEvent::sub_tick_order`].  `GameTickRange` is intentionally kept
/// as a range because a state transition may depend on the observed trigger
/// and re-trigger timing.  `Unavailable` is fail-closed and must not be
/// interpreted as an immediate transition.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum TransitionDelay {
    ExactGameTicks {
        game_ticks: u64,
    },
    GameTickRange {
        minimum_game_ticks: u64,
        maximum_game_ticks: u64,
    },
    SameGameTick,
    Unavailable {
        reason: String,
    },
}

impl Default for TransitionDelay {
    fn default() -> Self {
        Self::Unavailable {
            reason: "timing has not been verified".to_owned(),
        }
    }
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct TimedEdge {
    pub source: ComponentId,
    pub sink: ComponentId,
    pub delay: DelayRange,
    /// Scheduler-aware timing. `delay` remains for the legacy redstone-tick
    /// projection and must not be used when this field is unavailable.
    #[serde(default)]
    pub transition_delay: TransitionDelay,
    pub behavior: EdgeBehavior,
    pub physical_components: BTreeSet<ComponentId>,
}

#[derive(Clone, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
pub struct TimedCircuit {
    pub nodes: Vec<TemporalNode>,
    pub edges: Vec<TimedEdge>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct SteadyStateEdge {
    pub source: ComponentId,
    pub sink: ComponentId,
    pub inverted: bool,
    /// Retained for later temporal verification; steady-state evaluation does
    /// not use this value.
    pub retained_delay: DelayRange,
    pub physical_path: Vec<ComponentId>,
}

#[derive(Clone, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
pub struct SteadyStateProjection {
    pub retained_components: BTreeSet<ComponentId>,
    pub edges: Vec<SteadyStateEdge>,
    pub compressed_components: BTreeSet<ComponentId>,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum TemporalScope {
    SteadyStateSafe,
    TimingSensitive,
    TemporalRequired,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum TimingReason {
    UnequalReconvergentDelay {
        component: ComponentId,
        minimum_redstone_ticks: u32,
        maximum_redstone_ticks: u32,
    },
    Feedback {
        components: Vec<ComponentId>,
    },
    StatefulOrMechanicalDevice {
        component: ComponentId,
        block: BlockKind,
    },
    TransitionTimingUnavailable {
        component: ComponentId,
        block: BlockKind,
        reason: String,
    },
    LockedRepeater {
        component: ComponentId,
    },
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct TimingAssessment {
    pub scope: TemporalScope,
    pub reasons: Vec<TimingReason>,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum TemporalSemantics {
    DelayedForward,
    DelayedInvert,
    CompareOrSubtract,
    MechanicalActuation,
    ObserverPulse,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct TemporalDevice {
    pub component: ComponentId,
    pub physical_position: Pos,
    pub semantics: TemporalSemantics,
    pub minimum_delay_redstone_ticks: u8,
    /// A zero-capable, game-tick-based timing contract. The legacy scalar
    /// above is retained for existing consumers and is not authoritative for
    /// mechanical devices.
    #[serde(default)]
    pub transition_delay: TransitionDelay,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct BehaviorEvent {
    pub tick: u64,
    /// Ordering within `tick`. Zero is used by traces that only have coarse
    /// tick resolution or by a settled baseline observation.
    #[serde(default)]
    pub sub_tick_order: u64,
    /// Coarse event classification. Older traces deserialize as a generic
    /// state transition and therefore remain compatible.
    #[serde(default)]
    pub event_kind: crate::EventKind,
    /// Causal evidence. `unknown` is intentional when the scheduler cause is
    /// not exposed by the source of the trace.
    #[serde(default)]
    pub cause: crate::EventCause,
    #[serde(default)]
    pub source: crate::EventSource,
    /// Optional parent event in a causal trace. Current traces leave this
    /// unset until a scheduler-aware event engine can provide it honestly.
    #[serde(default)]
    pub cause_sequence: Option<u64>,
    pub component: ComponentId,
    pub powered: bool,
}

#[derive(Clone, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
pub struct BehaviorTrace {
    pub label: String,
    #[serde(default)]
    pub time_unit: TraceTimeUnit,
    pub events: Vec<BehaviorEvent>,
    pub stable: bool,
}

#[derive(Clone, Copy, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum TraceTimeUnit {
    #[default]
    RedstoneTick,
    GameTick,
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

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct TemporalAnalysis {
    pub behavior: BehaviorIr,
    pub timed_circuit: TimedCircuit,
    pub steady_state: SteadyStateProjection,
    pub timing: TimingAssessment,
    pub transients: Vec<crate::TransientAssessment>,
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
                kind: connection_kind(edge.transfer),
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
                    minimum_delay_redstone_ticks: component_delay(c.block.kind, c.block.delay),
                    transition_delay: component_transition_delay(c.block.kind, c.block.delay),
                })
            })
            .collect::<Vec<_>>();
        let dependencies = TemporalDependencyGraph { nodes, edges };
        let timed_circuit = timed_circuit(scene, &dependencies);
        let timing = timing_assessment(scene, &timed_circuit);
        let steady_state = steady_state_projection(&timed_circuit);
        let behavior = BehaviorIr {
            patterns: classify_behavior_patterns(&dependencies, &devices, &timing),
            devices,
            traces: Vec::new(),
            physical_origins: origins.clone(),
        };
        Self {
            behavior,
            timed_circuit,
            steady_state,
            timing,
            transients: Vec::new(),
            confidence_percent: 100,
            evidence,
        }
    }

    pub fn record_trace(
        &mut self,
        trace: BehaviorTrace,
        contracts: &BTreeMap<ComponentId, crate::SignalIntent>,
    ) {
        self.transients
            .push(crate::assess_transients(&trace, contracts));
        self.behavior.traces.push(trace);
    }

    /// Records a transition-first trace while retaining the legacy behavior
    /// projection used by transient analysis. This adapter is deliberately
    /// explicit: no tick samples or missing before-values are synthesized.
    pub fn record_transition_trace(
        &mut self,
        trace: TransitionTrace,
        contracts: &BTreeMap<ComponentId, crate::SignalIntent>,
    ) {
        self.record_trace(trace.to_behavior_trace(), contracts);
    }
}

fn timed_circuit(scene: &PhysicalScene, dependencies: &TemporalDependencyGraph) -> TimedCircuit {
    let by_id: BTreeMap<_, _> = scene
        .components
        .iter()
        .map(|component| (component.id, component))
        .collect();
    let edges = dependencies
        .edges
        .iter()
        .map(|edge| {
            let source = by_id[&edge.source];
            let sink = by_id[&edge.sink];
            let (delay, transition_delay, behavior) = edge_timing(
                source.block.kind,
                sink.block.kind,
                edge.kind,
                source.block.delay,
            );
            TimedEdge {
                source: edge.source,
                sink: edge.sink,
                delay,
                transition_delay,
                behavior,
                physical_components: BTreeSet::from([edge.source, edge.sink]),
            }
        })
        .collect();
    TimedCircuit {
        nodes: dependencies.nodes.clone(),
        edges,
    }
}

fn connection_kind(transfer: TransferKind) -> ConnectionKind {
    match transfer {
        TransferKind::DustPropagation => ConnectionKind::Dust,
        TransferKind::DirectSignal => ConnectionKind::DirectSource,
        TransferKind::WeakPower => ConnectionKind::WeakPower,
        TransferKind::StrongPower => ConnectionKind::StrongPower,
        TransferKind::DirectionalDevice => ConnectionKind::DirectionalOutput,
        TransferKind::SideControl => ConnectionKind::Control,
        TransferKind::Observation => ConnectionKind::ObserverInput,
        TransferKind::StructuralSupport => ConnectionKind::Support,
    }
}

fn edge_timing(
    source_kind: BlockKind,
    sink_kind: BlockKind,
    connection: ConnectionKind,
    delay: Option<u8>,
) -> (DelayRange, TransitionDelay, EdgeBehavior) {
    let ticks = u32::from(component_delay(source_kind, delay));
    let range = DelayRange {
        minimum_redstone_ticks: ticks,
        maximum_redstone_ticks: ticks,
    };
    let transition_delay = edge_transition_delay(source_kind, sink_kind, connection, delay);
    let behavior = if connection == ConnectionKind::PistonInput || sink_kind == BlockKind::Piston {
        EdgeBehavior::Mechanical
    } else {
        match source_kind {
            BlockKind::Repeater => EdgeBehavior::DelayedForward,
            BlockKind::RedstoneTorch => EdgeBehavior::DelayedInvert,
            BlockKind::Comparator => EdgeBehavior::Analog,
            BlockKind::Observer => EdgeBehavior::Pulse,
            BlockKind::Piston => EdgeBehavior::Mechanical,
            BlockKind::RedstoneWire => EdgeBehavior::OrderSensitive,
            BlockKind::Air
            | BlockKind::Solid
            | BlockKind::Transparent
            | BlockKind::Lever
            | BlockKind::Button
            | BlockKind::PressurePlate
            | BlockKind::RedstoneLamp
            | BlockKind::RedstoneBlock => EdgeBehavior::Immediate,
        }
    };
    (range, transition_delay, behavior)
}

fn edge_transition_delay(
    source_kind: BlockKind,
    sink_kind: BlockKind,
    connection: ConnectionKind,
    delay: Option<u8>,
) -> TransitionDelay {
    if connection == ConnectionKind::PistonInput
        || sink_kind == BlockKind::Piston
        || source_kind == BlockKind::Piston
    {
        return TransitionDelay::Unavailable {
            reason:
                "piston motion has phase-dependent timing; a live block-event trace is required"
                    .to_owned(),
        };
    }
    component_transition_delay(source_kind, delay)
}

fn component_transition_delay(kind: BlockKind, delay: Option<u8>) -> TransitionDelay {
    match kind {
        BlockKind::Repeater => TransitionDelay::ExactGameTicks {
            game_ticks: u64::from(delay.unwrap_or(1)) * 2,
        },
        BlockKind::RedstoneTorch | BlockKind::Observer => {
            TransitionDelay::ExactGameTicks { game_ticks: 2 }
        }
        BlockKind::Piston => TransitionDelay::Unavailable {
            reason: "piston start/completion timing is not in the stable structural subset"
                .to_owned(),
        },
        _ => TransitionDelay::SameGameTick,
    }
}

fn timing_assessment(scene: &PhysicalScene, circuit: &TimedCircuit) -> TimingAssessment {
    let feedback = active_feedback_components(circuit);
    let mut reasons = Vec::new();
    if !feedback.is_empty() {
        reasons.push(TimingReason::Feedback {
            components: feedback,
        });
    }
    for component in &scene.components {
        if matches!(
            component.block.kind,
            BlockKind::Comparator | BlockKind::Observer | BlockKind::Piston
        ) {
            reasons.push(TimingReason::StatefulOrMechanicalDevice {
                component: component.id,
                block: component.block.kind,
            });
        }
        if component.block.kind == BlockKind::Piston {
            reasons.push(TimingReason::TransitionTimingUnavailable {
                component: component.id,
                block: component.block.kind,
                reason:
                    "piston start/completion timing is not verified in the stable structural subset"
                        .to_owned(),
            });
        }
        if component.block.kind == BlockKind::Repeater
            && component
                .block
                .observed_properties
                .get("locked")
                .map(String::as_str)
                == Some("true")
        {
            reasons.push(TimingReason::LockedRepeater {
                component: component.id,
            });
        }
    }
    reasons.extend(reconvergent_delay_reasons(circuit));
    let scope = if reasons.iter().any(|reason| {
        matches!(
            reason,
            TimingReason::Feedback { .. }
                | TimingReason::StatefulOrMechanicalDevice { .. }
                | TimingReason::TransitionTimingUnavailable { .. }
                | TimingReason::LockedRepeater { .. }
        )
    }) {
        TemporalScope::TemporalRequired
    } else if reasons.is_empty() {
        TemporalScope::SteadyStateSafe
    } else {
        TemporalScope::TimingSensitive
    };
    TimingAssessment { scope, reasons }
}

/// Collapses passive bidirectional dust/conductor connectivity before looking
/// for cycles. Otherwise every two-way dust adjacency appears to be feedback.
fn active_feedback_components(circuit: &TimedCircuit) -> Vec<ComponentId> {
    let kinds: BTreeMap<_, _> = circuit
        .nodes
        .iter()
        .map(|node| (node.component, node.kind))
        .collect();
    let passive = |component: ComponentId| {
        matches!(
            kinds[&component],
            TemporalNodeKind::Wire | TemporalNodeKind::Conductor
        )
    };
    let directed = circuit
        .edges
        .iter()
        .map(|edge| (edge.source, edge.sink))
        .collect::<BTreeSet<_>>();
    let mut passive_adjacency = BTreeMap::<ComponentId, Vec<ComponentId>>::new();
    for edge in &circuit.edges {
        if passive(edge.source)
            && passive(edge.sink)
            && directed.contains(&(edge.sink, edge.source))
        {
            passive_adjacency
                .entry(edge.source)
                .or_default()
                .push(edge.sink);
            passive_adjacency
                .entry(edge.sink)
                .or_default()
                .push(edge.source);
        }
    }
    let mut group = BTreeMap::<ComponentId, ComponentId>::new();
    for node in &circuit.nodes {
        if group.contains_key(&node.component) {
            continue;
        }
        if !passive(node.component) {
            group.insert(node.component, node.component);
            continue;
        }
        let mut members = Vec::new();
        let mut pending = vec![node.component];
        while let Some(component) = pending.pop() {
            if group.contains_key(&component) || !passive(component) {
                continue;
            }
            group.insert(component, node.component);
            members.push(component);
            pending.extend(
                passive_adjacency
                    .get(&component)
                    .into_iter()
                    .flatten()
                    .copied(),
            );
        }
        let representative = members.iter().copied().min().unwrap_or(node.component);
        for member in members {
            group.insert(member, representative);
        }
    }
    let quotient_edges = circuit
        .edges
        .iter()
        .filter_map(|edge| {
            let source = group[&edge.source];
            let sink = group[&edge.sink];
            (source != sink).then_some((source, sink))
        })
        .collect::<BTreeSet<_>>();
    let quotient_adjacency = quotient_edges.iter().fold(
        BTreeMap::<ComponentId, Vec<ComponentId>>::new(),
        |mut result, (source, sink)| {
            result.entry(*source).or_default().push(*sink);
            result
        },
    );
    let quotient_reverse = quotient_edges.iter().fold(
        BTreeMap::<ComponentId, Vec<ComponentId>>::new(),
        |mut result, (source, sink)| {
            result.entry(*sink).or_default().push(*source);
            result
        },
    );
    let quotient_nodes = group.values().copied().collect::<BTreeSet<_>>();
    let cyclic_groups =
        strongly_connected_components(&quotient_nodes, &quotient_adjacency, &quotient_reverse)
            .into_iter()
            .filter(|component| component.len() > 1)
            .flatten()
            .collect::<BTreeSet<_>>();
    circuit
        .nodes
        .iter()
        .filter(|node| cyclic_groups.contains(&group[&node.component]))
        .filter(|node| !passive(node.component))
        .map(|node| node.component)
        .collect()
}

fn reconvergent_delay_reasons(circuit: &TimedCircuit) -> Vec<TimingReason> {
    let mut indegree = circuit
        .nodes
        .iter()
        .map(|node| (node.component, 0usize))
        .collect::<BTreeMap<_, _>>();
    for edge in &circuit.edges {
        *indegree.entry(edge.sink).or_default() += 1;
    }
    let original_indegree = indegree.clone();
    let mut ranges = circuit
        .nodes
        .iter()
        .map(|node| (node.component, DelayRange::default()))
        .collect::<BTreeMap<_, _>>();
    let mut queue = indegree
        .iter()
        .filter(|(_, degree)| **degree == 0)
        .map(|(component, _)| *component)
        .collect::<VecDeque<_>>();
    let mut incoming = BTreeMap::<ComponentId, Vec<DelayRange>>::new();
    let mut outgoing = BTreeMap::<ComponentId, Vec<&TimedEdge>>::new();
    for edge in &circuit.edges {
        outgoing.entry(edge.source).or_default().push(edge);
    }
    while let Some(component) = queue.pop_front() {
        let base = ranges[&component];
        for edge in outgoing.get(&component).into_iter().flatten() {
            incoming
                .entry(edge.sink)
                .or_default()
                .push(base.add(edge.delay));
            let degree = indegree.get_mut(&edge.sink).expect("edge sink is a node");
            *degree -= 1;
            if *degree == 0 {
                let candidates = &incoming[&edge.sink];
                ranges.insert(
                    edge.sink,
                    DelayRange {
                        minimum_redstone_ticks: candidates
                            .iter()
                            .map(|range| range.minimum_redstone_ticks)
                            .min()
                            .unwrap_or(0),
                        maximum_redstone_ticks: candidates
                            .iter()
                            .map(|range| range.maximum_redstone_ticks)
                            .max()
                            .unwrap_or(0),
                    },
                );
                queue.push_back(edge.sink);
            }
        }
    }
    original_indegree
        .into_iter()
        .filter(|(_, degree)| *degree > 1)
        .filter_map(|(component, _)| {
            let candidates = incoming.get(&component)?;
            let minimum_redstone_ticks = candidates
                .iter()
                .map(|range| range.minimum_redstone_ticks)
                .min()?;
            let maximum_redstone_ticks = candidates
                .iter()
                .map(|range| range.maximum_redstone_ticks)
                .max()?;
            (minimum_redstone_ticks != maximum_redstone_ticks).then_some(
                TimingReason::UnequalReconvergentDelay {
                    component,
                    minimum_redstone_ticks,
                    maximum_redstone_ticks,
                },
            )
        })
        .collect()
}

fn steady_state_projection(circuit: &TimedCircuit) -> SteadyStateProjection {
    let mut incoming = BTreeMap::<ComponentId, Vec<&TimedEdge>>::new();
    let mut outgoing = BTreeMap::<ComponentId, Vec<&TimedEdge>>::new();
    for edge in &circuit.edges {
        incoming.entry(edge.sink).or_default().push(edge);
        outgoing.entry(edge.source).or_default().push(edge);
    }
    let kinds: BTreeMap<_, _> = circuit
        .nodes
        .iter()
        .map(|node| (node.component, node.kind))
        .collect();
    let compressible = |component: ComponentId| {
        matches!(
            kinds[&component],
            TemporalNodeKind::Wire | TemporalNodeKind::Conductor | TemporalNodeKind::Delay
        ) && incoming.get(&component).map_or(0, Vec::len) == 1
            && outgoing.get(&component).map_or(0, Vec::len) == 1
    };
    let retained_components = circuit
        .nodes
        .iter()
        .map(|node| node.component)
        .filter(|component| !compressible(*component))
        .collect::<BTreeSet<_>>();
    let compressed_components = circuit
        .nodes
        .iter()
        .map(|node| node.component)
        .filter(|component| compressible(*component))
        .collect();
    let mut edges = Vec::new();
    for source in &retained_components {
        for first in outgoing.get(source).into_iter().flatten() {
            let mut current = first.sink;
            let mut delay = first.delay;
            let mut inverted = first.behavior == EdgeBehavior::DelayedInvert;
            let mut path = vec![*source, current];
            let mut seen = BTreeSet::from([*source]);
            while compressible(current) && seen.insert(current) {
                let next = outgoing[&current][0];
                delay = delay.add(next.delay);
                inverted ^= next.behavior == EdgeBehavior::DelayedInvert;
                current = next.sink;
                path.push(current);
            }
            if retained_components.contains(&current) {
                edges.push(SteadyStateEdge {
                    source: *source,
                    sink: current,
                    inverted,
                    retained_delay: delay,
                    physical_path: path,
                });
            }
        }
    }
    SteadyStateProjection {
        retained_components,
        edges,
        compressed_components,
    }
}

fn classify_behavior_patterns(
    signal: &TemporalDependencyGraph,
    devices: &[TemporalDevice],
    timing: &TimingAssessment,
) -> Vec<BehaviorPattern> {
    let Some(feedback) = timing.reasons.iter().find_map(|reason| match reason {
        TimingReason::Feedback { components } => Some(components.clone()),
        _ => None,
    }) else {
        return (!devices.is_empty())
            .then_some(BehaviorPattern::DelayedPath)
            .into_iter()
            .collect();
    };
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

fn strongly_connected_components(
    nodes: &BTreeSet<ComponentId>,
    forward: &BTreeMap<ComponentId, Vec<ComponentId>>,
    reverse: &BTreeMap<ComponentId, Vec<ComponentId>>,
) -> Vec<BTreeSet<ComponentId>> {
    fn visit_order(
        node: ComponentId,
        adjacency: &BTreeMap<ComponentId, Vec<ComponentId>>,
        seen: &mut BTreeSet<ComponentId>,
        order: &mut Vec<ComponentId>,
    ) {
        if !seen.insert(node) {
            return;
        }
        for next in adjacency.get(&node).into_iter().flatten().copied() {
            visit_order(next, adjacency, seen, order);
        }
        order.push(node);
    }

    fn collect(
        node: ComponentId,
        adjacency: &BTreeMap<ComponentId, Vec<ComponentId>>,
        seen: &mut BTreeSet<ComponentId>,
        component: &mut BTreeSet<ComponentId>,
    ) {
        if !seen.insert(node) {
            return;
        }
        component.insert(node);
        for next in adjacency.get(&node).into_iter().flatten().copied() {
            collect(next, adjacency, seen, component);
        }
    }

    let mut seen = BTreeSet::new();
    let mut order = Vec::with_capacity(nodes.len());
    for node in nodes {
        visit_order(*node, forward, &mut seen, &mut order);
    }
    seen.clear();
    let mut result = Vec::new();
    while let Some(node) = order.pop() {
        if seen.contains(&node) {
            continue;
        }
        let mut component = BTreeSet::new();
        collect(node, reverse, &mut seen, &mut component);
        result.push(component);
    }
    result
}

const fn signal_kind(kind: BlockKind) -> TemporalNodeKind {
    match kind {
        BlockKind::Lever
        | BlockKind::Button
        | BlockKind::PressurePlate
        | BlockKind::RedstoneBlock => TemporalNodeKind::Source,
        BlockKind::RedstoneWire => TemporalNodeKind::Wire,
        BlockKind::RedstoneTorch => TemporalNodeKind::Inverter,
        BlockKind::Repeater => TemporalNodeKind::Delay,
        BlockKind::Comparator => TemporalNodeKind::Comparator,
        BlockKind::Observer => TemporalNodeKind::Observer,
        BlockKind::Piston => TemporalNodeKind::Actuator,
        BlockKind::Air | BlockKind::Solid | BlockKind::Transparent | BlockKind::RedstoneLamp => {
            TemporalNodeKind::Conductor
        }
    }
}

fn component_delay(kind: BlockKind, delay: Option<u8>) -> u8 {
    match kind {
        BlockKind::Repeater => delay.unwrap_or(1),
        BlockKind::RedstoneTorch | BlockKind::Observer => 1,
        // Keep the legacy scalar conservative. Piston timing is represented
        // by `TransitionDelay::Unavailable` until its phase profile is
        // measured and implemented; `1` would falsely claim one redstone
        // tick and hide the 1.5-tick/short-pulse boundary.
        BlockKind::Piston => 0,
        _ => 0,
    }
}

const fn temporal_semantics(kind: BlockKind) -> Option<TemporalSemantics> {
    match kind {
        BlockKind::Repeater => Some(TemporalSemantics::DelayedForward),
        BlockKind::RedstoneTorch => Some(TemporalSemantics::DelayedInvert),
        BlockKind::Comparator => Some(TemporalSemantics::CompareOrSubtract),
        BlockKind::Piston => Some(TemporalSemantics::MechanicalActuation),
        BlockKind::Observer => Some(TemporalSemantics::ObserverPulse),
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
        assert_eq!(
            projection.behavior.devices[0].minimum_delay_redstone_ticks,
            3
        );
        assert_eq!(
            projection.behavior.devices[0].transition_delay,
            TransitionDelay::ExactGameTicks { game_ticks: 6 }
        );
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
    fn projects_a_direct_piston_input_as_mechanical_and_temporal() {
        let mut piston = Block::new(BlockKind::Piston);
        piston.facing = Some(dustroute_physical::Facing::East);
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
                    block: piston,
                },
            ],
            [PhysicalConnection {
                source: ComponentId(0),
                sink: ComponentId(1),
                kind: ConnectionKind::PistonInput,
            }],
        );
        let analysis = analyze(&topology);
        assert_eq!(analysis.timing.scope, TemporalScope::TemporalRequired);
        assert!(analysis.timing.reasons.iter().any(|reason| matches!(
            reason,
            TimingReason::TransitionTimingUnavailable {
                component: ComponentId(1),
                block: BlockKind::Piston,
                ..
            }
        )));
        let edge = analysis
            .timed_circuit
            .edges
            .iter()
            .find(|edge| edge.source == ComponentId(0) && edge.sink == ComponentId(1))
            .expect("direct piston input edge");
        assert_eq!(edge.behavior, EdgeBehavior::Mechanical);
        assert!(matches!(
            edge.transition_delay,
            TransitionDelay::Unavailable { .. }
        ));
        let device = analysis
            .behavior
            .devices
            .iter()
            .find(|device| device.component == ComponentId(1))
            .expect("piston temporal device");
        assert_eq!(device.semantics, TemporalSemantics::MechanicalActuation);
        assert!(matches!(
            device.transition_delay,
            TransitionDelay::Unavailable { .. }
        ));
        assert_eq!(device.minimum_delay_redstone_ticks, 0);
    }

    #[test]
    fn immediate_edges_are_explicitly_same_game_tick() {
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
                    block: Block::new(BlockKind::RedstoneWire),
                },
            ],
            [PhysicalConnection {
                source: ComponentId(0),
                sink: ComponentId(1),
                kind: ConnectionKind::DirectSource,
            }],
        );
        let analysis = analyze(&topology);
        let edge = analysis
            .timed_circuit
            .edges
            .iter()
            .find(|edge| edge.source == ComponentId(0) && edge.sink == ComponentId(1))
            .expect("direct source edge");
        assert_eq!(edge.transition_delay, TransitionDelay::SameGameTick);
    }

    #[test]
    fn transition_delay_preserves_zero_and_variable_intervals() {
        let zero = TransitionDelay::ExactGameTicks { game_ticks: 0 };
        let variable = TransitionDelay::GameTickRange {
            minimum_game_ticks: 0,
            maximum_game_ticks: 3,
        };
        assert!(matches!(
            zero,
            TransitionDelay::ExactGameTicks { game_ticks: 0 }
        ));
        assert!(matches!(
            variable,
            TransitionDelay::GameTickRange {
                minimum_game_ticks: 0,
                maximum_game_ticks: 3
            }
        ));
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
        assert_eq!(projection.timing.scope, TemporalScope::TemporalRequired);
    }

    #[test]
    fn steady_state_projection_compresses_a_repeater_without_losing_delay() {
        let mut repeater = Block::new(BlockKind::Repeater);
        repeater.delay = Some(4);
        repeater.facing = Some(dustroute_physical::Facing::East);
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
                PhysicalComponent {
                    id: ComponentId(2),
                    pos: Pos::new(2, 64, 0),
                    block: Block::new(BlockKind::RedstoneWire),
                },
                PhysicalComponent {
                    id: ComponentId(3),
                    pos: Pos::new(3, 64, 0),
                    block: Block::new(BlockKind::RedstoneTorch),
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
                    kind: ConnectionKind::DirectionalOutput,
                },
                PhysicalConnection {
                    source: ComponentId(2),
                    sink: ComponentId(3),
                    kind: ConnectionKind::Dust,
                },
            ],
        );
        let analysis = analyze(&topology);
        assert_eq!(analysis.timing.scope, TemporalScope::SteadyStateSafe);
        assert!(
            analysis
                .steady_state
                .compressed_components
                .contains(&ComponentId(1))
        );
        let edge = &analysis.steady_state.edges[0];
        assert_eq!(edge.source, ComponentId(0));
        assert_eq!(edge.sink, ComponentId(2));
        assert_eq!(edge.retained_delay.minimum_redstone_ticks, 4);
        assert_eq!(
            edge.physical_path,
            vec![ComponentId(0), ComponentId(1), ComponentId(2)]
        );
    }

    #[test]
    fn unequal_delayed_paths_make_reconvergence_timing_sensitive() {
        let mut repeater = Block::new(BlockKind::Repeater);
        repeater.delay = Some(3);
        repeater.facing = Some(dustroute_physical::Facing::East);
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
                PhysicalComponent {
                    id: ComponentId(2),
                    pos: Pos::new(0, 64, 1),
                    block: Block::new(BlockKind::RedstoneWire),
                },
                PhysicalComponent {
                    id: ComponentId(3),
                    pos: Pos::new(2, 64, 0),
                    block: Block::new(BlockKind::RedstoneWire),
                },
                PhysicalComponent {
                    id: ComponentId(4),
                    pos: Pos::new(1, 64, 1),
                    block: Block::new(BlockKind::RedstoneWire),
                },
                PhysicalComponent {
                    id: ComponentId(5),
                    pos: Pos::new(2, 64, 1),
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
                    sink: ComponentId(3),
                    kind: ConnectionKind::DirectionalOutput,
                },
                PhysicalConnection {
                    source: ComponentId(0),
                    sink: ComponentId(2),
                    kind: ConnectionKind::DirectSource,
                },
                PhysicalConnection {
                    source: ComponentId(2),
                    sink: ComponentId(4),
                    kind: ConnectionKind::Dust,
                },
                PhysicalConnection {
                    source: ComponentId(4),
                    sink: ComponentId(5),
                    kind: ConnectionKind::Dust,
                },
                PhysicalConnection {
                    source: ComponentId(5),
                    sink: ComponentId(3),
                    kind: ConnectionKind::Dust,
                },
            ],
        );
        let analysis = analyze(&topology);
        assert_eq!(analysis.timing.scope, TemporalScope::TimingSensitive);
        assert!(analysis.timing.reasons.iter().any(|reason| matches!(
            reason,
            TimingReason::UnequalReconvergentDelay {
                component: ComponentId(3),
                minimum_redstone_ticks: 0,
                maximum_redstone_ticks: 3
            }
        )));
    }

    #[test]
    fn bidirectional_dust_is_a_net_not_temporal_feedback() {
        let topology = VerifiedTopology::from_parts(
            vec![
                PhysicalComponent {
                    id: ComponentId(0),
                    pos: Pos::new(0, 64, 0),
                    block: Block::new(BlockKind::RedstoneWire),
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
                    kind: ConnectionKind::Dust,
                },
                PhysicalConnection {
                    source: ComponentId(1),
                    sink: ComponentId(0),
                    kind: ConnectionKind::Dust,
                },
            ],
        );
        let analysis = analyze(&topology);
        assert_eq!(analysis.timing.scope, TemporalScope::SteadyStateSafe);
        assert!(
            analysis
                .timing
                .reasons
                .iter()
                .all(|reason| !matches!(reason, TimingReason::Feedback { .. }))
        );
    }
}
