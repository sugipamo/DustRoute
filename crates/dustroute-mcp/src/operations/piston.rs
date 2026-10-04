//! Diagnostic results of the fixed piston/door contracts. No restore, plan or
//! replay authority is recovered by serializing these records.
use super::mutation::Success;
use crate::discovery::RegionBoundsDto;
use crate::failure::{ExecutionProgress, FailureReport};
use crate::piston_door::DoorState;
use dustroute_translate::piston_observation::PistonObservation;
use serde::{Serialize, Serializer};
use uuid::Uuid;

#[derive(Clone, Debug, PartialEq)]
pub(crate) enum ExecutionOutcome {
    Verified(ExecutionProgress),
    Failed(Box<FailureReport>),
}
impl ExecutionOutcome {
    pub fn progress(&self) -> &ExecutionProgress {
        match self {
            Self::Verified(progress) => progress,
            Self::Failed(report) => &report.progress,
        }
    }
    pub fn failed(&self) -> bool {
        matches!(self, Self::Failed(_))
    }
    pub fn from_attempt(progress: ExecutionProgress, failure: Option<FailureReport>) -> Self {
        match failure {
            Some(mut report) => {
                *report.progress = progress;
                Self::Failed(Box::new(report))
            }
            None => Self::Verified(progress),
        }
    }
}
impl Serialize for ExecutionOutcome {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        #[derive(Serialize)]
        struct Verified<'a> {
            ok: Success,
            status: &'static str,
            retry_allowed: bool,
            execution_progress: &'a ExecutionProgress,
        }
        match self {
            Self::Verified(progress) => Verified {
                ok: Success,
                status: "verified",
                retry_allowed: false,
                execution_progress: progress,
            }
            .serialize(serializer),
            Self::Failed(report) => report.as_response().serialize(serializer),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct PistonPlacementResult {
    operation_id: Uuid,
    #[serde(flatten)]
    body: PistonBody,
}
#[derive(Clone, Debug, PartialEq)]
enum PistonBody {
    Executed(Box<PistonExecution>),
    Failed(Box<FailureReport>),
}
#[derive(Clone, Debug, PartialEq, Serialize)]
pub(crate) struct PistonExecution {
    pub verified: bool,
    pub undo: bool,
    pub write_error: Option<String>,
    pub wait_error: Option<String>,
    pub verification_error: Option<String>,
    pub automatic_rollback: bool,
    pub bounds: RegionBoundsDto,
    #[serde(flatten)]
    pub outcome: ExecutionOutcome,
}
impl Serialize for PistonBody {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        match self {
            Self::Executed(result) => result.serialize(serializer),
            Self::Failed(report) => report.as_response().serialize(serializer),
        }
    }
}
impl PistonPlacementResult {
    pub(crate) fn executed(operation_id: Uuid, result: PistonExecution) -> Self {
        Self {
            operation_id,
            body: PistonBody::Executed(Box::new(result)),
        }
    }
    pub(crate) fn failed_attempt(operation_id: Uuid, report: FailureReport) -> Self {
        Self {
            operation_id,
            body: PistonBody::Failed(Box::new(report)),
        }
    }
    pub fn failed(&self) -> bool {
        match &self.body {
            PistonBody::Executed(result) => result.outcome.failed(),
            PistonBody::Failed(_) => true,
        }
    }
    pub fn progress(&self) -> &ExecutionProgress {
        match &self.body {
            PistonBody::Executed(result) => result.outcome.progress(),
            PistonBody::Failed(report) => &report.progress,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct DoorOperationResult {
    operation_id: Uuid,
    #[serde(flatten)]
    body: DoorBody,
}
#[derive(Clone, Debug, PartialEq)]
enum DoorBody {
    Executed(Box<DoorExecution>),
    Unchanged(Box<UnchangedDoor>),
    Refused(Box<DoorRefusal>),
    Failed(Box<FailureReport>),
}
#[derive(Clone, Debug, PartialEq, Serialize)]
pub(crate) struct DoorExecution {
    pub target: DoorState,
    pub observed_state: Option<DoorState>,
    pub observation: PistonObservation,
    pub verified: bool,
    pub verification_error: Option<String>,
    pub activation_error: Option<String>,
    pub wait_error: Option<String>,
    pub scan_error: Option<String>,
    pub automatic_rollback: bool,
    #[serde(flatten)]
    pub outcome: ExecutionOutcome,
}
#[derive(Clone, Debug, PartialEq, Serialize)]
struct UnchangedDoor {
    ok: Success,
    execution_progress: ExecutionProgress,
    state: DoorState,
    observed_state: DoorState,
    changed: bool,
    verified: bool,
    observation: PistonObservation,
}
#[derive(Clone, Debug, PartialEq, Serialize)]
struct DoorRefusal {
    ok: bool,
    error: String,
    observation: PistonObservation,
}
impl Serialize for DoorBody {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        match self {
            Self::Executed(result) => result.serialize(serializer),
            Self::Unchanged(result) => result.serialize(serializer),
            Self::Refused(result) => result.serialize(serializer),
            Self::Failed(report) => report.as_response().serialize(serializer),
        }
    }
}
impl DoorOperationResult {
    pub(crate) fn executed(operation_id: Uuid, result: DoorExecution) -> Self {
        Self {
            operation_id,
            body: DoorBody::Executed(Box::new(result)),
        }
    }
    pub(crate) fn unchanged(
        operation_id: Uuid,
        state: DoorState,
        observation: PistonObservation,
        progress: ExecutionProgress,
    ) -> Self {
        Self {
            operation_id,
            body: DoorBody::Unchanged(Box::new(UnchangedDoor {
                ok: Success,
                execution_progress: progress,
                state,
                observed_state: state,
                changed: false,
                verified: true,
                observation,
            })),
        }
    }
    pub(crate) fn refused(
        operation_id: Uuid,
        error: impl Into<String>,
        observation: PistonObservation,
    ) -> Self {
        Self {
            operation_id,
            body: DoorBody::Refused(Box::new(DoorRefusal {
                ok: false,
                error: error.into(),
                observation,
            })),
        }
    }
    pub(crate) fn failed_attempt(operation_id: Uuid, report: FailureReport) -> Self {
        Self {
            operation_id,
            body: DoorBody::Failed(Box::new(report)),
        }
    }
    pub fn failed(&self) -> bool {
        match &self.body {
            DoorBody::Executed(result) => result.outcome.failed(),
            DoorBody::Unchanged(_) => false,
            DoorBody::Failed(_) | DoorBody::Refused(_) => true,
        }
    }
    pub fn progress(&self) -> Option<&ExecutionProgress> {
        match &self.body {
            DoorBody::Executed(result) => Some(result.outcome.progress()),
            DoorBody::Unchanged(result) => Some(&result.execution_progress),
            DoorBody::Failed(report) => Some(&report.progress),
            DoorBody::Refused(_) => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::failure::{CauseKind, FailureCause, FailurePhase, WorldOutcome};
    use crate::operations::{OperationKind, OperationRegistry, OperationStatus};
    use dustroute_physical::Pos;
    use dustroute_translate::piston_observation::PistonObservationState;
    use serde_json::json;

    fn observation(state: PistonObservationState) -> PistonObservation {
        PistonObservation {
            schema_version: "dustroute.piston-observation.v1".into(),
            state,
            issues: vec![],
            next_action: "inspect".into(),
        }
    }
    #[test]
    fn unchanged_door_is_known_zero_and_refusal_has_no_execution_facts() {
        let id = Uuid::from_u128(1);
        let observed = observation(PistonObservationState::Open);
        let unchanged = DoorOperationResult::unchanged(
            id,
            DoorState::Open,
            observed.clone(),
            ExecutionProgress {
                operation_consumed: true,
                total_changes: Some(0),
                world: WorldOutcome::Verified,
                ..Default::default()
            },
        );
        assert!(!unchanged.failed());
        assert_eq!(unchanged.progress().unwrap().total_changes, Some(0));
        let wire = serde_json::to_value(&unchanged).unwrap();
        assert_eq!(wire["changed"], false);
        assert_eq!(wire["state"], "open");
        assert_eq!(wire["observed_state"], "open");
        for missing in ["error", "status", "target", "retry_allowed"] {
            assert!(wire.get(missing).is_none());
        }
        let refused = DoorOperationResult::refused(id, "layout changed", observed);
        assert!(refused.failed());
        assert!(refused.progress().is_none());
        let wire = serde_json::to_value(refused).unwrap();
        assert_eq!(wire["error"], "layout changed");
        assert!(wire.get("execution_progress").is_none());
        assert!(wire.get("failure").is_none());
    }
    #[tokio::test]
    async fn observed_target_does_not_erase_activation_reply_loss_or_allow_replay() {
        let id = Uuid::from_u128(2);
        let progress = ExecutionProgress {
            operation_consumed: true,
            submitted_changes: None,
            world: WorldOutcome::Verified,
            verified_steps: 1,
            total_changes: Some(1),
            phase: FailurePhase::Verification,
            ..Default::default()
        };
        let failure = progress.cause(
            FailureCause::new(CauseKind::Connection, "activation reply lost")
                .at(FailurePhase::Submission),
        );
        let result = DoorOperationResult::executed(
            id,
            DoorExecution {
                target: DoorState::Closed,
                observed_state: Some(DoorState::Closed),
                observation: observation(PistonObservationState::Closed),
                verified: true,
                verification_error: None,
                activation_error: Some("activation reply lost".into()),
                wait_error: None,
                scan_error: None,
                automatic_rollback: false,
                outcome: ExecutionOutcome::from_attempt(progress, Some(failure)),
            },
        );
        assert!(result.failed());
        let registry = OperationRegistry::default();
        registry
            .record_completed(id, OperationKind::PistonDoorRun, result.clone().into())
            .await;
        let record = registry.get(id).await.unwrap();
        assert_eq!(record.status, OperationStatus::Failed);
        assert_eq!(record.progress_percent, 100);
        let wire = serde_json::to_value(&result).unwrap();
        assert_eq!(wire["ok"], false);
        assert_eq!(wire["verified"], true);
        assert_eq!(wire["observed_state"], "closed");
        assert_eq!(
            wire["failure"]["progress"]["submitted_changes"],
            json!(null)
        );
        assert_eq!(wire["failure"]["primary"]["phase"], "submission");
        assert_eq!(wire["recovery"]["same_operation_replay_allowed"], false);
        registry
            .record_completed(
                id,
                OperationKind::PistonDoorRun,
                DoorOperationResult::failed_attempt(
                    id,
                    ExecutionProgress::default()
                        .cause(FailureCause::new(CauseKind::InvalidState, "consumed")),
                )
                .into(),
            )
            .await;
        assert_eq!(registry.get(id).await.unwrap().result, Some(result.into()));
    }
    #[test]
    fn successful_piston_response_preserves_optional_diagnostics_without_inventing_an_error() {
        let result = PistonPlacementResult::executed(
            Uuid::from_u128(3),
            PistonExecution {
                verified: true,
                undo: true,
                write_error: None,
                wait_error: None,
                verification_error: None,
                automatic_rollback: false,
                bounds: RegionBoundsDto {
                    min: Pos::new(0, 0, 0),
                    max: Pos::new(1, 1, 1),
                },
                outcome: ExecutionOutcome::Verified(ExecutionProgress {
                    world: WorldOutcome::Verified,
                    total_changes: Some(2),
                    verified_steps: 2,
                    ..Default::default()
                }),
            },
        );
        assert!(!result.failed());
        let wire = serde_json::to_value(result).unwrap();
        assert_eq!(wire["ok"], true);
        assert_eq!(wire["undo"], true);
        assert_eq!(wire["verification_error"], json!(null));
        assert_eq!(wire["write_error"], json!(null));
        assert!(wire.get("error").is_none());
        assert!(wire.get("failure").is_none());
    }
}
