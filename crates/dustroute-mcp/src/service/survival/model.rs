//! Typed job state and persisted diagnostic data. None restores native authority.
use super::*;
use crate::survival_execution::checkpoint::SafeCheckpoint;
use crate::survival_execution::diagnostic::{DiagnosticOnly, RecordedConstructionPreview};
use crate::survival_execution::{ExecutionError, ExecutionEvent};

/// Stored refusals describe why work stopped; they contain no action authority.
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(untagged)]
pub(super) enum JobFailure {
    Boundary(BoundaryFailure),
    Execution(ExecutionFailure),
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct BoundaryFailure {
    ok: DiagnosticOnly,
    schema_version: FailureSchema,
    error: BoundaryError,
    automatic_replay: DiagnosticOnly,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct ExecutionFailure {
    ok: DiagnosticOnly,
    error: ExecutionError,
}
#[derive(Clone, Copy, Debug, Serialize, Deserialize)]
enum FailureSchema {
    #[serde(rename = "dustroute.survival-job.v1")]
    V1,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct BoundaryError {
    code: JobRefusalCode,
    detail: String,
}
#[derive(Clone, Copy, Debug, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub(super) enum JobRefusalCode {
    JournalIo,
    ExecutionTaskFailed,
    AdmissionTaskFailed,
    SourceChanged,
    PermissionDenied,
    CheckpointEndpointChanged,
    CancelledBeforeStart,
}
impl JobFailure {
    pub(super) fn boundary(code: JobRefusalCode, detail: impl std::fmt::Display) -> Self {
        Self::Boundary(BoundaryFailure {
            ok: DiagnosticOnly,
            schema_version: FailureSchema::V1,
            error: BoundaryError {
                code,
                detail: detail.to_string(),
            },
            automatic_replay: DiagnosticOnly,
        })
    }
}
impl From<ExecutionError> for JobFailure {
    fn from(error: ExecutionError) -> Self {
        Self::Execution(ExecutionFailure {
            ok: DiagnosticOnly,
            error,
        })
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(tag = "state", rename_all = "snake_case")]
pub(super) enum JobStatus {
    Planned,
    CancelledBeforeStart {
        world_writes: bool,
    },
    PlanExpiredOrCancelled {
        construction_dispatched: bool,
    },
    Admitting,
    AdmissionRefused {
        failure: JobFailure,
        construction_dispatched: bool,
    },
    Running {
        progress: ExecutionProgress,
        completed_steps: usize,
    },
    Checkpointed {
        completed_steps: usize,
        safe_idle: bool,
        checkpoint: Box<SafeCheckpoint>,
        next_step: String,
    },
    Completed {
        #[serde(skip_serializing_if = "Option::is_none")]
        completed_steps: Option<usize>,
        #[serde(skip_serializing_if = "Option::is_none")]
        final_evidence: Option<ExecutionEvent>,
    },
    CancelledNeedsInspection {
        error: Option<ExecutionError>,
        completed_steps: usize,
    },
    NeedsInspection {
        #[serde(flatten)]
        reason: InspectionReason,
    },
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(untagged)]
pub(super) enum InspectionReason {
    Execution {
        error: ExecutionError,
        completed_steps: usize,
    },
    Task {
        failure: JobFailure,
    },
    Persistence {
        last_status: Box<JobStatus>,
        persistence_error: String,
    },
}
impl JobStatus {
    pub(super) fn completed_steps(&self) -> Option<usize> {
        match self {
            Self::Running {
                completed_steps, ..
            }
            | Self::Checkpointed {
                completed_steps, ..
            }
            | Self::CancelledNeedsInspection {
                completed_steps, ..
            }
            | Self::NeedsInspection {
                reason:
                    InspectionReason::Execution {
                        completed_steps, ..
                    },
            } => Some(*completed_steps),
            Self::Completed {
                completed_steps, ..
            } => *completed_steps,
            _ => None,
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct ConstructionSpecification {
    pub specification: GroundedBuildingDesignRequest,
    pub scope: ConstructionScope,
    pub temporary_material: String,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub(super) enum JobSchema {
    #[serde(rename = "dustroute.survival-job.v1")]
    V1,
    #[serde(other)]
    Unknown,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct JobManifest {
    pub schema: JobSchema,
    pub job_id: uuid::Uuid,
    pub owner: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub source: Option<SourceIdentity>,
    /// Historical plan/search facts, separate from the process-local native plan.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub preview: Option<Box<RecordedConstructionPreview>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub construction: Option<ConstructionSpecification>,
    pub parent_job_id: Option<uuid::Uuid>,
    pub execution_authority_restorable: DiagnosticOnly,
}
impl JobManifest {
    pub(super) fn validate_identity(&self, id: uuid::Uuid) -> Result<(), &'static str> {
        if self.schema != JobSchema::V1 || self.job_id != id {
            return Err("unknown schema or mismatched job identity");
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unknown_job_state_and_restorable_manifest_are_rejected() {
        assert!(serde_json::from_value::<JobStatus>(json!({"state":"future_running"})).is_err());
        let id = uuid::Uuid::new_v4();
        let mut wire = json!({"schema":"dustroute.survival-job.v1","job_id":id,
            "owner":"Tester","execution_authority_restorable":false});
        let valid: JobManifest = serde_json::from_value(wire.clone()).unwrap();
        valid.validate_identity(id).unwrap();
        wire["execution_authority_restorable"] = json!(true);
        assert!(serde_json::from_value::<JobManifest>(wire).is_err());
        assert!(valid.validate_identity(uuid::Uuid::new_v4()).is_err());
    }

    #[test]
    fn persistence_failure_never_reports_completion_or_checkpoint_readiness() {
        let completed = JobStatus::Completed {
            completed_steps: Some(12),
            final_evidence: None,
        };
        let failed = JobStatus::NeedsInspection {
            reason: InspectionReason::Persistence {
                last_status: Box::new(completed),
                persistence_error: "fixture disk failure".into(),
            },
        };
        let wire = json!(failed);
        assert_eq!(wire["state"], "needs_inspection");
        assert_eq!(wire["last_status"]["completed_steps"], 12);
        let reread: JobStatus = serde_json::from_value(wire.clone()).unwrap();
        assert!(matches!(reread, JobStatus::NeedsInspection { .. }));
        assert_eq!(reread.completed_steps(), None);
        assert_eq!(json!(reread), wire);
    }

    #[test]
    fn refused_job_history_keeps_typed_causes_without_restoring_success_or_replay() {
        let examples = [
            (
                JobFailure::boundary(JobRefusalCode::SourceChanged, "source changed"),
                json!({"ok":false,"schema_version":"dustroute.survival-job.v1",
                    "error":{"code":"source_changed","detail":"source changed"},
                    "automatic_replay":false}),
            ),
            (
                ExecutionError {
                    code: SurvivalErrorCode::CheckpointConsumed,
                    detail: "checkpoint already consumed".into(),
                }
                .into(),
                json!({"ok":false,"error":{"code":"checkpoint_consumed",
                    "detail":"checkpoint already consumed"}}),
            ),
        ];
        for (failure, wire) in examples {
            assert_eq!(json!(failure), wire);
            let stored = JobStatus::AdmissionRefused {
                failure,
                construction_dispatched: false,
            };
            let bytes = serde_json::to_vec(&stored).unwrap();
            let reread: JobStatus = serde_json::from_slice(&bytes).unwrap();
            assert!(matches!(reread, JobStatus::AdmissionRefused { .. }));
            assert_eq!(reread.completed_steps(), None);
            assert_eq!(json!(reread)["failure"], wire);
            let mut altered = wire;
            altered["ok"] = json!(true);
            assert!(serde_json::from_value::<JobFailure>(altered).is_err());
        }
        let mut wire = json!(JobFailure::boundary(
            JobRefusalCode::ExecutionTaskFailed,
            "worker failed"
        ));
        wire["automatic_replay"] = json!(true);
        assert!(serde_json::from_value::<JobFailure>(wire.clone()).is_err());
        wire["automatic_replay"] = json!(false);
        wire["error"]["code"] = json!("future_execution_failure");
        assert!(serde_json::from_value::<JobFailure>(wire).is_err());
    }
}
