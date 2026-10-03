//! Facts about a failed operation. No inference from diagnostic message text,
//! replay authority or conversion of submitted commands into verified writes.
use dustroute_physical::Pos;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::fmt::{Display, Formatter};

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CauseKind {
    InvalidInput,
    InvalidState,
    NotFound,
    PermissionDenied,
    Unsupported,
    ResourceLimit,
    Connection,
    Disconnected,
    Timeout,
    Protocol,
    Serialization,
    ObservationUnavailable,
    ObservationIncomplete,
    MovingObservation,
    VerificationMismatch,
    Persistence,
    Rejected,
    Unknown,
}
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FailurePhase {
    Admission,
    Normalization,
    Analysis,
    ModelProof,
    BeforeReadback,
    Submission,
    Wait,
    AfterReadback,
    Verification,
    IntentSave,
    CheckpointSave,
    FinalSave,
    Restore,
    PostAnalysis,
    Unknown,
}
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct CauseDetails {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub position: Option<Pos>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub parameter: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub allowed_range: Option<[f64; 2]>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub supplied_number: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub resource: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub actual: Option<usize>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub maximum: Option<usize>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reconstruction_issue: Option<ReconstructionDiagnostic>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub native_error_kind: Option<NativeFailureKind>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub recovery_chunks: Vec<[i32; 2]>,
    #[serde(default)]
    pub recovery_chunks_truncated: bool,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub mismatches: Vec<Pos>,
    #[serde(default)]
    pub mismatches_truncated: bool,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(untagged, deny_unknown_fields)]
pub enum ReconstructionDiagnostic {
    Client(ClientReconstructionDiagnostic),
    World {
        issues: Vec<dustroute_translate::world::WorldValidationIssue>,
    },
}

/// Native categories remain facts; diagnostic text never determines this value.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub enum NativeFailureKind {
    Unsupported,
    InvalidInput,
    Connection,
    UncertainDispatch,
    Timeout,
    Disconnected,
    Protocol,
    ResourceLimit,
    Rejected,
    State,
    Other,
    Unknown,
}

/// Archived reconstruction reason, never a usable client observation.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum ClientReconstructionDiagnostic {
    UnsupportedTickControl,
    MissingBlock { position: [i32; 3] },
    UnsupportedBlock { position: [i32; 3], name: String },
    MissingCarrier { position: [i32; 3] },
    ChunkInvalidated { chunk: [i32; 2] },
    Limit,
    InvalidAction,
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct FailureCause {
    pub kind: CauseKind,
    pub message: String,
    pub details: Box<CauseDetails>,
    pub phase: Option<FailurePhase>,
}
impl FailureCause {
    pub fn new(kind: CauseKind, message: impl Into<String>) -> Self {
        Self {
            kind,
            message: message.into(),
            details: Default::default(),
            phase: None,
        }
    }
    /// Diagnostics without execution facts must not invent zero world changes.
    pub fn response(&self) -> Value {
        json!({"ok":false,"schema_version":"dustroute.error.v2",
            "error":self.message,"error_code":self.error_code(),"retryable":false,
            "failure":{"primary":self,"secondary":[],"progress":null},
            "recovery":{"reobserve_required":match self.kind {
                CauseKind::ObservationUnavailable | CauseKind::ObservationIncomplete |
                CauseKind::MovingObservation | CauseKind::VerificationMismatch => Some(true),
                _ => None },"replan_required":null,"inspect_saved_record":null,
                "same_operation_replay_allowed":false}})
    }
    pub fn input_range(parameter: &str, actual: f64, min: f64, max: f64) -> Self {
        let mut cause = Self::new(
            CauseKind::InvalidInput,
            format!("{parameter} must be {min}..{max}"),
        );
        cause.details.parameter = Some(parameter.into());
        cause.details.allowed_range = Some([min, max]);
        cause.details.supplied_number = actual.is_finite().then_some(actual);
        cause.at(FailurePhase::Admission)
    }
    pub fn mismatch(message: impl Into<String>, positions: &[Pos]) -> Self {
        let mut cause = Self::new(CauseKind::VerificationMismatch, message);
        cause.details.actual = Some(positions.len());
        cause.details.mismatches = positions.iter().take(64).copied().collect();
        cause.details.mismatches_truncated = positions.len() > 64;
        cause
    }
    pub fn at(mut self, phase: FailurePhase) -> Self {
        self.phase = Some(phase);
        self
    }
    pub fn error_code(&self) -> &'static str {
        match self.kind {
            CauseKind::InvalidInput => "invalid_argument",
            CauseKind::InvalidState => "invalid_state",
            CauseKind::NotFound => "not_found",
            CauseKind::PermissionDenied => "permission_denied",
            CauseKind::Connection
            | CauseKind::Disconnected
            | CauseKind::Timeout
            | CauseKind::Protocol
            | CauseKind::Rejected => "bridge_unavailable",
            CauseKind::Serialization => "serialization_failed",
            CauseKind::ObservationUnavailable
            | CauseKind::ObservationIncomplete
            | CauseKind::MovingObservation => "observation_unavailable",
            CauseKind::VerificationMismatch => "verification_failed",
            CauseKind::Unsupported => "unsupported",
            CauseKind::ResourceLimit => "resource_limit",
            CauseKind::Persistence => "persistence_failed",
            CauseKind::Unknown => "internal",
        }
    }
}
impl Display for FailureCause {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.message)
    }
}
impl std::error::Error for FailureCause {}
impl From<String> for FailureCause {
    fn from(message: String) -> Self {
        Self::new(CauseKind::Unknown, message)
    }
}
impl From<&str> for FailureCause {
    fn from(message: &str) -> Self {
        Self::new(CauseKind::Unknown, message)
    }
}
impl From<FailureCause> for String {
    fn from(cause: FailureCause) -> Self {
        cause.message
    }
}
impl From<dustroute_translate::snapshot::SnapshotError> for FailureCause {
    fn from(error: dustroute_translate::snapshot::SnapshotError) -> Self {
        use dustroute_translate::snapshot::SnapshotError::*;
        let mut cause = Self::new(CauseKind::ObservationIncomplete, error.to_string());
        match error {
            Json(_) => cause.kind = CauseKind::Serialization,
            InvalidFacing { pos, .. } => cause.details.position = Some(pos),
            InvalidSnapshot(_) => {}
        }
        cause
    }
}
impl From<dustroute_translate::snapshot::LiteralSnapshotError> for FailureCause {
    fn from(error: dustroute_translate::snapshot::LiteralSnapshotError) -> Self {
        let mut cause = Self::new(CauseKind::ObservationIncomplete, error.to_string());
        if let dustroute_translate::snapshot::LiteralSnapshotError::InvalidCoordinate {
            position,
            ..
        } = error
        {
            cause.details.position = Some(position);
        }
        cause
    }
}
impl From<crate::discovery::DiscoveryError> for FailureCause {
    fn from(error: crate::discovery::DiscoveryError) -> Self {
        let mut cause = Self::new(CauseKind::NotFound, error.to_string());
        if let crate::discovery::DiscoveryError::NodeLimitExceeded { limit } = error {
            cause.kind = CauseKind::ResourceLimit;
            cause.details.resource = Some("discovery_nodes".into());
            cause.details.maximum = Some(limit);
        }
        cause
    }
}
impl From<crate::selection::SelectionError> for FailureCause {
    fn from(error: crate::selection::SelectionError) -> Self {
        Self::new(CauseKind::InvalidState, error.to_string())
    }
}
impl From<serde_json::Error> for FailureCause {
    fn from(error: serde_json::Error) -> Self {
        Self::new(CauseKind::Serialization, error.to_string())
    }
}
impl From<dustroute_app::PlanningError> for FailureCause {
    fn from(error: dustroute_app::PlanningError) -> Self {
        let mut cause = Self::new(CauseKind::InvalidInput, error.to_string());
        match error {
            dustroute_app::PlanningError::BlockLimitExceeded { limit, actual } => {
                cause.kind = CauseKind::ResourceLimit;
                cause.details.resource = Some("placement_blocks".into());
                cause.details.actual = Some(actual);
                cause.details.maximum = Some(limit);
            }
            dustroute_app::PlanningError::InvalidWorld(error) => {
                cause.details.reconstruction_issue = Some(ReconstructionDiagnostic::World {
                    issues: error.issues,
                });
            }
        }
        cause
    }
}
impl From<crate::PolicyError> for FailureCause {
    fn from(error: crate::PolicyError) -> Self {
        use crate::PolicyError::*;
        let mut cause = Self::new(CauseKind::PermissionDenied, error.to_string());
        cause.details.resource = Some(match &error {
            ScanVolumeExceeded { .. } => "scan_blocks".into(),
            PlacementLimitExceeded { .. } => "placement_blocks".into(),
            PlayerDenied(player) => format!("player:{player}"),
            DimensionDenied(dimension) => format!("dimension:{dimension}"),
            OutsideAllowedRegion => "allowed_region".into(),
            MutationDenied => "world_mutation".into(),
        });
        match error {
            ScanVolumeExceeded { actual, limit } | PlacementLimitExceeded { actual, limit } => {
                cause.kind = CauseKind::ResourceLimit;
                cause.details.actual = Some(actual);
                cause.details.maximum = Some(limit);
            }
            _ => {}
        }
        cause
    }
}
impl From<dustroute_translate::piston_construction::ConstructionError> for FailureCause {
    fn from(error: dustroute_translate::piston_construction::ConstructionError) -> Self {
        use dustroute_translate::piston_construction::ConstructionError::*;
        let mut cause = Self::new(CauseKind::Unknown, error.to_string());
        match error {
            InvalidInput(_) => cause.kind = CauseKind::InvalidInput,
            Budget {
                resource,
                actual,
                maximum,
            } => {
                cause.kind = CauseKind::ResourceLimit;
                cause.details.resource = Some(format!("{resource:?}"));
                cause.details.actual = Some(actual);
                cause.details.maximum = Some(maximum);
            }
            Literal(error) => {
                cause.kind = CauseKind::InvalidInput;
                if let dustroute_translate::snapshot::LiteralSnapshotError::InvalidCoordinate {
                    position,
                    ..
                } = error
                {
                    cause.details.position = Some(position);
                }
            }
            Runtime(error) => {
                use dustroute_translate::world::time::runtime::RuntimeError as R;
                match error {
                    R::Invalid(_) => cause.kind = CauseKind::InvalidInput,
                    R::UnknownSpace(position) => {
                        cause.kind = CauseKind::ObservationIncomplete;
                        cause.details.position = Some(position);
                    }
                    R::CarrierConflict(position) => {
                        cause.kind = CauseKind::InvalidState;
                        cause.details.position = Some(position);
                    }
                    R::PastDelivery { .. } | R::InputInsideCallback | R::UnfinishedMotion => {
                        cause.kind = CauseKind::InvalidState
                    }
                    R::Limit(resource) => {
                        cause.kind = CauseKind::ResourceLimit;
                        cause.details.resource = Some(resource.into());
                    }
                    R::ClockOverflow => {
                        cause.kind = CauseKind::ResourceLimit;
                        cause.details.resource = Some("runtime_clock".into());
                    }
                    R::Handler(_) | R::Failed(_) => {}
                }
            }
            Model(_) => {}
        }
        cause
    }
}
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum WorldOutcome {
    NotAttempted,
    Unknown,
    Verified,
}
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PersistenceOutcome {
    NotRequired,
    IntentSaved,
    CheckpointSaved,
    FinalSaved,
    Uncertain,
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ExecutionProgress {
    pub phase: FailurePhase,
    pub world: WorldOutcome,
    /// A lower bound of locally submitted complete changes; null means unavailable.
    pub submitted_changes: Option<usize>,
    pub total_changes: Option<usize>,
    pub verified_steps: usize,
    pub durable_verified_steps: Option<usize>,
    pub persistence: PersistenceOutcome,
    pub operation_consumed: bool,
    pub first_step: Option<usize>,
    pub last_step: Option<usize>,
}
impl Default for ExecutionProgress {
    fn default() -> Self {
        Self {
            phase: FailurePhase::Admission,
            world: WorldOutcome::NotAttempted,
            submitted_changes: Some(0),
            total_changes: None,
            verified_steps: 0,
            durable_verified_steps: None,
            persistence: PersistenceOutcome::NotRequired,
            operation_consumed: false,
            first_step: None,
            last_step: None,
        }
    }
}
impl ExecutionProgress {
    pub fn begin_submission(&mut self) {
        self.phase = FailurePhase::Submission;
        self.world = WorldOutcome::Unknown;
    }
    pub fn submitted(&mut self, count: usize) {
        self.submitted_changes = self.submitted_changes.and_then(|n| n.checked_add(count));
    }
    pub fn submission_error(
        &mut self,
        error: crate::BotBridgeError,
        previous_world: WorldOutcome,
    ) -> FailureReport {
        if let crate::BotBridgeError::Submission(detail) = &error {
            if let Some(count) = detail.submitted_changes {
                self.submitted(count);
            } else {
                self.submitted_changes = None;
            }
            if !detail.may_have_changed_world {
                self.world = previous_world;
            }
        } else {
            self.submitted_changes = None;
        }
        self.cause(error.cause())
    }
    pub fn cause(&self, cause: impl Into<FailureCause>) -> FailureReport {
        let mut primary = cause.into();
        primary.phase.get_or_insert(self.phase);
        FailureReport {
            primary,
            secondary: vec![],
            progress: Box::new(self.clone()),
        }
    }
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct FailureReport {
    pub primary: FailureCause,
    pub secondary: Vec<FailureCause>,
    pub progress: Box<ExecutionProgress>,
}
impl Display for FailureReport {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        Display::fmt(&self.primary, f)
    }
}
impl std::error::Error for FailureReport {}
impl FailureReport {
    pub fn response(&self) -> Value {
        let inspection = self.progress.world != WorldOutcome::NotAttempted
            || self.progress.operation_consumed
            || self.progress.persistence == PersistenceOutcome::Uncertain;
        let reobserve = self.progress.world == WorldOutcome::Unknown
            || matches!(
                self.primary.kind,
                CauseKind::ObservationUnavailable
                    | CauseKind::ObservationIncomplete
                    | CauseKind::MovingObservation
                    | CauseKind::VerificationMismatch
            )
            || matches!(
                self.primary.phase,
                Some(FailurePhase::BeforeReadback | FailurePhase::AfterReadback)
            );
        json!({"ok":false,"schema_version":"dustroute.error.v2",
            "error":self.primary.message,"error_code":self.primary.error_code(),
            "retryable":false,"retry_allowed":false,
            "status":if inspection {"needs_inspection"} else {"refused"},
            "failure":self,
            "recovery":{"reobserve_required":reobserve,
                "replan_required":self.progress.operation_consumed || self.primary.kind==CauseKind::VerificationMismatch,
                "inspect_saved_record":self.progress.persistence == PersistenceOutcome::Uncertain,
                "same_operation_replay_allowed":false}})
    }
    pub fn attach(&self, response: &mut Value) {
        let extra = self.response();
        if let (Some(target), Some(fields)) = (response.as_object_mut(), extra.as_object()) {
            for (key, value) in fields {
                target.insert(key.clone(), value.clone());
            }
        }
    }
    pub fn append(
        progress: &ExecutionProgress,
        report: &mut Option<Self>,
        cause: impl Into<FailureCause>,
    ) {
        let cause = cause.into();
        if let Some(report) = report {
            report.secondary(cause);
            *report.progress = progress.clone();
        } else {
            *report = Some(progress.cause(cause));
        }
    }
    pub fn secondary(&mut self, cause: impl Into<FailureCause>) {
        self.secondary.push(cause.into());
    }
}

/// Inspect only the top-level outcome. Unknown JSON fields are skipped without
/// building a second snapshot/trace Value tree. A successful history query may
/// contain a failed operation; its nested result must not mark the query failed.
pub(crate) fn mark_tool_failure(
    mut response: rmcp::model::CallToolResponse,
) -> rmcp::model::CallToolResponse {
    #[derive(Deserialize)]
    struct Outcome {
        ok: bool,
    }
    if let rmcp::model::CallToolResponse::Complete(result) = &mut response {
        if result.is_error != Some(true)
            && result.content.iter().any(|content| {
                if let rmcp::model::ContentBlock::Text(text) = content {
                    serde_json::from_str::<Outcome>(&text.text).is_ok_and(|outcome| !outcome.ok)
                } else {
                    false
                }
            })
        {
            result.is_error = Some(true);
        }
    }
    response
}
/// A failed mutation can include a locally known submitted prefix. This is
/// diagnostic progress only, never an acknowledgement of server application.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct SubmissionFailure {
    pub cause: FailureCause,
    pub submitted_changes: Option<usize>,
    pub total_changes: usize,
    pub may_have_changed_world: bool,
}
#[cfg(feature = "voxrig")]
#[derive(Default)]
pub(crate) struct SubmissionProgress {
    pub submitted_changes: usize,
    pub may_have_changed_world: bool,
}
pub(crate) fn persistence_failed(
    run: &mut Result<ExecutionProgress, FailureReport>,
    message: String,
) {
    let mut cause = FailureCause::new(CauseKind::Persistence, message);
    cause.phase = Some(FailurePhase::FinalSave);
    match run {
        Ok(progress) => {
            let mut progress = progress.clone();
            progress.phase = FailurePhase::FinalSave;
            progress.persistence = PersistenceOutcome::Uncertain;
            *run = Err(progress.cause(cause));
        }
        Err(report) => {
            report.progress.persistence = PersistenceOutcome::Uncertain;
            report.secondary(cause);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn mcp_error_flag_tracks_top_level_outcome_and_keeps_history_query_success() {
        for (text, expected) in [
            (
                r#"{"ok":false,"failure":{"progress":{"world":"unknown"}}}"#,
                true,
            ),
            (
                r#"{"ok":true,"operation":{"status":"failed","result":{"ok":false}}}"#,
                false,
            ),
        ] {
            let response = rmcp::model::CallToolResponse::Complete(
                rmcp::model::CallToolResult::success(vec![rmcp::model::ContentBlock::text(text)]),
            );
            let rmcp::model::CallToolResponse::Complete(result) = mark_tool_failure(response)
            else {
                panic!("expected complete response")
            };
            assert_eq!(result.is_error == Some(true), expected);
        }
        let response =
            rmcp::model::CallToolResponse::Complete(rmcp::model::CallToolResult::error(vec![
                rmcp::model::ContentBlock::text("invalid tool arguments"),
            ]));
        let rmcp::model::CallToolResponse::Complete(result) = mark_tool_failure(response) else {
            panic!("expected complete response")
        };
        assert_eq!(result.is_error, Some(true));
    }
    #[test]
    fn unavailable_before_readback_requires_observation_even_without_writes() {
        let report = ExecutionProgress {
            phase: FailurePhase::BeforeReadback,
            ..Default::default()
        }
        .cause(FailureCause::new(CauseKind::Timeout, "read timed out"));
        assert_eq!(report.response()["recovery"]["reobserve_required"], true);
        assert_eq!(report.progress.world, WorldOutcome::NotAttempted);
    }
    #[test]
    fn model_budget_retains_resource_and_limits() {
        let cause = FailureCause::from(
            dustroute_translate::piston_construction::ConstructionError::Budget {
                resource:
                    dustroute_translate::piston_construction::ConstructionBudget::StageChanges,
                actual: 33,
                maximum: 32,
            },
        );
        let response = ExecutionProgress {
            phase: FailurePhase::ModelProof,
            ..Default::default()
        }
        .cause(cause)
        .response();
        assert_eq!(response["failure"]["primary"]["kind"], "resource_limit");
        assert_eq!(response["failure"]["primary"]["details"]["actual"], 33);
        assert_eq!(response["failure"]["primary"]["details"]["maximum"], 32);
        assert_eq!(response["failure"]["primary"]["phase"], "model_proof");
        assert_eq!(response["failure"]["progress"]["world"], "not_attempted");
    }
    #[test]
    fn reply_loss_is_unknown_but_explicit_admission_rejection_has_no_writes() {
        let mut progress = ExecutionProgress {
            operation_consumed: true,
            ..Default::default()
        };
        progress.begin_submission();
        let report = progress.submission_error(
            crate::BotBridgeError::Protocol("opaque".into()),
            WorldOutcome::NotAttempted,
        );
        assert_eq!(report.progress.submitted_changes, None);
        assert_eq!(report.progress.world, WorldOutcome::Unknown);
        let mut progress = ExecutionProgress::default();
        progress.begin_submission();
        let error =
            crate::BotBridgeError::Detailed(FailureCause::new(CauseKind::InvalidInput, "rejected"))
                .with_submission(Some(0), 3, false);
        let report = progress.submission_error(error, WorldOutcome::NotAttempted);
        assert_eq!(report.progress.submitted_changes, Some(0));
        assert_eq!(report.progress.world, WorldOutcome::NotAttempted);
    }
    #[test]
    fn final_save_failure_preserves_verified_world_and_an_existing_primary() {
        let progress = ExecutionProgress {
            world: WorldOutcome::Verified,
            verified_steps: 3,
            durable_verified_steps: Some(2),
            ..Default::default()
        };
        let mut run = Ok(progress);
        persistence_failed(&mut run, "store unavailable".into());
        let report = run.as_ref().unwrap_err();
        assert_eq!(report.progress.world, WorldOutcome::Verified);
        assert_eq!(report.progress.durable_verified_steps, Some(2));
        assert_eq!(report.primary.phase, Some(FailurePhase::FinalSave));
        assert_eq!(report.progress.persistence, PersistenceOutcome::Uncertain);
        let mut run = Err(ExecutionProgress::default().cause(
            FailureCause::new(CauseKind::Timeout, "read timeout").at(FailurePhase::AfterReadback),
        ));
        persistence_failed(&mut run, "journal unavailable".into());
        let report = run.unwrap_err();
        assert_eq!(report.primary.kind, CauseKind::Timeout);
        assert_eq!(report.primary.phase, Some(FailurePhase::AfterReadback));
        assert_eq!(report.secondary[0].kind, CauseKind::Persistence);
    }
    #[test]
    fn diagnostic_text_is_not_a_category_and_mismatch_details_are_bounded() {
        assert_eq!(
            FailureCause::from("timeout resource limit unsupported".to_owned()).kind,
            CauseKind::Unknown
        );
        let positions: Vec<_> = (0..100).map(|x| Pos::new(x, 0, 0)).collect();
        let cause = FailureCause::mismatch("different states", &positions);
        assert_eq!(cause.details.actual, Some(100));
        assert_eq!(cause.details.mismatches.len(), 64);
        assert!(cause.details.mismatches_truncated);
    }
    #[test]
    fn submitted_prefix_is_not_world_verification_and_secondary_errors_survive() {
        let mut progress = ExecutionProgress::default();
        progress.begin_submission();
        progress.submitted(8);
        let mut report = progress.cause(FailureCause::new(CauseKind::Timeout, "no reply"));
        report.secondary(FailureCause::new(
            CauseKind::Persistence,
            "journal unavailable",
        ));
        let response = report.response();
        assert_eq!(response["failure"]["progress"]["world"], "unknown");
        assert_eq!(response["failure"]["progress"]["verified_steps"], 0);
        assert_eq!(response["failure"]["secondary"][0]["kind"], "persistence");
        assert_eq!(response["recovery"]["reobserve_required"], true);
        assert_eq!(response["retry_allowed"], false);
    }
}
