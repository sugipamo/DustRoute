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
    })
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
}
