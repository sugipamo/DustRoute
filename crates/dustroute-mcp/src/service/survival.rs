//! Public survival job boundary. Stored data diagnoses; only process-local plans execute.
use super::*;
use crate::source_identity::SourceIdentity;
use crate::survival_construction::generation::{SearchLimits, generate_construction_plan_async};
use crate::survival_construction::{
    ConstructionScope, ConstructionSite, HypotheticalConstructionPlan,
};
use crate::survival_execution::{ExecutionProgress, SurvivalExecutor};
use crate::voxrig_bridge::SurvivalLease;
use dustroute_library::{blueprint::AssemblyRevisionId, building::GroundedBuildingDesignRequest};
use rmcp::schemars;
use std::{
    path::Path,
    sync::atomic::{AtomicBool, Ordering},
};

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub(super) struct Request {
    #[serde(flatten)]
    action: Action,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
#[serde(tag = "action", rename_all = "snake_case", deny_unknown_fields)]
enum Action {
    Plan {
        player: Option<String>,
        assembly_revision_id: AssemblyRevisionId,
        specification: Box<GroundedBuildingDesignRequest>,
        scope: Box<ConstructionScope>,
        supplied: BTreeMap<String, usize>,
        temporary_material: String,
        #[serde(default)]
        limits: SearchLimits,
    },
    Start {
        #[schemars(with = "String")]
        job_id: uuid::Uuid,
        confirmed: bool,
    },
    Get {
        #[schemars(with = "String")]
        job_id: uuid::Uuid,
        #[serde(default)]
        include_record: bool,
    },
    Cancel {
        #[schemars(with = "String")]
        job_id: uuid::Uuid,
    },
    Checkpoint {
        #[schemars(with = "String")]
        job_id: uuid::Uuid,
    },
    Continue {
        #[schemars(with = "String")]
        job_id: uuid::Uuid,
        #[serde(default)]
        limits: SearchLimits,
    },
}

#[derive(Default)]
pub(super) struct Jobs {
    entries: Mutex<HashMap<uuid::Uuid, Entry>>,
}
struct Entry {
    owner: String,
    source: SourceIdentity,
    plan: Option<HypotheticalConstructionPlan>,
    expires: Instant,
    status: Value,
    cancel: Arc<AtomicBool>,
    checkpoint: Arc<AtomicBool>,
    parent: Option<ContinuationParent>,
    // Keep uncertain native operations and exclusive ownership inspectable.
    stopped: Option<(SurvivalLease, SurvivalExecutor)>,
}
struct PlanningInput {
    source: SourceIdentity,
    site: ConstructionSite,
    supplied: BTreeMap<String, usize>,
    temporary_material: String,
    limits: SearchLimits,
    specification: Value,
}
#[derive(Clone)]
struct ContinuationParent {
    id: uuid::Uuid,
    checkpoint: crate::survival_execution::checkpoint::SafeCheckpoint,
}
struct ExecutionInput {
    id: uuid::Uuid,
    owner: String,
    source: SourceIdentity,
    plan: HypotheticalConstructionPlan,
    lease: SurvivalLease,
    cancel: Arc<AtomicBool>,
    checkpoint: Arc<AtomicBool>,
    parent: Option<ContinuationParent>,
}
fn failure(code: &str, detail: impl std::fmt::Display) -> Value {
    json!({"ok":false,"schema_version":"dustroute.survival-job.v1",
        "error":{"code":code,"detail":detail.to_string()},"automatic_replay":false})
}
fn region(r: dustroute_translate::world::Region) -> voxrig::Region {
    voxrig::Region {
        min: [r.min.x, r.min.y, r.min.z],
        max: [r.max.x, r.max.y, r.max.z],
    }
}
fn save(path: &Path, value: &Value) -> Result<(), String> {
    let bytes = serde_json::to_vec_pretty(value).map_err(|e| e.to_string())?;
    if bytes.len() > 16 * 1024 * 1024 {
        return Err("survival job record exceeds bound".into());
    }
    crate::storage::replace(path, &bytes, crate::storage::Durability::FileAndDirectory)
        .map_err(|e| e.to_string())
}
fn load(path: &Path) -> Result<Value, String> {
    use std::io::Read;
    let mut bytes = Vec::new();
    std::fs::File::open(path)
        .map_err(|e| e.to_string())?
        .take(16 * 1024 * 1024 + 1)
        .read_to_end(&mut bytes)
        .map_err(|e| e.to_string())?;
    if bytes.len() > 16 * 1024 * 1024 {
        return Err("survival job record exceeds bound".into());
    }
    serde_json::from_slice(&bytes).map_err(|e| e.to_string())
}
fn policy_scope(
    policy: &McpPolicy,
    scope: &ConstructionScope,
    dimension: &str,
) -> Result<(), String> {
    policy
        .authorize_dimension(dimension)
        .map_err(|e| e.to_string())?;
    policy
        .validate_region(dustroute_translate::world_reverse::RegionBounds::new(
            scope.observed.min,
            scope.observed.max,
        ))
        .map_err(|e| e.to_string())?;
    Ok(())
}

impl DustRouteMcp {
    /// Explicit deployment configuration; this independently connected client is
    /// used only for observation. It never receives construction action authority.
    pub async fn with_survival_observer(
        mut self,
        username: &str,
    ) -> Result<Self, crate::bridge::BotBridgeError> {
        use crate::bridge::BotBridgeError;
        if !crate::bridge::is_valid_minecraft_username(username)
            || self.assist_player.as_deref() == Some(username)
        {
            return Err(BotBridgeError::Protocol(
                "invalid independent observer username".into(),
            ));
        }
        if self.survival_observer.is_some() {
            return Err(BotBridgeError::Protocol(
                "observer already configured".into(),
            ));
        }
        let native = self.bridge.survival_bridge()?;
        let lease = native.lease_survival()?;
        let mut config = lease.reconnect();
        let source = lease.source();
        if config.username == username {
            lease.release(source);
            return Err(BotBridgeError::Protocol(
                "observer must differ from builder".into(),
            ));
        }
        config.username = username.into();
        lease.release(source);
        let observer = voxrig::Client::connect(config)
            .await
            .map_err(|e| BotBridgeError::Protocol(e.to_string()))?;
        if let Err(e) = observer.wait_until_ready().await {
            let _ = observer.disconnect().await;
            return Err(BotBridgeError::Protocol(e.to_string()));
        }
        self.survival_observer = Some(observer);
        Ok(self)
    }

    pub(super) async fn survival_request(&self, request: Request) -> Value {
        match request.action {
            Action::Plan {
                player,
                assembly_revision_id,
                specification,
                scope,
                supplied,
                temporary_material,
                limits,
            } => {
                let owner = match self.resolve_player(player.as_deref()) {
                    Ok(p) => p,
                    Err(e) => return failure("player_required", e),
                };
                if let Err(e) = self.policy.authorize_player(&owner) {
                    return failure("permission_denied", e);
                }
                if self.survival.entries.lock().await.len() >= 32 {
                    return failure(
                        "job_capacity",
                        "at most 32 process-local jobs; retain records before restarting",
                    );
                }
                let store = self.state_store.clone();
                let p = owner.clone();
                let saved_specification = json!(specification);
                let checked = tokio::task::spawn_blocking(move || {
                    let source = crate::blueprint_mcp::construction_source(
                        &store,
                        &p,
                        &assembly_revision_id,
                    )?;
                    let design = dustroute_translate::building::generate_grounded_building_design(
                        *specification,
                    )
                    .map_err(|e| e.to_string())?;
                    continuation::validate_design(&source, &design)?;
                    let site = ConstructionSite::from_grounded(&design, *scope)
                        .map_err(|e| e.to_string())?;
                    Ok::<_, String>((source, site))
                })
                .await;
                let (source, site) = match checked {
                    Ok(Ok(v)) => v,
                    Ok(Err(e)) => return failure("source_not_adopted_or_mismatched", e),
                    Err(e) => return failure("planning_task_failed", e),
                };
                let native = match self.bridge.survival_bridge() {
                    Ok(n) => n,
                    Err(e) => return failure("backend_unavailable", e),
                };
                let lease = match native.lease_survival() {
                    Ok(l) => l,
                    Err(e) => return failure("source_busy", e),
                };
                let bot = lease.source();
                let result = self
                    .plan_survival(
                        &owner,
                        PlanningInput {
                            source,
                            site,
                            supplied,
                            temporary_material,
                            limits,
                            specification: saved_specification,
                        },
                        &bot,
                    )
                    .await;
                // Planning is read-only, including cancellation; no native sends from generator.
                lease.release(bot);
                result
            }
            Action::Get {
                job_id,
                include_record,
            } => self.get_survival(job_id, include_record).await,
            Action::Start { job_id, confirmed } => self.start_survival(job_id, confirmed).await,
            Action::Continue { job_id, limits } => self.continue_survival(job_id, limits).await,
            Action::Checkpoint { job_id } => {
                let jobs = self.survival.entries.lock().await;
                let Some(entry) = jobs.get(&job_id) else {
                    return failure(
                        "job_not_live",
                        "only a live executor can establish a safe checkpoint",
                    );
                };
                if let Err(e) = self.policy.authorize_player(&entry.owner) {
                    return failure("permission_denied", e);
                }
                if !matches!(
                    entry.status["state"].as_str(),
                    Some("running" | "admitting")
                ) {
                    return failure(
                        "checkpoint_not_running",
                        "no running executor; inspect the saved record",
                    );
                }
                entry.checkpoint.store(true, Ordering::SeqCst);
                json!({"ok":true,"job_id":job_id,"checkpoint_requested":true,"idle_confirmed":false,"next_step":"get until checkpointed; pending mining must settle first"})
            }
            Action::Cancel { job_id } => {
                let mut jobs = self.survival.entries.lock().await;
                let Some(entry) = jobs.get_mut(&job_id) else {
                    return failure(
                        "job_not_live",
                        "get the persisted diagnosis; jobs cannot resume from disk",
                    );
                };
                if let Err(e) = self.policy.authorize_player(&entry.owner) {
                    return failure("permission_denied", e);
                }
                entry.cancel.store(true, Ordering::SeqCst);
                if entry.plan.take().is_some() {
                    entry.status = json!({"state":"cancelled_before_start","world_writes":false});
                    if let Err(e) = save(
                        &self
                            .state_store
                            .survival_job_root()
                            .join(job_id.to_string())
                            .join("status.json"),
                        &entry.status,
                    ) {
                        return failure("journal_io", e);
                    }
                }
                json!({"ok":true,"job_id":job_id,"cancel_requested":true,"immediate_native_abort":false,"next_step":"get job; outstanding operations may still require inspection"})
            }
        }
    }

    async fn plan_survival(
        &self,
        owner: &str,
        input: PlanningInput,
        bot: &voxrig::Client,
    ) -> Value {
        let PlanningInput {
            source,
            site,
            supplied,
            temporary_material,
            limits,
            specification,
        } = input;
        let scope = site.scope().clone();
        if let Err(e) =
            self.policy
                .validate_region(dustroute_translate::world_reverse::RegionBounds::new(
                    scope.observed.min,
                    scope.observed.max,
                ))
        {
            return failure("permission_denied", e);
        }
        let ops = match bot.survival() {
            Ok(o) => o,
            Err(e) => return failure("native_refused", e),
        };
        let scene = match ops.capture_survival_scene(region(scope.observed)).await {
            Ok(s) => s,
            Err(e) => return failure("observation_unavailable", e),
        };
        if let Err(e) = policy_scope(&self.policy, &scope, &scene.source().dimension) {
            return failure("permission_denied", e);
        }
        let result = generate_construction_plan_async(
            scene,
            site,
            supplied,
            temporary_material.clone(),
            limits,
        )
        .await;
        let generated = match result {
            Ok(r) => r,
            Err(e) => {
                return json!({"ok":false,"error":{"code":"generation_refused","cause":e},"writes_minecraft":false});
            }
        };
        self.publish_survival_plan(owner, source, generated,
            json!({"specification":specification,"scope":scope,"temporary_material":temporary_material}), None).await
    }

    async fn publish_survival_plan(
        &self,
        owner: &str,
        source: SourceIdentity,
        generated: crate::survival_construction::generation::GeneratedConstructionPlan,
        construction: Value,
        parent: Option<ContinuationParent>,
    ) -> Value {
        if self.survival.entries.lock().await.len() >= 32 {
            return failure("job_capacity", "at most 32 process-local jobs");
        }
        if let Err(e) = self
            .policy
            .validate_placement_size(generated.plan.steps().len())
        {
            return failure("permission_denied", e);
        }
        let id = uuid::Uuid::new_v4();
        let path = self.state_store.survival_job_root().join(id.to_string());
        if let Err(e) = std::fs::create_dir_all(&path) {
            return failure("journal_io", e);
        }
        let manifest = json!({"schema":"dustroute.survival-job.v1","job_id":id,"owner":owner,"source":source,
            "preview":generated,"construction":construction,"parent_job_id":parent.as_ref().map(|p|p.id),"execution_authority_restorable":false});
        if let Err(e) = save(&path.join("manifest.json"), &manifest) {
            return failure("journal_io", e);
        }
        let response = json!({"ok":true,"schema_version":"dustroute.survival-job.v1","job_id":id,
            "state":"planned","preview":generated,"writes_minecraft":false,"inventory_receipt":false,
            "expires_after_seconds":900,"next_step":"review plan, then action=start with confirmed=true; current inventory/site/source will be rechecked"});
        self.survival.entries.lock().await.insert(
            id,
            Entry {
                owner: owner.into(),
                source,
                plan: Some(generated.plan),
                expires: Instant::now() + Duration::from_secs(900),
                status: json!({"state":"planned"}),
                cancel: Arc::new(AtomicBool::new(false)),
                checkpoint: Arc::new(AtomicBool::new(false)),
                parent,
                stopped: None,
            },
        );
        response
    }

    async fn get_survival(&self, id: uuid::Uuid, include_record: bool) -> Value {
        // Progress polling must not repeatedly deserialize the entire plan and
        // journal or hold the registry mutex across filesystem I/O.
        let live = self
            .survival
            .entries
            .lock()
            .await
            .get(&id)
            .map(|e| (e.owner.clone(), e.status.clone()));
        if let Some((owner, status)) = &live {
            if let Err(e) = self.policy.authorize_player(owner) {
                return failure("permission_denied", e);
            }
            if !include_record {
                return json!({"ok":true,"schema_version":"dustroute.survival-job.v1","job_id":id,"owner":owner,
                    "process_local_job_present":true,"historical_only":false,"execution_authority_restored":false,
                    "completed_steps":status["completed_steps"],"status":status,
                    "next_step":"inspect status; stopped jobs never automatically replay"});
            }
        }
        let service = self.clone();
        match tokio::task::spawn_blocking(move || {
            service.read_survival_record(id, include_record, live)
        })
        .await
        {
            Ok(result) => result,
            Err(e) => failure("journal_unavailable", e),
        }
    }

    fn read_survival_record(
        &self,
        id: uuid::Uuid,
        include_record: bool,
        live: Option<(String, Value)>,
    ) -> Value {
        let path = self.state_store.survival_job_root().join(id.to_string());
        let manifest = match load(&path.join("manifest.json")) {
            Ok(m) => m,
            Err(e) => return failure("job_unavailable", e),
        };
        if manifest["schema"] != "dustroute.survival-job.v1"
            || manifest["job_id"] != json!(id)
            || manifest["execution_authority_restorable"] != false
        {
            return failure(
                "invalid_record",
                "unknown schema or mismatched job identity",
            );
        }
        let Some(owner) = manifest["owner"].as_str() else {
            return failure("invalid_record", "missing job owner");
        };
        if let Err(e) = self.policy.authorize_player(owner) {
            return failure("permission_denied", e);
        }
        if live
            .as_ref()
            .is_some_and(|(live_owner, _)| live_owner != owner)
        {
            return failure("invalid_record", "saved owner differs from live job");
        }
        let historical_status = if path.join("status.json").exists() {
            match load(&path.join("status.json")) {
                Ok(v) => Some(v),
                Err(e) => return failure("invalid_record", e),
            }
        } else {
            None
        };
        let directory = path.join("execution");
        let diagnosis = if directory.join("record.json").exists() {
            match crate::survival_execution::diagnose(&directory) {
                Ok(d) => Some(d),
                Err(e) => return failure("journal_unavailable", e),
            }
        } else {
            None
        };
        let record = diagnosis.as_ref().map(|d| &d.record);
        let mut response = json!({"ok":true,"schema_version":"dustroute.survival-job.v1","job_id":id,"owner":owner,
            "process_local_job_present":live.is_some(),"status":live.as_ref().map(|(_,s)|s).or(historical_status.as_ref()),
            "historical_only":live.is_none(),"execution_authority_restored":false,
            "completed_steps":record.map(|r|r.completed_steps),"recorded_continuation":record.map(|r|&r.continuation),
            "next_step":if live.is_none(){"historical diagnosis only; reobserve and resolve outstanding operations before any new plan"}else{"inspect status; stopped jobs never automatically replay"}});
        if include_record {
            response["manifest"] = manifest;
            response["diagnosis"] = json!(diagnosis);
            if directory.join("continuation-claim.json").exists() {
                match load(&directory.join("continuation-claim.json")) {
                    Ok(claim) => response["continuation_job_id"] = claim["new_job"].clone(),
                    Err(e) => return failure("invalid_continuation_claim", e),
                }
            }
        }
        response
    }

    async fn start_survival(&self, id: uuid::Uuid, confirmed: bool) -> Value {
        if !confirmed {
            return failure(
                "confirmation_required",
                "review the generated preview and authorize its scope/materials",
            );
        }
        if let Err(e) = self.policy.authorize_mutation() {
            return failure("permission_denied", e);
        }
        let mut jobs = self.survival.entries.lock().await;
        let Some(entry) = jobs.get_mut(&id) else {
            return failure(
                "plan_not_live",
                "saved plans are diagnosis-only; generate a fresh plan",
            );
        };
        if let Err(e) = self.policy.authorize_player(&entry.owner) {
            return failure("permission_denied", e);
        }
        if entry.cancel.load(Ordering::SeqCst) || Instant::now() > entry.expires {
            entry.plan = None;
            entry.status =
                json!({"state":"plan_expired_or_cancelled","construction_dispatched":false});
            if let Err(e) = save(
                &self
                    .state_store
                    .survival_job_root()
                    .join(id.to_string())
                    .join("status.json"),
                &entry.status,
            ) {
                return failure("journal_io", e);
            }
            return failure("plan_expired_or_cancelled", "generate a fresh plan");
        }
        if entry.plan.is_none() {
            return failure("job_already_started", "use get; no automatic replay");
        }
        let native = match self.bridge.survival_bridge() {
            Ok(n) => n,
            Err(e) => return failure("backend_unavailable", e),
        };
        let lease = match native.lease_survival() {
            Ok(l) => l,
            Err(e) => return failure("source_busy", e),
        };
        let plan = entry.plan.take().expect("checked plan");
        let source = entry.source.clone();
        let owner = entry.owner.clone();
        let cancel = entry.cancel.clone();
        let checkpoint = entry.checkpoint.clone();
        let parent = entry.parent.clone();
        entry.status = json!({"state":"admitting"});
        if let Err(e) = save(
            &self
                .state_store
                .survival_job_root()
                .join(id.to_string())
                .join("status.json"),
            &entry.status,
        ) {
            let original = lease.source();
            lease.release(original);
            entry.status = json!({"state":"admission_refused","failure":failure("journal_io", &e),"construction_dispatched":false});
            return failure("journal_io", e);
        }
        drop(jobs);
        let service = self.clone();
        let worker = tokio::spawn(async move {
            service
                .run_survival(ExecutionInput {
                    id,
                    owner,
                    source,
                    plan,
                    lease,
                    cancel,
                    checkpoint,
                    parent,
                })
                .await;
        });
        let service = self.clone();
        tokio::spawn(async move {
            if let Err(error) = worker.await {
                service.set_survival_status(id, json!({"state":"needs_inspection", "failure":failure("execution_task_failed", error)})).await;
            }
        });
        json!({"ok":true,"job_id":id,"state":"admitting","completed":false,"next_step":"action=get; admission can still refuse before any edits"})
    }

    async fn run_survival(&self, input: ExecutionInput) {
        let ExecutionInput {
            id,
            owner,
            source,
            plan,
            mut lease,
            cancel,
            checkpoint,
            parent,
        } = input;
        let bot = lease.source();
        let path = self.state_store.survival_job_root().join(id.to_string());
        let admission = async {
            let store = self.state_store.clone();
            let p = owner.clone();
            let original = source.clone();
            tokio::task::spawn_blocking(move || {
                let current =
                    crate::blueprint_mcp::construction_source(&store, &p, &original.record.id)?;
                if !original.matches(&current) {
                    return Err("adopted source changed since preview".into());
                }
                Ok(())
            })
            .await
            .map_err(|e| failure("admission_task_failed", e))?
            .map_err(|e: String| failure("source_changed", e))?;
            policy_scope(&self.policy, plan.scope(), &plan.source().dimension)
                .map_err(|e| failure("permission_denied", e))?;
            if parent.as_ref().is_some_and(|p| {
                p.checkpoint.endpoint
                    != crate::survival_execution::checkpoint::endpoint(&lease.reconnect())
            }) {
                return Err(failure(
                    "checkpoint_endpoint_changed",
                    "continuation requires the original endpoint and builder profile",
                ));
            }
            if cancel.load(Ordering::SeqCst) {
                return Err(failure(
                    "cancelled_before_start",
                    "no construction dispatched",
                ));
            }
            let executor = SurvivalExecutor::create(
                bot.clone(),
                lease.reconnect(),
                plan,
                &path.join("execution"),
            )
            .await
            .map_err(|e| json!({"ok":false,"error":e}))?;
            let parent_lock = if let Some(parent) = &parent {
                Some(
                    crate::survival_execution::checkpoint::claim(
                        &self
                            .state_store
                            .survival_job_root()
                            .join(parent.id.to_string())
                            .join("execution"),
                        &parent.checkpoint,
                        id,
                    )
                    .map_err(|e| json!({"ok":false,"error":e}))?,
                )
            } else {
                None
            };
            Ok((executor, parent_lock))
        }
        .await;
        let (mut executor, _parent_lock) = match admission {
            Ok(e) => e,
            Err(e) => {
                lease.release(bot);
                self.set_survival_status(id,json!({"state":"admission_refused","failure":e,"construction_dispatched":false})).await;
                return;
            }
        };
        lease.begin_execution();
        loop {
            if cancel.load(Ordering::SeqCst) {
                let result = executor.cancel();
                self.set_survival_status(id,json!({"state":"cancelled_needs_inspection","error":result.err(),"completed_steps":executor.record().completed_steps})).await;
                break;
            }
            if checkpoint.load(Ordering::SeqCst) && !executor.pending_mining() {
                match executor.checkpoint().await {
                    Ok(boundary) => {
                        let current = executor.client().clone();
                        // Release the old journal writer before publishing readiness.
                        drop(executor);
                        lease.release(current);
                        self.set_survival_status(id,json!({"state":"checkpointed","completed_steps":boundary.completed_steps,"safe_idle":true,"checkpoint":boundary,"next_step":"action=continue creates a fresh preview; start still requires confirmation"})).await;
                        return;
                    }
                    Err(error) => {
                        self.set_survival_status(id,json!({"state":"needs_inspection","error":error,"completed_steps":executor.record().completed_steps})).await;
                        break;
                    }
                }
            }
            match executor.advance().await {
                Ok(ExecutionProgress::Completed)=>{
                    lease.release(executor.client().clone());
                    self.set_survival_status(id,json!({"state":"completed","completed_steps":executor.record().completed_steps,"final_evidence":executor.record().events.last()})).await;
                    return;
                }
                Ok(progress)=> {
                    if !self.set_survival_status(id,json!({"state":"running","progress":progress,"completed_steps":executor.record().completed_steps})).await {
                        let _=executor.cancel();
                        break;
                    }
                },
                Err(error)=>{self.set_survival_status(id,json!({"state":"needs_inspection","error":error,"completed_steps":executor.record().completed_steps})).await;break;}
            }
        }
        if let Some(entry) = self.survival.entries.lock().await.get_mut(&id) {
            entry.stopped = Some((lease, executor));
        }
    }
    async fn set_survival_status(&self, id: uuid::Uuid, status: Value) -> bool {
        let result = save(
            &self
                .state_store
                .survival_job_root()
                .join(id.to_string())
                .join("status.json"),
            &status,
        );
        let persisted = result.is_ok();
        let status = match result {
            Ok(()) => status,
            Err(e) => {
                json!({"state":"needs_inspection","last_status":status,"persistence_error":e})
            }
        };
        if let Some(entry) = self.survival.entries.lock().await.get_mut(&id) {
            entry.status = status;
        }
        persisted
    }
}

#[cfg(test)]
mod tests;

mod continuation;

#[tool_router(router = survival_tool_router, vis = "pub(super)")]
impl DustRouteMcp {
    #[tool(
        description = "Plan and run bounded non-OP survival construction from a uniquely adopted grounded Blueprint. Supply materials to the source bot. action=plan requires matching specification, edit/temporary/travel/retreat scopes and material budget; returns a complete preview without world edits. action=start requires job_id and confirmed=true and freshly checks source/site/inventory; execution continues in background. action=get reports progress or durable diagnosis. action=checkpoint requests a sealed idle stop after any pending mining settles and retires; poll get until checkpointed. action=continue with that old job_id reobserves the site and current materials and generates a NEW preview, also after process restart; review and explicitly start its new job_id. External changes or unresolved lost actions refuse continuation. A checkpoint is consumed once on new admission. action=cancel stops at the next boundary and may retain uncertainty. Uses builder-received world evidence and explicit model-based motion continuation; no independent position error bound or server stop acknowledgement. No observation bot required. No gathering, native-token restoration, command placement or automatic replay."
    )]
    async fn survival_construction(&self, Parameters(params): Parameters<Request>) -> String {
        let mut result = self.survival_request(params).await;
        if let Some(object) = result.as_object_mut() {
            object.insert(
                "execution_contract".into(),
                json!({"motion":"predicted_dry_cube_v1",
                "world_evidence":"builder_received","independent_observer_required":false,
                "server_stop_acknowledged":false,"server_position_error_bound":null}),
            );
        }
        json_text(result)
    }
}
