use serde::{Deserialize, Serialize};

use crate::{Block, FragmentId, GapEvidence, Pos};

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct PhysicalBlockChange {
    pub pos: Pos,
    pub before: Block,
    pub after: Block,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum RepairReason {
    ConnectMissingWire,
    InsertDirectionalComponent,
    RestoreComponentSupport,
    ReorientDirectionalComponent,
    RemoveUnexpectedConnection,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct PhysicalPatch {
    pub reason: RepairReason,
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
    pub fragments_before: usize,
    pub fragments_after: usize,
    pub invalid_supports_before: usize,
    pub invalid_supports_after: usize,
}

impl RepairImpact {
    #[must_use]
    pub const fn improves(self) -> bool {
        self.fragments_after < self.fragments_before
            || self.invalid_supports_after < self.invalid_supports_before
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
}

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
}
