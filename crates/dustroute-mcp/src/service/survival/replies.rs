//! Owned survival response data. None is a job, native plan, or replay authority.
//! Only the public tool adapter encodes these records as JSON.
use super::guidance::NextAction;
use super::model::{JobManifest, JobSchema, JobStatus};
use crate::operations::mutation::Success;
use crate::survival_construction::{continuation::SiteDiagnosis, generation::GenerationFailure};
use crate::survival_error::SurvivalErrorCode;
use crate::survival_execution::diagnostic::{DiagnosticOnly, RecordedConstructionPreview};
use crate::survival_execution::{
    Continuation, ExecutionDiagnosis, ExecutionEvent, ExecutionRecord, OperationOutcome,
};
use serde::Serialize;
use std::collections::BTreeMap;
use uuid::Uuid;

#[derive(Clone, Copy, Debug, Serialize)]
#[serde(rename_all = "snake_case")]
pub(super) enum ServiceCode {
    BackendUnavailable,
    CheckpointDimensionChanged,
    CheckpointEndpointChanged,
    CheckpointNotRunning,
    ConfirmationRequired,
    ContinuationPreparationFailed,
    ContinuationSpecificationMissing,
    InvalidCheckpoint,
    InvalidContinuationClaim,
    InvalidRecord,
    InventoryUnavailable,
    JobAlreadyStarted,
    JobCapacity,
    JobNotLive,
    JobUnavailable,
    JournalIo,
    JournalUnavailable,
    NativeRefused,
    ObservationUnavailable,
    PermissionDenied,
    PlanExpiredOrCancelled,
    PlanNotLive,
    PlanningTaskFailed,
    PlayerRequired,
    SourceBusy,
    SourceChanged,
    SourceNotAdoptedOrMismatched,
    SpecificationInvalid,
    GenerationRefused,
    CheckpointSiteChanged,
}
#[derive(Debug, Serialize)]
#[serde(untagged)]
pub(super) enum RefusalCode {
    Service(ServiceCode),
    Survival(SurvivalErrorCode),
}
impl From<ServiceCode> for RefusalCode {
    fn from(c: ServiceCode) -> Self {
        Self::Service(c)
    }
}
impl From<SurvivalErrorCode> for RefusalCode {
    fn from(c: SurvivalErrorCode) -> Self {
        Self::Survival(c)
    }
}
#[derive(Debug, Serialize)]
pub(super) struct Refusal {
    ok: DiagnosticOnly,
    schema_version: JobSchema,
    error: RefusalDetail,
    automatic_replay: DiagnosticOnly,
    next_action: NextAction,
    #[serde(skip_serializing_if = "Option::is_none")]
    continuation_job_id: Option<Uuid>,
    #[serde(skip_serializing_if = "Option::is_none")]
    next_step: Option<&'static str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    historical_diagnosis: Option<Box<HistoricalDiagnosis>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    available_live_status: Option<Box<LiveDisplay>>,
}
#[derive(Debug, Serialize)]
struct RefusalDetail {
    code: RefusalCode,
    detail: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    native_error_kind: Option<crate::failure::NativeFailureKind>,
}
impl Refusal {
    pub(super) fn new(code: impl Into<RefusalCode>, detail: impl std::fmt::Display) -> Self {
        let code = code.into();
        let next_action = NextAction::for_refusal(&code);
        Self {
            ok: DiagnosticOnly,
            schema_version: JobSchema::V1,
            error: RefusalDetail {
                code,
                detail: detail.to_string(),
                native_error_kind: None,
            },
            automatic_replay: DiagnosticOnly,
            next_action,
            continuation_job_id: None,
            next_step: Some(next_action.description()),
            historical_diagnosis: None,
            available_live_status: None,
        }
    }
    pub(super) fn native(code: ServiceCode, error: voxrig::Error) -> Self {
        let kind = error.kind().into();
        let mut refusal = Self::new(code, error);
        refusal.error.native_error_kind = Some(kind);
        refusal
    }
    pub(super) fn linked_job(&mut self, id: Uuid) {
        self.continuation_job_id = Some(id);
        self.next_action = NextAction::InspectLinkedJob;
        self.next_step = Some(self.next_action.description());
    }
    pub(super) fn recorded_diagnosis(&mut self, record: ExecutionRecord) {
        let last_event = record.events.last().cloned();
        self.historical_diagnosis = Some(Box::new(HistoricalDiagnosis {
            execution_id: record.id,
            completed_steps: record.completed_steps,
            outcome: record.outcome,
            recorded_continuation: record.continuation,
            last_event,
            execution_authority_restored: DiagnosticOnly,
        }));
    }
}
#[derive(Debug, Serialize)]
struct HistoricalDiagnosis {
    execution_id: Uuid,
    completed_steps: usize,
    outcome: OperationOutcome,
    recorded_continuation: Continuation,
    last_event: Option<ExecutionEvent>,
    execution_authority_restored: DiagnosticOnly,
}
#[derive(Debug, Serialize)]
#[serde(untagged)]
pub(super) enum Reply {
    Refused(Box<Refusal>),
    Publication(Box<Publication>),
    Live(Box<LiveDisplay>),
    Record(Box<RecordDisplay>),
    Started(Started),
    Control(Control),
    Generation(Box<GenerationRefusal>),
    SiteChanged(Box<SiteChanged>),
    Continued(Box<ContinuedPublication>),
}
impl From<Refusal> for Reply {
    fn from(r: Refusal) -> Self {
        Self::Refused(Box::new(r))
    }
}
impl Reply {
    /// A detailed history read can fail without erasing the authorized live
    /// snapshot. It is diagnostic data, not a substitute for that failed read.
    pub(super) fn with_live_status(mut self, status: Option<LiveDisplay>) -> Self {
        if let Self::Refused(ref mut refusal) = self {
            refusal.available_live_status = status.map(Box::new);
        }
        self
    }
}
impl From<Publication> for Reply {
    fn from(p: Publication) -> Self {
        Self::Publication(Box::new(p))
    }
}
#[derive(Debug, Serialize)]
#[serde(untagged)]
pub(super) enum Publication {
    Published(Box<PublishedPlan>),
    Refused(Refusal),
}
impl From<Refusal> for Publication {
    fn from(r: Refusal) -> Self {
        Self::Refused(r)
    }
}
#[derive(Debug, Serialize)]
pub(super) struct PublishedPlan {
    ok: Success,
    schema_version: JobSchema,
    job_id: Uuid,
    state: PreviewState,
    preview: RecordedConstructionPreview,
    writes_minecraft: DiagnosticOnly,
    inventory_receipt: DiagnosticOnly,
    expires_after_seconds: u64,
    next_step: &'static str,
}
#[derive(Debug, Serialize)]
#[serde(rename_all = "snake_case")]
enum PreviewState {
    Planned,
}
impl PublishedPlan {
    pub(super) fn new(job_id: Uuid, preview: RecordedConstructionPreview) -> Self {
        Self {
            ok: Success,
            schema_version: JobSchema::V1,
            job_id,
            state: PreviewState::Planned,
            preview,
            writes_minecraft: DiagnosticOnly,
            inventory_receipt: DiagnosticOnly,
            expires_after_seconds: 900,
            next_step: "review plan, then action=start with confirmed=true; current inventory/site/source will be rechecked",
        }
    }
}
#[derive(Debug, Serialize)]
pub(super) struct ContinuedPublication {
    #[serde(flatten)]
    publication: Publication,
    parent_job_id: Uuid,
    diagnosis: SiteDiagnosis,
    received_materials: BTreeMap<String, usize>,
    execution_authority_restored: DiagnosticOnly,
}
impl Reply {
    pub(super) fn continued(
        publication: Publication,
        parent_job_id: Uuid,
        diagnosis: SiteDiagnosis,
        received_materials: BTreeMap<String, usize>,
    ) -> Self {
        Self::Continued(Box::new(ContinuedPublication {
            publication,
            parent_job_id,
            diagnosis,
            received_materials,
            execution_authority_restored: DiagnosticOnly,
        }))
    }
    pub(super) fn generation(
        error: GenerationFailure,
        continuation: Option<GenerationContinuation>,
    ) -> Self {
        Self::Generation(Box::new(GenerationRefusal {
            ok: DiagnosticOnly,
            error: GenerationError {
                code: ServiceCode::GenerationRefused,
                cause: error,
            },
            writes_minecraft: DiagnosticOnly,
            continuation,
            next_action: NextAction::ReviewSearchFailure,
            next_step: NextAction::ReviewSearchFailure.description(),
        }))
    }
    pub(super) fn site_changed(diagnosis: SiteDiagnosis) -> Self {
        Self::SiteChanged(Box::new(SiteChanged {
            ok: DiagnosticOnly,
            error: RefusalDetail {
                code: ServiceCode::CheckpointSiteChanged.into(),
                detail: "external differences require inspection; no automatic removal".into(),
                native_error_kind: None,
            },
            diagnosis,
            writes_minecraft: DiagnosticOnly,
            next_action: NextAction::ReviewSiteDifferences,
            next_step: NextAction::ReviewSiteDifferences.description(),
        }))
    }
}
#[derive(Debug, Serialize)]
pub(super) struct GenerationContinuation {
    pub diagnosis: SiteDiagnosis,
    pub received_materials: BTreeMap<String, usize>,
}
#[derive(Debug, Serialize)]
pub(super) struct GenerationRefusal {
    ok: DiagnosticOnly,
    error: GenerationError,
    writes_minecraft: DiagnosticOnly,
    #[serde(flatten)]
    continuation: Option<GenerationContinuation>,
    next_action: NextAction,
    next_step: &'static str,
}
#[derive(Debug, Serialize)]
struct GenerationError {
    code: ServiceCode,
    cause: GenerationFailure,
}
#[derive(Debug, Serialize)]
pub(super) struct SiteChanged {
    ok: DiagnosticOnly,
    error: RefusalDetail,
    diagnosis: SiteDiagnosis,
    writes_minecraft: DiagnosticOnly,
    next_action: NextAction,
    next_step: &'static str,
}

#[derive(Serialize)]
pub(super) struct PublicReply {
    #[serde(flatten)]
    pub response: Reply,
    pub execution_contract: ExecutionContract,
}
#[derive(Serialize)]
pub(super) struct ExecutionContract {
    motion: Motion,
    world_evidence: WorldEvidence,
    independent_observer_required: DiagnosticOnly,
    server_stop_acknowledged: DiagnosticOnly,
    server_position_error_bound: Option<f64>,
}
#[derive(Serialize)]
enum Motion {
    #[serde(rename = "predicted_dry_cube_v1")]
    PredictedDryCubeV1,
}
#[derive(Serialize)]
enum WorldEvidence {
    #[serde(rename = "builder_received")]
    BuilderReceived,
}
impl Default for ExecutionContract {
    fn default() -> Self {
        Self {
            motion: Motion::PredictedDryCubeV1,
            world_evidence: WorldEvidence::BuilderReceived,
            independent_observer_required: DiagnosticOnly,
            server_stop_acknowledged: DiagnosticOnly,
            server_position_error_bound: None,
        }
    }
}
#[derive(Debug, Serialize)]
pub(super) struct LiveDisplay {
    pub ok: Success,
    pub schema_version: JobSchema,
    pub job_id: Uuid,
    pub owner: String,
    pub process_local_job_present: bool,
    pub historical_only: bool,
    pub execution_authority_restored: DiagnosticOnly,
    pub completed_steps: Option<usize>,
    pub status: JobStatus,
    pub next_action: NextAction,
    pub next_step: &'static str,
}
impl LiveDisplay {
    pub(super) fn new(job_id: Uuid, owner: String, status: JobStatus) -> Self {
        let next_action = NextAction::for_status(Some(&status), false);
        Self {
            ok: Success,
            schema_version: JobSchema::V1,
            job_id,
            owner,
            process_local_job_present: true,
            historical_only: false,
            execution_authority_restored: DiagnosticOnly,
            completed_steps: status.completed_steps(),
            status,
            next_action,
            next_step: next_action.description(),
        }
    }
}
#[derive(Debug, Serialize)]
pub(super) struct RecordDisplay {
    pub ok: Success,
    pub schema_version: JobSchema,
    pub job_id: Uuid,
    pub owner: String,
    pub process_local_job_present: bool,
    pub historical_only: bool,
    pub execution_authority_restored: DiagnosticOnly,
    pub completed_steps: Option<usize>,
    pub status: Option<JobStatus>,
    pub recorded_continuation: Option<Continuation>,
    pub next_action: NextAction,
    pub next_step: &'static str,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub manifest: Option<JobManifest>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub diagnosis: Option<Option<ExecutionDiagnosis>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub continuation_job_id: Option<Uuid>,
}
#[derive(Debug, Serialize)]
pub(super) struct Started {
    ok: Success,
    job_id: Uuid,
    state: StartState,
    completed: DiagnosticOnly,
    next_step: &'static str,
}
#[derive(Debug, Serialize)]
#[serde(rename_all = "snake_case")]
enum StartState {
    Admitting,
}
impl Reply {
    pub(super) fn started(job_id: Uuid) -> Self {
        Self::Started(Started {
            ok: Success,
            job_id,
            state: StartState::Admitting,
            completed: DiagnosticOnly,
            next_step: "action=get; admission can still refuse before any edits",
        })
    }
}
#[derive(Debug, Serialize)]
#[serde(untagged)]
pub(super) enum Control {
    Checkpoint(CheckpointRequest),
    Cancel(CancelRequest),
}
#[derive(Debug, Serialize)]
pub(super) struct CheckpointRequest {
    ok: Success,
    job_id: Uuid,
    checkpoint_requested: Success,
    idle_confirmed: DiagnosticOnly,
    next_step: &'static str,
}
#[derive(Debug, Serialize)]
pub(super) struct CancelRequest {
    ok: Success,
    job_id: Uuid,
    cancel_requested: Success,
    immediate_native_abort: DiagnosticOnly,
    next_step: &'static str,
}
impl Reply {
    pub(super) fn checkpoint_requested(job_id: Uuid) -> Self {
        Self::Control(Control::Checkpoint(CheckpointRequest {
            ok: Success,
            job_id,
            checkpoint_requested: Success,
            idle_confirmed: DiagnosticOnly,
            next_step: "get until checkpointed; pending mining must settle first",
        }))
    }
    pub(super) fn cancel_requested(job_id: Uuid) -> Self {
        Self::Control(Control::Cancel(CancelRequest {
            ok: Success,
            job_id,
            cancel_requested: Success,
            immediate_native_abort: DiagnosticOnly,
            next_step: "get job; outstanding operations may still require inspection",
        }))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::{Value, json};

    fn diagnosis() -> SiteDiagnosis {
        SiteDiagnosis {
            completed_targets: vec![],
            unbuilt_targets: vec![[1, 2, 3]],
            remaining_owned_temporary: vec![],
            conflicts: vec![],
            ownership_is_conditional: true,
        }
    }
    #[test]
    fn publication_preview_matches_native_generated_wire_without_native_authority() {
        let generated = crate::survival_construction::tests::generated_wire_fixture();
        let expected = serde_json::to_value(&generated).unwrap();
        let recorded = RecordedConstructionPreview::from(&generated);
        assert_eq!(serde_json::to_value(&recorded).unwrap(), expected);
        let job_id = Uuid::new_v4();
        let reply = PublishedPlan::new(job_id, recorded);
        assert_eq!(
            serde_json::to_value(reply).unwrap(),
            json!({"ok":true,"schema_version":"dustroute.survival-job.v1","job_id":job_id,
            "state":"planned","preview":expected,"writes_minecraft":false,"inventory_receipt":false,
            "expires_after_seconds":900,"next_step":"review plan, then action=start with confirmed=true; current inventory/site/source will be rechecked"})
        );
    }
    #[test]
    fn continuation_keeps_diagnosis_even_when_publication_is_refused() {
        let parent = Uuid::new_v4();
        let expected_diagnosis = serde_json::to_value(diagnosis()).unwrap();
        let materials = BTreeMap::from([("minecraft:dirt".into(), 0)]);
        let reply = Reply::continued(
            Refusal::new(ServiceCode::JobCapacity, "at most 32 process-local jobs").into(),
            parent,
            diagnosis(),
            materials.clone(),
        );
        assert_eq!(
            serde_json::to_value(reply).unwrap(),
            json!({"ok":false,"schema_version":"dustroute.survival-job.v1",
            "error":{"code":"job_capacity","detail":"at most 32 process-local jobs"},"automatic_replay":false,
            "next_action":"inspect_refusal","next_step":"inspect error.code and its details; resolve the prerequisite before requesting a new plan or action",
            "parent_job_id":parent,"diagnosis":expected_diagnosis,"received_materials":materials,"execution_authority_restored":false})
        );
        let error = GenerationFailure::PlanningTaskFailed {
            reason: "worker unavailable".into(),
        };
        let expected = json!({"ok":false,"error":{"code":"generation_refused","cause":&error},"diagnosis":expected_diagnosis,"received_materials":materials,"writes_minecraft":false,
            "next_action":"review_search_failure","next_step":"inspect error.cause for input, material or search-budget limits; no complete plan was accepted and search exhaustion does not prove impossibility"});
        let reply = Reply::generation(
            error,
            Some(GenerationContinuation {
                diagnosis: diagnosis(),
                received_materials: materials,
            }),
        );
        assert_eq!(serde_json::to_value(reply).unwrap(), expected);
        let reply = Reply::generation(
            GenerationFailure::PlanningTaskFailed {
                reason: "worker unavailable".into(),
            },
            None,
        );
        let wire = serde_json::to_value(reply).unwrap();
        assert!(wire.get("diagnosis").is_none());
        assert!(wire.get("schema_version").is_none());
        assert!(wire.get("automatic_replay").is_none());
    }
    #[test]
    fn generation_refusals_preserve_resource_and_search_evidence_without_a_job() {
        use crate::survival_construction::generation::{
            ConstructionSearch, SearchLimits, SearchStop,
        };
        let limits = SearchLimits {
            candidate_checks: 1,
            ..Default::default()
        };
        let failures = [
            GenerationFailure::InsufficientMaterials {
                missing: BTreeMap::from([("minecraft:stone".into(), 3)]),
            },
            GenerationFailure::InvalidInput {
                error: crate::survival_construction::ConstructionPlanningError {
                    code: SurvivalErrorCode::InvalidSearchLimits,
                    action: None,
                    position: None,
                    detail: "candidate checks must be positive".into(),
                    missing_materials: BTreeMap::new(),
                },
            },
            GenerationFailure::NoCompletePlanWithinLimits {
                reason: SearchStop::CandidateBudget,
                limits,
                search: Box::new(ConstructionSearch {
                    candidate_checks: 1,
                    best_remaining_permanent: 3,
                    best_remaining_targets: vec![[1, 2, 3]],
                    ..Default::default()
                }),
            },
        ];
        for failure in failures {
            let result = crate::service::typed_reply(Reply::generation(failure, None));
            assert_eq!(result.is_error, Some(true));
            let wire = crate::service::test_support::decode_reply(&result).unwrap();
            assert_eq!(wire["ok"], false);
            assert_eq!(wire["writes_minecraft"], false);
            assert_eq!(wire["next_action"], "review_search_failure");
            assert!(wire.get("job_id").is_none());
            assert!(wire.get("generated").is_none());
            let cause = &wire["error"]["cause"];
            match cause["kind"].as_str().unwrap() {
                "insufficient_materials" => assert_eq!(cause["missing"]["minecraft:stone"], 3),
                "invalid_input" => assert_eq!(cause["error"]["code"], "invalid_search_limits"),
                "no_complete_plan_within_limits" => {
                    assert_eq!(cause["reason"], "candidate_budget");
                    assert_eq!(cause["limits"]["candidate_checks"], 1);
                    assert_eq!(cause["limits"]["actions"], limits.actions);
                    assert_eq!(cause["search"]["candidate_checks"], 1);
                    assert_eq!(
                        cause["search"]["best_remaining_targets"],
                        serde_json::json!([[1, 2, 3]])
                    );
                }
                unexpected => panic!("unexpected cause: {unexpected}"),
            }
        }
    }

    #[test]
    fn refusal_keeps_linked_claim_and_historical_facts_without_restoring_a_job() {
        let new_job = Uuid::new_v4();
        let execution_id = Uuid::new_v4();
        let mut refusal = Refusal::new(SurvivalErrorCode::CheckpointConsumed, "already claimed");
        let plain = serde_json::to_value(&refusal).unwrap();
        assert_eq!(
            plain,
            json!({"ok":false,"schema_version":"dustroute.survival-job.v1","error":{"code":"checkpoint_consumed","detail":"already claimed"},"automatic_replay":false,
                "next_action":"inspect_execution","next_step":"inspect saved execution and outstanding native operations, then reobserve when permitted; no automatic replay or continuation without a sealed checkpoint"})
        );
        refusal.linked_job(new_job);
        refusal.recorded_diagnosis(ExecutionRecord {
            schema: crate::survival_execution::JournalSchema::V3,
            id: execution_id,
            plan: Default::default(),
            completed_steps: 0,
            outcome: OperationOutcome::Uncertain,
            continuation: Continuation::NeedsInspection,
            reconnects: 0,
            events: vec![],
        });
        let expected = json!({"ok":false,"schema_version":"dustroute.survival-job.v1","error":{"code":"checkpoint_consumed","detail":"already claimed"},"automatic_replay":false,
            "next_action":"inspect_linked_job",
            "continuation_job_id":new_job,"next_step":"get the linked new job; do not replay the old checkpoint",
            "historical_diagnosis":{"execution_id":execution_id,"completed_steps":0,"outcome":"uncertain","recorded_continuation":"needs_inspection","last_event":null,"execution_authority_restored":false}});
        assert_eq!(serde_json::to_value(&refusal).unwrap(), expected);
        let public = super::PublicReply {
            response: refusal.into(),
            execution_contract: Default::default(),
        };
        let actual = super::super::super::test_support::decode_reply(
            &super::super::super::typed_reply(public),
        )
        .unwrap();
        let mut legacy = expected;
        legacy["execution_contract"] = json!({"motion":"predicted_dry_cube_v1","world_evidence":"builder_received","independent_observer_required":false,"server_stop_acknowledged":false,"server_position_error_bound":null});
        let expected = super::super::super::test_support::decode_reply(
            &super::super::super::json_reply(legacy),
        )
        .unwrap();
        assert_eq!(actual, expected);
    }

    #[test]
    fn control_requests_and_execution_contract_do_not_claim_completion_or_server_ack() {
        let id = Uuid::new_v4();
        assert_eq!(
            serde_json::to_value(Reply::started(id)).unwrap(),
            json!({"ok":true,"job_id":id,"state":"admitting","completed":false,"next_step":"action=get; admission can still refuse before any edits"})
        );
        assert_eq!(
            serde_json::to_value(Reply::checkpoint_requested(id)).unwrap(),
            json!({"ok":true,"job_id":id,"checkpoint_requested":true,"idle_confirmed":false,"next_step":"get until checkpointed; pending mining must settle first"})
        );
        assert_eq!(
            serde_json::to_value(Reply::cancel_requested(id)).unwrap(),
            json!({"ok":true,"job_id":id,"cancel_requested":true,"immediate_native_abort":false,"next_step":"get job; outstanding operations may still require inspection"})
        );
        assert_eq!(
            serde_json::to_value(ExecutionContract::default()).unwrap(),
            json!({"motion":"predicted_dry_cube_v1","world_evidence":"builder_received","independent_observer_required":false,"server_stop_acknowledged":false,"server_position_error_bound":null})
        );
    }
    #[test]
    fn live_and_saved_display_distinguish_unknown_zero_and_omitted_details() {
        let id = Uuid::new_v4();
        let live = LiveDisplay::new(id, "Tester".into(), JobStatus::Planned);
        let wire = serde_json::to_value(live).unwrap();
        assert!(wire["completed_steps"].is_null());
        assert!(wire.get("recorded_continuation").is_none());
        assert!(wire.get("diagnosis").is_none());
        assert_eq!(wire["next_action"], "review_preview");
        let mut saved = RecordDisplay {
            ok: Success,
            schema_version: JobSchema::V1,
            job_id: id,
            owner: "Tester".into(),
            process_local_job_present: false,
            historical_only: true,
            execution_authority_restored: DiagnosticOnly,
            completed_steps: Some(0),
            status: None,
            recorded_continuation: Some(Continuation::NeedsInspection),
            next_action: NextAction::InspectExecution,
            next_step: "historical diagnosis only; reobserve and resolve outstanding operations before any new plan",
            manifest: None,
            diagnosis: None,
            continuation_job_id: None,
        };
        let wire = serde_json::to_value(&saved).unwrap();
        assert_eq!(wire["completed_steps"], 0);
        assert!(wire["status"].is_null());
        assert!(wire.get("diagnosis").is_none());
        saved.diagnosis = Some(None);
        assert_eq!(
            serde_json::to_value(saved).unwrap().get("diagnosis"),
            Some(&Value::Null)
        );
    }

    #[test]
    fn native_refusal_keeps_the_received_category_and_the_failed_mcp_envelope() {
        let refusal = Refusal::native(
            ServiceCode::ObservationUnavailable,
            voxrig::Error::new(
                voxrig::ErrorKind::UncertainDispatch,
                anyhow::anyhow!("opaque native detail"),
            ),
        );
        let reply = super::super::super::typed_reply(PublicReply {
            response: refusal.into(),
            execution_contract: Default::default(),
        });
        assert_eq!(reply.is_error, Some(true));
        let wire = super::super::super::test_support::decode_reply(&reply).unwrap();
        assert_eq!(wire["ok"], false);
        assert_eq!(wire["error"]["code"], "observation_unavailable");
        assert_eq!(wire["error"]["native_error_kind"], "UncertainDispatch");
        assert_eq!(wire["automatic_replay"], false);
        assert!(wire.get("available_live_status").is_none());
    }
}
