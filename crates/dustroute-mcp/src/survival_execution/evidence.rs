//! Phase-specific facts. None is a native token, scene, or replay authority.
use super::checkpoint::SafeCheckpoint;
use super::diagnostic::{DiagnosticOnly, RecordedEdit, RecordedPlacement};
use super::{ExecutionError, ExecutionPhase};
use crate::survival_cleanup::{CleanupReconciliation, RecordedCleanupRecoveryPlan};
use serde::{Deserialize, Serialize};
use voxrig::checked_survival::diagnostic::*;

macro_rules! evidence {
    ($($variant:ident => $phase:ident $({$($field:ident: $ty:ty),* $(,)?})? $(($tuple:ty))?),* $(,)?) => {
        #[derive(Clone, Debug, Serialize, Deserialize)]
        #[serde(tag = "kind", content = "data", rename_all = "snake_case", deny_unknown_fields)]
        pub enum ExecutionEvidence {
            $($variant $({$($field: $ty,)*})? $(($tuple))?,)*
        }
        impl ExecutionEvidence {
            pub(super) fn phase(&self) -> ExecutionPhase {
                match self {
                    $(Self::$variant $({$($field: _,)*})? $((evidence!(@ignore $tuple)))? => ExecutionPhase::$phase,)*
                }
            }
        }
    };
    (@ignore $ty:ty) => {_};
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum WorldEvidence {
    BuilderReceived,
}
#[derive(Clone, Copy, Debug, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RetryReason {
    DifferentReceivedEmptyHotbarAfterRetirement,
}
evidence! {
    Cancelled => Cancelled {
        native_operation_may_remain: bool,
        remaining_owned_temporary: Vec<[i32; 3]>,
    },
    CheckpointRefused => CheckpointRefused {
        error: ExecutionError,
    },
    Checkpoint => IdleCheckpoint(Box<SafeCheckpoint>),
    Completed => Completed {
        remaining_owned_temporary: Vec<[i32; 3]>,
        builder_checked_cells: usize,
        world_evidence: WorldEvidence,
        standing: Box<RecordedStandingContext>,
        motion_contract: SurvivalMotionContract,
        server_stop_acknowledged: DiagnosticOnly,
        server_position_error_bound: Option<f64>,
        position: [f64; 3],
    },
    FreshReconnect => FreshReconnect {
        method: MiningRecoveryMethod,
        target_condition: MiningRecoveryTarget,
    },
    InventoryReceived => InventoryReceived(Box<RecordedInventorySwapObservation>),
    InventorySwap => InventorySwap {
        main_slot: usize,
        hotbar: u8,
        inventory_sequence: Option<u64>,
    },
    MiningAim => MiningAim {
        rotation: [f32; 2],
        edit: RecordedEdit,
    },
    MiningFinishSend => MiningFinishSend(Box<RecordedMiningIntent>),
    MiningOutcome => MiningOutcome {
        history: Box<RecordedOperationHistory>,
        recovery_plan: Box<RecordedCleanupRecoveryPlan>,
        attempt: usize,
    },
    MiningSelectEmpty => MiningSelectEmpty {
        hotbar: u8,
        attempt: usize,
        edit: RecordedEdit,
    },
    MiningStartSend => MiningStartSend {
        edit: RecordedEdit,
        face_id: u8,
        attempt: usize,
        hotbar: u8,
    },
    MiningStarted => MiningStarted {
        intent: Box<RecordedMiningIntent>,
        attempt: usize,
        selected_from: Box<RecordedPlayerState>,
    },
    MovementPredicted => MovementPredicted(Box<RecordedSurvivalMotionRecord>),
    MovementPredictionMismatch => MovementPredictionMismatch {
        first_differing_frame_index: Option<usize>,
        checked: Box<RecordedHypotheticalMovementPreview>,
        fresh: Box<RecordedSurvivalMovementPreview>,
    },
    MovementSend => MovementSend {
        controls: Vec<SurvivalControl>,
        planned_initial_position: [f64; 3],
    },
    PlacementAim => PlacementAim {
        rotation: [f32; 2],
        edit: RecordedEdit,
    },
    PlacementObserved => PlacementObserved {
        intent: Box<RecordedPlacementIntent>,
        status: Box<RecordedPlacementStatus>,
        world_evidence: WorldEvidence,
    },
    PlacementSend => PlacementSend(Box<RecordedPlacement>),
    ReconnectBoundaryVerified => ReconnectBoundaryVerified {
        requirement: RecordedHypotheticalReconnectBoundary,
        retired_connection_id: u64,
        received_start: Box<RecordedStandingContext>,
    },
    Recovered => Recovered {
        evidence: Box<RecordedMiningRecoveryEvidence>,
        decision: CleanupReconciliation,
        attempt: usize,
    },
    RetireSource => RetireSource(Box<RecordedMiningIntent>),
    RetryImprovement => RetryImprovement {
        reason: RetryReason,
        previous_hotbar: u8,
        next_hotbar: u8,
        inventory_sequence: Option<u64>,
        attempt: usize,
        owned_target: RecordedEdit,
    },
    SelectMaterial => SelectMaterial {
        hotbar: u8,
        material: String,
    },
    StepCompleted => StepCompleted {
        remaining_owned_temporary: Vec<[i32; 3]>,
    },
    Stopped => Stopped {
        error: ExecutionError,
        remaining_owned_temporary: Vec<[i32; 3]>,
    },
}
impl From<SafeCheckpoint> for ExecutionEvidence {
    fn from(checkpoint: SafeCheckpoint) -> Self {
        Self::Checkpoint(Box::new(checkpoint))
    }
}

#[cfg(test)]
pub(crate) fn mining_start_fixture() -> ExecutionEvidence {
    ExecutionEvidence::MiningStartSend {
        edit: RecordedEdit {
            position: [1, 2, 3],
            before: voxrig::NativeBlockState {
                name: "minecraft:dirt".into(),
                properties: Default::default(),
            },
            after: voxrig::NativeBlockState {
                name: "minecraft:air".into(),
                properties: Default::default(),
            },
        },
        face_id: 1,
        attempt: 1,
        hotbar: 0,
    }
}
#[cfg(test)]
pub(crate) fn placement_send_fixture() -> ExecutionEvidence {
    let ExecutionEvidence::MiningStartSend { edit, .. } = mining_start_fixture() else {
        unreachable!()
    };
    ExecutionEvidence::PlacementSend(Box::new(RecordedPlacement {
        edit: RecordedEdit {
            position: edit.position,
            before: edit.after,
            after: edit.before,
        },
        support: [1, 1, 3],
        face_id: 1,
        rotation: [0.0; 2],
        cursor: [0.5, 1.0, 0.5],
        aim_requirement: RecordedHypotheticalAimRequirement::ReceivedAfterReconnect,
    }))
}
