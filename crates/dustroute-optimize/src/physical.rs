use std::collections::{BTreeMap, BTreeSet, VecDeque};
use std::time::{Duration, Instant};

use dustroute_physical::{
    Block, BlockKind, PhysicalBlockChange, PhysicalPatch, PhysicalPatchReason, Pos, World,
};
use dustroute_translate::{RegionBounds, dust_connected};

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PhysicalWireOptimization {
    pub patch: PhysicalPatch,
    pub wire_blocks_before: usize,
    pub wire_blocks_after: usize,
    pub path_length_before: usize,
    pub path_length_after: usize,
    pub fixed_endpoints: [Pos; 2],
    pub phases: Vec<PhysicalOptimizationPhase>,
    pub search: PhysicalOptimizationSearchStats,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct PhysicalOptimizationSearchBudget {
    pub max_expansions: usize,
    pub max_candidates: usize,
    pub max_millis: u64,
}

impl Default for PhysicalOptimizationSearchBudget {
    fn default() -> Self {
        Self {
            max_expansions: 100_000,
            max_candidates: 1_024,
            max_millis: 1_000,
        }
    }
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct PhysicalOptimizationSearchStats {
    pub expansions: usize,
    pub candidates: usize,
    pub truncated: bool,
    pub stop_reason: Option<&'static str>,
}

struct SearchControl<'a> {
    budget: PhysicalOptimizationSearchBudget,
    deadline: Instant,
    stats: &'a mut PhysicalOptimizationSearchStats,
}

#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub struct PhasedPhysicalScore {
    pub bounding_volume: usize,
    pub occupied_blocks: usize,
    pub connector_length: usize,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PhysicalOptimizationPhase {
    pub name: &'static str,
    pub before: PhasedPhysicalScore,
    pub after: PhasedPhysicalScore,
    pub accepted: bool,
    pub connector_growth: usize,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct PhasedPhysicalSelection {
    pub local_density: PhasedPhysicalScore,
    pub final_global: PhasedPhysicalScore,
}

pub fn select_phased_physical_scores(
    baseline: PhasedPhysicalScore,
    local_candidates: impl IntoIterator<Item = PhasedPhysicalScore>,
    final_candidates: impl IntoIterator<Item = PhasedPhysicalScore>,
    connector_growth_budget: usize,
) -> Option<PhasedPhysicalSelection> {
    let local_density = local_candidates
        .into_iter()
        .filter(|score| {
            score.connector_length
                <= baseline
                    .connector_length
                    .saturating_add(connector_growth_budget)
        })
        .min_by_key(|score| {
            (
                score.bounding_volume,
                score.occupied_blocks,
                score.connector_length,
            )
        })?;
    let final_global = final_candidates
        .into_iter()
        .filter(|score| final_score(*score) < final_score(baseline))
        .min_by_key(|score| final_score(*score))?;
    Some(PhasedPhysicalSelection {
        local_density,
        final_global,
    })
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum PhysicalWireOptimizationError {
    NoWirePath,
    NotSimplePath,
    DifferentHeights,
    NoShorterRoute,
    UnsupportedRoute,
    RouteWouldCreateAdjacency,
    NoStrengthPreservingRoute,
}

pub fn optimize_physical_wire_path(
    world: &World,
    focus: RegionBounds,
) -> Result<PhysicalWireOptimization, PhysicalWireOptimizationError> {
    optimize_physical_wire_path_with_constraints(world, focus, false)
}

pub fn optimize_physical_wire_path_with_constraints(
    world: &World,
    focus: RegionBounds,
    preserve_strength: bool,
) -> Result<PhysicalWireOptimization, PhysicalWireOptimizationError> {
    optimize_physical_wire_path_with_budget(
        world,
        focus,
        preserve_strength,
        PhysicalOptimizationSearchBudget::default(),
    )
}

pub fn optimize_physical_wire_path_with_budget(
    world: &World,
    focus: RegionBounds,
    preserve_strength: bool,
    budget: PhysicalOptimizationSearchBudget,
) -> Result<PhysicalWireOptimization, PhysicalWireOptimizationError> {
    let deadline = Instant::now() + Duration::from_millis(budget.max_millis);
    let mut stats = PhysicalOptimizationSearchStats::default();
    let mut result = {
        let mut control = SearchControl {
            budget,
            deadline,
            stats: &mut stats,
        };
        optimize_physical_wire_path_internal(world, focus, preserve_strength, &mut control)?
    };
    result.search = stats;
    Ok(result)
}

fn optimize_physical_wire_path_internal(
    world: &World,
    focus: RegionBounds,
    preserve_strength: bool,
    control: &mut SearchControl<'_>,
) -> Result<PhysicalWireOptimization, PhysicalWireOptimizationError> {
    let wires = world
        .iter()
        .filter_map(|(pos, block)| {
            (block.kind == BlockKind::RedstoneWire && contains(focus, *pos)).then_some(*pos)
        })
        .collect::<BTreeSet<_>>();
    if wires.len() < 3 {
        return Err(PhysicalWireOptimizationError::NoWirePath);
    }
    let adjacency = wires
        .iter()
        .map(|pos| {
            let neighbors = wires
                .iter()
                .filter(|other| *other != pos && dust_connected(world, *pos, **other))
                .copied()
                .collect::<BTreeSet<_>>();
            (*pos, neighbors)
        })
        .collect::<BTreeMap<_, _>>();
    if adjacency.values().any(|neighbors| neighbors.len() > 2) {
        return optimize_branching_wire_network(
            world,
            focus,
            &wires,
            &adjacency,
            preserve_strength,
            control,
        );
    }
    let endpoints = adjacency
        .iter()
        .filter_map(|(pos, neighbors)| (neighbors.len() == 1).then_some(*pos))
        .collect::<Vec<_>>();
    if endpoints.len() != 2 || connected_count(&adjacency, endpoints[0]) != wires.len() {
        return optimize_disconnected_wire_network(
            world,
            focus,
            &wires,
            &adjacency,
            preserve_strength,
            control,
        );
    }
    let [start, end] = [endpoints[0], endpoints[1]];
    let candidates = if preserve_strength {
        strength_preserving_paths(world, focus, &wires, start, end, wires.len(), control)
    } else {
        let mut candidates = if start.y == end.y {
            vec![
                orthogonal_path(start, end, true),
                orthogonal_path(start, end, false),
            ]
        } else {
            Vec::new()
        };
        control.stats.candidates = control.stats.candidates.saturating_add(candidates.len());
        if let Some(path) = shortest_supported_3d_path(world, focus, &wires, start, end, control) {
            control.stats.candidates += 1;
            candidates.push(path);
        }
        candidates
    };
    let path = candidates
        .into_iter()
        .filter(|path| path.iter().all(|pos| contains(focus, *pos)))
        .filter(|path| path_supported_and_clear(world, &wires, path))
        .filter(|path| induced_path(path))
        .filter(|path| !creates_unexpected_adjacency(world, &wires, path, start, end))
        .min_by_key(|path| {
            let score = score_positions(&path.iter().copied().collect());
            let additions = path.iter().filter(|pos| !wires.contains(pos)).count();
            (score, additions, path.clone())
        })
        .ok_or(if preserve_strength {
            PhysicalWireOptimizationError::NoStrengthPreservingRoute
        } else {
            PhysicalWireOptimizationError::UnsupportedRoute
        })?;
    if (!preserve_strength && path.len() >= wires.len())
        || (preserve_strength
            && (path.len() != wires.len()
                || score_positions(&path.iter().copied().collect()) >= score_positions(&wires)))
    {
        return Err(PhysicalWireOptimizationError::NoShorterRoute);
    }

    let path_set = path.iter().copied().collect::<BTreeSet<_>>();
    let baseline_score = score_positions(&wires);
    let final_score = score_positions(&path_set);
    let selection = select_phased_physical_scores(
        baseline_score,
        [final_score],
        [final_score],
        baseline_score.connector_length / 2,
    )
    .ok_or(PhysicalWireOptimizationError::NoShorterRoute)?;
    let mut changes = Vec::new();
    for pos in wires.difference(&path_set).copied() {
        changes.push(PhysicalBlockChange {
            pos,
            before: world.get(pos).cloned().expect("wire position is observed"),
            after: Block::new(BlockKind::Air),
        });
    }
    for pos in path_set.difference(&wires).copied() {
        let mut wire = Block::new(BlockKind::RedstoneWire);
        wire.support_offset = Some(Pos::new(0, -1, 0));
        changes.push(PhysicalBlockChange {
            pos,
            before: world
                .get(pos)
                .cloned()
                .unwrap_or_else(|| Block::new(BlockKind::Air)),
            after: wire,
        });
    }
    changes.sort_by_key(|change| change.pos);
    Ok(PhysicalWireOptimization {
        patch: PhysicalPatch {
            reason: PhysicalPatchReason::OptimizePlacement,
            affected_fragments: Vec::new(),
            confidence_percent: 90,
            explanation: format!(
                "shorten one supported dust path from {} to {} wire blocks while fixing both endpoints",
                wires.len(),
                path.len()
            ),
            changes,
        },
        wire_blocks_before: wires.len(),
        wire_blocks_after: path.len(),
        path_length_before: wires.len().saturating_sub(1),
        path_length_after: path.len().saturating_sub(1),
        fixed_endpoints: [start, end],
        phases: vec![
            PhysicalOptimizationPhase {
                name: "local_density",
                before: baseline_score,
                after: selection.local_density,
                accepted: selection.local_density != baseline_score,
                connector_growth: selection
                    .local_density
                    .connector_length
                    .saturating_sub(baseline_score.connector_length),
            },
            PhysicalOptimizationPhase {
                name: "connector_recovery",
                before: selection.local_density,
                after: selection.final_global,
                accepted: selection.final_global != selection.local_density,
                connector_growth: selection
                    .final_global
                    .connector_length
                    .saturating_sub(selection.local_density.connector_length),
            },
            PhysicalOptimizationPhase {
                name: "global_compaction",
                before: baseline_score,
                after: selection.final_global,
                accepted: true,
                connector_growth: selection
                    .final_global
                    .connector_length
                    .saturating_sub(baseline_score.connector_length),
            },
        ],
        search: PhysicalOptimizationSearchStats::default(),
    })
}

fn optimize_disconnected_wire_network(
    world: &World,
    focus: RegionBounds,
    wires: &BTreeSet<Pos>,
    adjacency: &BTreeMap<Pos, BTreeSet<Pos>>,
    preserve_strength: bool,
    control: &mut SearchControl<'_>,
) -> Result<PhysicalWireOptimization, PhysicalWireOptimizationError> {
    let mut unseen = wires.clone();
    let mut components = Vec::new();
    while let Some(start) = unseen.iter().next().copied() {
        let mut component = BTreeSet::from([start]);
        let mut queue = VecDeque::from([start]);
        unseen.remove(&start);
        while let Some(position) = queue.pop_front() {
            for neighbor in &adjacency[&position] {
                if unseen.remove(neighbor) {
                    component.insert(*neighbor);
                    queue.push_back(*neighbor);
                }
            }
        }
        components.push(component);
    }
    let mut candidates = Vec::new();
    for component in components {
        if component.len() < 3 {
            continue;
        }
        let mut isolated = world.clone();
        for other in wires.difference(&component) {
            isolated.remove(*other);
        }
        if let Ok(candidate) =
            optimize_physical_wire_path_internal(&isolated, focus, preserve_strength, control)
        {
            if candidate.patch.changes.iter().all(|change| {
                change.after.kind != BlockKind::RedstoneWire
                    || world.kind_at(change.pos) == BlockKind::Air
                    || component.contains(&change.pos)
            }) {
                candidates.push(candidate);
            }
        }
    }
    candidates
        .into_iter()
        .min_by_key(|candidate| {
            (
                candidate.path_length_after,
                candidate.patch.changes.len(),
                candidate.fixed_endpoints,
            )
        })
        .ok_or(PhysicalWireOptimizationError::NotSimplePath)
}

fn optimize_branching_wire_network(
    world: &World,
    focus: RegionBounds,
    wires: &BTreeSet<Pos>,
    adjacency: &BTreeMap<Pos, BTreeSet<Pos>>,
    preserve_strength: bool,
    control: &mut SearchControl<'_>,
) -> Result<PhysicalWireOptimization, PhysicalWireOptimizationError> {
    let junctions = adjacency
        .iter()
        .filter_map(|(position, neighbors)| (neighbors.len() != 2).then_some(*position))
        .collect::<BTreeSet<_>>();
    let mut visited_edges = BTreeSet::<(Pos, Pos)>::new();
    let mut segments = Vec::<Vec<Pos>>::new();
    for start in &junctions {
        for first in &adjacency[start] {
            let edge = ordered_edge(*start, *first);
            if !visited_edges.insert(edge) {
                continue;
            }
            let mut segment = vec![*start, *first];
            let mut previous = *start;
            let mut current = *first;
            while !junctions.contains(&current) {
                let Some(next) = adjacency[&current]
                    .iter()
                    .copied()
                    .find(|next| *next != previous)
                else {
                    break;
                };
                visited_edges.insert(ordered_edge(current, next));
                segment.push(next);
                previous = current;
                current = next;
            }
            if segment.len() >= 3 {
                segments.push(segment);
            }
        }
    }
    let mut candidates = Vec::new();
    for segment in segments {
        let segment_set = segment.iter().copied().collect::<BTreeSet<_>>();
        let mut isolated = world.clone();
        for other in wires.difference(&segment_set) {
            isolated.remove(*other);
        }
        let Ok(candidate) =
            optimize_physical_wire_path_internal(&isolated, focus, preserve_strength, control)
        else {
            continue;
        };
        let candidate_world = match candidate.patch.apply_virtual(world) {
            Ok(candidate_world) => candidate_world,
            Err(_) => continue,
        };
        let candidate_wires = candidate_world
            .iter()
            .filter_map(|(position, block)| {
                (block.kind == BlockKind::RedstoneWire
                    && (segment_set.contains(position)
                        || candidate
                            .patch
                            .changes
                            .iter()
                            .any(|change| change.pos == *position)))
                .then_some(*position)
            })
            .collect::<BTreeSet<_>>();
        if candidate.patch.changes.iter().any(|change| {
            change.after.kind == BlockKind::RedstoneWire
                && world.kind_at(change.pos) != BlockKind::Air
                && !segment_set.contains(&change.pos)
        }) || creates_unexpected_adjacency(
            world,
            &segment_set,
            &candidate_wires.iter().copied().collect::<Vec<_>>(),
            candidate.fixed_endpoints[0],
            candidate.fixed_endpoints[1],
        ) {
            continue;
        }
        candidates.push(candidate);
    }
    candidates
        .into_iter()
        .min_by_key(|candidate| {
            (
                candidate.path_length_after,
                candidate.patch.changes.len(),
                candidate.fixed_endpoints,
            )
        })
        .ok_or(PhysicalWireOptimizationError::NotSimplePath)
}

fn ordered_edge(first: Pos, second: Pos) -> (Pos, Pos) {
    if first <= second {
        (first, second)
    } else {
        (second, first)
    }
}

fn shortest_supported_3d_path(
    world: &World,
    focus: RegionBounds,
    original: &BTreeSet<Pos>,
    start: Pos,
    end: Pos,
    control: &mut SearchControl<'_>,
) -> Option<Vec<Pos>> {
    let mut base = world.clone();
    for position in original {
        if *position != start && *position != end {
            base.remove(*position);
        }
    }
    let mut queue = VecDeque::from([start]);
    let mut previous = BTreeMap::<Pos, Pos>::new();
    let mut seen = BTreeSet::from([start]);
    while let Some(current) = queue.pop_front() {
        if current == end {
            let mut path = vec![end];
            while *path.last().expect("path has end") != start {
                path.push(previous[path.last().expect("path position")]);
            }
            path.reverse();
            return Some(path);
        }
        if control.stats.expansions >= control.budget.max_expansions {
            control.stats.truncated = true;
            control.stats.stop_reason = Some("max_expansions");
            return None;
        }
        if Instant::now() >= control.deadline {
            control.stats.truncated = true;
            control.stats.stop_reason = Some("time_budget");
            return None;
        }
        control.stats.expansions += 1;
        for horizontal in horizontal_neighbors(current) {
            for dy in -1..=1 {
                let next = horizontal.offset(0, dy, 0);
                if !contains(focus, next)
                    || seen.contains(&next)
                    || (next != end
                        && !original.contains(&next)
                        && base.kind_at(next) != BlockKind::Air)
                    || !base
                        .get(next.offset(0, -1, 0))
                        .is_some_and(|block| block.redstone_traits().supports_dust_on_top)
                    || !candidate_dust_connection(&base, current, next)
                {
                    continue;
                }
                seen.insert(next);
                previous.insert(next, current);
                queue.push_back(next);
            }
        }
    }
    None
}

fn candidate_dust_connection(world: &World, first: Pos, second: Pos) -> bool {
    let mut candidate = world.clone();
    candidate.place(BlockKind::RedstoneWire, first);
    candidate.place(BlockKind::RedstoneWire, second);
    dustroute_translate::update_wire_shapes(&mut candidate);
    dust_connected(&candidate, first, second)
}

fn strength_preserving_paths(
    world: &World,
    focus: RegionBounds,
    original: &BTreeSet<Pos>,
    start: Pos,
    end: Pos,
    target_cells: usize,
    control: &mut SearchControl<'_>,
) -> Vec<Vec<Pos>> {
    struct Search<'a> {
        world: &'a World,
        focus: RegionBounds,
        original: &'a BTreeSet<Pos>,
        end: Pos,
        target_cells: usize,
        budget: PhysicalOptimizationSearchBudget,
        deadline: Instant,
        stats: &'a mut PhysicalOptimizationSearchStats,
        results: Vec<Vec<Pos>>,
    }
    fn visit(search: &mut Search<'_>, path: &mut Vec<Pos>, seen: &mut BTreeSet<Pos>) {
        if search.stats.expansions >= search.budget.max_expansions {
            search.stats.truncated = true;
            search.stats.stop_reason = Some("max_expansions");
            return;
        }
        if Instant::now() >= search.deadline {
            search.stats.truncated = true;
            search.stats.stop_reason = Some("time_budget");
            return;
        }
        if search.results.len() >= search.budget.max_candidates {
            search.stats.truncated = true;
            search.stats.stop_reason = Some("max_candidates");
            return;
        }
        search.stats.expansions += 1;
        let current = *path.last().expect("path has a start");
        let remaining_edges = search.target_cells.saturating_sub(path.len());
        let distance =
            current.x.abs_diff(search.end.x) as usize + current.z.abs_diff(search.end.z) as usize;
        if distance > remaining_edges || (remaining_edges - distance) % 2 != 0 {
            return;
        }
        if path.len() == search.target_cells {
            if current == search.end
                && induced_path(path)
                && path.iter().copied().collect::<BTreeSet<_>>() != *search.original
            {
                search.results.push(path.clone());
                search.stats.candidates += 1;
            }
            return;
        }
        for next in horizontal_neighbors(current) {
            if next.y != current.y
                || !contains(search.focus, next)
                || seen.contains(&next)
                || (next == search.end && path.len() + 1 != search.target_cells)
                || (!search.original.contains(&next)
                    && search.world.kind_at(next) != BlockKind::Air)
                || !search
                    .world
                    .get(next.offset(0, -1, 0))
                    .is_some_and(|block| block.redstone_traits().supports_dust_on_top)
            {
                continue;
            }
            path.push(next);
            seen.insert(next);
            visit(search, path, seen);
            seen.remove(&next);
            path.pop();
        }
    }
    let mut search = Search {
        world,
        focus,
        original,
        end,
        target_cells,
        budget: control.budget,
        deadline: control.deadline,
        stats: &mut *control.stats,
        results: Vec::new(),
    };
    let mut path = vec![start];
    let mut seen = BTreeSet::from([start]);
    visit(&mut search, &mut path, &mut seen);
    search.results
}

fn induced_path(path: &[Pos]) -> bool {
    path.iter().enumerate().all(|(index, position)| {
        horizontal_neighbors(*position).into_iter().all(|neighbor| {
            path.iter()
                .position(|candidate| *candidate == neighbor)
                .is_none_or(|other| index.abs_diff(other) == 1)
        })
    })
}

fn score_positions(positions: &BTreeSet<Pos>) -> PhasedPhysicalScore {
    let min_x = positions.iter().map(|pos| pos.x).min().unwrap_or(0);
    let max_x = positions.iter().map(|pos| pos.x).max().unwrap_or(0);
    let min_y = positions.iter().map(|pos| pos.y).min().unwrap_or(0);
    let max_y = positions.iter().map(|pos| pos.y).max().unwrap_or(0);
    let min_z = positions.iter().map(|pos| pos.z).min().unwrap_or(0);
    let max_z = positions.iter().map(|pos| pos.z).max().unwrap_or(0);
    PhasedPhysicalScore {
        bounding_volume: axis_len(min_x, max_x)
            .saturating_mul(axis_len(min_y, max_y))
            .saturating_mul(axis_len(min_z, max_z)),
        occupied_blocks: positions.len(),
        connector_length: positions.len().saturating_sub(1),
    }
}

const fn axis_len(min: i32, max: i32) -> usize {
    max.abs_diff(min) as usize + 1
}

const fn final_score(score: PhasedPhysicalScore) -> (usize, usize, usize) {
    (
        score.bounding_volume,
        score.occupied_blocks,
        score.connector_length,
    )
}

fn connected_count(adjacency: &BTreeMap<Pos, BTreeSet<Pos>>, start: Pos) -> usize {
    let mut seen = BTreeSet::from([start]);
    let mut queue = VecDeque::from([start]);
    while let Some(pos) = queue.pop_front() {
        for neighbor in &adjacency[&pos] {
            if seen.insert(*neighbor) {
                queue.push_back(*neighbor);
            }
        }
    }
    seen.len()
}

fn orthogonal_path(start: Pos, end: Pos, x_first: bool) -> Vec<Pos> {
    let corner = if x_first {
        Pos::new(end.x, start.y, start.z)
    } else {
        Pos::new(start.x, start.y, end.z)
    };
    axis_segment(start, corner)
        .into_iter()
        .chain(axis_segment(corner, end).into_iter().skip(1))
        .collect()
}

fn axis_segment(start: Pos, end: Pos) -> Vec<Pos> {
    let dx = (end.x - start.x).signum();
    let dz = (end.z - start.z).signum();
    let length = start.x.abs_diff(end.x) + start.z.abs_diff(end.z);
    (0..=length)
        .map(|step| {
            let step = i32::try_from(step).expect("path step fits i32");
            start.offset(dx * step, 0, dz * step)
        })
        .collect()
}

fn path_supported_and_clear(world: &World, original: &BTreeSet<Pos>, path: &[Pos]) -> bool {
    path.iter().all(|pos| {
        (original.contains(pos) || world.kind_at(*pos) == BlockKind::Air)
            && world
                .get(pos.offset(0, -1, 0))
                .is_some_and(|block| block.redstone_traits().supports_dust_on_top)
    })
}

fn creates_unexpected_adjacency(
    world: &World,
    original: &BTreeSet<Pos>,
    path: &[Pos],
    start: Pos,
    end: Pos,
) -> bool {
    let path = path.iter().copied().collect::<BTreeSet<_>>();
    path.iter().any(|pos| {
        horizontal_neighbors(*pos).into_iter().any(|neighbor| {
            if path.contains(&neighbor) || original.contains(&neighbor) {
                return false;
            }
            world.kind_at(neighbor).is_redstone_related() && *pos != start && *pos != end
        })
    })
}

fn horizontal_neighbors(pos: Pos) -> [Pos; 4] {
    [
        pos.offset(1, 0, 0),
        pos.offset(-1, 0, 0),
        pos.offset(0, 0, 1),
        pos.offset(0, 0, -1),
    ]
}

const fn contains(bounds: RegionBounds, pos: Pos) -> bool {
    pos.x >= bounds.min.x
        && pos.x <= bounds.max.x
        && pos.y >= bounds.min.y
        && pos.y <= bounds.max.y
        && pos.z >= bounds.min.z
        && pos.z <= bounds.max.z
}

#[cfg(test)]
mod tests {
    use super::*;
    use dustroute_translate::update_wire_shapes;

    #[test]
    fn shortens_a_supported_detour_and_preserves_every_block_outside_focus() {
        let mut world = World::new();
        for x in 0..=4 {
            for z in 0..=2 {
                world.set(Pos::new(x, 0, z), Block::new(BlockKind::Solid));
            }
        }
        let detour = [
            Pos::new(0, 1, 0),
            Pos::new(0, 1, 1),
            Pos::new(0, 1, 2),
            Pos::new(1, 1, 2),
            Pos::new(2, 1, 2),
            Pos::new(3, 1, 2),
            Pos::new(4, 1, 2),
            Pos::new(4, 1, 1),
            Pos::new(4, 1, 0),
        ];
        for pos in detour {
            world.place(BlockKind::RedstoneWire, pos);
        }
        world.place(BlockKind::Lever, Pos::new(-1, 1, 0));
        let outside = Pos::new(8, 1, 8);
        world.place(BlockKind::RedstoneBlock, outside);
        update_wire_shapes(&mut world);
        let focus = RegionBounds::new(Pos::new(0, 1, 0), Pos::new(4, 1, 2));

        let optimization = optimize_physical_wire_path(&world, focus).unwrap();
        assert_eq!(optimization.wire_blocks_before, 9);
        assert_eq!(optimization.wire_blocks_after, 5);
        assert!(
            optimization
                .patch
                .changes
                .iter()
                .all(|change| contains(focus, change.pos))
        );
        let optimized = optimization.patch.apply_virtual(&world).unwrap();
        assert_eq!(optimized.kind_at(outside), BlockKind::RedstoneBlock);
        assert_eq!(
            optimization
                .patch
                .inverse()
                .apply_virtual(&optimized)
                .unwrap(),
            world
        );
    }

    #[test]
    fn shortened_detour_preserves_steady_and_transition_contracts() {
        let mut world = World::new();
        for x in -1..=5 {
            for z in 0..=2 {
                world.set(Pos::new(x, 0, z), Block::new(BlockKind::Solid));
            }
        }
        for pos in [
            Pos::new(0, 1, 0),
            Pos::new(0, 1, 1),
            Pos::new(0, 1, 2),
            Pos::new(1, 1, 2),
            Pos::new(2, 1, 2),
            Pos::new(3, 1, 2),
            Pos::new(4, 1, 2),
            Pos::new(4, 1, 1),
            Pos::new(4, 1, 0),
        ] {
            world.place(BlockKind::RedstoneWire, pos);
        }
        world.place(BlockKind::Lever, Pos::new(-1, 1, 0));
        world.place(BlockKind::RedstoneLamp, Pos::new(5, 1, 0));
        update_wire_shapes(&mut world);
        let bounds = RegionBounds::new(Pos::new(-1, 0, 0), Pos::new(5, 1, 2));
        let focus = RegionBounds::new(Pos::new(0, 1, 0), Pos::new(4, 1, 2));
        let analysis = dustroute_translate::analyze_world_region(&world, bounds);
        let truth = dustroute_translate::infer_truth_table(&world, &analysis, 4, 64)
            .expect("detour truth table");
        let optimization = optimize_physical_wire_path(&world, focus).unwrap();
        let mut optimized = optimization.patch.apply_virtual(&world).unwrap();
        update_wire_shapes(&mut optimized);
        let optimized_analysis = dustroute_translate::analyze_world_region(&optimized, bounds);
        let optimized_truth =
            dustroute_translate::infer_truth_table(&optimized, &optimized_analysis, 4, 64)
                .expect("optimized truth table");
        let comparison = dustroute_translate::compare_truth_tables(&truth, &optimized_truth);
        let steady = crate::MacroSteadyStateReport {
            state: if comparison.comparable && comparison.differing_bits == 0 {
                crate::ContextualVerificationState::Passed
            } else {
                crate::ContextualVerificationState::Failed
            },
            comparison: Some(comparison),
            input_mapping: vec![0],
            output_mapping: vec![0],
            differing_assignments: Vec::new(),
            reason: None,
        };
        let transitions = crate::verify_world_transitions(
            &world,
            &truth,
            &optimized,
            &optimized_truth,
            64,
            20,
            4,
        );
        let strength = crate::verify_boundary_strengths(
            &world,
            &optimized,
            &truth,
            &optimization.fixed_endpoints,
            64,
        );
        let assessment = crate::assess_macro_contract(
            crate::OptimizationContract::default(),
            &crate::MacroStructuralReport::default(),
            Some(&steady),
            Some(&transitions),
            optimization.patch.changes.len(),
            Some(strength.is_ok()),
        );
        assert_eq!(
            steady.state,
            crate::ContextualVerificationState::Passed,
            "{steady:#?}\noriginal={truth:#?}\noptimized={optimized_truth:#?}"
        );
        assert_eq!(
            transitions.state,
            crate::ContextualVerificationState::Passed
        );
        assert!(strength.is_err());
        assert!(assessment.satisfied(), "{assessment:#?}");
    }

    #[test]
    fn refuses_a_branching_dust_network() {
        let mut world = World::new();
        for pos in [
            Pos::new(0, 1, 0),
            Pos::new(1, 1, 0),
            Pos::new(2, 1, 0),
            Pos::new(1, 1, 1),
        ] {
            world.set(pos.offset(0, -1, 0), Block::new(BlockKind::Solid));
            world.place(BlockKind::RedstoneWire, pos);
        }
        update_wire_shapes(&mut world);
        assert_eq!(
            optimize_physical_wire_path(
                &world,
                RegionBounds::new(Pos::new(0, 1, 0), Pos::new(2, 1, 1))
            ),
            Err(PhysicalWireOptimizationError::NotSimplePath)
        );
    }

    #[test]
    fn optimizes_one_branch_segment_while_fixing_the_junction() {
        let mut world = World::new();
        for x in -1..=4 {
            for z in -1..=2 {
                world.set(Pos::new(x, 0, z), Block::new(BlockKind::Solid));
            }
        }
        for wire in [
            Pos::new(-1, 1, 0),
            Pos::new(0, 1, -1),
            Pos::new(0, 1, 0),
            Pos::new(0, 1, 1),
            Pos::new(0, 1, 2),
            Pos::new(1, 1, 2),
            Pos::new(2, 1, 2),
            Pos::new(3, 1, 2),
            Pos::new(4, 1, 2),
            Pos::new(4, 1, 1),
            Pos::new(4, 1, 0),
        ] {
            world.place(BlockKind::RedstoneWire, wire);
        }
        update_wire_shapes(&mut world);
        let optimization = optimize_physical_wire_path(
            &world,
            RegionBounds::new(Pos::new(-1, 1, -1), Pos::new(4, 1, 2)),
        )
        .unwrap();
        assert_eq!(
            optimization.fixed_endpoints,
            [Pos::new(0, 1, 0), Pos::new(4, 1, 0)]
        );
        assert_eq!(optimization.wire_blocks_before, 9);
        assert_eq!(optimization.wire_blocks_after, 5);
        let optimized = optimization.patch.apply_virtual(&world).unwrap();
        assert_eq!(
            optimized.kind_at(Pos::new(-1, 1, 0)),
            BlockKind::RedstoneWire
        );
        assert_eq!(
            optimized.kind_at(Pos::new(0, 1, -1)),
            BlockKind::RedstoneWire
        );
        assert_eq!(
            optimized.kind_at(Pos::new(0, 1, 0)),
            BlockKind::RedstoneWire
        );
    }

    #[test]
    fn optimizes_dust_around_a_fixed_delayed_and_locked_repeater() {
        let mut world = World::new();
        for x in 0..=8 {
            for z in 0..=2 {
                world.set(Pos::new(x, 0, z), Block::new(BlockKind::Solid));
            }
        }
        for wire in [
            Pos::new(0, 1, 0),
            Pos::new(0, 1, 1),
            Pos::new(0, 1, 2),
            Pos::new(1, 1, 2),
            Pos::new(2, 1, 2),
            Pos::new(3, 1, 2),
            Pos::new(4, 1, 2),
            Pos::new(4, 1, 1),
            Pos::new(4, 1, 0),
            Pos::new(6, 1, 0),
            Pos::new(7, 1, 0),
            Pos::new(8, 1, 0),
        ] {
            world.place(BlockKind::RedstoneWire, wire);
        }
        let repeater = world.place(BlockKind::Repeater, Pos::new(5, 1, 0));
        repeater.facing = Some(dustroute_physical::Facing::East);
        repeater.delay = Some(4);
        let lock = world.place(BlockKind::Repeater, Pos::new(5, 1, 1));
        lock.facing = Some(dustroute_physical::Facing::North);
        lock.delay = Some(2);
        update_wire_shapes(&mut world);
        let repeater_before = world.get(Pos::new(5, 1, 0)).cloned().unwrap();
        let lock_before = world.get(Pos::new(5, 1, 1)).cloned().unwrap();
        let optimization = optimize_physical_wire_path(
            &world,
            RegionBounds::new(Pos::new(0, 1, 0), Pos::new(8, 1, 2)),
        )
        .unwrap();
        let optimized = optimization.patch.apply_virtual(&world).unwrap();
        assert_eq!(optimized.get(Pos::new(5, 1, 0)), Some(&repeater_before));
        assert_eq!(optimized.get(Pos::new(5, 1, 1)), Some(&lock_before));
        assert!(
            optimization
                .patch
                .changes
                .iter()
                .all(|change| change.after.kind != BlockKind::Repeater)
        );
    }

    #[test]
    fn shortens_a_single_detour_terminated_by_a_repeater() {
        let mut world = World::new();
        for x in 0..=5 {
            for z in 0..=2 {
                world.set(Pos::new(x, 0, z), Block::new(BlockKind::Solid));
            }
        }
        for wire in [
            Pos::new(0, 1, 0),
            Pos::new(0, 1, 1),
            Pos::new(0, 1, 2),
            Pos::new(1, 1, 2),
            Pos::new(2, 1, 2),
            Pos::new(3, 1, 2),
            Pos::new(4, 1, 2),
            Pos::new(4, 1, 1),
            Pos::new(4, 1, 0),
        ] {
            world.place(BlockKind::RedstoneWire, wire);
        }
        world.place(BlockKind::Lever, Pos::new(-1, 1, 0));
        let repeater = world.place(BlockKind::Repeater, Pos::new(5, 1, 0));
        repeater.facing = Some(dustroute_physical::Facing::East);
        repeater.delay = Some(1);
        update_wire_shapes(&mut world);
        let optimization = optimize_physical_wire_path(
            &world,
            RegionBounds::new(Pos::new(-1, 0, 0), Pos::new(5, 1, 2)),
        )
        .unwrap();
        assert_eq!(optimization.wire_blocks_after, 5);
    }

    #[test]
    fn compacts_an_equal_length_path_when_strength_must_be_preserved() {
        let mut world = World::new();
        for x in 0..=8 {
            for z in 0..=4 {
                world.set(Pos::new(x, 0, z), Block::new(BlockKind::Solid));
            }
        }
        let original = (0..=8)
            .map(|x| Pos::new(x, 1, 0))
            .chain((1..=4).map(|z| Pos::new(8, 1, z)))
            .chain((4..=7).rev().map(|x| Pos::new(x, 1, 4)))
            .collect::<Vec<_>>();
        for pos in original {
            world.place(BlockKind::RedstoneWire, pos);
        }
        update_wire_shapes(&mut world);
        let optimization = optimize_physical_wire_path_with_constraints(
            &world,
            RegionBounds::new(Pos::new(0, 1, 0), Pos::new(8, 1, 4)),
            true,
        )
        .unwrap();
        assert_eq!(optimization.wire_blocks_before, 17);
        assert_eq!(optimization.wire_blocks_after, 17);
        let compact = optimization.patch.apply_virtual(&world).unwrap();
        let compact_wires = compact
            .iter()
            .filter(|(_, block)| block.kind == BlockKind::RedstoneWire)
            .map(|(pos, _)| *pos)
            .collect::<BTreeSet<_>>();
        assert!(score_positions(&compact_wires).bounding_volume < 45);

        let original = world
            .iter()
            .filter_map(|(position, block)| {
                (block.kind == BlockKind::RedstoneWire).then_some(*position)
            })
            .collect::<BTreeSet<_>>();
        let mut stats = PhysicalOptimizationSearchStats::default();
        {
            let mut control = SearchControl {
                budget: PhysicalOptimizationSearchBudget {
                    max_candidates: 1,
                    ..PhysicalOptimizationSearchBudget::default()
                },
                deadline: Instant::now() + Duration::from_secs(1),
                stats: &mut stats,
            };
            let _ = strength_preserving_paths(
                &world,
                RegionBounds::new(Pos::new(0, 1, 0), Pos::new(8, 1, 4)),
                &original,
                Pos::new(0, 1, 0),
                Pos::new(4, 1, 4),
                17,
                &mut control,
            );
        }
        assert!(stats.truncated);
        assert_eq!(stats.stop_reason, Some("max_candidates"));
    }

    #[test]
    fn shortens_a_supported_path_across_a_one_block_rise() {
        let mut world = World::new();
        for support in [
            Pos::new(0, 0, 0),
            Pos::new(0, 0, 1),
            Pos::new(1, 0, 1),
            Pos::new(2, 0, 1),
            Pos::new(1, 1, 0),
            Pos::new(2, 1, 0),
        ] {
            world.set(support, Block::new(BlockKind::Solid));
        }
        for wire in [
            Pos::new(0, 1, 0),
            Pos::new(0, 1, 1),
            Pos::new(1, 1, 1),
            Pos::new(2, 1, 1),
            Pos::new(2, 2, 0),
        ] {
            world.place(BlockKind::RedstoneWire, wire);
        }
        update_wire_shapes(&mut world);
        let optimization = optimize_physical_wire_path(
            &world,
            RegionBounds::new(Pos::new(0, 1, 0), Pos::new(2, 2, 1)),
        )
        .unwrap();
        assert_eq!(optimization.wire_blocks_before, 5);
        assert_eq!(optimization.wire_blocks_after, 3);
        assert!(
            optimization
                .patch
                .changes
                .iter()
                .any(|change| change.pos == Pos::new(1, 2, 0))
        );
    }

    #[test]
    fn local_density_can_spend_connector_length_before_global_recovery() {
        let baseline = PhasedPhysicalScore {
            bounding_volume: 120,
            occupied_blocks: 30,
            connector_length: 12,
        };
        let local = PhasedPhysicalScore {
            bounding_volume: 60,
            occupied_blocks: 26,
            connector_length: 17,
        };
        let recovered = PhasedPhysicalScore {
            bounding_volume: 48,
            occupied_blocks: 22,
            connector_length: 10,
        };
        let selected = select_phased_physical_scores(baseline, [local], [recovered], 6).unwrap();
        assert_eq!(selected.local_density, local);
        assert!(selected.local_density.connector_length > baseline.connector_length);
        assert_eq!(selected.final_global, recovered);
        assert!(final_score(selected.final_global) < final_score(baseline));
    }

    #[test]
    fn rejects_a_dense_local_move_when_no_final_global_improvement_exists() {
        let baseline = PhasedPhysicalScore {
            bounding_volume: 40,
            occupied_blocks: 20,
            connector_length: 8,
        };
        let local = PhasedPhysicalScore {
            bounding_volume: 30,
            occupied_blocks: 18,
            connector_length: 12,
        };
        assert_eq!(
            select_phased_physical_scores(baseline, [local], [baseline], 4),
            None
        );
    }
}
