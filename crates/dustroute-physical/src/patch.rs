use serde::{Deserialize, Serialize};

use crate::{Block, BlockKind, FragmentId, GapEvidence, Pos, TemporalRequirement, World};

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct PhysicalBlockChange {
    pub pos: Pos,
    pub before: Block,
    pub after: Block,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum PhysicalPatchReason {
    ConnectMissingWire,
    InsertDirectionalComponent,
    RestoreComponentSupport,
    ReorientDirectionalComponent,
    RemoveUnexpectedConnection,
    OptimizePlacement,
}

/// Compatibility name for repair-specific callers. New generic patch APIs
/// should use [`PhysicalPatchReason`].
pub type RepairReason = PhysicalPatchReason;

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct PhysicalPatch {
    pub reason: PhysicalPatchReason,
    pub affected_fragments: Vec<FragmentId>,
    /// Confidence in percent. This is evidence strength, not a guarantee of user intent.
    pub confidence_percent: u8,
    pub explanation: String,
    pub changes: Vec<PhysicalBlockChange>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct RepairProposal {
    pub patch: PhysicalPatch,
    pub evidence: Vec<GapEvidence>,
    pub impact: Option<RepairImpact>,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct RepairImpact {
    /// Compatibility metric for undirected discovery grouping.
    pub fragments_before: usize,
    /// Compatibility metric for undirected discovery grouping.
    pub fragments_after: usize,
    pub invalid_supports_before: usize,
    pub invalid_supports_after: usize,
    pub undriven_required_inputs_before: usize,
    pub undriven_required_inputs_after: usize,
    pub external_input_waiting_before: usize,
    pub external_input_waiting_after: usize,
    pub drive_reachable_components_before: usize,
    pub drive_reachable_components_after: usize,
    pub instantaneous_solve_converged_before: bool,
    pub instantaneous_solve_converged_after: bool,
    pub energized_positions_before: usize,
    pub energized_positions_after: usize,
    pub temporal_requirement_after: TemporalRequirement,
    pub requires_temporal_validation: bool,
}

impl RepairImpact {
    #[must_use]
    pub const fn improves(self) -> bool {
        (!self.instantaneous_solve_converged_before || self.instantaneous_solve_converged_after)
            && self.invalid_supports_after <= self.invalid_supports_before
            && self.undriven_required_inputs_after <= self.undriven_required_inputs_before
            && self.drive_reachable_components_after >= self.drive_reachable_components_before
            && (self.fragments_after < self.fragments_before
                || self.invalid_supports_after < self.invalid_supports_before
                || self.undriven_required_inputs_after < self.undriven_required_inputs_before
                || self.drive_reachable_components_after > self.drive_reachable_components_before)
    }
}

impl PhysicalPatch {
    #[must_use]
    pub fn inverse(&self) -> Self {
        Self {
            reason: self.reason,
            affected_fragments: self.affected_fragments.clone(),
            confidence_percent: 100,
            explanation: format!("undo: {}", self.explanation),
            changes: self
                .changes
                .iter()
                .map(|change| PhysicalBlockChange {
                    pos: change.pos,
                    before: change.after.clone(),
                    after: change.before.clone(),
                })
                .collect(),
        }
    }

    /// Apply this patch to a cloned observation without mutating the source.
    /// Every captured before-state must still match, preventing stale repairs
    /// from being evaluated against a different world.
    pub fn apply_virtual(&self, world: &World) -> Result<World, PatchApplyError> {
        for change in &self.changes {
            let actual = world
                .get(change.pos)
                .cloned()
                .unwrap_or_else(|| Block::new(BlockKind::Air));
            if actual != change.before {
                return Err(PatchApplyError {
                    position: change.pos,
                    expected: Box::new(change.before.clone()),
                    actual: Box::new(actual),
                });
            }
        }
        let mut result = world.clone();
        for change in &self.changes {
            result.set(change.pos, change.after.clone());
        }
        Ok(result)
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PatchApplyError {
    pub position: Pos,
    pub expected: Box<Block>,
    pub actual: Box<Block>,
}

impl std::fmt::Display for PatchApplyError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(formatter, "stale physical patch at {:?}", self.position)
    }
}

impl std::error::Error for PatchApplyError {}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::BlockKind;

    #[test]
    fn inverse_restores_exact_captured_blocks() {
        let patch = PhysicalPatch {
            reason: RepairReason::ConnectMissingWire,
            affected_fragments: vec![FragmentId(0), FragmentId(1)],
            confidence_percent: 90,
            explanation: "bridge one missing wire".into(),
            changes: vec![PhysicalBlockChange {
                pos: Pos::new(1, 64, 0),
                before: Block::new(BlockKind::Air),
                after: Block::new(BlockKind::RedstoneWire),
            }],
        };
        let inverse = patch.inverse();
        assert_eq!(inverse.changes[0].after.kind, BlockKind::Air);
        assert_eq!(inverse.changes[0].before.kind, BlockKind::RedstoneWire);
    }

    #[test]
    fn virtual_patch_checks_the_captured_before_state() {
        let world = World::new();
        let patch = PhysicalPatch {
            reason: RepairReason::ConnectMissingWire,
            affected_fragments: Vec::new(),
            confidence_percent: 80,
            explanation: "test".into(),
            changes: vec![PhysicalBlockChange {
                pos: Pos::new(1, 2, 3),
                before: Block::new(BlockKind::Air),
                after: Block::new(BlockKind::RedstoneWire),
            }],
        };
        let changed = patch.apply_virtual(&world).unwrap();
        assert_eq!(changed.kind_at(Pos::new(1, 2, 3)), BlockKind::RedstoneWire);
        assert!(patch.apply_virtual(&changed).is_err());
    }
}
