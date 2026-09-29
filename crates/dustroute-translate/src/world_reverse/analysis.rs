//! Snapshot topology, interface evidence and graph diagnostics.
use super::{
    InferredTerminal, InterfaceEvidence, RegionAnalysis, RegionBounds, SignalComponent,
    SignalDiagnostics, TerminalConfidence,
};
use crate::connectivity::{
    ConnectivityEdge, PhysicalConnectivityGraph, build_physical_circuit, extract_connectivity,
};
use crate::world::{BlockKind, Pos, World};
use std::collections::{BTreeMap, BTreeSet};

#[must_use]
pub fn analyze_world_region(world: &World, bounds: RegionBounds) -> RegionAnalysis {
    analyze_world_region_in_dimension(world, bounds, "unknown")
}

#[must_use]
pub fn analyze_world_region_in_dimension(
    world: &World,
    bounds: RegionBounds,
    dimension: impl Into<String>,
) -> RegionAnalysis {
    let bounded = bounded_world(world, bounds);
    let graph = extract_connectivity(&bounded);
    let physical = build_physical_circuit(&bounded, &graph);
    let propagating_supports: BTreeSet<_> = graph.edges.iter().map(|edge| edge.source).collect();
    let functional_nodes: BTreeSet<_> = graph
        .nodes
        .iter()
        .copied()
        .filter(|pos| is_redstone_kind(bounded.kind_at(*pos)) || propagating_supports.contains(pos))
        .collect();
    let functional_edges: BTreeSet<_> = graph
        .edges
        .iter()
        .filter(|edge| {
            functional_nodes.contains(&edge.source) && functional_nodes.contains(&edge.sink)
        })
        .copied()
        .collect();
    let active_nodes: BTreeSet<_> = functional_edges
        .iter()
        .flat_map(|edge| [edge.source, edge.sink])
        .collect();
    let redstone_blocks = bounded
        .iter()
        .filter(|(_, block)| is_redstone_kind(block.kind) || block.requires_live_observation())
        .map(|(pos, block)| (*pos, block.kind))
        .collect();
    let unsupported = bounded
        .iter()
        .filter(|(_, block)| {
            matches!(block.kind, BlockKind::Comparator | BlockKind::Piston)
                || block.requires_live_observation()
        })
        .map(|(pos, block)| (*pos, block.kind))
        .collect();
    let raw_components = strongly_connected_components(&active_nodes, &functional_edges);
    let owner: BTreeMap<_, _> = raw_components
        .iter()
        .enumerate()
        .flat_map(|(id, positions)| positions.iter().map(move |pos| (*pos, id)))
        .collect();
    let mut components: Vec<_> = raw_components
        .into_iter()
        .enumerate()
        .map(|(id, positions)| SignalComponent {
            id,
            positions,
            incoming: BTreeSet::new(),
            outgoing: BTreeSet::new(),
        })
        .collect();
    for edge in &functional_edges {
        let (Some(source), Some(sink)) = (owner.get(&edge.source), owner.get(&edge.sink)) else {
            continue;
        };
        if source != sink {
            components[*source].outgoing.insert(*sink);
            components[*sink].incoming.insert(*source);
        }
    }
    let inferred_inputs: Vec<_> = components
        .iter()
        .filter(|component| component.incoming.is_empty() && !component.outgoing.is_empty())
        .filter_map(|component| infer_input(&bounded, component))
        .collect();
    let inferred_outputs: Vec<_> = components
        .iter()
        .filter(|component| component.outgoing.is_empty() && !component.incoming.is_empty())
        .filter_map(|component| infer_output(&bounded, component))
        .collect();
    let (buffered_inputs, buffered_outputs) =
        infer_buffered_boundaries(&bounded, &graph, &components, &owner);
    let (inputs, outputs) = if buffered_inputs.is_empty() || buffered_outputs.is_empty() {
        (inferred_inputs, inferred_outputs)
    } else {
        (buffered_inputs, buffered_outputs)
    };
    let interface = interface_evidence(&bounded, &graph, &components, &inputs, &outputs);
    let diagnostics = signal_diagnostics(
        &bounded,
        &graph,
        &components,
        &inputs,
        &outputs,
        &redstone_blocks,
    );
    let mut observation = dustroute_physical::Observation::complete(
        dimension,
        dustroute_physical::SceneBounds::new(bounds.min, bounds.max),
    );
    observation.frontier = observation_frontier(&physical, bounds);
    if !observation.frontier.is_empty() {
        observation.regions[0].completeness = dustroute_physical::RegionCompleteness::OpenBoundary;
    }
    let scene = dustroute_physical::PhysicalScene::from_topology_and_world(
        observation,
        &physical,
        &bounded,
    );
    RegionAnalysis {
        bounds,
        redstone_blocks,
        graph,
        scene,
        components,
        inputs,
        outputs,
        interface,
        unsupported,
        diagnostics,
    }
}

fn interface_evidence(
    world: &World,
    graph: &PhysicalConnectivityGraph,
    components: &[SignalComponent],
    inputs: &[InferredTerminal],
    outputs: &[InferredTerminal],
) -> InterfaceEvidence {
    let external_inputs: BTreeSet<_> = world
        .iter()
        .filter(|(_, block)| block.is_external_input_source())
        .map(|(pos, _)| *pos)
        .collect();
    let observable_outputs: BTreeSet<_> = world
        .iter()
        .filter(|(_, block)| block.is_observable_output())
        .map(|(pos, _)| *pos)
        .collect();
    let mapped_inputs: BTreeSet<_> = external_inputs
        .iter()
        .copied()
        .filter(|position| {
            inputs.iter().any(|terminal| {
                terminal.component < components.len()
                    && components[terminal.component].positions.contains(position)
            })
        })
        .collect();
    let mapped_outputs: BTreeSet<_> = observable_outputs
        .iter()
        .copied()
        .filter(|position| {
            outputs.iter().any(|terminal| {
                terminal.component < components.len()
                    && components[terminal.component].positions.contains(position)
            }) || graph.edges.iter().any(|edge| {
                edge.sink == *position
                    && outputs.iter().any(|terminal| {
                        terminal.component < components.len()
                            && components[terminal.component]
                                .positions
                                .contains(&edge.source)
                    })
            })
        })
        .collect();
    let unmapped_inputs = external_inputs
        .difference(&mapped_inputs)
        .copied()
        .collect();
    let unmapped_outputs = observable_outputs
        .difference(&mapped_outputs)
        .copied()
        .collect();
    InterfaceEvidence {
        external_inputs,
        mapped_inputs,
        unmapped_inputs,
        observable_outputs,
        mapped_outputs,
        unmapped_outputs,
    }
}

fn infer_buffered_boundaries(
    world: &World,
    graph: &PhysicalConnectivityGraph,
    components: &[SignalComponent],
    owner: &BTreeMap<Pos, usize>,
) -> (Vec<InferredTerminal>, Vec<InferredTerminal>) {
    let mut inputs = BTreeMap::<usize, InferredTerminal>::new();
    let mut outputs = BTreeMap::<usize, InferredTerminal>::new();
    for (repeater_pos, repeater) in world
        .iter()
        .filter(|(_, block)| block.kind == BlockKind::Repeater)
    {
        let Some(facing) = repeater.facing else {
            continue;
        };
        let Some(delta) = facing.horizontal_offset() else {
            continue;
        };
        let input_pos = repeater_pos.offset(-delta.x, 0, -delta.z);
        let output_pos = repeater_pos.offset(delta.x, 0, delta.z);
        if world.kind_at(input_pos) != BlockKind::RedstoneWire
            || world.kind_at(output_pos) != BlockKind::RedstoneWire
            || [input_pos, *repeater_pos, output_pos]
                .iter()
                .any(|pos| world.kind_at(pos.offset(0, -1, 0)) != BlockKind::Solid)
        {
            continue;
        }
        let cell_positions = BTreeSet::from([
            input_pos,
            *repeater_pos,
            output_pos,
            input_pos.offset(0, -1, 0),
            repeater_pos.offset(0, -1, 0),
            output_pos.offset(0, -1, 0),
        ]);
        let externally_connected = |position: Pos| {
            graph.edges.iter().any(|edge| {
                (edge.source == position && !cell_positions.contains(&edge.sink))
                    || (edge.sink == position && !cell_positions.contains(&edge.source))
            })
        };
        let input_connected = externally_connected(input_pos);
        let output_connected = externally_connected(output_pos);
        if !input_connected && output_connected {
            if let Some(component) = owner.get(&input_pos).copied() {
                inputs.insert(
                    component,
                    InferredTerminal {
                        anchor: input_pos,
                        component,
                        confidence: TerminalConfidence::Likely,
                    },
                );
            }
        } else if input_connected
            && !output_connected
            && let Some(component) = owner.get(&output_pos).copied()
        {
            outputs.insert(
                component,
                InferredTerminal {
                    anchor: output_pos,
                    component,
                    confidence: TerminalConfidence::Likely,
                },
            );
        }
    }
    let valid_component = |terminal: &InferredTerminal| terminal.component < components.len();
    (
        inputs.into_values().filter(valid_component).collect(),
        outputs.into_values().filter(valid_component).collect(),
    )
}

fn observation_frontier(
    physical: &dustroute_physical::VerifiedTopology,
    bounds: RegionBounds,
) -> Vec<dustroute_physical::ObservationFrontier> {
    let mut frontier = Vec::new();
    for component in physical
        .components
        .iter()
        .filter(|component| component.block.kind.is_redstone_related())
    {
        for (at_boundary, direction) in [
            (component.pos.x == bounds.min.x, crate::world::Facing::West),
            (component.pos.x == bounds.max.x, crate::world::Facing::East),
            (component.pos.y == bounds.min.y, crate::world::Facing::Down),
            (component.pos.y == bounds.max.y, crate::world::Facing::Up),
            (component.pos.z == bounds.min.z, crate::world::Facing::North),
            (component.pos.z == bounds.max.z, crate::world::Facing::South),
        ] {
            if at_boundary {
                frontier.push(dustroute_physical::ObservationFrontier {
                    position: component.pos,
                    direction,
                    reason: dustroute_physical::FrontierReason::ScanLimitReached,
                });
            }
        }
    }
    frontier
}

fn signal_diagnostics(
    world: &World,
    graph: &PhysicalConnectivityGraph,
    components: &[SignalComponent],
    inputs: &[InferredTerminal],
    outputs: &[InferredTerminal],
    redstone_blocks: &BTreeMap<Pos, BlockKind>,
) -> SignalDiagnostics {
    let incident: BTreeSet<_> = graph
        .edges
        .iter()
        .flat_map(|edge| [edge.source, edge.sink])
        .collect();
    let isolated_redstone = redstone_blocks
        .keys()
        .filter(|pos| !incident.contains(pos))
        .copied()
        .collect();
    let signal_islands = component_islands(components);
    let input_components: BTreeSet<_> = inputs.iter().map(|terminal| terminal.component).collect();
    let output_components: BTreeSet<_> =
        outputs.iter().map(|terminal| terminal.component).collect();
    let reachable = component_reachable(components, &input_components, false);
    let reaches_output = component_reachable(components, &output_components, true);
    SignalDiagnostics {
        isolated_redstone,
        signal_islands,
        unreachable_from_inputs: (0..components.len())
            .filter(|id| !reachable.contains(id))
            .collect(),
        cannot_reach_outputs: (0..components.len())
            .filter(|id| !reaches_output.contains(id))
            .collect(),
        invalid_supports: world.support_issues(),
        non_controllable_torches: world
            .iter()
            .filter(|(_, block)| block.kind == BlockKind::RedstoneTorch)
            .filter(|(pos, block)| {
                block
                    .support_pos(**pos)
                    .is_none_or(|support| !world.kind_at(support).properties().can_be_powered())
            })
            .map(|(pos, _)| *pos)
            .collect(),
    }
}

fn component_islands(components: &[SignalComponent]) -> Vec<BTreeSet<usize>> {
    let mut unseen: BTreeSet<_> = (0..components.len()).collect();
    let mut islands = Vec::new();
    while let Some(start) = unseen.pop_first() {
        let mut island = BTreeSet::new();
        let mut stack = vec![start];
        while let Some(id) = stack.pop() {
            if !island.insert(id) {
                continue;
            }
            unseen.remove(&id);
            stack.extend(components[id].incoming.iter().copied());
            stack.extend(components[id].outgoing.iter().copied());
        }
        islands.push(island);
    }
    islands
}

fn component_reachable(
    components: &[SignalComponent],
    starts: &BTreeSet<usize>,
    reverse: bool,
) -> BTreeSet<usize> {
    let mut seen = BTreeSet::new();
    let mut stack: Vec<_> = starts.iter().copied().collect();
    while let Some(id) = stack.pop() {
        if !seen.insert(id) {
            continue;
        }
        let next = if reverse {
            &components[id].incoming
        } else {
            &components[id].outgoing
        };
        stack.extend(next.iter().copied());
    }
    seen
}

fn bounded_world(world: &World, bounds: RegionBounds) -> World {
    let mut bounded = World::new();
    for (pos, block) in world.iter().filter(|(pos, _)| bounds.contains(**pos)) {
        bounded.set(*pos, block.clone());
    }
    bounded
}

const fn is_redstone_kind(kind: BlockKind) -> bool {
    matches!(
        kind,
        BlockKind::RedstoneWire
            | BlockKind::RedstoneTorch
            | BlockKind::Repeater
            | BlockKind::Comparator
            | BlockKind::Lever
            | BlockKind::Button
            | BlockKind::PressurePlate
            | BlockKind::RedstoneBlock
            | BlockKind::Observer
            | BlockKind::Piston
    )
}

fn infer_input(world: &World, component: &SignalComponent) -> Option<InferredTerminal> {
    let certain = component.positions.iter().copied().find(|pos| {
        matches!(
            world.kind_at(*pos),
            BlockKind::Lever
                | BlockKind::Button
                | BlockKind::PressurePlate
                | BlockKind::RedstoneBlock
        )
    });
    let likely = component
        .positions
        .iter()
        .copied()
        .find(|pos| world.kind_at(*pos) == BlockKind::RedstoneWire);
    certain
        .map(|anchor| InferredTerminal {
            anchor,
            component: component.id,
            confidence: TerminalConfidence::Certain,
        })
        .or_else(|| {
            likely.map(|anchor| InferredTerminal {
                anchor,
                component: component.id,
                confidence: TerminalConfidence::Likely,
            })
        })
}

fn infer_output(world: &World, component: &SignalComponent) -> Option<InferredTerminal> {
    let preferred = component
        .positions
        .iter()
        .copied()
        .filter(|pos| {
            matches!(
                world.kind_at(*pos),
                BlockKind::RedstoneWire | BlockKind::Repeater | BlockKind::Piston
            )
        })
        .max();
    preferred
        .or_else(|| {
            component
                .positions
                .iter()
                .copied()
                .find(|pos| world.kind_at(*pos) == BlockKind::RedstoneLamp)
        })
        .map(|anchor| InferredTerminal {
            anchor,
            component: component.id,
            confidence: TerminalConfidence::Likely,
        })
}

fn strongly_connected_components(
    nodes: &BTreeSet<Pos>,
    edges: &BTreeSet<ConnectivityEdge>,
) -> Vec<BTreeSet<Pos>> {
    let mut outgoing = BTreeMap::<Pos, Vec<Pos>>::new();
    let mut incoming = BTreeMap::<Pos, Vec<Pos>>::new();
    for edge in edges {
        outgoing.entry(edge.source).or_default().push(edge.sink);
        incoming.entry(edge.sink).or_default().push(edge.source);
    }
    let mut visited = BTreeSet::new();
    let mut order = Vec::new();
    for node in nodes {
        visit_order(*node, &outgoing, &mut visited, &mut order);
    }
    visited.clear();
    let mut result = Vec::new();
    while let Some(node) = order.pop() {
        if visited.contains(&node) {
            continue;
        }
        let mut component = BTreeSet::new();
        collect_component(node, &incoming, &mut visited, &mut component);
        result.push(component);
    }
    result
}

fn visit_order(
    node: Pos,
    adjacency: &BTreeMap<Pos, Vec<Pos>>,
    visited: &mut BTreeSet<Pos>,
    order: &mut Vec<Pos>,
) {
    if !visited.insert(node) {
        return;
    }
    for next in adjacency.get(&node).into_iter().flatten() {
        visit_order(*next, adjacency, visited, order);
    }
    order.push(node);
}

fn collect_component(
    node: Pos,
    adjacency: &BTreeMap<Pos, Vec<Pos>>,
    visited: &mut BTreeSet<Pos>,
    component: &mut BTreeSet<Pos>,
) {
    if !visited.insert(node) {
        return;
    }
    component.insert(node);
    for next in adjacency.get(&node).into_iter().flatten() {
        collect_component(*next, adjacency, visited, component);
    }
}
