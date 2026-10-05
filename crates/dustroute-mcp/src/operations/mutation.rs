//! Recorded results of ordinary placement and repair attempts. These are owned
//! diagnostic values, never executable plans, fresh observations or replay keys.
use crate::api::{ERROR_SCHEMA_V1, McpErrorCode};
use crate::bridge_protocol::{CommandSubmission, PhysicalSubmission};
use crate::failure::{
    AttemptResponse, ExecutionProgress, FailureCause, FailureReport, WorldOutcome,
};
use dustroute_physical::Pos;
use dustroute_translate::analysis::LogicalRole;
use dustroute_translate::world_reverse::TruthTableComparison;
use serde::{Serialize, Serializer, ser::SerializeStruct};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Success;
impl Serialize for Success {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_bool(true)
    }
}
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum MutationAction {
    Apply,
    Undo,
}
impl MutationAction {
    pub(crate) fn from_undo(undo: bool) -> Self {
        if undo { Self::Undo } else { Self::Apply }
    }
}

/// A refusal with no trustworthy execution facts. In particular an unavailable
/// count is not a known zero, and a message does not establish a failure phase.
#[derive(Clone, Debug, PartialEq)]
pub enum UnrecordedFailure {
    Message(String),
    Coded(CodedRefusal),
    Cause(Box<FailureCause>),
    WireSearch(Box<super::preview::optimization::WireSearchRefusal>),
}
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct CodedRefusal {
    ok: bool,
    schema_version: &'static str,
    error: String,
    error_code: McpErrorCode,
    retryable: bool,
}
impl UnrecordedFailure {
    pub fn message(message: impl Into<String>) -> Self {
        Self::Message(message.into())
    }
    pub fn coded(code: McpErrorCode, message: impl Into<String>, retryable: bool) -> Self {
        Self::Coded(CodedRefusal {
            ok: false,
            schema_version: ERROR_SCHEMA_V1,
            error: message.into(),
            error_code: code,
            retryable,
        })
    }
}
impl From<String> for UnrecordedFailure {
    fn from(message: String) -> Self {
        Self::message(message)
    }
}
impl From<&str> for UnrecordedFailure {
    fn from(message: &str) -> Self {
        Self::message(message)
    }
}
impl From<FailureCause> for UnrecordedFailure {
    fn from(cause: FailureCause) -> Self {
        Self::Cause(Box::new(cause))
    }
}
impl Serialize for UnrecordedFailure {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        if let Self::WireSearch(error) = self {
            return error.serialize(serializer);
        }
        if let Self::Coded(error) = self {
            return error.serialize(serializer);
        }
        if let Self::Cause(cause) = self {
            return cause.as_response().serialize(serializer);
        }
        let mut record = serializer.serialize_struct("UnrecordedFailure", 2)?;
        record.serialize_field("ok", &false)?;
        match self {
            Self::Message(message) => record.serialize_field("error", message)?,
            Self::Cause(_) | Self::Coded(_) | Self::WireSearch(_) => unreachable!(),
        }
        record.end()
    }
}

#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct PlacementAttempt {
    pub operation_id: String,
    #[serde(flatten)]
    pub outcome: PlacementOutcome,
}
#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(untagged)]
pub enum PlacementOutcome {
    Verified(Box<PlacementReceipt>),
    Failed(Box<PlacementFailure>),
    Refused(UnrecordedFailure),
}
impl PlacementOutcome {
    pub(crate) fn refused(message: impl Into<String>) -> Self {
        Self::Refused(UnrecordedFailure::message(message))
    }
    pub(crate) fn coded(code: McpErrorCode, message: impl Into<String>, retryable: bool) -> Self {
        Self::Refused(UnrecordedFailure::coded(code, message, retryable))
    }
    pub(crate) fn failed(report: FailureReport) -> Self {
        Self::Failed(Box::new(PlacementFailure {
            report,
            mismatches: None,
            bridge: None,
        }))
    }
}
impl PlacementAttempt {
    pub fn failed(&self) -> bool {
        !matches!(self.outcome, PlacementOutcome::Verified(_))
    }
    pub fn progress(&self) -> Option<&ExecutionProgress> {
        match &self.outcome {
            PlacementOutcome::Verified(receipt) => Some(&receipt.execution_progress),
            PlacementOutcome::Failed(failure) => Some(&failure.report.progress),
            PlacementOutcome::Refused(_) => None,
        }
    }
}
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct PlacementReceipt {
    pub schema_version: &'static str,
    pub execution_progress: ExecutionProgress,
    pub ok: Success,
    pub action: MutationAction,
    pub changed_blocks: usize,
    pub verified: bool,
    pub dimension: String,
    pub bridge: CommandSubmission,
}
#[derive(Clone, Debug, PartialEq)]
pub struct PlacementFailure {
    pub report: FailureReport,
    pub mismatches: Option<Vec<Pos>>,
    pub bridge: Option<CommandSubmission>,
}
impl Serialize for PlacementFailure {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        #[derive(Serialize)]
        struct View<'a> {
            #[serde(flatten)]
            report: AttemptResponse<'a>,
            #[serde(skip_serializing_if = "Option::is_none")]
            mismatches: &'a Option<Vec<Pos>>,
            #[serde(skip_serializing_if = "Option::is_none")]
            bridge: &'a Option<CommandSubmission>,
        }
        View {
            report: self.report.as_response(),
            mismatches: &self.mismatches,
            bridge: &self.bridge,
        }
        .serialize(serializer)
    }
}

#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct RepairAttempt {
    pub operation_id: String,
    #[serde(flatten)]
    pub outcome: RepairOutcome,
}
#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(untagged)]
pub enum RepairOutcome {
    Verified(Box<RepairReceipt>),
    Failed(Box<RepairFailure>),
    Refused(UnrecordedFailure),
}
impl RepairOutcome {
    pub(crate) fn refused(message: impl Into<String>) -> Self {
        Self::Refused(UnrecordedFailure::message(message))
    }
    pub(crate) fn coded(code: McpErrorCode, message: impl Into<String>, retryable: bool) -> Self {
        Self::Refused(UnrecordedFailure::coded(code, message, retryable))
    }
    pub(crate) fn failed(report: FailureReport) -> Self {
        Self::Failed(Box::new(RepairFailure {
            report,
            restoration: None,
        }))
    }
}
impl RepairAttempt {
    pub fn failed(&self) -> bool {
        !matches!(self.outcome, RepairOutcome::Verified(_))
    }
    pub fn progress(&self) -> Option<&ExecutionProgress> {
        match &self.outcome {
            RepairOutcome::Verified(receipt) => Some(&receipt.execution_progress),
            RepairOutcome::Failed(failure) => Some(&failure.report.progress),
            RepairOutcome::Refused(_) => None,
        }
    }
}
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct RepairReceipt {
    pub schema_version: &'static str,
    pub execution_progress: ExecutionProgress,
    pub ok: Success,
    pub action: MutationAction,
    pub verified: bool,
    pub boundary_verified: bool,
    pub boundary_mismatches: Vec<Pos>,
    pub changed_blocks: usize,
    pub fragments_before: usize,
    pub fragments_after: usize,
    pub resulting_logic: LogicalRole,
    pub semantic_verification: SemanticVerification,
    pub bridge: PhysicalSubmission,
}
#[derive(Clone, Debug, PartialEq)]
pub struct RepairFailure {
    pub report: FailureReport,
    pub restoration: Option<RepairRestoration>,
}
impl Serialize for RepairFailure {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        #[derive(Serialize)]
        struct View<'a> {
            #[serde(flatten)]
            report: AttemptResponse<'a>,
            #[serde(flatten)]
            restoration: &'a Option<RepairRestoration>,
        }
        View {
            report: self.report.as_response(),
            restoration: &self.restoration,
        }
        .serialize(serializer)
    }
}
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct RepairRestoration {
    pub mismatches: Vec<Pos>,
    pub boundary_mismatches: Vec<Pos>,
    pub rollback_submitted: bool,
    pub rollback_ok: bool,
    pub restoration: RestorationOutcome,
}
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct RestorationOutcome {
    pub attempted: bool,
    pub world: WorldOutcome,
}
#[derive(Clone, Debug, PartialEq)]
pub enum SemanticVerification {
    Compared(TruthTableComparison),
    Unavailable(SemanticUnavailableReason),
}
#[derive(Clone, Copy, Debug, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum SemanticUnavailableReason {
    BaselineTruthTableNotRequested,
    PostAnalysisTruthTableUnavailable,
}
impl Serialize for SemanticVerification {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        let mut record = serializer.serialize_struct(
            "SemanticVerification",
            match self {
                Self::Compared(_) => 3,
                Self::Unavailable(_) => 2,
            },
        )?;
        match self {
            Self::Compared(comparison) => {
                record.serialize_field("available", &true)?;
                record.serialize_field(
                    "equivalent",
                    &(comparison.comparable && comparison.fitness_penalty == 0),
                )?;
                record.serialize_field("comparison", comparison)?;
            }
            Self::Unavailable(reason) => {
                record.serialize_field("available", &false)?;
                record.serialize_field("reason", reason)?;
            }
        }
        record.end()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn structured_refusal_exposes_diagnosis_without_reencoding_a_cause_as_error_data() {
        let refusal = UnrecordedFailure::from(
            FailureCause::new(crate::failure::CauseKind::PermissionDenied, "player denied")
                .at(crate::failure::FailurePhase::Admission),
        );
        let wire = serde_json::to_value(refusal).unwrap();
        assert_eq!(wire["ok"], false);
        assert_eq!(wire["error"], "player denied");
        assert_eq!(wire["error_code"], "permission_denied");
        assert_eq!(wire["failure"]["primary"]["phase"], "admission");
        assert!(wire["failure"]["progress"].is_null());
        assert_eq!(wire["recovery"]["same_operation_replay_allowed"], false);
    }

    use crate::failure::{CauseKind, FailurePhase, PersistenceOutcome};
    use serde_json::json;

    #[test]
    fn refused_attempt_has_no_execution_facts_but_known_zero_remains_known() {
        let unknown = PlacementAttempt {
            operation_id: "invalid-token".into(),
            outcome: PlacementOutcome::refused("confirm required"),
        };
        assert!(unknown.failed());
        assert!(unknown.progress().is_none());
        assert_eq!(
            serde_json::to_value(&unknown).unwrap(),
            json!({"ok":false,"error":"confirm required","operation_id":"invalid-token"})
        );
        let known = PlacementAttempt {
            operation_id: "invalid-token".into(),
            outcome: PlacementOutcome::failed(ExecutionProgress::default().cause(
                FailureCause::new(CauseKind::PermissionDenied, "mutation disabled"),
            )),
        };
        assert_eq!(known.progress().unwrap().submitted_changes, Some(0));
        let wire = serde_json::to_value(known).unwrap();
        assert_eq!(wire["status"], "refused");
        assert_eq!(wire["failure"]["progress"]["submitted_changes"], 0);
        assert_eq!(wire["failure"]["progress"]["world"], "not_attempted");
        assert!(wire.get("mismatches").is_none());
        assert!(wire.get("bridge").is_none());
    }

    #[test]
    fn restoration_does_not_turn_the_failed_target_into_success_or_durable_completion() {
        let progress = ExecutionProgress {
            phase: FailurePhase::Restore,
            world: WorldOutcome::Unknown,
            submitted_changes: Some(2),
            total_changes: Some(1),
            operation_consumed: true,
            persistence: PersistenceOutcome::Uncertain,
            ..Default::default()
        };
        let attempt = RepairAttempt {
            operation_id: "repair".into(),
            outcome: RepairOutcome::Failed(Box::new(RepairFailure {
                report: progress.cause(FailureCause::new(
                    CauseKind::VerificationMismatch,
                    "target mismatch",
                )),
                restoration: Some(RepairRestoration {
                    mismatches: vec![Pos::new(1, 2, 3)],
                    boundary_mismatches: vec![],
                    rollback_submitted: true,
                    rollback_ok: true,
                    restoration: RestorationOutcome {
                        attempted: true,
                        world: WorldOutcome::Verified,
                    },
                }),
            })),
        };
        assert!(attempt.failed());
        assert_eq!(
            attempt.progress().unwrap().persistence,
            PersistenceOutcome::Uncertain
        );
        let wire = serde_json::to_value(attempt).unwrap();
        assert_eq!(wire["ok"], false);
        assert_eq!(wire["status"], "needs_inspection");
        assert_eq!(
            wire["restoration"],
            json!({"attempted":true,"world":"verified"})
        );
        assert_eq!(wire["mismatches"], json!([{"x":1,"y":2,"z":3}]));
        assert_eq!(wire["recovery"]["inspect_saved_record"], true);
        assert_eq!(wire["recovery"]["same_operation_replay_allowed"], false);
    }

    #[test]
    fn successful_repair_keeps_unavailable_semantics_separate_from_an_equivalence_proof() {
        for (reason, spelling) in [
            (
                SemanticUnavailableReason::BaselineTruthTableNotRequested,
                "baseline_truth_table_not_requested",
            ),
            (
                SemanticUnavailableReason::PostAnalysisTruthTableUnavailable,
                "post_analysis_truth_table_unavailable",
            ),
        ] {
            let wire = serde_json::to_value(SemanticVerification::Unavailable(reason)).unwrap();
            assert_eq!(wire, json!({"available":false,"reason":spelling}));
            assert!(wire.get("equivalent").is_none());
            assert!(wire.get("comparison").is_none());
        }
    }
}
