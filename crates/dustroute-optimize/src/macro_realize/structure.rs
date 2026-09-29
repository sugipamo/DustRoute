//! Structural admissibility of a proposed replacement.
use super::ownership::replacement_ownership;
use super::{MacroReplacementPlan, MacroStructuralReport};
use dustroute_physical::{BlockKind, Pos, World};
use std::collections::BTreeSet;

/// Checks a proposal against the observed context without changing it.
/// `replaceable` is the exact ownership set of the focused implementation;
/// occupied blocks outside it are immutable obstacles.
#[must_use]
pub fn validate_macro_structure(
    plan: &MacroReplacementPlan,
    observed: &World,
    replaceable: &BTreeSet<Pos>,
) -> MacroStructuralReport {
    let replaceable = replacement_ownership(plan, observed, replaceable);
    let mut report = MacroStructuralReport::default();
    let candidate_blocks = plan
        .placed
        .blocks()
        .collect::<std::collections::BTreeMap<_, _>>();
    let boundary_positions = plan
        .routes
        .iter()
        .map(|route| route.boundary.position)
        .collect::<BTreeSet<_>>();
    for (pos, block) in &candidate_blocks {
        let compatible_boundary = boundary_positions.contains(pos)
            && observed
                .get(*pos)
                .is_some_and(|actual| actual.kind == block.kind);
        if observed.get(*pos).is_some_and(|actual| actual != block)
            && !replaceable.contains(pos)
            && !compatible_boundary
        {
            report.candidate_collisions.push(*pos);
        }
    }
    let mut candidate_world = observed.clone();
    for pos in &replaceable {
        candidate_world.remove(*pos);
    }
    for (pos, block) in &candidate_blocks {
        if boundary_positions.contains(pos)
            && observed
                .get(*pos)
                .is_some_and(|actual| actual.kind == block.kind)
        {
            continue;
        }
        candidate_world.set(*pos, block.clone());
    }
    report.candidate_support_issues = candidate_world
        .support_issues()
        .into_iter()
        .filter_map(|(pos, _, _)| candidate_blocks.contains_key(&pos).then_some(pos))
        .collect();

    for (index, route) in plan.routes.iter().enumerate() {
        for pos in route
            .path
            .iter()
            .copied()
            .skip(1)
            .take(route.path.len().saturating_sub(2))
        {
            if candidate_blocks.contains_key(&pos)
                || (observed.kind_at(pos) != BlockKind::Air && !replaceable.contains(&pos))
            {
                report.route_collisions.push(pos);
            }
            let support = pos.offset(0, -1, 0);
            if candidate_blocks
                .get(&support)
                .or_else(|| {
                    (!replaceable.contains(&support))
                        .then(|| observed.get(support))
                        .flatten()
                })
                .is_some_and(|block| block.redstone_traits().supports_dust_on_top)
            {
                continue;
            }
            if observed.kind_at(support) == BlockKind::Air || replaceable.contains(&support) {
                report.required_route_supports.push(support);
            } else {
                report.blocked_route_supports.push(support);
            }
        }
        for (other_index, other) in plan.routes.iter().enumerate().skip(index + 1) {
            for (first_index, first) in route.path.iter().enumerate() {
                for (second_index, second) in other.path.iter().enumerate() {
                    let dx = (first.x - second.x).abs();
                    let dy = (first.y - second.y).abs();
                    let dz = (first.z - second.z).abs();
                    let first_internal = first_index > 0 && first_index + 1 < route.path.len();
                    let second_internal = second_index > 0 && second_index + 1 < other.path.len();
                    if first == second
                        || (first_internal && second_internal && dx + dz == 1 && dy <= 1)
                    {
                        report
                            .route_cross_net_contacts
                            .push((index, other_index, *first, *second));
                    }
                }
            }
        }
    }
    report.candidate_collisions.sort_unstable();
    report.candidate_collisions.dedup();
    report.route_collisions.sort_unstable();
    report.route_collisions.dedup();
    report.candidate_support_issues.sort_unstable();
    report.candidate_support_issues.dedup();
    report.required_route_supports.sort_unstable();
    report.required_route_supports.dedup();
    report.blocked_route_supports.sort_unstable();
    report.blocked_route_supports.dedup();
    report
}
