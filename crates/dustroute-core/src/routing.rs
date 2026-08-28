use std::cmp::Reverse;
use std::collections::{BTreeMap, BinaryHeap};
use std::error::Error;
use std::fmt::{Display, Formatter};

use crate::world::{Block, BlockKind, Facing, Pos, World};

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RouteResult {
    pub path: Vec<Pos>,
    /// Cost in tenths; integer costs keep queue ordering deterministic.
    pub cost: u32,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct RouterConfig {
    pub max_nodes: usize,
    pub horizontal_cost: u32,
    pub stair_cost: u32,
    pub new_support_cost: u32,
}

impl Default for RouterConfig {
    fn default() -> Self {
        Self {
            max_nodes: 30_000,
            horizontal_cost: 10,
            stair_cost: 25,
            new_support_cost: 15,
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum RouteNotFound {
    InvalidEndpoint(Pos),
    Exhausted {
        start: Pos,
        goal: Pos,
        expanded: usize,
    },
}

impl Display for RouteNotFound {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::InvalidEndpoint(pos) => {
                write!(f, "invalid route endpoint {},{},{}", pos.x, pos.y, pos.z)
            }
            Self::Exhausted {
                start,
                goal,
                expanded,
            } => write!(f, "no route {start:?} -> {goal:?} after {expanded} nodes"),
        }
    }
}

impl Error for RouteNotFound {}

fn heuristic(a: Pos, b: Pos) -> u32 {
    a.x.abs_diff(b.x) + a.y.abs_diff(b.y) + a.z.abs_diff(b.z)
}

fn moves(pos: Pos) -> impl Iterator<Item = Pos> {
    [(1, 0), (-1, 0), (0, 1), (0, -1)]
        .into_iter()
        .flat_map(move |(dx, dz)| [-1, 0, 1].into_iter().map(move |dy| pos.offset(dx, dy, dz)))
}

fn routeable(world: &World, pos: Pos, start: Pos, goal: Pos) -> bool {
    let kind = world.kind_at(pos);
    if pos == start || pos == goal {
        matches!(kind, BlockKind::Air | BlockKind::RedstoneWire)
    } else {
        kind == BlockKind::Air
    }
}

pub fn astar_route(
    world: &World,
    start: Pos,
    goal: Pos,
    config: RouterConfig,
) -> Result<RouteResult, RouteNotFound> {
    if !routeable(world, start, start, goal) {
        return Err(RouteNotFound::InvalidEndpoint(start));
    }
    if !routeable(world, goal, start, goal) {
        return Err(RouteNotFound::InvalidEndpoint(goal));
    }
    let mut queue = BinaryHeap::from([Reverse((heuristic(start, goal) * 10, 0_u64, start))]);
    let mut serial = 0_u64;
    let mut costs = BTreeMap::from([(start, 0_u32)]);
    let mut previous = BTreeMap::new();
    let mut expanded = 0;
    while let Some(Reverse((_, _, current))) = queue.pop() {
        if current == goal {
            let mut path = vec![current];
            let mut cursor = current;
            while let Some(parent) = previous.get(&cursor).copied() {
                cursor = parent;
                path.push(cursor);
            }
            path.reverse();
            return Ok(RouteResult {
                path,
                cost: costs[&goal],
            });
        }
        expanded += 1;
        if expanded > config.max_nodes {
            break;
        }
        for next in moves(current) {
            if !routeable(world, next, start, goal) {
                continue;
            }
            let support = world.kind_at(next.offset(0, -1, 0));
            if !matches!(
                support,
                BlockKind::Air
                    | BlockKind::Solid
                    | BlockKind::Transparent
                    | BlockKind::RedstoneBlock
            ) {
                continue;
            }
            let step = if next.y == current.y {
                config.horizontal_cost
            } else {
                config.stair_cost
            };
            let new_cost = costs[&current]
                + step
                + if support == BlockKind::Air {
                    config.new_support_cost
                } else {
                    0
                };
            if costs.get(&next).is_some_and(|known| new_cost >= *known) {
                continue;
            }
            costs.insert(next, new_cost);
            previous.insert(next, current);
            serial += 1;
            queue.push(Reverse((
                new_cost + heuristic(next, goal) * 10,
                serial,
                next,
            )));
        }
    }
    Err(RouteNotFound::Exhausted {
        start,
        goal,
        expanded,
    })
}

pub fn materialize_route(world: &mut World, route: &RouteResult, support_kind: BlockKind) {
    for pos in &route.path {
        let support = pos.offset(0, -1, 0);
        if world.kind_at(support) == BlockKind::Air {
            world.set(support, Block::new(support_kind));
        }
        if world.kind_at(*pos) == BlockKind::Air {
            world.place(BlockKind::RedstoneWire, *pos);
        }
    }
}

fn facing(a: Pos, b: Pos) -> Option<Facing> {
    if a.y != b.y {
        return None;
    }
    match (b.x - a.x, b.z - a.z) {
        (1, 0) => Some(Facing::East),
        (-1, 0) => Some(Facing::West),
        (0, 1) => Some(Facing::South),
        (0, -1) => Some(Facing::North),
        _ => None,
    }
}

pub fn insert_repeaters(
    world: &mut World,
    path: &[Pos],
    max_wire_run: usize,
    delay: u8,
) -> Vec<Pos> {
    let mut repeaters = Vec::new();
    let mut run = 0;
    for index in 1..path.len().saturating_sub(1) {
        run += 1;
        if run < max_wire_run {
            continue;
        }
        let incoming = facing(path[index - 1], path[index]);
        let outgoing = facing(path[index], path[index + 1]);
        if incoming.is_none() || incoming != outgoing {
            continue;
        }
        let block = world.place(BlockKind::Repeater, path[index]);
        block.facing = outgoing;
        block.delay = Some(delay);
        repeaters.push(path[index]);
        run = 0;
    }
    repeaters
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn routes_and_refreshes_long_line() {
        let mut world = World::new();
        world.fill(
            Pos::new(0, 0, 0),
            Pos::new(40, 0, 0),
            Block::new(BlockKind::Solid),
        );
        let result = astar_route(
            &world,
            Pos::new(1, 1, 0),
            Pos::new(39, 1, 0),
            RouterConfig::default(),
        )
        .unwrap();
        materialize_route(&mut world, &result, BlockKind::Solid);
        let repeaters = insert_repeaters(&mut world, &result.path, 14, 1);
        assert!(repeaters.len() >= 2);
        assert!(world.validate_supports().is_ok());
    }
}
