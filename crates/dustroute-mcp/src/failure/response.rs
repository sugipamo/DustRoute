//! Typed presentation of failure facts. Serialization never publishes live
//! progress or grants replay/observation authority.
use super::{
    CauseKind, ExecutionProgress, FailureCause, FailurePhase, FailureReport, PersistenceOutcome,
    WorldOutcome,
};
use serde::Serialize;

#[derive(Clone, Copy, Debug, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum FailureDisposition {
    Refused,
    NeedsInspection,
}

#[derive(Debug, Serialize)]
pub struct RecoveryAdvice {
    reobserve_required: Option<bool>,
    replan_required: Option<bool>,
    inspect_saved_record: Option<bool>,
    same_operation_replay_allowed: bool,
}

#[derive(Debug, Serialize)]
pub struct UnrecordedDiagnostic<'a> {
    primary: &'a FailureCause,
    secondary: &'a [FailureCause],
    progress: Option<&'a ExecutionProgress>,
}

#[derive(Debug, Serialize)]
pub struct CauseResponse<'a> {
    ok: bool,
    schema_version: &'static str,
    error: &'a str,
    error_code: &'static str,
    retryable: bool,
    failure: UnrecordedDiagnostic<'a>,
    recovery: RecoveryAdvice,
}

#[derive(Debug, Serialize)]
pub struct AttemptResponse<'a> {
    ok: bool,
    schema_version: &'static str,
    error: &'a str,
    error_code: &'static str,
    retryable: bool,
    retry_allowed: bool,
    status: FailureDisposition,
    failure: &'a FailureReport,
    recovery: RecoveryAdvice,
}

impl FailureCause {
    pub fn as_response(&self) -> CauseResponse<'_> {
        CauseResponse {
            ok: false,
            schema_version: "dustroute.error.v2",
            error: &self.message,
            error_code: self.error_code(),
            retryable: false,
            failure: UnrecordedDiagnostic {
                primary: self,
                secondary: &[],
                progress: None,
            },
            recovery: RecoveryAdvice {
                reobserve_required: matches!(
                    self.kind,
                    CauseKind::ObservationUnavailable
                        | CauseKind::ObservationIncomplete
                        | CauseKind::MovingObservation
                        | CauseKind::VerificationMismatch
                )
                .then_some(true),
                replan_required: None,
                inspect_saved_record: None,
                same_operation_replay_allowed: false,
            },
        }
    }
}
impl FailureReport {
    pub fn as_response(&self) -> AttemptResponse<'_> {
        let inspection = self.progress.world != WorldOutcome::NotAttempted
            || self.progress.operation_consumed
            || self.progress.persistence == PersistenceOutcome::Uncertain;
        // Cleanup/readback failures remain independent facts. They can require
        // a fresh observation even when the original failure was unrelated.
        let causes = || std::iter::once(&self.primary).chain(&self.secondary);
        let reobserve = self.progress.world == WorldOutcome::Unknown
            || causes().any(|cause| {
                matches!(
                    cause.kind,
                    CauseKind::ObservationUnavailable
                        | CauseKind::ObservationIncomplete
                        | CauseKind::MovingObservation
                        | CauseKind::VerificationMismatch
                ) || matches!(
                    cause.phase,
                    Some(FailurePhase::BeforeReadback | FailurePhase::AfterReadback)
                )
            });
        AttemptResponse {
            ok: false,
            schema_version: "dustroute.error.v2",
            error: &self.primary.message,
            error_code: self.primary.error_code(),
            retryable: false,
            retry_allowed: false,
            status: if inspection {
                FailureDisposition::NeedsInspection
            } else {
                FailureDisposition::Refused
            },
            failure: self,
            recovery: RecoveryAdvice {
                reobserve_required: Some(reobserve),
                replan_required: Some(
                    self.progress.operation_consumed
                        || causes().any(|cause| cause.kind == CauseKind::VerificationMismatch),
                ),
                inspect_saved_record: Some(
                    self.progress.persistence != PersistenceOutcome::NotRequired
                        || causes().any(|cause| cause.kind == CauseKind::Persistence),
                ),
                same_operation_replay_allowed: false,
            },
        }
    }
}
