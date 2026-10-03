//! Typed job state and persisted diagnostic data. None restores native authority.
use super::*;
use crate::survival_execution::checkpoint::SafeCheckpoint;
use crate::survival_execution::diagnostic::{DiagnosticOnly, DiagnosticPayload};
use crate::survival_execution::{ExecutionError, ExecutionEvent};

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
        failure: DiagnosticPayload,
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
        failure: DiagnosticPayload,
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
    /// Full native preview is retained only for display; no fields drive execution.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub preview: Option<DiagnosticPayload>,
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
}
