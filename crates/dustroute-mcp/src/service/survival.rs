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
#[serde(tag = "action", rename_all = "snake_case", deny_unknown_fields)]
pub(super) enum Request {
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
    // Keep uncertain native operations and exclusive ownership inspectable.
    stopped: Option<(SurvivalLease, SurvivalExecutor)>,
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
        match request {
            Request::Plan {
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
                    if source.record != design.request.candidate_state
                        || source.context != design.context
                    {
                        return Err(
                            "specification does not match the exact adopted Assembly and context"
                                .to_owned(),
                        );
                    }
                    let catalog = design.records.catalog().map_err(|e| e.to_string())?;
                    if !catalog
                        .revisions()
                        .chain(design.request.revisions.iter())
                        .all(|r| source.catalog.revision(&r.id) == Some(r))
                        || !catalog
                            .type_revisions()
                            .all(|r| source.catalog.type_revision(&r.id) == Some(r))
                        || !catalog
                            .classifications()
                            .all(|r| source.catalog.classification(&r.id) == Some(r))
                        || !catalog
                            .assemblies()
                            .all(|r| source.catalog.assembly(&r.id) == Some(r))
                    {
                        return Err(
                            "adopted definitions differ from the grounded specification".into()
                        );
                    }
                    let site = ConstructionSite::from_grounded(&design, *scope)
                        .map_err(|e| e.to_string())?;
                    Ok((source, site))
                })
                .await;
                let (source, site) = match checked {
                    Ok(Ok(v)) => v,
                    Ok(Err(e)) => return failure("source_not_adopted_or_mismatched", e),
                    Err(e) => return failure("planning_task_failed", e),
                };
                if self.survival_observer.is_none() {
                    return failure(
                        "observer_not_configured",
                        "configure DUSTROUTE_SURVIVAL_OBSERVER_USERNAME on the same server before planning",
                    );
                }
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
                        source,
                        site,
                        supplied,
                        temporary_material,
                        limits,
                        &bot,
                    )
                    .await;
                // Planning is read-only, including cancellation; no native sends from generator.
                lease.release(bot);
                result
            }
            Request::Get {
                job_id,
                include_record,
            } => self.get_survival(job_id, include_record).await,
            Request::Start { job_id, confirmed } => self.start_survival(job_id, confirmed).await,
            Request::Cancel { job_id } => {
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
        source: SourceIdentity,
        site: ConstructionSite,
        supplied: BTreeMap<String, usize>,
        temporary_material: String,
        limits: SearchLimits,
        bot: &voxrig::Client,
    ) -> Value {
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
        let result =
            generate_construction_plan_async(scene, site, supplied, temporary_material, limits)
                .await;
        let generated = match result {
            Ok(r) => r,
            Err(e) => {
                return json!({"ok":false,"error":{"code":"generation_refused","cause":e},"writes_minecraft":false});
            }
        };
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
            "preview":generated,"execution_authority_restorable":false});
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
                stopped: None,
            },
        );
        response
    }

    async fn get_survival(&self, id: uuid::Uuid, include_record: bool) -> Value {
        let path = self.state_store.survival_job_root().join(id.to_string());
        let manifest = match load(&path.join("manifest.json")) {
            Ok(m) => m,
            Err(e) => return failure("job_unavailable", e),
        };
        let Some(owner) = manifest["owner"].as_str() else {
            return failure("invalid_record", "missing job owner");
        };
        if let Err(e) = self.policy.authorize_player(owner) {
            return failure("permission_denied", e);
        }
        let jobs = self.survival.entries.lock().await;
        let entry = jobs.get(&id);
        let historical_status = load(&path.join("status.json")).ok();
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
            "process_local_job_present":entry.is_some(),"status":entry.map(|e|&e.status).or(historical_status.as_ref()),
            "historical_only":entry.is_none(),"execution_authority_restored":false,
            "completed_steps":record.map(|r|r.completed_steps),"recorded_continuation":record.map(|r|&r.continuation),
            "next_step":if entry.is_none(){"historical diagnosis only; reobserve and resolve outstanding operations before any new plan"}else{"inspect status; stopped jobs never automatically replay"}});
        if include_record {
            response["manifest"] = manifest;
            response["diagnosis"] = json!(diagnosis);
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
            return failure("plan_expired_or_cancelled", "generate a fresh plan");
        }
        if entry.plan.is_none() {
            return failure("job_already_started", "use get; no automatic replay");
        }
        let Some(observer) = self.survival_observer.clone() else {
            return failure("observer_not_configured", "independent observer required");
        };
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
            return failure("journal_io", e);
        }
        drop(jobs);
        let service = self.clone();
        tokio::spawn(async move {
            service
                .run_survival(id, owner, source, plan, lease, observer, cancel)
                .await;
        });
        json!({"ok":true,"job_id":id,"state":"admitting","completed":false,"next_step":"action=get; admission can still refuse before any edits"})
    }

    async fn run_survival(
        &self,
        id: uuid::Uuid,
        owner: String,
        source: SourceIdentity,
        plan: HypotheticalConstructionPlan,
        mut lease: SurvivalLease,
        observer: voxrig::Client,
        cancel: Arc<AtomicBool>,
    ) {
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
            let observer_state = observer
                .java_1_21_11_operations()
                .map_err(|e| failure("observer_unavailable", e))?
                .player_state()
                .await
                .map_err(|e| failure("observer_unavailable", e))?;
            let observed = observer
                .observe_region(region(plan.scope().observed))
                .await
                .map_err(|e| failure("observer_unavailable", e))?;
            let scene = bot
                .survival()
                .map_err(|e| failure("native_refused", e))?
                .capture_survival_scene(region(plan.scope().observed))
                .await
                .map_err(|e| failure("observation_unavailable", e))?;
            let scenario = scene.scenario();
            if observer_state.dimension.as_deref() != Some(plan.source().dimension.as_str())
                || observed.connection_id == scene.source().connection_id
                || observed.connection_id != observer_state.connection_id
                || observed.version != voxrig::MinecraftVersion::Java1_21_11
                || observed.blocks.len()
                    != scene
                        .region()
                        .volume()
                        .map_err(|e| failure("invalid_observation", e))?
                || observed
                    .blocks
                    .iter()
                    .any(|b| scenario.block(b.position).ok().as_ref() != b.state.as_ref())
            {
                return Err(failure(
                    "observer_mismatch",
                    "independent initial scene does not match builder",
                ));
            }
            if cancel.load(Ordering::SeqCst) {
                return Err(failure(
                    "cancelled_before_start",
                    "no construction dispatched",
                ));
            }
            SurvivalExecutor::create(
                bot.clone(),
                observer,
                lease.reconnect(),
                plan,
                &path.join("execution"),
            )
            .await
            .map_err(|e| json!({"ok":false,"error":e}))
        }
        .await;
        let mut executor = match admission {
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
