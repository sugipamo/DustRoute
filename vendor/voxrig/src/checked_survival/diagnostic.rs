//! Serializable facts from checked operations. No record is accepted as a live plan,
//! intent, scene, watch or recovery token. There is no reverse conversion.
//!
//! Pure coordinates, inputs and received values are shared. Checked values and
//! their records are declared together, so field additions cannot drift silently.
//!
//! Native inputs remain non-deserializable:
//! ```compile_fail
//! fn restore<'de, T: serde::Deserialize<'de>>() {}
//! restore::<voxrig::checked_survival::MiningIntent>();
//! ```
//! ```compile_fail
//! fn restore<'de, T: serde::Deserialize<'de>>() {}
//! restore::<voxrig::checked_survival::PlacementIntent>();
//! ```
//! ```compile_fail
//! fn restore<'de, T: serde::Deserialize<'de>>() {}
//! restore::<voxrig::checked_survival::StandingContext>();
//! ```
//! ```compile_fail
//! fn restore<'de, T: serde::Deserialize<'de>>() {}
//! restore::<voxrig::checked_survival::HypotheticalMovementPreview>();
//! ```
//! ```compile_fail
//! fn restore<'de, T: serde::Deserialize<'de>>() {}
//! restore::<voxrig::versions::java_1_21_11::players::PlayerMotionWatch>();
//! ```
//! ```compile_fail
//! fn restore<'de, T: serde::Deserialize<'de>>() {}
//! restore::<voxrig::checked_survival::SurvivalMotionRecord>();
//! ```
//! A deserialized record cannot be passed back as a live intent:
//! ```compile_fail
//! use voxrig::checked_survival::{MiningIntent, diagnostic::RecordedMiningIntent};
//! fn promote(record: RecordedMiningIntent) -> MiningIntent { record.into() }
//! ```
//! ```compile_fail
//! use voxrig::checked_survival::{StandingContext, diagnostic::RecordedStandingContext};
//! fn promote(record: RecordedStandingContext) -> StandingContext { record.into() }
//! ```
pub use super::{SurvivalCapabilities, SurvivalContract};
pub use crate::MinecraftVersion;
pub use crate::diagnostic_projection::ToDiagnostic;
pub use crate::versions::java_1_21_11::operations::{
    AttributeValue, GameMode, HotbarSelection, InteractionLoading, InventorySlot, LoadingAttempt,
    LocalPlayerState, MiningInventoryChange, MiningInventoryChangeKind, MiningRecoveryAttempt,
    MiningRecoveryMethod, MiningRecoveryTarget, MiningSend, MiningTargetReceipt,
    MotionInterruption, OwnMotion, PlainItem, PlayerHealth, PositionBasis, PositionSubmission,
    PredictedMotionFrame, ReceivedEffect, ReceivedPose, RecordedHypotheticalAimRequirement,
    RecordedHypotheticalBlockEdit, RecordedHypotheticalMovementPreview,
    RecordedHypotheticalPlacement, RecordedHypotheticalReconnectBoundary, RecordedInventory,
    RecordedInventorySwap, RecordedInventorySwapObservation, RecordedMiningIntent,
    RecordedMiningRecord, RecordedMiningRecoveryBoundary, RecordedMiningRecoveryEvidence,
    RecordedMiningRemoval, RecordedMiningRetirementRecord, RecordedMiningRetirementWatch,
    RecordedMiningStatus, RecordedOperationHistory, RecordedPlacementIntent,
    RecordedPlacementObservation, RecordedPlacementRecord, RecordedPlacementStatus,
    RecordedPlayerState, RecordedStandingContext, RecordedSurvivalMotionRecheck,
    RecordedSurvivalMotionRecord, RecordedSurvivalMovementPreview, ServerTime,
    StandingPositionBasis, SurvivalControl, SurvivalInput, SurvivalMotionContract,
    SurvivalMotionStatus, TerminalClearance,
};
pub use crate::versions::java_1_21_11::players::{
    GroundReceipt, ObservedPlayer, PlayerMotion, PlayerPose, RecordedPlayerMotionWatch,
};

use crate::diagnostic_projection::identity;
pub use crate::versions::java_1_21_11::operations::{ValueBasis, VelocitySample};
identity!(
    ValueBasis,
    VelocitySample,
    AttributeValue,
    GameMode,
    GroundReceipt,
    HotbarSelection,
    InteractionLoading,
    InventorySlot,
    LoadingAttempt,
    LocalPlayerState,
    MinecraftVersion,
    MiningInventoryChange,
    MiningInventoryChangeKind,
    MiningRecoveryAttempt,
    MiningRecoveryMethod,
    MiningRecoveryTarget,
    MiningSend,
    MiningTargetReceipt,
    MotionInterruption,
    ObservedPlayer,
    OwnMotion,
    PlainItem,
    PlayerHealth,
    PlayerMotion,
    PlayerPose,
    PositionBasis,
    PositionSubmission,
    PredictedMotionFrame,
    ReceivedEffect,
    ReceivedPose,
    ServerTime,
    StandingPositionBasis,
    SurvivalCapabilities,
    SurvivalContract,
    SurvivalControl,
    SurvivalInput,
    SurvivalMotionContract,
    SurvivalMotionStatus,
    TerminalClearance
);

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cancelled_mining_facts_roundtrip_without_recategorizing_the_outcome() {
        use crate::checked_survival::{MiningIntent, MiningRecord, MiningStatus};
        let intent = MiningIntent {
            connection_id: 9,
            after_sequence: 12,
            start_sequence: 3,
            dimension: "minecraft:overworld".into(),
            target: [1, 2, 3],
            face_id: 1,
            baseline: crate::NativeBlockState {
                name: "minecraft:dirt".into(),
                properties: Default::default(),
            },
            position: [0.5, 2.0, 1.5],
            selection: HotbarSelection {
                slot: 1,
                sequence: 10,
                dispatched: true,
                from_server: false,
            },
            held_receive_sequence: 11,
            estimated_wait_ms: 50,
        };
        let status = MiningStatus::RequiresInspection {
            record: MiningRecord {
                intent,
                start_dispatched: false,
                finish: None,
                abort: None,
                target_receipt: None,
                requires_inspection: Some("partial dispatch".into()),
                inventory_change: None,
                removal: None,
                recovery_attempt: None,
            },
        };
        let record = status.diagnostic();
        let encoded = serde_json::to_vec(&record).unwrap();
        let reread: RecordedMiningStatus = serde_json::from_slice(&encoded).unwrap();
        let RecordedMiningStatus::RequiresInspection { record: facts } = &reread else {
            panic!("lost uncertainty")
        };
        assert!(!facts.start_dispatched);
        assert!(facts.removal.is_none());
        assert_eq!(facts.intent.connection_id, 9);
        assert_eq!(
            serde_json::to_value(reread).unwrap(),
            serde_json::to_value(status).unwrap()
        );
    }
}
