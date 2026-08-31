use std::collections::{BTreeMap, BTreeSet};
use std::error::Error;
use std::fmt::{Display, Formatter};

use crate::cells::PortKind;
use crate::connectivity::physical_step_connected;
use crate::physical::{Endpoint, PhysicalError, PlacementCircuit};
use crate::port_realization::{PortRealizationError, realize_sink_endpoint, terminal_for_endpoint};
use crate::routing::{RouteNotFound, RouterConfig, astar_route};
use crate::routing_resources::{RoutingResources, branch_stair_clearances};
use crate::wire::{dust_connected, update_wire_shapes};
use crate::world::{Block, BlockKind, Facing, Pos, World};

#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct NetId(pub u32);

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RoutedNet {
    pub id: NetId,
    pub source: Endpoint,
    pub sinks: Vec<Endpoint>,
    pub branches: Vec<Vec<Pos>>,
    pub occupied: BTreeSet<Pos>,
    pub repeaters: BTreeSet<Pos>,
}

impl RoutedNet {
    #[must_use]
    pub fn wire_count(&self) -> usize {
        self.occupied.len()
    }

    #[must_use]
    pub fn stair_clearances(&self) -> BTreeSet<Pos> {
        self.branches
            .iter()
            .flat_map(|branch| branch_stair_clearances(branch))
            .collect()
    }
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct MultiNetRouting {
    pub nets: BTreeMap<NetId, RoutedNet>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RoutingJob {
    pub id: NetId,
    pub source: Endpoint,
    pub sinks: Vec<Endpoint>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RerouteEvent {
    pub failed_net: NetId,
    pub ripped_up: Vec<NetId>,
    pub attempt: usize,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RipupRoutingResult {
    pub routing: MultiNetRouting,
    pub events: Vec<RerouteEvent>,
    pub attempts: usize,
}

#[derive(Debug)]
pub enum RipupRoutingError {
    InvalidConfig(&'static str),
    Unroutable {
        net: NetId,
        source: MultiNetError,
    },
    Exhausted {
        max_attempts: usize,
        remaining: Vec<NetId>,
    },
}

impl Display for RipupRoutingError {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::InvalidConfig(message) => f.write_str(message),
            Self::Unroutable { net, source } => write!(f, "net {} is unroutable: {source}", net.0),
            Self::Exhausted {
                max_attempts,
                remaining,
            } => write!(
                f,
                "rip-up router exhausted {max_attempts} attempts; remaining={remaining:?}"
            ),
        }
    }
}

impl Error for RipupRoutingError {}

impl MultiNetRouting {
    #[must_use]
    pub fn resources(&self, exclude: Option<NetId>) -> RoutingResources {
        self.nets
            .iter()
            .filter(|(id, _)| Some(**id) != exclude)
            .fold(RoutingResources::default(), |resources, (_, net)| {
                resources.merged(&RoutingResources::from_conductors(
                    net.occupied.iter().copied(),
                    net.stair_clearances(),
                    std::iter::empty(),
                ))
            })
    }
}

pub fn route_jobs_ripup(
    circuit: &PlacementCircuit,
    jobs: Vec<RoutingJob>,
    config: RouterConfig,
    max_attempts: usize,
    ripup_width: usize,
) -> Result<RipupRoutingResult, RipupRoutingError> {
    route_jobs_ripup_with_fixed(
        circuit,
        jobs,
        MultiNetRouting::default(),
        config,
        max_attempts,
        ripup_width,
    )
}

pub fn route_jobs_ripup_with_fixed(
    circuit: &PlacementCircuit,
    mut jobs: Vec<RoutingJob>,
    fixed: MultiNetRouting,
    config: RouterConfig,
    max_attempts: usize,
    ripup_width: usize,
) -> Result<RipupRoutingResult, RipupRoutingError> {
    if max_attempts == 0 {
        return Err(RipupRoutingError::InvalidConfig(
            "max_attempts must be >= 1",
        ));
    }
    if ripup_width == 0 {
        return Err(RipupRoutingError::InvalidConfig("ripup_width must be >= 1"));
    }
    jobs.sort_by_key(|job| std::cmp::Reverse(job.sinks.len()));
    let by_id: BTreeMap<_, _> = jobs.into_iter().map(|job| (job.id, job)).collect();
    let terminals: BTreeMap<_, BTreeSet<_>> = by_id
        .iter()
        .map(|(id, job)| {
            let mut positions =
                BTreeSet::from([terminal_for_endpoint(&job.source).expect("valid endpoint")]);
            for sink in &job.sinks {
                let realized = realize_sink_endpoint(sink).expect("valid endpoint");
                positions.extend([realized.terminal, realized.approach]);
            }
            let protected: Vec<_> = positions
                .iter()
                .flat_map(|pos| crate::routing_resources::electrical_keepout_for_wire(*pos))
                .collect();
            positions.extend(protected);
            (*id, positions)
        })
        .collect();
    let mut pending: Vec<_> = by_id.keys().copied().collect();
    pending.sort_by_key(|id| std::cmp::Reverse(by_id[id].sinks.len()));
    let mut routing = fixed;
    let mut routed_order = Vec::new();
    let mut events = Vec::new();
    let mut attempts = 0;
    while !pending.is_empty() {
        if attempts >= max_attempts {
            return Err(RipupRoutingError::Exhausted {
                max_attempts,
                remaining: pending,
            });
        }
        attempts += 1;
        let id = pending.remove(0);
        let job = &by_id[&id];
        let reserved = terminals
            .iter()
            .filter(|(other, _)| **other != id)
            .flat_map(|(_, positions)| positions.iter().copied())
            .collect();
        let attempt = route_net_tree(
            circuit,
            id,
            job.source.clone(),
            job.sinks.clone(),
            &routing.resources(None),
            &reserved,
            config,
        );
        let accepted = match attempt {
            Ok(net) => {
                routing.nets.insert(id, net);
                match materialize_multinet(circuit, &routing) {
                    Ok(world) => {
                        let report =
                            validate_routing_legality(circuit, &routing, &world, usize::MAX);
                        report.cross_net_contacts.is_empty()
                            && report.support_conflicts.is_empty()
                            && report.broken_steps.is_empty()
                    }
                    Err(_) => false,
                }
            }
            Err(error) => {
                if routed_order.is_empty() {
                    return Err(RipupRoutingError::Unroutable {
                        net: id,
                        source: error,
                    });
                }
                false
            }
        };
        if accepted {
            routed_order.push(id);
            continue;
        }
        routing.nets.remove(&id);
        if routed_order.is_empty() {
            return Err(RipupRoutingError::Unroutable {
                net: id,
                source: MultiNetError::NoTreeConnection(job.source.pos),
            });
        }
        let count = ripup_width.min(routed_order.len());
        let ripped = routed_order.split_off(routed_order.len() - count);
        for old in &ripped {
            routing.nets.remove(old);
        }
        events.push(RerouteEvent {
            failed_net: id,
            ripped_up: ripped.clone(),
            attempt: attempts,
        });
        pending = std::iter::once(id).chain(ripped).chain(pending).collect();
        if events.len() >= 2
            && events[events.len() - 1].failed_net == events[events.len() - 2].failed_net
            && events[events.len() - 1].ripped_up == events[events.len() - 2].ripped_up
        {
            let width = events.last().expect("event exists").ripped_up.len();
            pending[1..=width].reverse();
        }
    }
    Ok(RipupRoutingResult {
        routing,
        events,
        attempts,
    })
}

#[derive(Debug)]
pub enum MultiNetError {
    Physical(PhysicalError),
    Port(PortRealizationError),
    Route(RouteNotFound),
    NoTreeConnection(Pos),
    NetOverlap {
        pos: Pos,
        first: NetId,
        second: NetId,
    },
}

impl Display for MultiNetError {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Physical(error) => Display::fmt(error, f),
            Self::Port(error) => Display::fmt(error, f),
            Self::Route(error) => Display::fmt(error, f),
            Self::NoTreeConnection(pos) => write!(f, "cannot connect {pos:?} to net tree"),
            Self::NetOverlap { pos, first, second } => {
                write!(f, "net overlap at {pos:?}: {} vs {}", first.0, second.0)
            }
        }
    }
}

impl Error for MultiNetError {}
impl From<PhysicalError> for MultiNetError {
    fn from(value: PhysicalError) -> Self {
        Self::Physical(value)
    }
}
impl From<PortRealizationError> for MultiNetError {
    fn from(value: PortRealizationError) -> Self {
        Self::Port(value)
    }
}
impl From<RouteNotFound> for MultiNetError {
    fn from(value: RouteNotFound) -> Self {
        Self::Route(value)
    }
}

fn blocked_world(base: &World, blocked: &BTreeSet<Pos>, allowed: &[Pos]) -> World {
    let mut world = base.clone();
    for pos in blocked {
        if !allowed.contains(pos) && world.kind_at(*pos) == BlockKind::Air {
            world.set(*pos, Block::new(BlockKind::Solid));
        }
    }
    world
}

fn cell_source_keepout(circuit: &PlacementCircuit) -> BTreeSet<Pos> {
    let mut keepout = BTreeSet::new();
    for node in circuit.cells.values() {
        let cell_world: BTreeMap<_, _> = node.placed.blocks().collect();
        for (pos, block) in &cell_world {
            if matches!(
                block.kind,
                BlockKind::RedstoneTorch | BlockKind::RedstoneBlock | BlockKind::Lever
            ) {
                keepout.extend(crate::routing_resources::electrical_keepout_for_wire(*pos));
            }
            if block.kind == BlockKind::Repeater {
                if let Some(delta) = block.facing.and_then(Facing::horizontal_offset) {
                    let output = pos.offset(delta.x, 0, delta.z);
                    if cell_world
                        .get(&output)
                        .is_some_and(|target| target.kind == BlockKind::Solid)
                    {
                        keepout.extend(crate::routing_resources::electrical_keepout_for_wire(
                            output,
                        ));
                    }
                }
            }
        }
    }
    keepout
}

fn route_to_tree(
    world: &World,
    start: Pos,
    tree: &BTreeSet<Pos>,
    blocked: &BTreeSet<Pos>,
    config: RouterConfig,
) -> Result<Vec<Pos>, MultiNetError> {
    let mut best = None;
    for goal in tree {
        let mut local_blocked = blocked.clone();
        for _ in 0..64 {
            let route_world = blocked_world(world, &local_blocked, &[start, *goal]);
            let Ok(route) = astar_route(&route_world, start, *goal, config) else {
                break;
            };
            if let Some(conflict) = geometry_conflict(&route.path, tree, world) {
                if conflict == start || conflict == *goal || !local_blocked.insert(conflict) {
                    break;
                }
                continue;
            }
            if best
                .as_ref()
                .is_none_or(|current: &crate::routing::RouteResult| route.cost < current.cost)
            {
                best = Some(route);
            }
            break;
        }
    }
    best.map(|route| route.path)
        .ok_or(MultiNetError::NoTreeConnection(start))
}

fn geometry_conflict(path: &[Pos], tree: &BTreeSet<Pos>, world: &World) -> Option<Pos> {
    let conductors: BTreeSet<_> = tree.iter().copied().chain(path.iter().copied()).collect();
    for conductor in &conductors {
        if conductors.contains(&conductor.offset(0, -1, 0)) {
            return Some(*conductor);
        }
    }
    for pair in path.windows(2).filter(|pair| pair[0].y != pair[1].y) {
        let lower = if pair[0].y < pair[1].y {
            pair[0]
        } else {
            pair[1]
        };
        let clearance = lower.offset(0, 1, 0);
        if world.kind_at(clearance) != BlockKind::Air || conductors.contains(&clearance) {
            return path
                .iter()
                .copied()
                .find(|pos| *pos == clearance)
                .or(Some(pair[1]));
        }
        if conductors.contains(&clearance.offset(0, 1, 0)) {
            return Some(clearance.offset(0, 1, 0));
        }
    }
    None
}

fn tree_adjacency(branches: &[Vec<Pos>]) -> BTreeMap<Pos, BTreeSet<Pos>> {
    let mut adjacency: BTreeMap<Pos, BTreeSet<Pos>> = BTreeMap::new();
    for branch in branches {
        for pair in branch.windows(2) {
            adjacency.entry(pair[0]).or_default().insert(pair[1]);
            adjacency.entry(pair[1]).or_default().insert(pair[0]);
        }
    }
    adjacency
}

fn rooted_parent(source: Pos, branches: &[Vec<Pos>]) -> BTreeMap<Pos, Option<Pos>> {
    let adjacency = tree_adjacency(branches);
    let mut parent = BTreeMap::from([(source, None)]);
    let mut queue = std::collections::VecDeque::from([source]);
    while let Some(current) = queue.pop_front() {
        for next in adjacency.get(&current).into_iter().flatten() {
            if !parent.contains_key(next) {
                parent.insert(*next, Some(current));
                queue.push_back(*next);
            }
        }
    }
    parent
}

fn root_path(parent: &BTreeMap<Pos, Option<Pos>>, sink: Pos) -> Option<Vec<Pos>> {
    let mut path = vec![sink];
    let mut current = sink;
    while let Some(previous) = *parent.get(&current)? {
        current = previous;
        path.push(current);
    }
    path.reverse();
    Some(path)
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

fn plan_repeaters(
    source: Pos,
    sinks: &[Pos],
    branches: &[Vec<Pos>],
    forbidden: &BTreeSet<Pos>,
    max_run: usize,
) -> BTreeSet<Pos> {
    let adjacency = tree_adjacency(branches);
    let parent = rooted_parent(source, branches);
    let paths: Vec<_> = sinks
        .iter()
        .filter_map(|sink| root_path(&parent, *sink))
        .collect();
    let mut repeaters = BTreeSet::new();
    for path in paths {
        let mut last_reset = 0;
        let mut index = 1;
        while index < path.len() {
            if repeaters.contains(&path[index]) {
                last_reset = index;
            } else if index - last_reset > max_run {
                let candidate = (last_reset + 1..=index).rev().find(|candidate| {
                    let pos = path[*candidate];
                    !forbidden.contains(&pos)
                        && *candidate + 1 < path.len()
                        && adjacency
                            .get(&pos)
                            .is_some_and(|neighbors| neighbors.len() == 2)
                        && facing(path[*candidate - 1], pos) == facing(pos, path[*candidate + 1])
                        && facing(path[*candidate - 1], pos).is_some()
                });
                if let Some(candidate) = candidate {
                    repeaters.insert(path[candidate]);
                    last_reset = candidate;
                    index = candidate;
                }
            }
            index += 1;
        }
    }
    repeaters
}

fn reinforce_wire_sink_tails(
    source: Pos,
    sinks: &[Endpoint],
    branches: &[Vec<Pos>],
    forbidden: &BTreeSet<Pos>,
    repeaters: &mut BTreeSet<Pos>,
    max_tail: usize,
) {
    let adjacency = tree_adjacency(branches);
    let parent = rooted_parent(source, branches);
    for sink in sinks.iter().filter(|sink| sink.kind == PortKind::Wire) {
        let Ok(terminal) = terminal_for_endpoint(sink) else {
            continue;
        };
        let Some(path) = root_path(&parent, terminal) else {
            continue;
        };
        let last_reset = path
            .iter()
            .enumerate()
            .filter_map(|(index, pos)| repeaters.contains(pos).then_some(index))
            .max()
            .unwrap_or(0);
        let end = path.len().saturating_sub(1);
        if end - last_reset <= max_tail {
            continue;
        }
        let start = end.saturating_sub(max_tail);
        if let Some(candidate) = (start..end).find(|candidate| {
            let pos = path[*candidate];
            *candidate > 0
                && !forbidden.contains(&pos)
                && !repeaters.contains(&path[*candidate - 1])
                && !repeaters.contains(&path[*candidate + 1])
                && adjacency
                    .get(&pos)
                    .is_some_and(|neighbors| neighbors.len() == 2)
                && facing(path[*candidate - 1], pos) == facing(pos, path[*candidate + 1])
                && facing(path[*candidate - 1], pos).is_some()
        }) {
            repeaters.insert(path[candidate]);
        }
    }
}

pub fn route_net_tree(
    circuit: &PlacementCircuit,
    id: NetId,
    source: Endpoint,
    sinks: Vec<Endpoint>,
    other_resources: &RoutingResources,
    reserved_terminals: &BTreeSet<Pos>,
    config: RouterConfig,
) -> Result<RoutedNet, MultiNetError> {
    let world = circuit.cell_world()?;
    let source_pos = terminal_for_endpoint(&source)?;
    let realized: Vec<_> = sinks
        .iter()
        .map(realize_sink_endpoint)
        .collect::<Result<_, _>>()?;
    let mut blocked = other_resources.blocked_conductors();
    blocked.extend(reserved_terminals);
    blocked.extend(cell_source_keepout(circuit));
    let own_terminals: BTreeSet<_> = std::iter::once(source_pos)
        .chain(
            realized
                .iter()
                .flat_map(|sink| [sink.terminal, sink.approach]),
        )
        .collect();
    for terminal in &own_terminals {
        blocked.remove(terminal);
        blocked.remove(&terminal.offset(0, -1, 0));
    }

    let mut joinable = BTreeSet::from([source_pos]);
    let mut occupied = joinable.clone();
    let mut indexed: Vec<_> = realized.into_iter().enumerate().collect();
    indexed.sort_by_key(|(_, sink)| {
        source_pos.x.abs_diff(sink.approach.x)
            + source_pos.y.abs_diff(sink.approach.y)
            + source_pos.z.abs_diff(sink.approach.z)
    });
    let mut routed = BTreeMap::new();
    for (index, sink) in indexed {
        let mut core = route_to_tree(&world, sink.approach, &joinable, &blocked, config)?;
        core.reverse();
        if sink.approach != sink.terminal {
            core.push(sink.terminal);
        }
        joinable.extend(core.iter().take(core.len().saturating_sub(1)).copied());
        occupied.extend(&core);
        blocked.extend(branch_stair_clearances(&core));
        routed.insert(index, core);
    }
    let branches: Vec<_> = (0..sinks.len())
        .map(|index| routed.remove(&index).expect("sink routed"))
        .collect();
    let sink_positions: Vec<_> = realized_positions(&sinks)?;
    let mut repeaters = plan_repeaters(source_pos, &sink_positions, &branches, &own_terminals, 12);
    // Reserve signal strength for dust inside Wire-input cells. The generic route
    // budget ends at the port and therefore cannot account for that final run.
    reinforce_wire_sink_tails(
        source_pos,
        &sinks,
        &branches,
        &own_terminals,
        &mut repeaters,
        8,
    );
    Ok(RoutedNet {
        id,
        source,
        sinks,
        branches,
        occupied,
        repeaters,
    })
}

fn realized_positions(sinks: &[Endpoint]) -> Result<Vec<Pos>, PortRealizationError> {
    sinks.iter().map(terminal_for_endpoint).collect()
}

pub fn materialize_multinet(
    circuit: &PlacementCircuit,
    routing: &MultiNetRouting,
) -> Result<World, MultiNetError> {
    let mut world = circuit.cell_world()?;
    let mut owners = BTreeMap::new();
    for (id, net) in &routing.nets {
        let structural_supports: BTreeSet<_> = net
            .repeaters
            .iter()
            .copied()
            .chain(net.sinks.iter().filter_map(|sink| {
                (sink.kind == PortKind::BlockPower)
                    .then(|| terminal_for_endpoint(sink).ok())
                    .flatten()
            }))
            .chain(net.branches.iter().flat_map(|branch| {
                branch
                    .windows(2)
                    .filter(|pair| pair[0].y != pair[1].y)
                    .flat_map(|pair| [pair[0], pair[1]])
            }))
            .collect();
        for pos in &net.occupied {
            if let Some(first) = owners.insert(*pos, *id).filter(|first| first != id) {
                return Err(MultiNetError::NetOverlap {
                    pos: *pos,
                    first,
                    second: *id,
                });
            }
            let support = pos.offset(0, -1, 0);
            if world.kind_at(support) == BlockKind::Air {
                world.set(
                    support,
                    Block::new(if structural_supports.contains(pos) {
                        BlockKind::Solid
                    } else {
                        BlockKind::Transparent
                    }),
                );
            }
            if world.kind_at(*pos) == BlockKind::Air && !net.repeaters.contains(pos) {
                world.place(BlockKind::RedstoneWire, *pos);
            }
        }
        let source = terminal_for_endpoint(&net.source)?;
        let parent = rooted_parent(source, &net.branches);
        for repeater in &net.repeaters {
            let direction = parent
                .get(repeater)
                .and_then(|previous| *previous)
                .and_then(|previous| facing(previous, *repeater))
                .ok_or(MultiNetError::NoTreeConnection(*repeater))?;
            let block = world.place(BlockKind::Repeater, *repeater);
            block.facing = Some(direction);
            block.delay = Some(1);
        }
        for sink in &net.sinks {
            if sink.kind != PortKind::BlockPower {
                continue;
            }
            let terminal = terminal_for_endpoint(sink)?;
            let direction = sink
                .facing
                .map(Facing::opposite)
                .ok_or(MultiNetError::NoTreeConnection(terminal))?;
            let block = world.place(BlockKind::Repeater, terminal);
            block.facing = Some(direction);
            block.delay = Some(1);
        }
    }
    update_wire_shapes(&mut world);
    Ok(world)
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct BrokenStep {
    pub net: NetId,
    pub source: Pos,
    pub sink: Pos,
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct LegalityReport {
    pub cross_net_contacts: Vec<(NetId, NetId, Pos, Pos)>,
    pub support_conflicts: Vec<(NetId, NetId, Pos)>,
    pub over_budget_paths: Vec<(NetId, usize)>,
    pub broken_steps: Vec<BrokenStep>,
}

impl LegalityReport {
    #[must_use]
    pub fn valid(&self) -> bool {
        self.cross_net_contacts.is_empty()
            && self.support_conflicts.is_empty()
            && self.over_budget_paths.is_empty()
            && self.broken_steps.is_empty()
    }
}

#[must_use]
pub fn validate_routing_legality(
    _circuit: &PlacementCircuit,
    routing: &MultiNetRouting,
    world: &World,
    max_wire_run: usize,
) -> LegalityReport {
    let mut report = LegalityReport::default();
    let ids: Vec<_> = routing.nets.keys().copied().collect();
    let resources = routing
        .nets
        .iter()
        .map(|(id, net)| {
            (
                *id,
                RoutingResources::from_conductors(
                    net.occupied.iter().copied(),
                    net.stair_clearances(),
                    std::iter::empty(),
                ),
            )
        })
        .collect::<BTreeMap<_, _>>();
    let occupied_by_pos = routing
        .nets
        .iter()
        .flat_map(|(id, net)| net.occupied.iter().map(move |pos| (*pos, *id)))
        .collect::<BTreeMap<_, _>>();
    for (position, first) in &occupied_by_pos {
        for (dx, dy, dz) in DUST_CONTACT_OFFSETS {
            let neighbor = position.offset(dx, dy, dz);
            let Some(second) = occupied_by_pos.get(&neighbor) else {
                continue;
            };
            if first < second && dust_connected(world, *position, neighbor) {
                report
                    .cross_net_contacts
                    .push((*first, *second, *position, neighbor));
            }
        }
    }
    for (offset, first) in ids.iter().enumerate() {
        for second in &ids[offset + 1..] {
            let a = &routing.nets[first];
            let b = &routing.nets[second];
            let ra = &resources[first];
            let rb = &resources[second];
            for pos in ra
                .supports
                .intersection(&b.occupied)
                .chain(rb.supports.intersection(&a.occupied))
            {
                report.support_conflicts.push((*first, *second, *pos));
            }
        }
    }
    for (id, net) in &routing.nets {
        let block_power_terminals: BTreeSet<_> = net
            .sinks
            .iter()
            .filter(|sink| sink.kind == PortKind::BlockPower)
            .filter_map(|sink| terminal_for_endpoint(sink).ok())
            .collect();
        for branch in &net.branches {
            let mut run = 0;
            for pair in branch.windows(2) {
                if !physical_step_connected(world, pair[0], pair[1]) {
                    report.broken_steps.push(BrokenStep {
                        net: *id,
                        source: pair[0],
                        sink: pair[1],
                    });
                }
                if net.repeaters.contains(&pair[1]) || block_power_terminals.contains(&pair[1]) {
                    run = 0;
                } else {
                    run += 1;
                }
                if run > max_wire_run {
                    report.over_budget_paths.push((*id, run));
                    break;
                }
            }
        }
    }
    report
}

const DUST_CONTACT_OFFSETS: [(i32, i32, i32); 12] = [
    (-1, -1, 0),
    (-1, 0, 0),
    (-1, 1, 0),
    (0, -1, -1),
    (0, -1, 1),
    (0, 0, -1),
    (0, 0, 1),
    (0, 1, -1),
    (0, 1, 1),
    (1, -1, 0),
    (1, 0, 0),
    (1, 1, 0),
];

#[cfg(test)]
mod tests {
    use crate::cells::{PlacedCell, RotationY, terminal_cell};
    use crate::logic::GateKind;

    use super::*;

    #[test]
    fn fanout_branches_share_tree_and_validate() {
        let placed = |pos| PlacedCell {
            cell: terminal_cell("terminal"),
            origin: pos,
            rotation: RotationY::R0,
        };
        let mut circuit = PlacementCircuit::new();
        let source_id = circuit.add_cell(GateKind::Input, placed(Pos::new(0, 0, 0)));
        let left_id = circuit.add_cell(GateKind::Output, placed(Pos::new(16, 0, -6)));
        let right_id = circuit.add_cell(GateKind::Output, placed(Pos::new(16, 0, 6)));
        let source = circuit.output_endpoint(source_id, "out").unwrap();
        let sinks = vec![
            circuit.input_endpoint(left_id, "in").unwrap(),
            circuit.input_endpoint(right_id, "in").unwrap(),
        ];
        let net = route_net_tree(
            &circuit,
            NetId(99),
            source,
            sinks,
            &RoutingResources::default(),
            &BTreeSet::new(),
            RouterConfig::default(),
        )
        .unwrap();
        assert!(
            !net.branches[0]
                .iter()
                .copied()
                .collect::<BTreeSet<_>>()
                .is_disjoint(&net.branches[1].iter().copied().collect())
        );
        let routing = MultiNetRouting {
            nets: BTreeMap::from([(net.id, net)]),
        };
        let world = materialize_multinet(&circuit, &routing).unwrap();
        let report = validate_routing_legality(&circuit, &routing, &world, 14);
        assert!(report.valid(), "{report:?}");
    }
}
