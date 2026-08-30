use std::collections::BTreeSet;
use std::error::Error;
use std::fmt::{Display, Formatter};

use dustroute_model::Pos;
use dustroute_translate::{RegionAnalysis, RegionBounds};
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct CircuitDiscovery {
    pub seed: Pos,
    pub looked_at: Pos,
    pub bounds: RegionBoundsDto,
    pub node_count: usize,
    pub fragment_count: usize,
    pub gap_candidate_count: usize,
    pub touches_scan_boundary: bool,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct RegionBoundsDto {
    pub min: Pos,
    pub max: Pos,
}

impl From<RegionBounds> for RegionBoundsDto {
    fn from(value: RegionBounds) -> Self {
        Self {
            min: value.min,
            max: value.max,
        }
    }
}

impl From<RegionBoundsDto> for RegionBounds {
    fn from(value: RegionBoundsDto) -> Self {
        Self::new(value.min, value.max)
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum DiscoveryError {
    NoRedstoneNearTarget,
    NodeLimitExceeded { limit: usize },
}

impl Display for DiscoveryError {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::NoRedstoneNearTarget => f.write_str("no connected redstone near the gaze target"),
            Self::NodeLimitExceeded { limit } => {
                write!(f, "connected circuit exceeds the {limit} node limit")
            }
        }
    }
}

impl Error for DiscoveryError {}

pub fn discover_connected_region(
    analysis: &RegionAnalysis,
    looked_at: Pos,
    seed_distance: i32,
    fragment_gap: u32,
    padding: i32,
    max_nodes: usize,
) -> Result<CircuitDiscovery, DiscoveryError> {
    let verified = analysis.scene.verified_topology();
    let seed_component = verified
        .components
        .iter()
        .filter(|component| component.block.kind.is_redstone_related())
        .filter_map(|component| {
            let distance = manhattan(component.pos, looked_at);
            (distance <= seed_distance).then_some((distance, component))
        })
        .min_by_key(|(distance, component)| (*distance, component.pos))
        .ok_or(DiscoveryError::NoRedstoneNearTarget)?;
    let seed_fragment = verified
        .fragments
        .iter()
        .find(|fragment| fragment.components.contains(&seed_component.1.id))
        .ok_or(DiscoveryError::NoRedstoneNearTarget)?;
    let fragments = verified.discover_nearby_fragments(seed_fragment.id, fragment_gap);
    let component_ids: BTreeSet<_> = verified
        .fragments
        .iter()
        .filter(|fragment| fragments.contains(&fragment.id))
        .flat_map(|fragment| fragment.components.iter().copied())
        .collect();
    if component_ids.len() > max_nodes {
        return Err(DiscoveryError::NodeLimitExceeded { limit: max_nodes });
    }
    let nodes: BTreeSet<_> = verified
        .components
        .iter()
        .filter(|component| component_ids.contains(&component.id))
        .map(|component| component.pos)
        .collect();
    let gap_candidate_count = verified
        .gap_candidates(fragment_gap)
        .iter()
        .filter(|candidate| {
            fragments.contains(&candidate.left) && fragments.contains(&candidate.right)
        })
        .count();
    let seed = seed_component.1.pos;

    let min = Pos::new(
        nodes.iter().map(|pos| pos.x).min().unwrap() - padding,
        nodes.iter().map(|pos| pos.y).min().unwrap() - padding,
        nodes.iter().map(|pos| pos.z).min().unwrap() - padding,
    );
    let max = Pos::new(
        nodes.iter().map(|pos| pos.x).max().unwrap() + padding,
        nodes.iter().map(|pos| pos.y).max().unwrap() + padding,
        nodes.iter().map(|pos| pos.z).max().unwrap() + padding,
    );
    let scan = analysis.bounds;
    let touches_scan_boundary = nodes.iter().any(|pos| {
        pos.x == scan.min.x
            || pos.x == scan.max.x
            || pos.y == scan.min.y
            || pos.y == scan.max.y
            || pos.z == scan.min.z
            || pos.z == scan.max.z
    });
    Ok(CircuitDiscovery {
        seed,
        looked_at,
        bounds: RegionBounds::new(min, max).into(),
        node_count: nodes.len(),
        fragment_count: fragments.len(),
        gap_candidate_count,
        touches_scan_boundary,
    })
}

fn manhattan(a: Pos, b: Pos) -> i32 {
    (a.x - b.x).abs() + (a.y - b.y).abs() + (a.z - b.z).abs()
}

#[cfg(test)]
mod tests {
    use dustroute_model::{Block, BlockKind, World};
    use dustroute_translate::{RegionBounds, analyze_world_region};

    use super::*;

    #[test]
    fn discovers_only_the_connected_circuit_near_gaze() {
        let mut world = World::new();
        for x in 0..=3 {
            world.set(Pos::new(x, 0, 0), Block::new(BlockKind::Solid));
            world.set(Pos::new(x, 1, 0), Block::new(BlockKind::RedstoneWire));
        }
        world.set(Pos::new(20, 0, 0), Block::new(BlockKind::Solid));
        world.set(Pos::new(20, 1, 0), Block::new(BlockKind::RedstoneWire));
        dustroute_translate::update_wire_shapes(&mut world);
        let scan = RegionBounds::new(Pos::new(-2, -1, -2), Pos::new(22, 3, 2));
        let analysis = analyze_world_region(&world, scan);
        let found = discover_connected_region(&analysis, Pos::new(0, 0, 0), 2, 2, 1, 100).unwrap();
        assert_eq!(found.node_count, 8);
        assert_eq!(found.bounds.min, Pos::new(-1, -1, -1));
        assert_eq!(found.bounds.max, Pos::new(4, 2, 1));
    }

    #[test]
    fn reports_when_candidate_reaches_initial_scan_edge() {
        let mut world = World::new();
        for x in 0..=2 {
            world.set(Pos::new(x, 0, 0), Block::new(BlockKind::Solid));
            world.set(Pos::new(x, 1, 0), Block::new(BlockKind::RedstoneWire));
        }
        dustroute_translate::update_wire_shapes(&mut world);
        let scan = RegionBounds::new(Pos::new(0, 0, 0), Pos::new(2, 2, 0));
        let analysis = analyze_world_region(&world, scan);
        let found = discover_connected_region(&analysis, Pos::new(1, 1, 0), 1, 2, 0, 100).unwrap();
        assert!(found.touches_scan_boundary);
    }

    #[test]
    fn discovers_nearby_broken_fragment_without_unioning_it() {
        let mut world = World::new();
        for x in [0, 1, 3, 4] {
            world.set(Pos::new(x, 0, 0), Block::new(BlockKind::Solid));
            world.set(Pos::new(x, 1, 0), Block::new(BlockKind::RedstoneWire));
        }
        dustroute_translate::update_wire_shapes(&mut world);
        let scan = RegionBounds::new(Pos::new(-2, -1, -2), Pos::new(6, 3, 2));
        let analysis = analyze_world_region(&world, scan);
        let found = discover_connected_region(&analysis, Pos::new(0, 1, 0), 1, 2, 0, 100).unwrap();
        assert_eq!(found.fragment_count, 2);
        assert_eq!(found.gap_candidate_count, 1);
        assert_eq!(analysis.scene.fragments.len(), 2);
        assert_eq!(found.bounds.max.x, 4);
    }
}
