//! Existing-world modifications are fresh capabilities, separate from adoption
//! and from durable historical records. All live steps use the shared executor.
mod presentation;
mod retention;
use super::*;
use crate::assembly_registry::{TargetServer, now_ms};
use crate::construction_jobs::{JobAttempt, JobRegistry, JobStageBinding, JobState};
use crate::edit_registry::{EditAttempt, EditRecord, EditRegistry, EditState, SCHEMA};
use crate::performance::{Phase, current, span};
use crate::piston_assembly::ValidatedAssemblyPlacement;
use dustroute_translate::piston_construction::ElectricalModification;
use dustroute_translate::snapshot::MinecraftSnapshot;
pub(super) use presentation::state_summary;

pub(super) struct EditOrigin {
    pub revision_id: uuid::Uuid,
    pub source: Value,
    pub job: Option<JobStageBinding>,
}

#[derive(Clone, Debug)]
pub(super) struct ElectricalEditPlan {
    player: String,
    revision_id: uuid::Uuid,
    source: Value,
    target: TargetServer,
    proof: std::sync::Arc<ElectricalModification>,
    previewed: bool,
    state: PistonPlacementState,
    expires_at: Instant,
    pub(super) job: Option<JobStageBinding>,
    retained_records: usize,
    retained_bytes: usize,
}
impl ElectricalEditPlan {
    fn preview(&self, id: uuid::Uuid, read_only: bool) -> Value {
        let mut result = json!({"schema_version":"dustroute.electrical-edit-preview.v2",
            "ok":true,"operation_id":id,"kind":"electrical_revision_modification",
            "source":self.source,"revision_id":self.revision_id,"read_only":read_only,"job_stage":self.job,
            "bounds":{"min":self.proof.before().min,"max":self.proof.before().max},
            "edit_scope":self.proof.scope(),
            "execution_batches":construction_executor::batch_summary(self.proof.steps(false)),
            "undo_execution_batches":construction_executor::batch_summary(self.proof.steps(true)),
            "conditions":{"stationary_observation_required":true,"model_initial_queue":"assumed_empty",
                "runtime_history_reconstructed":false,"functional_behavior_verified":false,
                "protected_state_checked":"every modeled committed microstep, forward and undo; live full-region readback at batch boundaries",
                "outside_editable":"all other observed cells are protected; no ownership inferred",
                "fixed_environment":"enclosed source water only; source or containment changes are unsupported",
                "natural_growth":"not modeled; live state drift stops execution",
                "operator_requirement":"finish prior motion and keep external inputs/edits out of the work region"},
            "validation_scope":"complete declared state and per-command physics; live readback at batch boundaries; no flying/harvest contract implied",
            "next_step":"show_operation then confirm invoke_operation; no automatic retry/rollback"});
        result.as_object_mut().expect("preview object").extend(
            presentation::states(&self.proof)
                .as_object()
                .expect("state object")
                .clone(),
        );
        result
    }
}

impl DustRouteMcp {
    pub(super) fn electrical_edit_history(&self, id: uuid::Uuid) -> Result<Option<Value>, String> {
        if !self
            .state_store
            .edit_record_root()
            .join(format!("{id}.json"))
            .exists()
        {
            return Ok(None);
        }
        let player = self.resolve_player(None)?;
        self.policy
            .authorize_player(&player)
            .map_err(|e| e.to_string())?;
        let registry = EditRegistry::acquire(&self.state_store)?;
        Ok(registry.load(id, &player)?.map(|record| {
            json!({"ok":true,"operation_id":id,
            "kind":"electrical_revision_modification","record":record,"historical":true,
            "saved_record_is_validation_proof":false,"executable_plan_restored":false})
        }))
    }

    pub(super) async fn electrical_edit_view(&self, id: uuid::Uuid) -> Result<Value, String> {
        Ok(self
            .owned_edit(id, None)
            .await?
            .preview(id, self.policy.read_only))
    }

    pub(super) async fn plan_electrical_edit(
        &self,
        params: &PreviewPlacementParams,
        before: MinecraftSnapshot,
        after: MinecraftSnapshot,
        status: crate::bridge::BotStatus,
        origin: EditOrigin,
    ) -> Result<Value, String> {
        let player = self.resolve_player(params.player.as_deref())?;
        self.policy
            .authorize_player(&player)
            .map_err(|e| e.to_string())?;
        let dimension = status
            .dimension
            .as_deref()
            .ok_or("server dimension unavailable")?;
        assembly_placement::server_contract(&status, dimension)?;
        let scope = params.edit_scope.clone().unwrap_or_else(|| {
            dustroute_library::world_edit::WorldEditScope::entire(
                dustroute_translate::world::Region::new(before.min, before.max),
            )
        });
        let capture = current();
        let queued_at = std::time::Instant::now();
        let proof = tokio::task::spawn_blocking(move || {
            capture.record_queue(queued_at);
            capture.in_blocking(|| {
                let _measurement = span(Phase::ModelProof);
                ElectricalModification::new_scoped(&before, &after, scope, Default::default())
            })
        })
        .await
        .map_err(|e| e.to_string())??;
        self.register_electrical_edit(params, status, origin, proof)
            .await
    }

    pub(super) async fn register_electrical_edit(
        &self,
        params: &PreviewPlacementParams,
        status: crate::bridge::BotStatus,
        origin: EditOrigin,
        proof: ElectricalModification,
    ) -> Result<Value, String> {
        let player = self.resolve_player(params.player.as_deref())?;
        self.policy
            .authorize_player(&player)
            .map_err(|e| e.to_string())?;
        let dimension = status
            .dimension
            .as_deref()
            .ok_or("server dimension unavailable")?;
        assembly_placement::server_contract(&status, dimension)?;
        let target = TargetServer::observed(&status, dimension)?;
        let baseline = proof.before();
        let size = proof.steps(false).len().max(proof.steps(true).len());
        self.policy
            .validate_placement_size(size)
            .map_err(|e| e.to_string())?;
        if params.max_blocks.is_some_and(|n| size > n) {
            return Err("electrical edit exceeds max_blocks".into());
        }
        self.check_stationary_edit_baseline(&target, baseline)
            .await?;
        let retained_records = retention::records(&proof);
        let retained_bytes = retention::estimated_bytes(&proof);
        if retained_records > retention::MAX_RETAINED_BLOCK_RECORDS
            || retained_bytes > retention::MAX_RETAINED_MODEL_BYTES
        {
            return Err(
                "current electrical proof exceeds its retention budget; use smaller work regions"
                    .into(),
            );
        }
        let plan = ElectricalEditPlan {
            retained_records,
            retained_bytes,
            player,
            revision_id: origin.revision_id,
            source: origin.source,
            target,
            proof: std::sync::Arc::new(proof),
            previewed: false,
            state: PistonPlacementState::Planned,
            expires_at: Instant::now() + Duration::from_secs(300),
            job: origin.job,
        };
        let id = uuid::Uuid::new_v4();
        let response = plan.preview(id, self.policy.read_only);
        let mut plans = self.plans.table::<ElectricalEditPlan>().lock().await;
        plans.retain(|_, p| {
            p.state != PistonPlacementState::Planned || p.expires_at > Instant::now()
        });
        let same_job = |p: &ElectricalEditPlan| {
            origin
                .job
                .is_some_and(|binding| p.job.is_some_and(|old| old.job_id == binding.job_id))
        };
        let retained = plans
            .iter()
            .map(|(_, p)| p)
            .filter(|p| !same_job(p))
            .map(|p| p.retained_records)
            .sum::<usize>();
        let retained_bytes = plans
            .iter()
            .map(|(_, p)| p)
            .filter(|p| !same_job(p))
            .map(|p| p.retained_bytes)
            .sum::<usize>();
        let other_plans = plans.iter().filter(|(_, p)| !same_job(p)).count();
        if other_plans >= 256
            || retained + plan.retained_records > retention::MAX_RETAINED_BLOCK_RECORDS
            || retained_bytes + plan.retained_bytes > retention::MAX_RETAINED_MODEL_BYTES
        {
            return Err("retained electrical proof budget exhausted; finish or cancel pending jobs before planning more work".into());
        }
        let superseded = plans
            .iter()
            .filter_map(|(id, p)| same_job(p).then_some(*id))
            .collect::<Vec<_>>();
        plans.retain(|_, p| !same_job(p));
        plans.insert(id, plan);
        drop(plans);
        for old in superseded {
            self.operations.record_completed(old,OperationKind::PlacementPreview,
                json!({"ok":false,"status":"job_stage_capability_discarded","next_step":"freshly plan the job stage"})).await;
        }
        self.operations
            .record_completed(id, OperationKind::PlacementPreview, response.clone())
            .await;
        Ok(response)
    }

    async fn owned_edit(
        &self,
        id: uuid::Uuid,
        player: Option<&str>,
    ) -> Result<ElectricalEditPlan, FailureCause> {
        let player = self.resolve_player(player)?;
        self.policy
            .authorize_player(&player)
            .map_err(FailureCause::from)?;
        let plan = self
            .plans
            .table::<ElectricalEditPlan>()
            .lock()
            .await
            .get(&id)
            .cloned()
            .ok_or_else(|| {
                FailureCause::new(
                    CauseKind::NotFound,
                    "electrical edit plan unavailable; recapture/replan after restart",
                )
            })?;
        if plan.player != player {
            return Err(FailureCause::new(
                CauseKind::PermissionDenied,
                "electrical edit belongs to another player",
            ));
        }
        self.policy
            .authorize_dimension(&plan.target.dimension)
            .map_err(FailureCause::from)?;
        self.policy
            .validate_region(dustroute_translate::world_reverse::RegionBounds::new(
                plan.proof.before().min,
                plan.proof.before().max,
            ))
            .map_err(FailureCause::from)?;
        if let Some(binding) = plan.job {
            let registry = JobRegistry::acquire(&self.state_store)?;
            let job = registry.load(binding.job_id, &player)?;
            job.check_binding(id, binding)?;
            if job.target != plan.target || job.source_revision_id != plan.revision_id {
                return Err("job target/source changed".into());
            }
        }
        Ok(plan)
    }

    pub(super) async fn check_stationary_edit_baseline(
        &self,
        target: &TargetServer,
        expected: &MinecraftSnapshot,
    ) -> Result<Vec<crate::observation_evidence::ObservationEvidence>, FailureCause> {
        let mut receipts = Vec::new();
        for index in 0..2 {
            let status = self.bridge.status().await.map_err(FailureCause::from)?;
            assembly_placement::server_contract(&status, &target.dimension)?;
            target.check(&status)?;
            let observed = self
                .bridge
                .scan_region_fresh(expected.min, expected.max, &target.dimension)
                .await
                .map_err(FailureCause::from)?
                .into_stationary_record()?;
            ValidatedAssemblyPlacement::matches(&observed.snapshot, expected, &status.version)
                .map_err(|e| FailureCause::new(CauseKind::VerificationMismatch, e))?;
            if let Some(first) = receipts.first() {
                observed.readback.interval_since(first)?;
            }
            receipts.push(observed.readback);
            if index == 0 {
                self.bridge
                    .wait_ticks(20, &target.dimension)
                    .await
                    .map_err(FailureCause::from)?;
            }
        }
        let status = self.bridge.status().await.map_err(FailureCause::from)?;
        target.check(&status)?;
        Ok(receipts)
    }

    pub(super) async fn show_electrical_edit(&self, id: uuid::Uuid, player: Option<&str>) -> Value {
        let result: Result<Value, FailureCause> = async {
            let plan = self.owned_edit(id, player).await?;
            if plan.state != PistonPlacementState::Planned || plan.expires_at <= Instant::now() {
                return Err(FailureCause::new(
                    CauseKind::InvalidState,
                    "electrical edit expired or consumed; inspect and replan",
                ));
            }
            self.check_stationary_edit_baseline(&plan.target, plan.proof.before())
                .await?;
            let preview = self
                .bridge
                .preview_region(
                    &plan.player,
                    plan.proof.before().min,
                    plan.proof.before().max,
                    &plan.target.dimension,
                )
                .await
                .map_err(FailureCause::from)?;
            if plan.expires_at <= Instant::now() {
                return Err(FailureCause::new(
                    CauseKind::InvalidState,
                    "electrical edit expired during preview",
                ));
            }
            self.plans
                .table::<ElectricalEditPlan>()
                .lock()
                .await
                .get_mut(&id)
                .ok_or("edit unavailable")?
                .previewed = true;
            let mut response = plan.preview(id, self.policy.read_only);
            response["preview"] = json!(preview);
            Ok(response)
        }
        .await;
        result.unwrap_or_else(|error| json!({"ok":false,"error":error}))
    }

    pub(super) async fn mutate_electrical_edit(
        &self,
        id: uuid::Uuid,
        confirm: bool,
        undo: bool,
    ) -> Value {
        let mut progress = ExecutionProgress::default();
        self.execute_electrical_edit(id, confirm, undo, &mut progress)
            .await
            .unwrap_or_else(|error| progress.cause(error).response())
    }

    async fn execute_electrical_edit(
        &self,
        id: uuid::Uuid,
        confirm: bool,
        undo: bool,
        progress: &mut ExecutionProgress,
    ) -> Result<Value, FailureCause> {
        if !confirm {
            return Err(FailureCause::new(
                CauseKind::InvalidInput,
                "confirm=true is required",
            ));
        }
        self.policy
            .authorize_mutation()
            .map_err(FailureCause::from)?;
        let queue_measurement = span(Phase::MutationQueue);
        let _guard = self.mutation_lock.lock().await;
        drop(queue_measurement);
        let plan = self.owned_edit(id, None).await?;
        if undo && plan.job.is_some() {
            return Err(
                "job undo requires manage_construction_job(action=plan_undo) and a fresh preview"
                    .into(),
            );
        }
        if (undo && plan.state != PistonPlacementState::Applied)
            || (!undo
                && (plan.state != PistonPlacementState::Planned
                    || !plan.previewed
                    || plan.expires_at <= Instant::now()))
        {
            return Err(
                "edit requires unused unexpired preview; undo requires verified application".into(),
            );
        }
        progress.phase = FailurePhase::ModelProof;
        crate::performance::execution_progress(progress);
        let saved_proof = plan.proof.clone();
        let capture = current();
        let queued_at = std::time::Instant::now();
        let proof = tokio::task::spawn_blocking(move || {
            capture.record_queue(queued_at);
            capture.in_blocking(|| {
                let _measurement = span(Phase::ModelProof);
                saved_proof.reprove(Default::default())
            })
        })
        .await
        .map_err(|e| FailureCause::new(CauseKind::Unknown, e.to_string()))??;
        if proof.steps(undo) != plan.proof.steps(undo) {
            return Err("fresh edit steps differ from the preview; replan".into());
        }
        let steps = proof.steps(undo);
        self.policy
            .validate_placement_size(steps.len())
            .map_err(FailureCause::from)?;
        let registry = EditRegistry::acquire(&self.state_store)?;
        let profile = dustroute_translate::world::time::piston_runtime::ELECTRICAL_PROFILE;
        let mut record = if undo {
            let record = registry
                .load(id, &plan.player)?
                .ok_or("verified edit history missing")?;
            if record.state != EditState::Applied
                || record.execution_profile != profile
                || record.target != plan.target
                || record.source_revision_id != plan.revision_id
                || record.before != *proof.before()
                || record.after != *proof.after()
                || record.edit_scope.as_ref() != Some(proof.scope())
                || record.job_stage != plan.job
            {
                return Err("edit record differs from the undo plan".into());
            }
            record
        } else {
            if registry.load(id, &plan.player)?.is_some() {
                return Err("edit attempt already recorded; inspect and replan".into());
            }
            EditRecord {
                schema: SCHEMA.into(),
                execution_profile: profile.into(),
                operation_id: id,
                player: plan.player.clone(),
                source_revision_id: plan.revision_id,
                target: plan.target.clone(),
                before: proof.before().clone(),
                after: proof.after().clone(),
                edit_scope: Some(proof.scope().clone()),
                job_stage: plan.job,
                state: EditState::NeedsInspection,
                attempts: vec![],
            }
        };
        let baseline = if undo { proof.after() } else { proof.before() };
        progress.phase = FailurePhase::BeforeReadback;
        crate::performance::execution_progress(progress);
        let readbacks = self
            .check_stationary_edit_baseline(&plan.target, baseline)
            .await?;
        if !undo && plan.expires_at <= Instant::now() {
            return Err("edit expired during validation".into());
        }
        progress.phase = FailurePhase::IntentSave;
        crate::performance::execution_progress(progress);
        // Journal uncertain intent before writes. Loading it restores history,
        // never authority to replay a previous executable operation.
        let job_registry = plan
            .job
            .map(|_| JobRegistry::acquire(&self.state_store))
            .transpose()?;
        let mut job_record =
            if let (Some(binding), Some(registry)) = (plan.job, job_registry.as_ref()) {
                let mut job = registry.load(binding.job_id, &plan.player)?;
                self.check_job_stage(&job, id, binding, &proof)?;
                job.state = JobState::NeedsInspection;
                job.attempts.push(JobAttempt {
                    operation_id: id,
                    region_index: binding.region_index,
                    undo: binding.undo,
                    verified: false,
                    error: None,
                    failure: None,
                });
                JobRegistry::ensure_completion_fits(&job, binding, &proof)?;
                registry.save(&job).map_err(|e| {
                    progress.persistence = PersistenceOutcome::Uncertain;
                    FailureCause::new(CauseKind::Persistence, e)
                })?;
                Some(job)
            } else {
                None
            };
        self.plans
            .table::<ElectricalEditPlan>()
            .lock()
            .await
            .get_mut(&id)
            .ok_or("edit unavailable")?
            .state = PistonPlacementState::NeedsInspection;
        progress.operation_consumed = true;
        record.state = EditState::NeedsInspection;
        record.attempts.push(EditAttempt {
            undo,
            verified_steps: 0,
            total_steps: steps.len(),
            started_at_unix_ms: now_ms()?,
            finished_at_unix_ms: None,
            error: None,
            readbacks,
            failure: None,
            progress: Some(progress.clone()),
        });
        registry.save(&record).map_err(|e| {
            progress.persistence = PersistenceOutcome::Uncertain;
            FailureCause::new(CauseKind::Persistence, e)
        })?;
        progress.persistence = PersistenceOutcome::IntentSaved;
        let mut completed = 0;
        let mut run = construction_executor::ConstructionExecutor {
            bridge: &self.bridge,
            policy: &self.policy,
            target: &plan.target,
        }
        .execute(baseline, steps, |progress| {
            let attempt = record.attempts.last_mut().ok_or("missing edit attempt")?;
            match progress {
                construction_executor::StageProgress::Readback(receipt) => {
                    attempt.readbacks.push(*receipt)
                }
                construction_executor::StageProgress::WriteIntent(intent) => {
                    attempt.progress = Some(intent)
                }
                construction_executor::StageProgress::Verified(count) => {
                    completed = count;
                    attempt.verified_steps = count;
                }
            }
            registry
                .save(&record)
                .map_err(|e| FailureCause::new(CauseKind::Persistence, e))
        })
        .await;
        *progress = match &run {
            Ok(p) => p.clone(),
            Err(report) => (*report.progress).clone(),
        };
        record.state = if run.is_ok() {
            if undo {
                EditState::Undone
            } else {
                EditState::Applied
            }
        } else {
            EditState::NeedsInspection
        };
        let attempt = record.attempts.last_mut().ok_or("missing edit attempt")?;
        attempt.finished_at_unix_ms = Some(now_ms()?);
        attempt.error = run.as_ref().err().map(ToString::to_string);
        attempt.failure = run.as_ref().err().cloned();
        attempt.progress = Some(progress.clone());
        if let Err(error) = registry.save(&record) {
            crate::failure::persistence_failed(&mut run, error);
        }
        if let (Some(binding), Some(job), Some(registry)) =
            (plan.job, job_record.as_mut(), job_registry.as_ref())
        {
            let attempt = job.attempts.last_mut().ok_or("missing job attempt")?;
            attempt.verified = run.is_ok();
            attempt.error = run.as_ref().err().map(ToString::to_string);
            attempt.failure = run.as_ref().err().cloned();
            if run.is_ok() {
                if binding.undo {
                    job.boundaries.truncate(binding.region_index);
                    job.completed_regions = binding.region_index;
                } else {
                    job.boundaries.push(
                        dustroute_translate::piston_construction::ElectricalBoundary::between(
                            proof.before(),
                            proof.after(),
                        )?,
                    );
                    job.completed_regions = binding.region_index + 1;
                }
                job.active_operation_id = None;
                job.state = if job.forward_cancelled {
                    JobState::Cancelled
                } else if job.completed_regions == job.regions.len() {
                    JobState::Completed
                } else {
                    JobState::Ready
                };
            }
            if let Err(error) = registry.save(job) {
                crate::failure::persistence_failed(&mut run, error);
            }
        }
        if let Ok(result) = &mut run {
            result.phase = FailurePhase::FinalSave;
            result.persistence = PersistenceOutcome::FinalSaved;
        }
        *progress = match &run {
            Ok(p) => p.clone(),
            Err(report) => (*report.progress).clone(),
        };
        if run.is_ok()
            && let Some(binding) = plan.job
        {
            self.discard_job_stage_plans(binding.job_id, None).await;
        }
        if run.is_ok() && plan.job.is_none() {
            self.plans
                .table::<ElectricalEditPlan>()
                .lock()
                .await
                .get_mut(&id)
                .ok_or("edit unavailable")?
                .state = if undo {
                PistonPlacementState::Undone
            } else {
                PistonPlacementState::Applied
            };
        }
        let mut response = json!({"ok":run.is_ok(),"operation_id":id,"kind":"electrical_revision_modification","undo":undo,"job_stage":plan.job,
            "status":if run.is_ok(){"verified"}else{"needs_inspection"},"verified_steps":completed,"total_steps":steps.len(),
            "error":run.as_ref().err().map(ToString::to_string),"retry_allowed":false,"automatic_rollback":false,"assembly_adoption_modified":false});
        if let Err(report) = &run {
            report.attach(&mut response);
        } else {
            response["execution_progress"] = json!(progress);
        }
        self.operations
            .record_completed(
                id,
                if undo {
                    OperationKind::PlacementUndo
                } else {
                    OperationKind::PlacementApply
                },
                response.clone(),
            )
            .await;
        Ok(response)
    }
}
