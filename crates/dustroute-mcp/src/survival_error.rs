//! Typed survival refusals shared by planning, cleanup and execution.
//! Wire spellings are declared here; callers branch on enum variants.
use serde::{Deserialize, Serialize};

macro_rules! refusal_codes {
    ($($variant:ident => $wire:literal),+ $(,)?) => {
        #[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
        pub enum SurvivalErrorCode {
            $(#[serde(rename = $wire)] $variant),+
        }
        impl std::fmt::Display for SurvivalErrorCode {
            fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                match self { $(Self::$variant => f.write_str($wire)),+ }
            }
        }
    };
}

refusal_codes! {
    BodyScopeChanged => "body_scope_changed",
    CheckpointChanged => "checkpoint_changed",
    CheckpointConsumed => "checkpoint_consumed",
    CheckpointNativeUnsettled => "checkpoint_native_unsettled",
    CheckpointNotIdle => "checkpoint_not_idle",
    CheckpointSiteChanged => "checkpoint_site_changed",
    CheckpointWriterLive => "checkpoint_writer_live",
    EmptyHandUnavailable => "empty_hand_unavailable",
    ExecutionBusy => "execution_busy",
    ExecutionExists => "execution_exists",
    ExecutionNeedsInspection => "execution_needs_inspection",
    FinalGeometryMismatch => "final_geometry_mismatch",
    FinalObligationMissing => "final_obligation_missing",
    FreshTargetChanged => "fresh_target_changed",
    FreshTargetUnavailable => "fresh_target_unavailable",
    IncompleteSequence => "incomplete_sequence",
    InsufficientSuppliedMaterials => "insufficient_supplied_materials",
    InvalidCheckpoint => "invalid_checkpoint",
    InvalidCheckpointSite => "invalid_checkpoint_site",
    InvalidFace => "invalid_face",
    InvalidSearchLimits => "invalid_search_limits",
    InvalidSequence => "invalid_sequence",
    InvalidSiteContract => "invalid_site_contract",
    InvalidTemporaryMaterial => "invalid_temporary_material",
    InventoryUnavailable => "inventory_unavailable",
    JournalEncoding => "journal_encoding",
    JournalIo => "journal_io",
    JournalSchema => "journal_schema",
    JournalTooLarge => "journal_too_large",
    MaterialUnavailable => "material_unavailable",
    MiningHistoryMissing => "mining_history_missing",
    MiningIntentChanged => "mining_intent_changed",
    MiningNeedsInspection => "mining_needs_inspection",
    MissingMiningHistory => "missing_mining_history",
    MovementContractChanged => "movement_contract_changed",
    MovementMissing => "movement_missing",
    MovementNeedsInspection => "movement_needs_inspection",
    MovementPlanChanged => "movement_plan_changed",
    MovementTimeout => "movement_timeout",
    NativeGeometryRefused => "native_geometry_refused",
    NativeRefused => "native_refused",
    PlacementIntentChanged => "placement_intent_changed",
    PlacementOutsidePlan => "placement_outside_plan",
    PlacementPlanChanged => "placement_plan_changed",
    PlacementUnconfirmed => "placement_unconfirmed",
    PlanSourceChanged => "plan_source_changed",
    RecoveryPlanChanged => "recovery_plan_changed",
    RecoveryProvenanceMismatch => "recovery_provenance_mismatch",
    RecoverySiteChanged => "recovery_site_changed",
    RemovalIntentMismatch => "removal_intent_mismatch",
    RemovalOutsideTemporaryWorks => "removal_outside_temporary_works",
    RemovalPlanChanged => "removal_plan_changed",
    RemovalReceiptMismatch => "removal_receipt_mismatch",
    RetiredTargetChanged => "retired_target_changed",
    RetreatNotReached => "retreat_not_reached",
    RetryLimit => "retry_limit",
    RetryPrerequisiteChanged => "retry_prerequisite_changed",
    RetryWithoutImprovement => "retry_without_improvement",
    SafeCheckpointMissing => "safe_checkpoint_missing",
    SceneScopeMismatch => "scene_scope_mismatch",
    SiteBaselineMismatch => "site_baseline_mismatch",
    SiteChanged => "site_changed",
    SnapshotInvalid => "snapshot_invalid",
    SnapshotMismatch => "snapshot_mismatch",
    SuppliedMaterialsMissing => "supplied_materials_missing",
    TemporaryNotOwned => "temporary_not_owned",
    TravelScopeMismatch => "travel_scope_mismatch",
    UnsafeFinalStanding => "unsafe_final_standing",
    UnsafePlannedMotion => "unsafe_planned_motion",
    UnsupportedMotionContract => "unsupported_motion_contract",
}
