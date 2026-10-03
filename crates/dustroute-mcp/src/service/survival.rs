//! Public survival job boundary. Stored data diagnoses; only process-local plans execute.
use super::*;
use crate::source_identity::SourceIdentity;
use crate::survival_construction::generation::{SearchLimits, generate_construction_plan_async};
use crate::survival_construction::{
    ConstructionScope, ConstructionSite, HypotheticalConstructionPlan,
};
use crate::survival_error::SurvivalErrorCode;
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
struct PlanningInput {
    source: SourceIdentity,
    site: ConstructionSite,
    supplied: BTreeMap<String, usize>,
    temporary_material: String,
    limits: SearchLimits,
    specification: GroundedBuildingDesignRequest,
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
fn failure(code: impl serde::Serialize, detail: impl std::fmt::Display) -> Value {
    json!({"ok":false,"schema_version":"dustroute.survival-job.v1",
        "error":{"code":code,"detail":detail.to_string()},"automatic_replay":false})
}
fn region(r: dustroute_translate::world::Region) -> voxrig::Region {
    voxrig::Region {
        min: [r.min.x, r.min.y, r.min.z],
        max: [r.max.x, r.max.y, r.max.z],
    }
}
fn save(path: &Path, value: &impl serde::Serialize) -> Result<(), String> {
    let bytes = serde_json::to_vec_pretty(value).map_err(|e| e.to_string())?;
    if bytes.len() > 16 * 1024 * 1024 {
        return Err("survival job record exceeds bound".into());
    }
    crate::storage::replace(path, &bytes, crate::storage::Durability::FileAndDirectory)
        .map_err(|e| e.to_string())
}
fn load<T: serde::de::DeserializeOwned>(path: &Path) -> Result<T, String> {
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
                let saved_specification = (*specification).clone();
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
                if !entry.allows_checkpoint() {
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
                if entry.request_cancel() {
                    if let Err(e) = save(
                        &self
                            .state_store
                            .survival_job_root()
                            .join(job_id.to_string())
                            .join("status.json"),
                        &entry.status(),
                    ) {
                        return failure("journal_io", e);
                    }
                }
                json!({"ok":true,"job_id":job_id,"cancel_requested":true,"immediate_native_abort":false,"next_step":"get job; outstanding operations may still require inspection"})
            }
        }
    }
}

#[cfg(test)]
mod tests;

mod jobs;
use jobs::{Entry, StartReadiness};

mod model;
use model::{ConstructionSpecification, InspectionReason, JobManifest, JobSchema, JobStatus};

mod continuation;
mod execution;
mod planning;
mod records;

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
