//! Advice from existing diagnostic facts, never an execution or recovery gate.
use super::model::{InspectionReason, JobStatus};
use super::replies::{RefusalCode, ServiceCode};
use crate::survival_error::SurvivalErrorCode;
use serde::Serialize;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub(super) enum NextAction {
    ReviewPreview,
    PollJob,
    PlanFresh,
    InspectJob,
    InspectExecution,
    InspectPersistence,
    ContinueCheckpoint,
    ObserveBeforeNewPlan,
    InspectLinkedJob,
    InspectRefusal,
    ReviewSiteDifferences,
    ReviewSearchFailure,
}

impl NextAction {
    pub(super) fn for_status(status: Option<&JobStatus>, historical: bool) -> Self {
        match status {
            Some(JobStatus::Planned) if !historical => Self::ReviewPreview,
            Some(JobStatus::Planned) => Self::PlanFresh,
            Some(JobStatus::Admitting | JobStatus::Running { .. }) if !historical => Self::PollJob,
            Some(JobStatus::Admitting | JobStatus::Running { .. }) | None => Self::InspectExecution,
            Some(
                JobStatus::CancelledBeforeStart {
                    world_writes: false,
                }
                | JobStatus::PlanExpiredOrCancelled {
                    construction_dispatched: false,
                }
                | JobStatus::AdmissionRefused {
                    construction_dispatched: false,
                    ..
                },
            ) => Self::PlanFresh,
            Some(
                JobStatus::CancelledBeforeStart { world_writes: true }
                | JobStatus::PlanExpiredOrCancelled {
                    construction_dispatched: true,
                }
                | JobStatus::AdmissionRefused {
                    construction_dispatched: true,
                    ..
                },
            ) => Self::InspectExecution,
            Some(JobStatus::Checkpointed {
                safe_idle: true, ..
            }) => Self::ContinueCheckpoint,
            Some(JobStatus::Completed { .. }) => Self::ObserveBeforeNewPlan,
            Some(JobStatus::NeedsInspection {
                reason: InspectionReason::Persistence { .. },
            }) => Self::InspectPersistence,
            Some(
                JobStatus::Checkpointed {
                    safe_idle: false, ..
                }
                | JobStatus::CancelledNeedsInspection { .. }
                | JobStatus::NeedsInspection { .. },
            ) => Self::InspectExecution,
        }
    }

    pub(super) fn for_refusal(code: &RefusalCode) -> Self {
        match code {
            RefusalCode::Service(
                ServiceCode::JournalIo
                | ServiceCode::JournalUnavailable
                | ServiceCode::InvalidRecord
                | ServiceCode::InvalidContinuationClaim,
            ) => Self::InspectPersistence,
            RefusalCode::Survival(
                SurvivalErrorCode::JournalIo
                | SurvivalErrorCode::JournalEncoding
                | SurvivalErrorCode::JournalSchema
                | SurvivalErrorCode::JournalTooLarge,
            ) => Self::InspectPersistence,
            RefusalCode::Service(
                ServiceCode::JobAlreadyStarted
                | ServiceCode::SourceBusy
                | ServiceCode::JobNotLive
                | ServiceCode::CheckpointNotRunning,
            ) => Self::InspectJob,
            RefusalCode::Service(
                ServiceCode::PlanNotLive | ServiceCode::PlanExpiredOrCancelled,
            ) => Self::PlanFresh,
            RefusalCode::Survival(
                SurvivalErrorCode::SafeCheckpointMissing
                | SurvivalErrorCode::CheckpointConsumed
                | SurvivalErrorCode::ExecutionNeedsInspection
                | SurvivalErrorCode::MiningNeedsInspection
                | SurvivalErrorCode::MovementNeedsInspection,
            ) => Self::InspectExecution,
            _ => Self::InspectRefusal,
        }
    }

    pub(super) fn description(self) -> &'static str {
        match self {
            Self::ReviewPreview => {
                "review this live preview, then action=start with confirmed=true; admission rechecks current conditions"
            }
            Self::PollJob => {
                "action=get until execution stops; acceptance or progress is not completion"
            }
            Self::PlanFresh => {
                "resolve the refusal or stop condition, then generate and review a fresh plan; do not replay this job"
            }
            Self::InspectJob => {
                "action=get for the affected job and inspect its state; source ownership is not released by retrying"
            }
            Self::InspectExecution => {
                "inspect saved execution and outstanding native operations, then reobserve when permitted; no automatic replay or continuation without a sealed checkpoint"
            }
            Self::InspectPersistence => {
                "inspect the storage error and available live/saved evidence; a failed save does not prove the previous record intact or permit replay"
            }
            Self::ContinueCheckpoint => {
                "action=continue revalidates the sealed checkpoint and creates a NEW preview; review and explicitly start its new job_id"
            }
            Self::ObserveBeforeNewPlan => {
                "completion is recorded under its declared contract; freshly observe before planning any new work"
            }
            Self::InspectLinkedJob => "get the linked new job; do not replay the old checkpoint",
            Self::InspectRefusal => {
                "inspect error.code and its details; resolve the prerequisite before requesting a new plan or action"
            }
            Self::ReviewSiteDifferences => {
                "inspect diagnosis.conflicts against the declared site; do not remove foreign blocks automatically; request a new preview only after resolving differences"
            }
            Self::ReviewSearchFailure => {
                "inspect error.cause for input, material or search-budget limits; no complete plan was accepted and search exhaustion does not prove impossibility"
            }
        }
    }
}
