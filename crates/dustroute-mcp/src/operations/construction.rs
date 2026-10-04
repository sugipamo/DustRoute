//! Construction reports are diagnostic values, not executable plans or saved
//! capabilities. Detailed execution and failures without detail stay distinct.
use super::mutation::Success;
use crate::construction_jobs::JobStageBinding;
use crate::discovery::RegionBoundsDto;
use crate::failure::{ExecutionProgress, FailureReport};
use serde::{Serialize, Serializer};
use uuid::Uuid;

#[derive(Clone, Debug, PartialEq)]
enum ConstructionResult<D> {
    Detailed(Box<ConstructionAttempt<D>>),
    Failed(Box<FailureReport>),
}
#[derive(Clone, Debug, PartialEq)]
struct ConstructionAttempt<D> {
    details: D,
    outcome: Result<ExecutionProgress, FailureReport>,
}
impl<D> ConstructionResult<D> {
    fn failed(&self) -> bool {
        match self {
            Self::Detailed(attempt) => attempt.outcome.is_err(),
            Self::Failed(_) => true,
        }
    }
    fn progress(&self) -> &ExecutionProgress {
        match self {
            Self::Detailed(attempt) => match &attempt.outcome {
                Ok(progress) => progress,
                Err(report) => &report.progress,
            },
            Self::Failed(report) => &report.progress,
        }
    }
}
impl<D: Serialize> Serialize for ConstructionResult<D> {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        #[derive(Serialize)]
        struct Verified<'a> {
            ok: Success,
            error: Option<&'a str>,
            status: &'static str,
            retry_allowed: bool,
            execution_progress: &'a ExecutionProgress,
        }
        #[derive(Serialize)]
        struct View<'a, D, O> {
            #[serde(flatten)]
            details: &'a D,
            #[serde(flatten)]
            outcome: O,
        }
        match self {
            Self::Failed(report) => report.as_response().serialize(serializer),
            Self::Detailed(attempt) => match &attempt.outcome {
                Ok(progress) => View {
                    details: &attempt.details,
                    outcome: Verified {
                        ok: Success,
                        error: None,
                        status: "verified",
                        retry_allowed: false,
                        execution_progress: progress,
                    },
                }
                .serialize(serializer),
                Err(report) => View {
                    details: &attempt.details,
                    outcome: report.as_response(),
                }
                .serialize(serializer),
            },
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct ElectricalEditResult(ConstructionResult<ElectricalEditDetails>);
#[derive(Clone, Debug, PartialEq, Serialize)]
struct ElectricalEditDetails {
    operation_id: Uuid,
    kind: &'static str,
    undo: bool,
    job_stage: Option<JobStageBinding>,
    verified_steps: usize,
    total_steps: usize,
    automatic_rollback: bool,
    assembly_adoption_modified: bool,
}
impl ElectricalEditResult {
    pub(crate) fn completed(
        operation_id: Uuid,
        undo: bool,
        job_stage: Option<JobStageBinding>,
        verified_steps: usize,
        total_steps: usize,
        outcome: Result<ExecutionProgress, FailureReport>,
    ) -> Self {
        Self(ConstructionResult::Detailed(Box::new(
            ConstructionAttempt {
                details: ElectricalEditDetails {
                    operation_id,
                    kind: "electrical_revision_modification",
                    undo,
                    job_stage,
                    verified_steps,
                    total_steps,
                    automatic_rollback: false,
                    assembly_adoption_modified: false,
                },
                outcome,
            },
        )))
    }
    pub(crate) fn failed_attempt(report: FailureReport) -> Self {
        Self(ConstructionResult::Failed(Box::new(report)))
    }
    pub fn failed(&self) -> bool {
        self.0.failed()
    }
    pub fn progress(&self) -> &ExecutionProgress {
        self.0.progress()
    }
}
impl Serialize for ElectricalEditResult {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        self.0.serialize(serializer)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
pub enum AssemblyConstructionKind {
    #[serde(rename = "custom_piston_assembly_construction")]
    Construct,
    #[serde(rename = "placed_assembly_removal")]
    Remove,
    #[serde(rename = "placed_assembly_reconstruction")]
    Reconstruct,
}
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct ReconstructionConditions {
    strategy: &'static str,
    server_readiness_proven: bool,
    runtime_history_reconstructed: bool,
    automatic_retry: bool,
    operator_requirement: &'static str,
    limitation: &'static str,
}
impl ReconstructionConditions {
    pub(crate) fn declared() -> Self {
        Self {
            strategy: "teardown_observed_layout_then_rebuild_declared_initial_state",
            server_readiness_proven: false,
            runtime_history_reconstructed: false,
            automatic_retry: false,
            operator_requirement: "review all affected blocks; let previous commands finish and keep other inputs/edits out of the region during reconstruction",
            limitation: "matching client samples cannot prove empty server queues, ownership of identical material, or atomic check-and-write",
        }
    }
}
#[derive(Clone, Debug, PartialEq, Serialize)]
pub(crate) struct AssemblyConstructionDetails {
    pub instance_id: Uuid,
    pub record_revision: u64,
    pub undo: bool,
    pub kind: AssemblyConstructionKind,
    pub verified_steps: usize,
    pub total_steps: usize,
    pub reconstruction_conditions: Option<ReconstructionConditions>,
    pub automatic_rollback: bool,
    pub bounds: RegionBoundsDto,
}
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct AssemblyConstructionResult {
    operation_id: Uuid,
    #[serde(flatten)]
    result: ConstructionResult<AssemblyConstructionDetails>,
}
impl AssemblyConstructionResult {
    pub(crate) fn completed(
        operation_id: Uuid,
        details: AssemblyConstructionDetails,
        outcome: Result<ExecutionProgress, FailureReport>,
    ) -> Self {
        Self {
            operation_id,
            result: ConstructionResult::Detailed(Box::new(ConstructionAttempt {
                details,
                outcome,
            })),
        }
    }
    pub(crate) fn failed_attempt(operation_id: Uuid, report: FailureReport) -> Self {
        Self {
            operation_id,
            result: ConstructionResult::Failed(Box::new(report)),
        }
    }
    pub fn failed(&self) -> bool {
        self.result.failed()
    }
    pub fn progress(&self) -> &ExecutionProgress {
        self.result.progress()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::failure::{CauseKind, FailureCause, FailurePhase, PersistenceOutcome, WorldOutcome};
    use crate::operations::{OperationKind, OperationRegistry, OperationStatus};
    use dustroute_physical::Pos;
    use serde_json::json;

    #[test]
    fn unknown_execution_details_are_not_an_empty_completed_stage() {
        let progress = ExecutionProgress::default();
        let failed = ElectricalEditResult::failed_attempt(progress.cause(FailureCause::new(
            CauseKind::InvalidState,
            "preview missing",
        )));
        let wire = serde_json::to_value(&failed).unwrap();
        assert!(failed.failed());
        assert!(failed.progress().total_changes.is_none());
        assert_eq!(wire["failure"]["progress"]["total_changes"], json!(null));
        assert_eq!(wire["status"], "refused");
        for missing in [
            "operation_id",
            "job_stage",
            "total_steps",
            "execution_progress",
        ] {
            assert!(
                wire.get(missing).is_none(),
                "unknown field must be absent: {missing}"
            );
        }
        let id = Uuid::from_u128(1);
        let completed = ElectricalEditResult::completed(
            id,
            false,
            None,
            0,
            0,
            Ok(ExecutionProgress {
                total_changes: Some(0),
                world: WorldOutcome::Verified,
                persistence: PersistenceOutcome::FinalSaved,
                ..Default::default()
            }),
        );
        let wire = serde_json::to_value(&completed).unwrap();
        assert!(!completed.failed());
        assert_eq!(wire["execution_progress"]["total_changes"], 0);
        assert_eq!(wire["job_stage"], json!(null));
        assert_eq!(wire["error"], json!(null));
        assert_eq!(wire["kind"], "electrical_revision_modification");
        assert_eq!(wire["status"], "verified");
        assert_eq!(wire["assembly_adoption_modified"], false);
    }

    #[tokio::test]
    async fn partial_stage_keeps_binding_and_verified_prefix_across_replay_refusal() {
        let registry = OperationRegistry::default();
        let id = Uuid::from_u128(2);
        let job = JobStageBinding {
            job_id: Uuid::from_u128(3),
            region_index: 4,
            undo: false,
        };
        let progress = ExecutionProgress {
            operation_consumed: true,
            submitted_changes: Some(7),
            verified_steps: 2,
            total_changes: Some(8),
            world: WorldOutcome::Unknown,
            ..Default::default()
        };
        let failure = progress.cause(FailureCause::new(
            CauseKind::VerificationMismatch,
            "readback differs",
        ));
        let result = ElectricalEditResult::completed(id, false, Some(job), 2, 8, Err(failure));
        registry
            .record_completed(id, OperationKind::PlacementApply, result.clone().into())
            .await;
        let saved = registry.get(id).await.unwrap();
        assert_eq!(saved.status, OperationStatus::Failed);
        assert_eq!(saved.progress_percent, 25);
        let wire = serde_json::to_value(saved.result.unwrap()).unwrap();
        assert_eq!(
            wire["job_stage"],
            json!({"job_id":job.job_id, "region_index":4, "undo":false})
        );
        assert_eq!(wire["verified_steps"], 2);
        assert_eq!(wire["error"], "readback differs");
        assert_eq!(wire["status"], "needs_inspection");
        assert!(wire.get("execution_progress").is_none());
        let refused = ElectricalEditResult::failed_attempt(ExecutionProgress::default().cause(
            FailureCause::new(CauseKind::InvalidState, "already consumed"),
        ));
        registry
            .record_completed(id, OperationKind::PlacementApply, refused.into())
            .await;
        assert_eq!(registry.get(id).await.unwrap().result, Some(result.into()));
    }

    #[tokio::test]
    async fn assembly_final_save_failure_keeps_verified_world_separate_from_completion() {
        let registry = OperationRegistry::default();
        let id = Uuid::from_u128(5);
        let progress = ExecutionProgress {
            phase: FailurePhase::FinalSave,
            operation_consumed: true,
            world: WorldOutcome::Verified,
            verified_steps: 3,
            total_changes: Some(3),
            durable_verified_steps: Some(2),
            persistence: PersistenceOutcome::Uncertain,
            ..Default::default()
        };
        let report = progress.cause(FailureCause::new(CauseKind::Persistence, "save failed"));
        let result = AssemblyConstructionResult::completed(
            id,
            AssemblyConstructionDetails {
                instance_id: Uuid::from_u128(6),
                record_revision: 7,
                undo: false,
                kind: AssemblyConstructionKind::Reconstruct,
                verified_steps: 3,
                total_steps: 3,
                reconstruction_conditions: Some(ReconstructionConditions::declared()),
                automatic_rollback: false,
                bounds: RegionBoundsDto {
                    min: Pos::new(1, 2, 3),
                    max: Pos::new(4, 5, 6),
                },
            },
            Err(report),
        );
        registry
            .record_completed(id, OperationKind::PlacementApply, result.clone().into())
            .await;
        let saved = registry.get(id).await.unwrap();
        assert_eq!(saved.status, OperationStatus::Failed);
        assert_eq!(saved.progress_percent, 100);
        assert_eq!(result.progress().durable_verified_steps, Some(2));
        let wire = serde_json::to_value(&result).unwrap();
        assert_eq!(wire["kind"], "placed_assembly_reconstruction");
        assert_eq!(wire["record_revision"], 7);
        assert_eq!(
            wire["bounds"],
            json!({"min":{"x":1,"y":2,"z":3},"max":{"x":4,"y":5,"z":6}})
        );
        assert_eq!(wire["ok"], false);
        assert_eq!(wire["recovery"]["inspect_saved_record"], true);
        assert_eq!(wire["retry_allowed"], false);
        assert_eq!(wire["automatic_rollback"], false);
        assert_eq!(
            wire["reconstruction_conditions"]["server_readiness_proven"],
            false
        );
    }
}
