//! Existing-world modifications are fresh capabilities, separate from adoption
//! and from durable historical records. All live steps use the shared executor.
use super::*;
use crate::assembly_registry::{TargetServer, now_ms};
use crate::edit_registry::{EditAttempt, EditRecord, EditRegistry, EditState, SCHEMA};
use crate::piston_assembly::ValidatedAssemblyPlacement;
use dustroute_translate::piston_construction::ElectricalModification;
use dustroute_translate::snapshot::MinecraftSnapshot;

#[derive(Clone, Debug)]
pub(super) struct ElectricalEditPlan {
    player: String,
    revision_id: uuid::Uuid,
    source: Value,
    target: TargetServer,
    proof: ElectricalModification,
    previewed: bool,
    state: PistonPlacementState,
    expires_at: Instant,
}
impl ElectricalEditPlan {
    fn preview(&self, id: uuid::Uuid, read_only: bool) -> Value {
        json!({"ok":true,"operation_id":id,"kind":"electrical_revision_modification",
            "source":self.source,"revision_id":self.revision_id,"read_only":read_only,
            "bounds":{"min":self.proof.before().min,"max":self.proof.before().max},
            "before":self.proof.before(),"after":self.proof.after(),
            "steps":self.proof.steps(false),"undo_steps":self.proof.steps(true),
            "conditions":{"stationary_observation_required":true,"model_initial_queue":"assumed_empty",
                "runtime_history_reconstructed":false,"functional_behavior_verified":false,
                "fixed_environment":"enclosed source water only; source or containment changes are unsupported",
                "natural_growth":"not modeled; live state drift stops execution",
                "operator_requirement":"finish prior motion and keep external inputs/edits out of the work region"},
            "validation_scope":"complete declared state and per-command physics; no flying/harvest contract implied",
            "next_step":"show_operation then confirm invoke_operation; no automatic retry/rollback"})
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
        revision: &crate::revision::CircuitRevision,
        before: MinecraftSnapshot,
        after: MinecraftSnapshot,
        status: crate::bridge::BotStatus,
        source: Value,
    ) -> Result<Value, String> {
        assembly_placement::server_contract(&status, &revision.dimension)?;
        let target = TargetServer::observed(&status, &revision.dimension)?;
        let baseline = before.clone();
        let proof = tokio::task::spawn_blocking(move || {
            ElectricalModification::new(&before, &after, Default::default())
        })
        .await
        .map_err(|e| e.to_string())??;
        let size = proof.steps(false).len().max(proof.steps(true).len());
        self.policy
            .validate_placement_size(size)
            .map_err(|e| e.to_string())?;
        if params.max_blocks.is_some_and(|n| size > n) {
            return Err("electrical edit exceeds max_blocks".into());
        }
        self.check_stationary_edit_baseline(&target, &baseline)
            .await?;
        let plan = ElectricalEditPlan {
            player: revision.player.clone(),
            revision_id: revision.revision_id,
            source,
            target,
            proof,
            previewed: false,
            state: PistonPlacementState::Planned,
            expires_at: Instant::now() + Duration::from_secs(300),
        };
        let id = uuid::Uuid::new_v4();
        let response = plan.preview(id, self.policy.read_only);
        let mut plans = self.plans.table::<ElectricalEditPlan>().lock().await;
        plans.retain(|_, p| {
            p.state != PistonPlacementState::Planned || p.expires_at > Instant::now()
        });
        if plans.len() >= 256 {
            return Err("too many retained electrical edits".into());
        }
        plans.insert(id, plan);
        drop(plans);
        self.operations
            .record_completed(id, OperationKind::PlacementPreview, response.clone())
            .await;
        Ok(response)
    }

    async fn owned_edit(
        &self,
        id: uuid::Uuid,
        player: Option<&str>,
    ) -> Result<ElectricalEditPlan, String> {
        let player = self.resolve_player(player)?;
        self.policy
            .authorize_player(&player)
            .map_err(|e| e.to_string())?;
        let plan = self
            .plans
            .table::<ElectricalEditPlan>()
            .lock()
            .await
            .get(&id)
            .cloned()
            .ok_or("electrical edit plan unavailable; recapture/replan after restart")?;
        if plan.player != player {
            return Err("electrical edit belongs to another player".into());
        }
        self.policy
            .authorize_dimension(&plan.target.dimension)
            .map_err(|e| e.to_string())?;
        self.policy
            .validate_region(dustroute_translate::world_reverse::RegionBounds::new(
                plan.proof.before().min,
                plan.proof.before().max,
            ))
            .map_err(|e| e.to_string())?;
        Ok(plan)
    }

    async fn check_stationary_edit_baseline(
        &self,
        target: &TargetServer,
        expected: &MinecraftSnapshot,
    ) -> Result<Vec<crate::observation_evidence::ObservationEvidence>, String> {
        let mut receipts = Vec::new();
        for index in 0..2 {
            let status = self.bridge.status().await.map_err(|e| e.to_string())?;
            assembly_placement::server_contract(&status, &target.dimension)?;
            target.check(&status)?;
            let observed = self
                .bridge
                .scan_region_fresh(expected.min, expected.max, &target.dimension)
                .await
                .map_err(|e| e.to_string())?
                .into_stationary_record()?;
            ValidatedAssemblyPlacement::matches(&observed.snapshot, expected, &status.version)?;
            if let Some(first) = receipts.first() {
                observed.readback.interval_since(first)?;
            }
            receipts.push(observed.readback);
            if index == 0 {
                self.bridge
                    .wait_ticks(20, &target.dimension)
                    .await
                    .map_err(|e| e.to_string())?;
            }
        }
        let status = self.bridge.status().await.map_err(|e| e.to_string())?;
        target.check(&status)?;
        Ok(receipts)
    }

    pub(super) async fn show_electrical_edit(&self, id: uuid::Uuid, player: Option<&str>) -> Value {
        let result: Result<Value, String> = async {
            let plan = self.owned_edit(id, player).await?;
            if plan.state != PistonPlacementState::Planned || plan.expires_at <= Instant::now() {
                return Err("electrical edit expired or consumed; inspect and replan".into());
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
                .map_err(|e| e.to_string())?;
            if plan.expires_at <= Instant::now() {
                return Err("electrical edit expired during preview".into());
            }
            self.plans
                .table::<ElectricalEditPlan>()
                .lock()
                .await
                .get_mut(&id)
                .ok_or("edit unavailable")?
                .previewed = true;
            let mut response = plan.preview(id, self.policy.read_only);
            response["preview"] = preview;
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
        self.execute_electrical_edit(id, confirm, undo)
            .await
            .unwrap_or_else(|error| json!({"ok":false,"error":error}))
    }

    async fn execute_electrical_edit(
        &self,
        id: uuid::Uuid,
        confirm: bool,
        undo: bool,
    ) -> Result<Value, String> {
        if !confirm {
            return Err("confirm=true is required".into());
        }
        self.policy
            .authorize_mutation()
            .map_err(|e| e.to_string())?;
        let _guard = self.mutation_lock.lock().await;
        let plan = self.owned_edit(id, None).await?;
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
        let before = plan.proof.before().clone();
        let after = plan.proof.after().clone();
        let proof = tokio::task::spawn_blocking(move || {
            ElectricalModification::new(&before, &after, Default::default())
        })
        .await
        .map_err(|e| e.to_string())??;
        if proof.steps(undo) != plan.proof.steps(undo) {
            return Err("fresh edit steps differ from the preview; replan".into());
        }
        let steps = proof.steps(undo);
        self.policy
            .validate_placement_size(steps.len())
            .map_err(|e| e.to_string())?;
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
                state: EditState::NeedsInspection,
                attempts: vec![],
            }
        };
        let baseline = if undo { proof.after() } else { proof.before() };
        let readbacks = self
            .check_stationary_edit_baseline(&plan.target, baseline)
            .await?;
        if !undo && plan.expires_at <= Instant::now() {
            return Err("edit expired during validation".into());
        }
        self.plans
            .table::<ElectricalEditPlan>()
            .lock()
            .await
            .get_mut(&id)
            .ok_or("edit unavailable")?
            .state = PistonPlacementState::NeedsInspection;
        record.state = EditState::NeedsInspection;
        record.attempts.push(EditAttempt {
            undo,
            verified_steps: 0,
            total_steps: steps.len(),
            started_at_unix_ms: now_ms()?,
            finished_at_unix_ms: None,
            error: None,
            readbacks,
        });
        registry.save(&record)?;
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
                construction_executor::StageProgress::Verified(count) => {
                    completed = count;
                    attempt.verified_steps = count;
                }
            }
            registry.save(&record)
        })
        .await;
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
        attempt.error = run.as_ref().err().cloned();
        if let Err(error) = registry.save(&record) {
            run = Err(error);
        }
        if run.is_ok() {
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
        let response = json!({"ok":run.is_ok(),"operation_id":id,"kind":"electrical_revision_modification","undo":undo,
            "status":if run.is_ok(){"verified"}else{"needs_inspection"},"verified_steps":completed,"total_steps":steps.len(),
            "error":run.err(),"retry_allowed":false,"automatic_rollback":false,"assembly_adoption_modified":false});
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
