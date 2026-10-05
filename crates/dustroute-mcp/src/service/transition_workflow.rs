//! Executes a transition attempt independently of MCP argument and reply encoding.
//! The session retains mutation exclusivity through preparation, contract decoding,
//! execution, restoration and result encoding by the caller.
use super::operation_lifecycle::InvocationState;
use super::operation_plans::OperationPlans;
use crate::api::McpErrorCode;
use crate::failure::FailureCause;
use crate::operations::transition::TransitionRefusal;
use crate::{BotBridge, McpPolicy, OperationRegistry, TransitionSafetyAssessment};
use dustroute_physical::{PhysicalScene, Pos};
use dustroute_translate::snapshot::MinecraftSnapshot;
use dustroute_translate::world_reverse::RegionBounds;
use tokio::sync::{Mutex, MutexGuard};
use uuid::Uuid;

mod analysis;
mod preparation;
mod restoration;
mod run;

#[derive(Clone, Debug)]
pub(super) struct StoredTransitionPlan {
    pub player: String,
    pub dimension: String,
    pub bounds: RegionBounds,
    pub lever: Pos,
    pub original_powered: bool,
    pub initial_snapshot: MinecraftSnapshot,
    pub observation_ticks: u16,
    pub max_events: usize,
    pub safety: TransitionSafetyAssessment,
    pub lifecycle: InvocationState,
}

pub(super) struct TransitionWorkflow<'a> {
    pub bridge: &'a BotBridge,
    pub policy: &'a McpPolicy,
    pub plans: &'a OperationPlans,
    pub operations: &'a OperationRegistry,
    pub mutation_lock: &'a Mutex<()>,
}

/// Exclusivity is acquired before operation-id decoding and retained until the
/// caller has encoded the result. Dropping a session releases it.
pub(super) struct TransitionSession<'a> {
    bridge: &'a BotBridge,
    policy: &'a McpPolicy,
    plans: &'a OperationPlans,
    operations: &'a OperationRegistry,
    _mutation_guard: MutexGuard<'a, ()>,
}

/// Freshly checked inputs for a run. Construction stays inside this workflow;
/// the MCP caller can inspect the scene to decode signal contracts, then run
/// through the same borrowed session. Prepared inputs cannot outlive its lock.
pub(super) struct PreparedRun<'session> {
    session: &'session TransitionSession<'session>,
    operation_id: Uuid,
    plan: StoredTransitionPlan,
    scene: PhysicalScene,
}
impl PreparedRun<'_> {
    pub fn scene(&self) -> &PhysicalScene {
        &self.scene
    }
}

impl<'a> TransitionWorkflow<'a> {
    pub async fn admit(&self, confirmed: bool) -> Result<TransitionSession<'a>, TransitionRefusal> {
        if !confirmed {
            return Err(TransitionRefusal::coded(
                McpErrorCode::InvalidArgument,
                "confirm=true is required",
            ));
        }
        self.policy
            .authorize_mutation()
            .map_err(FailureCause::from)?;
        Ok(TransitionSession {
            bridge: self.bridge,
            policy: self.policy,
            plans: self.plans,
            operations: self.operations,
            _mutation_guard: self.mutation_lock.lock().await,
        })
    }
}

impl TransitionSession<'_> {
    async fn plan(&self, operation_id: Uuid) -> Result<StoredTransitionPlan, TransitionRefusal> {
        self.plans
            .table::<StoredTransitionPlan>()
            .lock()
            .await
            .get(&operation_id)
            .cloned()
            .ok_or_else(|| {
                TransitionRefusal::coded(McpErrorCode::NotFound, "transition scenario not found")
            })
    }
}
