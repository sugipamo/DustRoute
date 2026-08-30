use std::collections::{BTreeMap, BTreeSet, VecDeque};

use crate::wire::{dust_connected, wire_has_arm};
use crate::world::{BlockKind, Facing, Pos, World};
use dustroute_physical::{
    ComponentId, ConnectionKind, PhysicalComponent, PhysicalConnection, VerifiedTopology,
};

#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum PhysicalStepKind {
    Dust,
    DustToRepeater,
    RepeaterToDust,
    DustToBlock,
    BlockToRepeater,
    RepeaterToBlock,
    SourceToDust,
}

#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct PhysicalStep {
    pub source: Pos,
    pub sink: Pos,
    pub kind: PhysicalStepKind,
}

#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum EdgeKind {
    Dust,
    DustToBlockWeak,
    BlockToDustStrong,
    BlockToRepeater,
    RepeaterInput,
    RepeaterOutput,
    DirectSource,
    TorchControl,
    LeverOutput,
}

#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct ConnectivityEdge {
    pub source: Pos,
    pub sink: Pos,
    pub kind: EdgeKind,
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct PhysicalConnectivityGraph {
    pub nodes: BTreeSet<Pos>,
    pub edges: BTreeSet<ConnectivityEdge>,
}

impl PhysicalConnectivityGraph {
    #[must_use]
    pub fn reachable_from(&self, source: Pos) -> BTreeSet<Pos> {
        let mut adjacency: BTreeMap<Pos, Vec<Pos>> = BTreeMap::new();
        for edge in &self.edges {
            adjacency.entry(edge.source).or_default().push(edge.sink);
        }
        let mut seen = BTreeSet::from([source]);
        let mut queue = VecDeque::from([source]);
        while let Some(current) = queue.pop_front() {
            for next in adjacency.get(&current).into_iter().flatten() {
                if seen.insert(*next) {
                    queue.push_back(*next);
                }
            }
        }
        seen
    }

    #[must_use]
    pub fn can_reach(&self, source: Pos, sink: Pos) -> bool {
        self.reachable_from(source).contains(&sink)
    }

    #[must_use]
    pub fn conductive_components(&self) -> Vec<BTreeSet<Pos>> {
        let mut adjacency: BTreeMap<Pos, BTreeSet<Pos>> = BTreeMap::new();
        for edge in self.edges.iter().filter(|edge| edge.kind == EdgeKind::Dust) {
            adjacency.entry(edge.source).or_default().insert(edge.sink);
            adjacency.entry(edge.sink).or_default().insert(edge.source);
        }
        let mut seen = BTreeSet::new();
        let mut result = Vec::new();
        for start in adjacency.keys() {
            if seen.contains(start) {
                continue;
            }
            let mut component = BTreeSet::new();
            let mut stack = vec![*start];
            seen.insert(*start);
            while let Some(current) = stack.pop() {
                component.insert(current);
                for next in adjacency.get(&current).into_iter().flatten() {
                    if seen.insert(*next) {
                        stack.push(*next);
                    }
                }
            }
            result.push(component);
        }
        result
    }
}

fn horizontal_facing_between(source: Pos, sink: Pos) -> Option<Facing> {
    if source.y != sink.y {
        return None;
    }
    match (sink.x - source.x, sink.z - source.z) {
        (1, 0) => Some(Facing::East),
        (-1, 0) => Some(Facing::West),
        (0, 1) => Some(Facing::South),
        (0, -1) => Some(Facing::North),
        _ => None,
    }
}

#[must_use]
pub fn repeater_input_pos(world: &World, pos: Pos) -> Option<Pos> {
    let block = world.get(pos)?;
    if block.kind != BlockKind::Repeater {
        return None;
    }
    let delta = block.facing?.opposite().horizontal_offset()?;
    Some(pos.offset(delta.x, 0, delta.z))
}

#[must_use]
pub fn repeater_output_pos(world: &World, pos: Pos) -> Option<Pos> {
    let block = world.get(pos)?;
    if block.kind != BlockKind::Repeater {
        return None;
    }
    let delta = block.facing?.horizontal_offset()?;
    Some(pos.offset(delta.x, 0, delta.z))
}

#[must_use]
pub fn physical_step(world: &World, source: Pos, sink: Pos) -> Option<PhysicalStep> {
    let a = world.kind_at(source);
    let b = world.kind_at(sink);
    let kind = if a == BlockKind::RedstoneWire && b == BlockKind::RedstoneWire {
        dust_connected(world, source, sink).then_some(PhysicalStepKind::Dust)
    } else if a == BlockKind::RedstoneWire && b == BlockKind::Repeater {
        (repeater_input_pos(world, sink) == Some(source))
            .then_some(PhysicalStepKind::DustToRepeater)
    } else if a == BlockKind::Repeater && b == BlockKind::RedstoneWire {
        (repeater_output_pos(world, source) == Some(sink))
            .then_some(PhysicalStepKind::RepeaterToDust)
    } else if a == BlockKind::RedstoneWire && b.properties().receives_weak_power {
        (sink == source.offset(0, -1, 0) || horizontal_facing_between(source, sink).is_some())
            .then_some(PhysicalStepKind::DustToBlock)
    } else if a.properties().repeater_reads_block_power && b == BlockKind::Repeater {
        (repeater_input_pos(world, sink) == Some(source))
            .then_some(PhysicalStepKind::BlockToRepeater)
    } else if a == BlockKind::Repeater && b.properties().receives_strong_power {
        (repeater_output_pos(world, source) == Some(sink))
            .then_some(PhysicalStepKind::RepeaterToBlock)
    } else if matches!(
        a,
        BlockKind::RedstoneBlock | BlockKind::Lever | BlockKind::RedstoneTorch
    ) && b == BlockKind::RedstoneWire
    {
        horizontal_facing_between(source, sink)
            .filter(|facing| wire_has_arm(world, sink, facing.opposite()))
            .map(|_| PhysicalStepKind::SourceToDust)
    } else {
        None
    }?;
    Some(PhysicalStep { source, sink, kind })
}

#[must_use]
pub fn physical_step_connected(world: &World, source: Pos, sink: Pos) -> bool {
    physical_step(world, source, sink).is_some()
}

#[must_use]
pub fn extract_connectivity(world: &World) -> PhysicalConnectivityGraph {
    let nodes = world.positions().collect();
    let positions: Vec<_> = world.positions().collect();
    let mut edges = BTreeSet::new();
    for source in &positions {
        for sink in &positions {
            if source == sink {
                continue;
            }
            if let Some(step) = physical_step(world, *source, *sink) {
                let kind = match step.kind {
                    PhysicalStepKind::Dust => EdgeKind::Dust,
                    PhysicalStepKind::DustToRepeater => EdgeKind::RepeaterInput,
                    PhysicalStepKind::RepeaterToDust | PhysicalStepKind::RepeaterToBlock => {
                        EdgeKind::RepeaterOutput
                    }
                    PhysicalStepKind::DustToBlock => EdgeKind::DustToBlockWeak,
                    PhysicalStepKind::BlockToRepeater => EdgeKind::BlockToRepeater,
                    PhysicalStepKind::SourceToDust => EdgeKind::DirectSource,
                };
                edges.insert(ConnectivityEdge {
                    source: *source,
                    sink: *sink,
                    kind,
                });
            }
        }
    }
    for (pos, block) in world.iter() {
        if block.kind == BlockKind::RedstoneTorch {
            if let Some(support) = block.support_pos(*pos) {
                edges.insert(ConnectivityEdge {
                    source: support,
                    sink: *pos,
                    kind: EdgeKind::TorchControl,
                });
            }
        }
        if block.kind == BlockKind::Lever {
            if let Some(support) = block.support_pos(*pos) {
                edges.insert(ConnectivityEdge {
                    source: *pos,
                    sink: support,
                    kind: EdgeKind::LeverOutput,
                });
            }
        }
    }
    PhysicalConnectivityGraph { nodes, edges }
}

#[must_use]
pub fn build_physical_circuit(
    world: &World,
    graph: &PhysicalConnectivityGraph,
) -> VerifiedTopology {
    let edge_positions: BTreeSet<_> = graph
        .edges
        .iter()
        .flat_map(|edge| [edge.source, edge.sink])
        .collect();
    let positions: Vec<_> = world
        .iter()
        .filter_map(|(pos, block)| {
            let redstone_related = matches!(
                block.kind,
                BlockKind::RedstoneWire
                    | BlockKind::RedstoneTorch
                    | BlockKind::Repeater
                    | BlockKind::Comparator
                    | BlockKind::Lever
                    | BlockKind::RedstoneBlock
            );
            (redstone_related || edge_positions.contains(pos)).then_some(*pos)
        })
        .collect();
    let ids: BTreeMap<_, _> = positions
        .iter()
        .enumerate()
        .map(|(id, pos)| (*pos, ComponentId(id)))
        .collect();
    let components = positions
        .iter()
        .filter_map(|pos| {
            world.get(*pos).cloned().map(|block| PhysicalComponent {
                id: ids[pos],
                pos: *pos,
                block,
            })
        })
        .collect();
    let connections = graph.edges.iter().filter_map(|edge| {
        let source = *ids.get(&edge.source)?;
        let sink = *ids.get(&edge.sink)?;
        let kind = match edge.kind {
            EdgeKind::Dust => ConnectionKind::Dust,
            EdgeKind::DustToBlockWeak => ConnectionKind::WeakPower,
            EdgeKind::BlockToDustStrong => ConnectionKind::StrongPower,
            EdgeKind::BlockToRepeater | EdgeKind::RepeaterInput => ConnectionKind::DirectionalInput,
            EdgeKind::RepeaterOutput => ConnectionKind::DirectionalOutput,
            EdgeKind::DirectSource | EdgeKind::LeverOutput => ConnectionKind::DirectSource,
            EdgeKind::TorchControl => ConnectionKind::Control,
        };
        Some(PhysicalConnection { source, sink, kind })
    });
    VerifiedTopology::from_parts(components, connections)
}

#[cfg(test)]
mod tests {
    use crate::wire::update_wire_shapes;
    use crate::world::Block;

    use super::*;

    #[test]
    fn repeater_is_directional() {
        let mut world = World::new();
        world.fill(
            Pos::new(0, 0, 0),
            Pos::new(2, 0, 0),
            Block::new(BlockKind::Solid),
        );
        world.place(BlockKind::RedstoneWire, Pos::new(0, 1, 0));
        let repeater = world.place(BlockKind::Repeater, Pos::new(1, 1, 0));
        repeater.facing = Some(Facing::East);
        repeater.delay = Some(1);
        world.place(BlockKind::RedstoneWire, Pos::new(2, 1, 0));
        update_wire_shapes(&mut world);
        let graph = extract_connectivity(&world);
        assert!(graph.can_reach(Pos::new(0, 1, 0), Pos::new(2, 1, 0)));
        assert!(!graph.can_reach(Pos::new(2, 1, 0), Pos::new(0, 1, 0)));
    }

    #[test]
    fn detects_dust_stair() {
        let mut world = World::new();
        world.set(Pos::new(0, 0, 0), Block::new(BlockKind::Solid));
        world.set(Pos::new(1, 1, 0), Block::new(BlockKind::Solid));
        world.place(BlockKind::RedstoneWire, Pos::new(0, 1, 0));
        world.place(BlockKind::RedstoneWire, Pos::new(1, 2, 0));
        update_wire_shapes(&mut world);
        assert!(physical_step_connected(
            &world,
            Pos::new(0, 1, 0),
            Pos::new(1, 2, 0)
        ));
        assert!(physical_step_connected(
            &world,
            Pos::new(1, 2, 0),
            Pos::new(0, 1, 0)
        ));
    }
}
