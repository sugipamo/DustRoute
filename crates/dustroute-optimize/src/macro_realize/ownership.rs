//! Source-owned cells shared by validation and materialization.
use super::MacroReplacementPlan;
use dustroute_physical::{Pos, World};
use std::collections::BTreeSet;

/// Fixed observed blocks retain their existing supports. The replaceable set
/// permits edits; it does not grant ownership of another retained block's
/// support. This preserves existing state and never synthesizes a repair.
pub(super) fn replacement_ownership(
    plan: &MacroReplacementPlan,
    observed: &World,
    replaceable: &BTreeSet<Pos>,
) -> BTreeSet<Pos> {
    let mut protected = observed
        .positions()
        .filter(|pos| !replaceable.contains(pos))
        .collect::<BTreeSet<_>>();
    protected.extend(plan.routes.iter().map(|route| route.boundary.position));
    protected.extend(
        plan.routes
            .iter()
            .filter_map(|route| route.boundary.driver_position),
    );
    let mut pending = protected.iter().copied().collect::<Vec<_>>();
    while let Some(position) = pending.pop() {
        if let Some(support) = observed
            .get(position)
            .and_then(|block| block.support_pos(position))
            && observed.get(support).is_some()
            && protected.insert(support)
        {
            pending.push(support);
        }
    }
    replaceable.difference(&protected).copied().collect()
}
