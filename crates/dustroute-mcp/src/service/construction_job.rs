//! Region job orchestration. Durable intentions are re-proved one region at a
//! time; every executable stage still uses the common live edit executor.
use super::*;
use crate::BotStatus;
use crate::assembly_registry::TargetServer;
use crate::construction_jobs::{JobRecord, JobRegistry, JobStageBinding, JobState, SCHEMA};
use dustroute_library::world_edit::WorldEditScope;
use dustroute_translate::piston_construction::{ElectricalModification, ElectricalWorkPlan};
use dustroute_translate::snapshot::MinecraftSnapshot;
use rmcp::schemars;

#[derive(Debug, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub(super) struct ManageConstructionJobParams {
    job_id: String,
    action: JobAction,
    #[serde(default)]
    /// For action=get only: explicitly expand all saved intention/checkpoint data.
    include_intention: bool,
}
#[derive(Debug, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "snake_case")]
enum JobAction {
    Get,
    Observe,
    PlanNext,
    PlanUndo,
    PlanRecovery,
    Cancel,
}

fn profile() -> &'static str {
    dustroute_translate::world::time::piston_runtime::ELECTRICAL_PROFILE
}
fn summary(record: &JobRecord) -> Value {
    let expanded = record.before.blocks.len()
        + record.after.blocks.len()
        + record
            .boundaries
            .iter()
            .map(|b| b.changes.len())
            .sum::<usize>()
        <= 512;
    let job = if expanded {
        json!(record)
    } else {
        json!({"schema":record.schema,"execution_profile":record.execution_profile,
            "id":record.id,"player":record.player,"source_revision_id":record.source_revision_id,
            "source":record.source,"target":record.target,"scope":record.scope,
            "regions":record.regions,"completed_regions":record.completed_regions,
            "state":record.state,"forward_cancelled":record.forward_cancelled,
            "active_operation_id":record.active_operation_id,"attempts":record.attempts,
            "before":electrical_edit::state_summary(&record.before),
            "after":electrical_edit::state_summary(&record.after),
            "boundaries":record.boundaries.iter().map(|b|json!({"changed_positions":b.changes.len(),
                "changes":b.changes.iter().take(64).collect::<Vec<_>>(),"truncated":b.changes.len()>64})).collect::<Vec<_>>()})
    };
    let mut result = json!({"schema_version":"dustroute.construction-job-response.v2",
        "ok":true,"job":job,"executable_plan_restored":false,
        "future_regions_verified":false,"functional_behavior_verified":false,
        "intention_expanded":expanded,
        "history_scope":"durable intention and verified batch progress; not live-world evidence",
        "next_step":"observe or plan_next/plan_undo; show and confirm each fresh operation"});
    if !expanded {
        result["read_full_intention"] = json!(
            "manage_construction_job(action=get, include_intention=true); historical intention, not executable proof"
        );
    }
    result
}

impl DustRouteMcp {
    pub(super) async fn discard_job_stage_plans(
        &self,
        job_id: uuid::Uuid,
        keep: Option<uuid::Uuid>,
    ) {
        let mut discarded = vec![];
        self.plans
            .table::<electrical_edit::ElectricalEditPlan>()
            .lock()
            .await
            .retain(|id, plan| {
                let retain =
                    plan.job.is_none_or(|binding| binding.job_id != job_id) || Some(*id) == keep;
                if !retain {
                    discarded.push(*id);
                }
                retain
            });
        for id in discarded {
            self.operations.record_completed(id, OperationKind::PlacementPreview,
                json!({"ok":false,"status":"job_stage_capability_discarded","job_id":job_id,
                    "next_step":"get_operation for durable edit history, or manage_construction_job"})).await;
        }
    }
    pub(super) async fn create_construction_job(
        &self,
        params: &PreviewPlacementParams,
        revision: &crate::revision::CircuitRevision,
        before: MinecraftSnapshot,
        after: MinecraftSnapshot,
        status: BotStatus,
        source: Value,
    ) -> Result<Value, String> {
        assembly_placement::server_contract(&status, &revision.dimension)?;
        let target = TargetServer::observed(&status, &revision.dimension)?;
        let regions = params.work_regions.clone().ok_or("work_regions required")?;
        let scope = params.edit_scope.clone().unwrap_or_else(|| WorldEditScope {
            editable: regions.clone(),
            protected: vec![],
        });
        let plan = ElectricalWorkPlan::new(&before, &after, regions, scope.clone())?;
        let changed: usize = plan
            .regions()
            .iter()
            .map(|r| r.changed_positions.len())
            .sum();
        self.policy
            .validate_placement_size(changed)
            .map_err(|e| e.to_string())?;
        if params.max_blocks.is_some_and(|limit| changed > limit) {
            return Err("construction job diff exceeds max_blocks".into());
        }
        let mut record = JobRecord {
            schema: SCHEMA.into(),
            execution_profile: profile().into(),
            id: uuid::Uuid::new_v4(),
            player: revision.player.clone(),
            source_revision_id: revision.revision_id,
            source,
            target,
            before: plan.before().clone(),
            after: plan.target().clone(),
            scope,
            regions: plan.regions().to_vec(),
            boundaries: vec![],
            completed_regions: 0,
            state: JobState::Ready,
            forward_cancelled: false,
            active_operation_id: None,
            attempts: vec![],
        };
        let _guard = self.mutation_lock.lock().await;
        let registry = JobRegistry::acquire(&self.state_store)?;
        let mut result = self.prepare_job_region(&mut record, false).await?;
        registry.save(&record)?;
        self.discard_job_stage_plans(record.id, record.active_operation_id)
            .await;
        result["job_id"] = json!(record.id);
        result["job"] = summary(&record)["job"].clone();
        result["future_regions_verified"] = json!(false);
        Ok(result)
    }

    pub(super) fn check_job_stage(
        &self,
        job: &JobRecord,
        id: uuid::Uuid,
        binding: JobStageBinding,
        proof: &ElectricalModification,
    ) -> Result<(), String> {
        job.check_binding(id, binding)?;
        if job.execution_profile != profile() || job.scope != *proof.scope() {
            return Err("job execution profile/scope differs from the freshly proved stage".into());
        }
        let plan = job.work_plan()?;
        let before = job.snapshot_at(job.completed_regions)?;
        let target = if binding.undo {
            job.snapshot_at(binding.region_index)?
        } else {
            job.after.clone()
        };
        let old = crate::revision::blocks(&before)?;
        let effective = |requests: Vec<dustroute_translate::snapshot::MinecraftSnapshotBlock>| {
            requests
                .into_iter()
                .filter(|r| {
                    if r.name == "minecraft:air" {
                        old.contains_key(&r.pos)
                    } else {
                        old.get(&r.pos) != Some(r)
                    }
                })
                .collect::<Vec<_>>()
        };
        let exact = effective(plan.requests(binding.region_index, &target)?);
        let allowed = exact == proof.requests()
            || (!binding.undo
                && effective(plan.temporary_requests(binding.region_index)?) == proof.requests());
        if before != *proof.before()
            || !allowed
            || ((binding.undo || binding.region_index + 1 == job.regions.len())
                && target != *proof.after())
        {
            return Err(
                "job baseline/command intention differs from the freshly proved stage".into(),
            );
        }
        Ok(())
    }

    async fn prepare_job_region(
        &self,
        record: &mut JobRecord,
        undo: bool,
    ) -> Result<Value, String> {
        if record.execution_profile != profile() {
            return Err(
                "job execution profile changed; recapture and create a new revision/job".into(),
            );
        }
        if record.forward_cancelled && !undo {
            return Err(
                "forward work is cancelled; only fresh inverse cleanup is permitted".into(),
            );
        }
        let plan = record.work_plan()?;
        let index = if undo {
            record
                .completed_regions
                .checked_sub(1)
                .ok_or("no verified region to undo")?
        } else {
            record.completed_regions
        };
        if index >= plan.regions().len() {
            return Err("all work regions are already completed".into());
        }
        let before = record.snapshot_at(record.completed_regions)?;
        let inverse_target = if undo {
            Some(record.snapshot_at(index)?)
        } else {
            None
        };
        let bounds = dustroute_translate::world_reverse::RegionBounds::new(before.min, before.max);
        self.policy
            .validate_region(bounds)
            .map_err(|e| e.to_string())?;
        self.bridge
            .observation_capabilities()
            .validate_region(bounds.min, bounds.max)
            .map_err(|e| e.to_string())?;
        let status = self.bridge.status().await.map_err(|e| e.to_string())?;
        assembly_placement::server_contract(&status, &record.target.dimension)?;
        record.target.check(&status)?;
        let params = PreviewPlacementParams {
            player: Some(record.player.clone()),
            edit_scope: Some(record.scope.clone()),
            ..Default::default()
        };
        let capture = crate::performance::current();
        let queued_at = std::time::Instant::now();
        let proof = tokio::task::spawn_blocking(move || {
            capture.record_queue(queued_at);
            capture.in_blocking(|| {
                let _measurement = crate::performance::span(crate::performance::Phase::ModelProof);
                if let Some(target) = inverse_target {
                    plan.prove_undo(index, &before, &target, Default::default())
                } else {
                    plan.prove_region(index, &before, Default::default())
                }
            })
        })
        .await
        .map_err(|e| e.to_string())??;
        let result = self
            .register_electrical_edit(
                &params,
                status,
                electrical_edit::EditOrigin {
                    revision_id: record.source_revision_id,
                    source: record.source.clone(),
                    job: Some(JobStageBinding {
                        job_id: record.id,
                        region_index: index,
                        undo,
                    }),
                },
                proof,
            )
            .await?;
        let id = result["operation_id"]
            .as_str()
            .ok_or("missing stage operation ID")?;
        record.active_operation_id = Some(uuid::Uuid::parse_str(id).map_err(|e| e.to_string())?);
        record.state = if record.forward_cancelled {
            JobState::Cancelled
        } else if record.completed_regions == record.regions.len() {
            JobState::Completed
        } else {
            JobState::Ready
        };
        Ok(result)
    }

    pub(super) async fn manage_construction_job_inner(
        &self,
        params: ManageConstructionJobParams,
    ) -> Result<Value, String> {
        if params.include_intention && !matches!(params.action, JobAction::Get) {
            return Err("include_intention is only supported by action=get".into());
        }
        let player = self.resolve_player(None)?;
        self.policy
            .authorize_player(&player)
            .map_err(|e| e.to_string())?;
        // Serialize replanning/cancellation with writes so an old stage cannot
        // acquire permission concurrently with a new stage identity.
        let _guard = self.mutation_lock.lock().await;
        let registry = JobRegistry::acquire(&self.state_store)?;
        let mut record = registry.load(
            uuid::Uuid::parse_str(&params.job_id).map_err(|e| e.to_string())?,
            &player,
        )?;
        self.policy
            .authorize_dimension(&record.target.dimension)
            .map_err(|e| e.to_string())?;
        match params.action {
            JobAction::Get => {
                let mut result = summary(&record);
                if params.include_intention {
                    result["job"] = json!(record);
                    result["intention_expanded"] = json!(true);
                }
                Ok(result)
            }
            JobAction::Cancel => {
                if record.state == JobState::NeedsInspection {
                    return Err("uncertain job needs inspection; cancellation cannot erase uncertain write intent".into());
                }
                record.state = JobState::Cancelled;
                record.forward_cancelled = true;
                record.active_operation_id = None;
                registry.save(&record)?;
                self.discard_job_stage_plans(record.id, None).await;
                Ok(summary(&record))
            }
            JobAction::Observe => {
                record.work_plan()?;
                let expected = record.snapshot_at(record.completed_regions)?;
                self.policy
                    .validate_region(dustroute_translate::world_reverse::RegionBounds::new(
                        expected.min,
                        expected.max,
                    ))
                    .map_err(|e| e.to_string())?;
                let status = self.bridge.status().await.map_err(|e| e.to_string())?;
                record.target.check(&status)?;
                let observed = self
                    .bridge
                    .scan_region_fresh(expected.min, expected.max, &record.target.dimension)
                    .await
                    .map_err(|e| e.to_string())?
                    .into_stationary_record()?;
                record
                    .target
                    .check(&self.bridge.status().await.map_err(|e| e.to_string())?)?;
                let matches = crate::piston_assembly::ValidatedAssemblyPlacement::matches(
                    &observed.snapshot,
                    &expected,
                    &record.target.version,
                );
                let differences = dustroute_translate::diagnostic::difference::differences(
                    &observed.snapshot,
                    &expected,
                )?;
                let count = differences.len();
                Ok(json!({"ok":true,"job_id":record.id,"state":record.state,
                    "matches_verified_prefix":matches.is_ok(),"difference":matches.err(),
                    "observation":{"observation_id":observed.observation_id,"readback":observed.readback,
                        "bounds":{"min":expected.min,"max":expected.max},
                        "differences":differences.into_iter().take(64).collect::<Vec<_>>(),
                        "differing_positions":count,"differences_truncated":count>64},
                    "write_authorized":false,"history_advanced":false,
                    "interpretation":"literal differences; cause and ownership are not inferred",
                    "hidden_runtime_observed":false}))
            }
            JobAction::PlanNext | JobAction::PlanUndo | JobAction::PlanRecovery => {
                let recovery = matches!(params.action, JobAction::PlanRecovery);
                let undo = if recovery {
                    if record.state != JobState::NeedsInspection {
                        return Err("plan_recovery requires an uncertain failed stage".into());
                    }
                    record
                        .attempts
                        .last()
                        .ok_or("missing uncertain job attempt")?
                        .undo
                } else {
                    if !(matches!(record.state, JobState::Ready | JobState::Completed)
                        || (matches!(params.action, JobAction::PlanUndo)
                            && record.state == JobState::Cancelled))
                    {
                        return Err("job is cancelled or needs inspection; observe before explicit recovery".into());
                    }
                    matches!(params.action, JobAction::PlanUndo)
                };
                if undo && record.state == JobState::Cancelled {
                    record.forward_cancelled = true;
                }
                let mut result = self.prepare_job_region(&mut record, undo).await?;
                // Recovery is explicit and still requires the exact original
                // stage baseline. A partial prefix never grants a fresh retry.
                registry.save(&record)?;
                self.discard_job_stage_plans(record.id, record.active_operation_id)
                    .await;
                result["job_id"] = json!(record.id);
                result["job"] = summary(&record)["job"].clone();
                result["future_regions_verified"] = json!(false);
                Ok(result)
            }
        }
    }
}
