//! Deterministic support/watch precedence and teardown selection.
//! These are candidate orders; the shared runtime verifies every step.
use super::policy::{Predecessor, for_kind};
use dustroute_minecraft::piston_electrical::along;
use dustroute_minecraft::{Block, BlockKind, Pos, World};
use std::collections::{BTreeMap, BTreeSet};

pub(super) fn ordered_blocks(literal: &World) -> Vec<(Pos, Block)> {
    let mut remaining: Vec<_> = literal.iter().map(|(p, b)| (*p, b.clone())).collect();
    remaining.sort_by_key(|(p, b)| (for_kind(b.kind).build, p.y, p.x, p.z));
    remaining
}

pub(super) fn observer_predecessors(
    remaining: &[(Pos, Block)],
    literal: &World,
) -> Result<BTreeMap<Pos, Pos>, String> {
    // Construct a declared occupied watched cell before its observer. This
    // is only a candidate-order constraint: later callbacks may still
    // change that cell, so full settling/final-state/teardown checks remain.
    // Observer Block.facing is its output; the watched side is opposite.
    let mut observer_predecessors = BTreeMap::new();
    for (pos, block) in remaining {
        if for_kind(block.kind).predecessor == Predecessor::OccupiedWatchedCell {
            let output = block.facing.ok_or("observer output required")?;
            let watched = along(*pos, output.opposite()).map_err(|e| e.to_string())?;
            if literal
                .get(watched)
                .is_some_and(|b| b.kind != BlockKind::Air)
            {
                observer_predecessors.insert(*pos, watched);
            }
        }
    }
    Ok(observer_predecessors)
}

pub(super) fn next_build_index(
    remaining: &[(Pos, Block)],
    observer_predecessors: &BTreeMap<Pos, Pos>,
    installed: &BTreeSet<Pos>,
    world: &World,
) -> Result<usize, String> {
    remaining
        .iter()
        .position(|(pos, block)| {
            if observer_predecessors
                .get(pos)
                .is_some_and(|watched| !installed.contains(watched))
            {
                return false;
            }
            block.support_offset.is_none_or(|d| {
                pos.x
                    .checked_add(d.x)
                    .zip(pos.y.checked_add(d.y))
                    .zip(pos.z.checked_add(d.z))
                    .is_some_and(|((x, y), z)| {
                        world
                            .get(Pos::new(x, y, z))
                            .is_some_and(|b| b.kind != BlockKind::Air)
                    })
            })
        })
        .ok_or("construction support and observer-front dependencies cannot be ordered".into())
}

pub(super) fn next_removal_position(world: &World) -> Result<Pos, String> {
    next_removal_position_matching(world, |_| true)
}

pub(super) fn next_removal_position_matching(
    world: &World,
    selected: impl Fn(Pos) -> bool,
) -> Result<Pos, String> {
    // Rebuild from the current world after every settled command. Callbacks
    // can change supports; a cached index across commands would be unsound.
    let occupied_supports = world
        .iter()
        .filter_map(|(other, block)| {
            let d = block.support_offset?;
            other
                .x
                .checked_add(d.x)
                .zip(other.y.checked_add(d.y))
                .zip(other.z.checked_add(d.z))
                .map(|((x, y), z)| Pos::new(x, y, z))
        })
        .collect::<BTreeSet<_>>();
    world
        .iter()
        .filter(|(pos, _)| selected(**pos))
        .filter(|(_, b)| for_kind(b.kind).removal.is_some())
        .filter(|(pos, _)| !occupied_supports.contains(pos))
        .min_by_key(|(p, b)| (for_kind(b.kind).removal, p.y, p.x, p.z))
        .map(|(p, _)| *p)
        .ok_or_else(|| "teardown has an orphan piston head".into())
}
