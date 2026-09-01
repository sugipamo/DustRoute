use std::collections::{BTreeMap, BTreeSet, VecDeque};

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
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
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
}

pub fn optimize_physical_wire_path(
    world: &World,
    focus: RegionBounds,
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
        return Err(PhysicalWireOptimizationError::NotSimplePath);
    }
    let endpoints = adjacency
        .iter()
        .filter_map(|(pos, neighbors)| (neighbors.len() == 1).then_some(*pos))
        .collect::<Vec<_>>();
    if endpoints.len() != 2 || connected_count(&adjacency, endpoints[0]) != wires.len() {
        return Err(PhysicalWireOptimizationError::NotSimplePath);
    }
    let [start, end] = [endpoints[0], endpoints[1]];
    if start.y != end.y {
        return Err(PhysicalWireOptimizationError::DifferentHeights);
    }

    let candidates = [
        orthogonal_path(start, end, true),
        orthogonal_path(start, end, false),
    ];
    let path = candidates
        .into_iter()
        .filter(|path| path.iter().all(|pos| contains(focus, *pos)))
        .filter(|path| path_supported_and_clear(world, &wires, path))
        .filter(|path| !creates_unexpected_adjacency(world, &wires, path, start, end))
        .min_by_key(|path| {
            let additions = path.iter().filter(|pos| !wires.contains(pos)).count();
            (additions, path.clone())
        })
        .ok_or(PhysicalWireOptimizationError::UnsupportedRoute)?;
    if path.len() >= wires.len() {
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
        let assessment = crate::assess_macro_contract(
            crate::OptimizationContract::default(),
            &crate::MacroStructuralReport::default(),
            Some(&steady),
            Some(&transitions),
            optimization.patch.changes.len(),
            false,
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
