//! Caller-side reconciliation of owned temporary removals, without replay authority.
//! Native observation/retirement belongs to Voxrig. Permissions, bounded retries,
//! temporary-block ownership and fresh action selection remain with the caller.
use serde::Serialize;
use voxrig::NativeBlockState;
use voxrig::checked_survival::{
    CapturedSurvivalScene, HypotheticalBlockEdit, InventorySlot, MiningIntent,
    MiningInventoryChangeKind, MiningRecord, MiningRecoveryEvidence, PlayerState,
};

#[derive(Clone, Debug, Serialize)]
pub struct CleanupError {
    pub code: &'static str,
    pub detail: String,
}
fn refusal(code: &'static str, detail: &str) -> CleanupError {
    CleanupError {
        code,
        detail: detail.into(),
    }
}

/// A received empty slot is a candidate, never a reservation or future guarantee.
/// Automatic pickup is not credited toward any construction material budget.
pub fn choose_empty_hand(player: &PlayerState) -> Result<u8, CleanupError> {
    let inv = &player.inventory;
    if inv.window_id != Some(0)
        || inv.cursor != InventorySlot::Empty
        || inv.unsupported_components
        || inv.pending_swap.is_some()
        || !inv.pending_creative.is_empty()
        || inv.slots.len() != 46
    {
        return Err(refusal(
            "inventory_unavailable",
            "a supported received player inventory is required",
        ));
    }
    inv.slots[36..45]
        .iter()
        .position(|s| *s == InventorySlot::Empty)
        .map(|n| n as u8)
        .ok_or_else(|| {
            refusal(
                "empty_hand_unavailable",
                "no received empty hotbar slot; caller must arrange inventory",
            )
        })
}

/// No Deserialize: diagnostic JSON is not an executable cleanup/recovery token.
#[derive(Clone, Debug, Serialize)]
pub struct CleanupRecoveryPlan {
    edit: HypotheticalBlockEdit,
    intent: MiningIntent,
    removal_observed: bool,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum CleanupReconciliation {
    /// Fresh target is exactly the declared final air. No actor attribution.
    AlreadyAbsent,
    /// Fresh target is the exact predecessor. Requires a new geometry/permission
    /// check and empty-hand selection; never reuses the old intent.
    NeedsNewPlan,
}

impl CleanupRecoveryPlan {
    /// Caller supplies an owned, permission-checked temporary removal. This only
    /// checks correspondence and known native outcomes; it grants no permission.
    pub fn for_record(
        edit: &HypotheticalBlockEdit,
        record: &MiningRecord,
    ) -> Result<Self, CleanupError> {
        if edit.position != record.intent.target
            || edit.before != record.intent.baseline
            || edit.after.name != "minecraft:air"
            || !edit.after.properties.is_empty()
            || !record.start_dispatched
        {
            return Err(refusal(
                "removal_intent_mismatch",
                "record does not match the declared temporary removal",
            ));
        }
        let removed = record.removal.is_some();
        if removed {
            let removal = record.removal.as_ref().unwrap();
            if removal.intent != record.intent
                || removal.target_receipt.state != edit.after
                || removal.target_receipt.receive_sequence <= record.intent.after_sequence
            {
                return Err(refusal(
                    "removal_receipt_mismatch",
                    "removal lacks the exact fresh target receipt",
                ));
            }
        } else if !record.inventory_change.as_ref().is_some_and(|change| {
            change.sole_cause
                && change.kind == MiningInventoryChangeKind::SelectedHandChanged
                && change.receive_sequence > record.intent.after_sequence
                && matches!(change.original_hand, InventorySlot::Item { .. })
                && record.requires_inspection.is_some()
        }) {
            return Err(refusal(
                "mining_needs_inspection",
                "only a received held-item change with no other conflict can enter this recovery policy",
            ));
        }
        Ok(Self {
            edit: edit.clone(),
            intent: record.intent.clone(),
            removal_observed: removed,
        })
    }

    /// Classify a target read from the independent observer AFTER exact retirement.
    /// The subsequent native reconnect still checks its own fresh target receipt.
    pub fn target_for_reconnect(
        &self,
        observed: Option<&NativeBlockState>,
    ) -> Result<NativeBlockState, CleanupError> {
        match observed {
            Some(state) if *state == self.edit.after => Ok(state.clone()),
            Some(state) if *state == self.edit.before && !self.removal_observed => {
                Ok(state.clone())
            }
            _ => Err(refusal(
                "retired_target_changed",
                "target is missing, foreign, or reappeared after confirmed removal",
            )),
        }
    }

    /// Reconcile an already validated fresh native session with a new captured
    /// scene. Retained history cannot confer authority on a different connection.
    pub fn reconcile_fresh(
        &self,
        evidence: &MiningRecoveryEvidence,
        scene: &CapturedSurvivalScene,
    ) -> Result<CleanupReconciliation, CleanupError> {
        let old = &evidence.old_history;
        if !old.connection_closed
            || old.connection_id != self.intent.connection_id
            || old.receive_failure.is_some()
            || old.interrupted_packet_id.is_some()
            || evidence.connection_id == old.connection_id
            || !evidence.interaction_ready
            || scene.source().connection_id != evidence.connection_id
            || scene.source().dimension != self.intent.dimension
        {
            return Err(refusal(
                "recovery_provenance_mismatch",
                "fresh scene and closed original history must match this recovery",
            ));
        }
        let record = old.mining.as_ref().ok_or_else(|| {
            refusal(
                "missing_mining_history",
                "original mining history is unavailable",
            )
        })?;
        if record.intent != self.intent {
            return Err(refusal(
                "removal_intent_mismatch",
                "recovery belongs to another removal attempt",
            ));
        }
        // A later non-inventory conflict cannot be hidden by the earlier assessment.
        Self::for_record(&self.edit, record)?;
        let actual = scene
            .scenario()
            .block(self.edit.position)
            .map_err(|e| CleanupError {
                code: "fresh_target_unavailable",
                detail: e.to_string(),
            })?;
        if actual != evidence.target {
            return Err(refusal(
                "fresh_target_changed",
                "target changed after fresh-session validation",
            ));
        }
        self.target_for_reconnect(Some(&actual))?;
        Ok(if actual == self.edit.after {
            CleanupReconciliation::AlreadyAbsent
        } else {
            CleanupReconciliation::NeedsNewPlan
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use voxrig::versions::java_1_21_11::operations::{
        HotbarSelection, MiningInventoryChange, MiningRemoval, MiningTargetReceipt, default_item,
    };
    fn block(name: &str) -> NativeBlockState {
        NativeBlockState {
            name: format!("minecraft:{name}"),
            properties: Default::default(),
        }
    }
    fn fixture() -> (HypotheticalBlockEdit, MiningRecord) {
        let edit = HypotheticalBlockEdit {
            position: [1, 1, 0],
            before: block("dirt"),
            after: block("air"),
        };
        let selection = HotbarSelection {
            slot: 0,
            sequence: 9,
            dispatched: true,
            from_server: false,
        };
        let intent = MiningIntent {
            connection_id: 7,
            after_sequence: 10,
            start_sequence: 1,
            dimension: "minecraft:overworld".into(),
            target: edit.position,
            face_id: 4,
            baseline: edit.before.clone(),
            position: [0.5, 1.0, 0.5],
            selection: selection.clone(),
            held_receive_sequence: 8,
            estimated_wait_ms: 1100,
        };
        let record = MiningRecord {
            intent,
            start_dispatched: true,
            finish: None,
            abort: None,
            target_receipt: None,
            requires_inspection: Some("received hand changed".into()),
            inventory_change: Some(MiningInventoryChange {
                kind: MiningInventoryChangeKind::SelectedHandChanged,
                receive_sequence: 11,
                selection: Some(selection),
                original_hand: InventorySlot::Item {
                    item: default_item("dirt", 1).unwrap(),
                },
                hand_receive_sequence: Some(11),
                window_id: Some(0),
                cursor: InventorySlot::Empty,
                unsupported_components: false,
                sole_cause: true,
            }),
            removal: None,
        };
        (edit, record)
    }
    #[test]
    fn only_corresponding_owned_attempt_with_inventory_only_diagnosis_is_admitted() {
        let (edit, record) = fixture();
        assert!(CleanupRecoveryPlan::for_record(&edit, &record).is_ok());
        let mut wrong = edit.clone();
        wrong.position[0] += 1;
        assert_eq!(
            CleanupRecoveryPlan::for_record(&wrong, &record)
                .unwrap_err()
                .code,
            "removal_intent_mismatch"
        );
        let mut conflicted = record.clone();
        conflicted.inventory_change.as_mut().unwrap().sole_cause = false;
        assert!(CleanupRecoveryPlan::for_record(&edit, &conflicted).is_err());
        let mut pending = record.clone();
        pending.inventory_change = None;
        pending.requires_inspection = None;
        assert!(CleanupRecoveryPlan::for_record(&edit, &pending).is_err());
        let mut stale = record;
        stale.inventory_change.as_mut().unwrap().receive_sequence = 10;
        assert!(CleanupRecoveryPlan::for_record(&edit, &stale).is_err());
    }
    #[test]
    fn retired_target_requires_exact_baseline_or_air_and_never_credits_materials() {
        let (edit, record) = fixture();
        let plan = CleanupRecoveryPlan::for_record(&edit, &record).unwrap();
        assert_eq!(
            plan.target_for_reconnect(Some(&block("dirt"))).unwrap(),
            edit.before
        );
        assert_eq!(
            plan.target_for_reconnect(Some(&block("air"))).unwrap(),
            edit.after
        );
        assert!(plan.target_for_reconnect(Some(&block("stone"))).is_err());
        assert!(plan.target_for_reconnect(None).is_err());
    }
    #[test]
    fn confirmed_removed_block_must_not_be_reclaimed_if_it_reappears() {
        let (edit, mut record) = fixture();
        record.inventory_change = None;
        record.requires_inspection = None;
        record.removal = Some(MiningRemoval {
            intent: record.intent.clone(),
            target_receipt: MiningTargetReceipt {
                state: edit.after.clone(),
                receive_sequence: 12,
            },
            continuation_validated: false,
        });
        let plan = CleanupRecoveryPlan::for_record(&edit, &record).unwrap();
        assert!(plan.target_for_reconnect(Some(&edit.after)).is_ok());
        assert!(plan.target_for_reconnect(Some(&edit.before)).is_err());
        record
            .removal
            .as_mut()
            .unwrap()
            .target_receipt
            .receive_sequence = 9;
        assert!(CleanupRecoveryPlan::for_record(&edit, &record).is_err());
    }
}
