use std::collections::{BTreeMap, BTreeSet};

use dustroute_physical::{
    BlockKind, ComponentId, Confidence, NetId, PhysicalEvidence, PhysicalScene, PortRef,
    TransferKind,
};
use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, Deserialize, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize)]
pub struct GateId(pub usize);

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum RecognizedGateKind {
    Buffer,
    Not,
    And,
    Or,
    Xor,
    Nand,
    Nor,
    Xnor,
    Comparator,
    Actuator,
    Unknown,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum RecognitionStatus {
    Complete,
    Partial,
    Conflicting,
    BoundaryLimited,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct RecognizedGate {
    pub id: GateId,
    pub kind: RecognizedGateKind,
    pub status: RecognitionStatus,
    pub inputs: Vec<PortRef>,
    pub outputs: Vec<PortRef>,
    pub physical_components: BTreeSet<ComponentId>,
    pub confidence: Confidence,
    pub evidence: Vec<GateEvidence>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum GateEvidence {
    DeviceKind {
        component: ComponentId,
        block: BlockKind,
    },
    ConvergingNet {
        net: NetId,
        sources: BTreeSet<ComponentId>,
    },
    Physical(PhysicalEvidence),
    OpenBoundary {
        components: BTreeSet<ComponentId>,
    },
    TruthTableInference {
        rows: usize,
    },
}

#[derive(Clone, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
pub struct GateView {
    pub gates: Vec<RecognizedGate>,
    pub unresolved_components: BTreeSet<ComponentId>,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize)]
pub struct ExpressionId(pub usize);

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(tag = "kind", content = "arguments", rename_all = "snake_case")]
pub enum DerivedExpr {
    Signal(PortRef),
    Buffer(Box<DerivedExpr>),
    Not(Box<DerivedExpr>),
    And(Vec<DerivedExpr>),
    Or(Vec<DerivedExpr>),
    Xor(Vec<DerivedExpr>),
    Unknown(Vec<PortRef>),
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct DerivedExpression {
    pub id: ExpressionId,
    pub gate: GateId,
    pub expression: DerivedExpr,
    pub physical_components: BTreeSet<ComponentId>,
    pub status: RecognitionStatus,
}

#[derive(Clone, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
pub struct ExpressionView {
    pub expressions: Vec<DerivedExpression>,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum FunctionalKind {
    HalfAdder,
    FullAdder,
    Multiplexer,
    Decoder,
    UnknownCombinational,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct FunctionalCandidate {
    pub kind: FunctionalKind,
    pub covered_gates: BTreeSet<GateId>,
    pub confidence: Confidence,
    pub status: RecognitionStatus,
    pub missing_features: Vec<String>,
    pub conflicts: Vec<String>,
}

#[derive(Clone, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
pub struct FunctionalView {
    pub candidates: Vec<FunctionalCandidate>,
}

#[must_use]
pub fn recognize_gates(scene: &PhysicalScene) -> GateView {
    let incoming = connections_by_component(scene, false);
    let outgoing = connections_by_component(scene, true);
    let frontier = scene.open_frontier_components();
    let mut gates = Vec::new();
    let mut covered = BTreeSet::new();
    let component_net = scene
        .nets
        .iter()
        .flat_map(|net| {
            net.components
                .iter()
                .map(move |component| (*component, net.id))
        })
        .collect::<BTreeMap<_, _>>();
    let mut external_inputs = BTreeMap::<NetId, Vec<_>>::new();
    let mut external_outputs = BTreeMap::<NetId, Vec<_>>::new();
    for connection in &scene.connections {
        let source_net = component_net.get(&connection.source.component);
        let sink_net = component_net.get(&connection.sink.component);
        if source_net == sink_net {
            continue;
        }
        if connection.transfer != TransferKind::StructuralSupport
            && let Some(net) = sink_net
        {
            external_inputs.entry(*net).or_default().push(connection);
        }
        if let Some(net) = source_net {
            external_outputs.entry(*net).or_default().push(connection);
        }
    }

    for component in &scene.components {
        let kind = match component.block.kind {
            BlockKind::RedstoneTorch => Some(RecognizedGateKind::Not),
            BlockKind::Repeater => Some(RecognizedGateKind::Buffer),
            BlockKind::Comparator => Some(RecognizedGateKind::Comparator),
            BlockKind::Piston => Some(RecognizedGateKind::Actuator),
            _ => None,
        };
        let Some(kind) = kind else { continue };
        let inputs = incoming.get(&component.id).cloned().unwrap_or_default();
        let outputs = outgoing.get(&component.id).cloned().unwrap_or_default();
        let components = BTreeSet::from([component.id]);
        let status = recognition_status(&components, &frontier, &inputs, &outputs);
        let confidence = if status == RecognitionStatus::Complete {
            Confidence::Certain
        } else {
            Confidence::Medium
        };
        covered.insert(component.id);
        gates.push(RecognizedGate {
            id: GateId(gates.len()),
            kind,
            status,
            inputs,
            outputs,
            physical_components: components,
            confidence,
            evidence: vec![GateEvidence::DeviceKind {
                component: component.id,
                block: component.block.kind,
            }],
        });
    }

    for net in &scene.nets {
        let external_inputs = external_inputs.get(&net.id).cloned().unwrap_or_default();
        let sources = external_inputs
            .iter()
            .map(|connection| connection.source.component)
            .collect::<BTreeSet<_>>();
        if sources.len() < 2 {
            continue;
        }
        let outputs = external_outputs
            .get(&net.id)
            .into_iter()
            .flatten()
            .map(|connection| connection.source)
            .collect::<Vec<_>>();
        let inputs = external_inputs
            .iter()
            .map(|connection| connection.sink)
            .collect::<Vec<_>>();
        let status = recognition_status(&net.components, &frontier, &inputs, &outputs);
        covered.extend(&net.components);
        gates.push(RecognizedGate {
            id: GateId(gates.len()),
            kind: RecognizedGateKind::Or,
            status,
            inputs,
            outputs,
            physical_components: net.components.clone(),
            confidence: if status == RecognitionStatus::Complete {
                Confidence::High
            } else {
                Confidence::Medium
            },
            evidence: vec![GateEvidence::ConvergingNet {
                net: net.id,
                sources,
            }],
        });
    }

    let unresolved_components = scene
        .components
        .iter()
        .filter(|component| component.block.kind.is_redstone_related())
        .map(|component| component.id)
        .filter(|component| !covered.contains(component))
        .collect();
    GateView {
        gates,
        unresolved_components,
    }
}

#[must_use]
pub fn derive_expressions(scene: &PhysicalScene, gates: &GateView) -> ExpressionView {
    let drivers = scene
        .connections
        .iter()
        .map(|connection| (connection.sink, connection.source))
        .collect::<BTreeMap<_, _>>();
    let incoming_sources = scene.connections.iter().fold(
        BTreeMap::<ComponentId, Vec<PortRef>>::new(),
        |mut incoming, connection| {
            incoming
                .entry(connection.sink.component)
                .or_default()
                .push(connection.source);
            incoming
        },
    );
    let output_gate = gates
        .gates
        .iter()
        .flat_map(|gate| gate.outputs.iter().map(move |output| (*output, gate.id)))
        .collect::<BTreeMap<_, _>>();
    let by_id = gates
        .gates
        .iter()
        .map(|gate| (gate.id, gate))
        .collect::<BTreeMap<_, _>>();
    let mut memo = BTreeMap::new();
    let mut expressions = Vec::new();
    for gate in &gates.gates {
        let mut visiting = BTreeSet::new();
        let expression = expression_for_gate(
            gate.id,
            &by_id,
            &drivers,
            &incoming_sources,
            &output_gate,
            &mut visiting,
            &mut memo,
            0,
        );
        expressions.push(DerivedExpression {
            id: ExpressionId(gate.id.0),
            gate: gate.id,
            expression: simplify_expression(expression),
            physical_components: gate.physical_components.clone(),
            status: gate.status,
        });
    }
    ExpressionView { expressions }
}

#[must_use]
pub fn classify_function(gates: &GateView, expressions: &ExpressionView) -> FunctionalView {
    let ands = expressions
        .expressions
        .iter()
        .filter(|expression| matches!(expression.expression, DerivedExpr::And(_)))
        .collect::<Vec<_>>();
    let mut xors_by_signals = BTreeMap::<BTreeSet<PortRef>, Vec<&DerivedExpression>>::new();
    for expression in &expressions.expressions {
        if matches!(expression.expression, DerivedExpr::Xor(_)) {
            xors_by_signals
                .entry(expression_signals(&expression.expression))
                .or_default()
                .push(expression);
        }
    }
    let mut candidates = Vec::new();
    for and in &ands {
        let signals = expression_signals(&and.expression);
        for xor in xors_by_signals.get(&signals).into_iter().flatten() {
            let covered_gates = BTreeSet::from([and.gate, xor.gate]);
            let boundary_limited = covered_gates.iter().any(|id| {
                gates
                    .gates
                    .get(id.0)
                    .is_some_and(|gate| gate.status == RecognitionStatus::BoundaryLimited)
            });
            candidates.push(FunctionalCandidate {
                kind: FunctionalKind::HalfAdder,
                covered_gates,
                confidence: if boundary_limited {
                    Confidence::Medium
                } else {
                    Confidence::High
                },
                status: if boundary_limited {
                    RecognitionStatus::BoundaryLimited
                } else {
                    RecognitionStatus::Complete
                },
                missing_features: Vec::new(),
                conflicts: Vec::new(),
            });
        }
    }
    FunctionalView { candidates }
}

#[allow(clippy::too_many_arguments)]
fn expression_for_gate(
    id: GateId,
    gates: &BTreeMap<GateId, &RecognizedGate>,
    drivers: &BTreeMap<PortRef, PortRef>,
    incoming_sources: &BTreeMap<ComponentId, Vec<PortRef>>,
    output_gate: &BTreeMap<PortRef, GateId>,
    visiting: &mut BTreeSet<GateId>,
    memo: &mut BTreeMap<GateId, DerivedExpr>,
    depth: usize,
) -> DerivedExpr {
    let Some(gate) = gates.get(&id) else {
        return DerivedExpr::Unknown(Vec::new());
    };
    if let Some(expression) = memo.get(&id) {
        return expression.clone();
    }
    if depth >= 8 {
        return DerivedExpr::Unknown(gate.inputs.clone());
    }
    if !visiting.insert(id) {
        return DerivedExpr::Unknown(gate.inputs.clone());
    }
    let inputs = gate
        .inputs
        .iter()
        .map(|input| {
            let source = drivers.get(input).copied().unwrap_or(*input);
            expression_for_source(
                source,
                gates,
                drivers,
                incoming_sources,
                output_gate,
                visiting,
                memo,
                &mut BTreeSet::new(),
                depth + 1,
            )
        })
        .collect::<Vec<_>>();
    visiting.remove(&id);
    let expression = match gate.kind {
        RecognizedGateKind::Buffer => inputs
            .into_iter()
            .next()
            .map(|input| DerivedExpr::Buffer(Box::new(input)))
            .unwrap_or_else(|| DerivedExpr::Unknown(gate.inputs.clone())),
        RecognizedGateKind::Not => inputs
            .into_iter()
            .next()
            .map(|input| DerivedExpr::Not(Box::new(input)))
            .unwrap_or_else(|| DerivedExpr::Unknown(gate.inputs.clone())),
        RecognizedGateKind::And => DerivedExpr::And(inputs),
        RecognizedGateKind::Or => DerivedExpr::Or(inputs),
        RecognizedGateKind::Xor => DerivedExpr::Xor(inputs),
        RecognizedGateKind::Nand => DerivedExpr::Not(Box::new(DerivedExpr::And(inputs))),
        RecognizedGateKind::Nor => DerivedExpr::Not(Box::new(DerivedExpr::Or(inputs))),
        _ => DerivedExpr::Unknown(gate.inputs.clone()),
    };
    memo.insert(id, expression.clone());
    expression
}

#[allow(clippy::too_many_arguments)]
fn expression_for_source(
    source: PortRef,
    gates: &BTreeMap<GateId, &RecognizedGate>,
    drivers: &BTreeMap<PortRef, PortRef>,
    incoming_sources: &BTreeMap<ComponentId, Vec<PortRef>>,
    output_gate: &BTreeMap<PortRef, GateId>,
    visiting_gates: &mut BTreeSet<GateId>,
    memo: &mut BTreeMap<GateId, DerivedExpr>,
    visiting_components: &mut BTreeSet<ComponentId>,
    depth: usize,
) -> DerivedExpr {
    if depth >= 8 {
        return DerivedExpr::Signal(source);
    }
    if let Some(gate) = output_gate.get(&source) {
        return expression_for_gate(
            *gate,
            gates,
            drivers,
            incoming_sources,
            output_gate,
            visiting_gates,
            memo,
            depth,
        );
    }
    if !visiting_components.insert(source.component) {
        return DerivedExpr::Signal(source);
    }
    let upstream = incoming_sources
        .get(&source.component)
        .into_iter()
        .flatten()
        .copied()
        .filter(|candidate| *candidate != source)
        .map(|candidate| {
            expression_for_source(
                candidate,
                gates,
                drivers,
                incoming_sources,
                output_gate,
                visiting_gates,
                memo,
                visiting_components,
                depth + 1,
            )
        })
        .collect::<Vec<_>>();
    visiting_components.remove(&source.component);
    match upstream.len() {
        0 => DerivedExpr::Signal(source),
        1 => upstream.into_iter().next().unwrap(),
        _ => DerivedExpr::Or(upstream),
    }
}

fn simplify_expression(expression: DerivedExpr) -> DerivedExpr {
    match expression {
        DerivedExpr::Not(inner) => match simplify_expression(*inner) {
            DerivedExpr::Not(inner) => simplify_expression(*inner),
            DerivedExpr::Or(inputs)
                if inputs
                    .iter()
                    .all(|input| matches!(input, DerivedExpr::Not(_))) =>
            {
                DerivedExpr::And(
                    inputs
                        .into_iter()
                        .map(|input| match input {
                            DerivedExpr::Not(inner) => *inner,
                            _ => unreachable!(),
                        })
                        .collect(),
                )
            }
            inner => DerivedExpr::Not(Box::new(inner)),
        },
        DerivedExpr::Buffer(inner) => simplify_expression(*inner),
        DerivedExpr::And(inputs) => {
            DerivedExpr::And(inputs.into_iter().map(simplify_expression).collect())
        }
        DerivedExpr::Or(inputs) => {
            DerivedExpr::Or(inputs.into_iter().map(simplify_expression).collect())
        }
        DerivedExpr::Xor(inputs) => {
            DerivedExpr::Xor(inputs.into_iter().map(simplify_expression).collect())
        }
        other => other,
    }
}

fn expression_signals(expression: &DerivedExpr) -> BTreeSet<PortRef> {
    match expression {
        DerivedExpr::Signal(signal) => BTreeSet::from([*signal]),
        DerivedExpr::Buffer(inner) | DerivedExpr::Not(inner) => expression_signals(inner),
        DerivedExpr::And(inputs) | DerivedExpr::Or(inputs) | DerivedExpr::Xor(inputs) => {
            inputs.iter().flat_map(expression_signals).collect()
        }
        DerivedExpr::Unknown(signals) => signals.iter().copied().collect(),
    }
}

fn connections_by_component(
    scene: &PhysicalScene,
    outgoing: bool,
) -> BTreeMap<ComponentId, Vec<PortRef>> {
    let mut result: BTreeMap<ComponentId, Vec<PortRef>> = BTreeMap::new();
    for connection in &scene.connections {
        let endpoint = if outgoing {
            connection.source
        } else {
            connection.sink
        };
        result.entry(endpoint.component).or_default().push(endpoint);
    }
    result
}

fn recognition_status(
    components: &BTreeSet<ComponentId>,
    frontier: &BTreeSet<ComponentId>,
    inputs: &[PortRef],
    outputs: &[PortRef],
) -> RecognitionStatus {
    if !components.is_disjoint(frontier) {
        RecognitionStatus::BoundaryLimited
    } else if inputs.is_empty() || outputs.is_empty() {
        RecognitionStatus::Partial
    } else {
        RecognitionStatus::Complete
    }
}

#[cfg(test)]
mod tests {
    use dustroute_physical::{
        Block, ConnectionKind, Facing, Observation, PhysicalComponent, PhysicalConnection,
        SceneBounds, VerifiedTopology,
    };

    use super::*;

    #[test]
    fn recognizes_a_torch_as_a_traceable_not_gate() {
        let support = PhysicalComponent {
            id: ComponentId(0),
            pos: dustroute_physical::Pos::new(0, 0, 0),
            block: Block::new(BlockKind::Solid),
        };
        let mut torch_block = Block::new(BlockKind::RedstoneTorch);
        torch_block.support_offset = Some(dustroute_physical::Pos::new(0, -1, 0));
        let torch = PhysicalComponent {
            id: ComponentId(1),
            pos: dustroute_physical::Pos::new(0, 1, 0),
            block: torch_block,
        };
        let wire = PhysicalComponent {
            id: ComponentId(2),
            pos: dustroute_physical::Pos::new(1, 1, 0),
            block: Block::new(BlockKind::RedstoneWire),
        };
        let topology = VerifiedTopology::from_parts(
            vec![support, torch, wire],
            [
                PhysicalConnection {
                    source: ComponentId(0),
                    sink: ComponentId(1),
                    kind: ConnectionKind::Control,
                },
                PhysicalConnection {
                    source: ComponentId(1),
                    sink: ComponentId(2),
                    kind: ConnectionKind::DirectSource,
                },
            ],
        );
        let scene = PhysicalScene::from_unvalidated_topology(
            Observation::complete(
                "minecraft:overworld",
                SceneBounds::new(
                    dustroute_physical::Pos::new(-1, -1, -1),
                    dustroute_physical::Pos::new(2, 2, 1),
                ),
            ),
            &topology,
        );
        let view = recognize_gates(&scene);
        let gate = view
            .gates
            .iter()
            .find(|gate| gate.kind == RecognizedGateKind::Not)
            .unwrap();
        assert_eq!(gate.status, RecognitionStatus::Complete);
        assert_eq!(gate.physical_components, BTreeSet::from([ComponentId(1)]));
    }

    #[test]
    fn boundary_limited_device_is_not_claimed_complete() {
        let mut repeater = Block::new(BlockKind::Repeater);
        repeater.facing = Some(Facing::East);
        let topology = VerifiedTopology::from_parts(
            vec![PhysicalComponent {
                id: ComponentId(0),
                pos: dustroute_physical::Pos::new(0, 1, 0),
                block: repeater,
            }],
            [],
        );
        let observation = dustroute_physical::Observation {
            dimension: "minecraft:overworld".to_owned(),
            regions: vec![dustroute_physical::ObservedRegion {
                bounds: SceneBounds::new(
                    dustroute_physical::Pos::new(0, 0, 0),
                    dustroute_physical::Pos::new(1, 2, 1),
                ),
                completeness: dustroute_physical::RegionCompleteness::OpenBoundary,
            }],
            frontier: vec![dustroute_physical::ObservationFrontier {
                position: dustroute_physical::Pos::new(0, 1, 0),
                direction: Facing::West,
                reason: dustroute_physical::FrontierReason::ScanLimitReached,
            }],
        };
        let view = recognize_gates(&PhysicalScene::from_unvalidated_topology(
            observation,
            &topology,
        ));
        assert_eq!(view.gates[0].status, RecognitionStatus::BoundaryLimited);
    }

    #[test]
    fn de_morgan_chain_is_exposed_as_and_without_losing_signals() {
        let a = PortRef {
            component: ComponentId(1),
            port: dustroute_physical::PortId(0),
        };
        let b = PortRef {
            component: ComponentId(2),
            port: dustroute_physical::PortId(0),
        };
        let expression = DerivedExpr::Not(Box::new(DerivedExpr::Or(vec![
            DerivedExpr::Not(Box::new(DerivedExpr::Signal(a))),
            DerivedExpr::Not(Box::new(DerivedExpr::Signal(b))),
        ])));
        let simplified = simplify_expression(expression);
        assert!(matches!(simplified, DerivedExpr::And(_)));
        assert_eq!(expression_signals(&simplified), BTreeSet::from([a, b]));
    }

    #[test]
    fn half_adder_is_optional_metadata_over_gate_expressions() {
        let a = PortRef {
            component: ComponentId(1),
            port: dustroute_physical::PortId(0),
        };
        let b = PortRef {
            component: ComponentId(2),
            port: dustroute_physical::PortId(0),
        };
        let gates = GateView {
            gates: vec![
                RecognizedGate {
                    id: GateId(0),
                    kind: RecognizedGateKind::And,
                    status: RecognitionStatus::Complete,
                    inputs: vec![a, b],
                    outputs: Vec::new(),
                    physical_components: BTreeSet::new(),
                    confidence: Confidence::High,
                    evidence: Vec::new(),
                },
                RecognizedGate {
                    id: GateId(1),
                    kind: RecognizedGateKind::Xor,
                    status: RecognitionStatus::Complete,
                    inputs: vec![a, b],
                    outputs: Vec::new(),
                    physical_components: BTreeSet::new(),
                    confidence: Confidence::High,
                    evidence: Vec::new(),
                },
            ],
            unresolved_components: BTreeSet::new(),
        };
        let expressions = ExpressionView {
            expressions: vec![
                DerivedExpression {
                    id: ExpressionId(0),
                    gate: GateId(0),
                    expression: DerivedExpr::And(vec![
                        DerivedExpr::Signal(a),
                        DerivedExpr::Signal(b),
                    ]),
                    physical_components: BTreeSet::new(),
                    status: RecognitionStatus::Complete,
                },
                DerivedExpression {
                    id: ExpressionId(1),
                    gate: GateId(1),
                    expression: DerivedExpr::Xor(vec![
                        DerivedExpr::Signal(a),
                        DerivedExpr::Signal(b),
                    ]),
                    physical_components: BTreeSet::new(),
                    status: RecognitionStatus::Complete,
                },
            ],
        };
        let functions = classify_function(&gates, &expressions);
        assert_eq!(functions.candidates[0].kind, FunctionalKind::HalfAdder);
    }
}
