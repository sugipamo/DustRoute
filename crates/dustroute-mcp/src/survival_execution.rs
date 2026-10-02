//! Caller-authorized, checked fixed sequences. No automatic adoption or plan replay.
//! The caller exclusively owns the source bot for this executor's lifetime.
//! Voxrig retains physical admission, native action guards and retirement receipts.
//! JSON journals diagnose uncertainty; they cannot restore a native session/token.
mod journal;
mod native;
use crate::survival_construction::{
    HypotheticalConstructionPlan, HypotheticalConstructionStep, PlacementPurpose,
};
use journal::Journal;
pub use journal::{
    Continuation, ExecutionDiagnosis, ExecutionEvent, ExecutionRecord, OperationOutcome, diagnose,
};
use serde::Serialize;
use serde_json::json;
use std::{collections::BTreeMap, path::Path};
use voxrig::checked_survival::{CapturedSurvivalScene, HypotheticalBlockEdit, MiningIntent};
use voxrig::{Client, ConnectionConfig, NativeBlockState};

#[derive(Clone, Debug, Serialize)]
pub struct ExecutionError {
    pub code: &'static str,
    pub detail: String,
}
impl ExecutionError {
    fn new(code: &'static str, detail: impl ToString) -> Self {
        Self {
            code,
            detail: detail.to_string(),
        }
    }
}
impl std::fmt::Display for ExecutionError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}: {}", self.code, self.detail)
    }
}
impl std::error::Error for ExecutionError {}
impl From<voxrig::Error> for ExecutionError {
    fn from(e: voxrig::Error) -> Self {
        Self::new("native_refused", e)
    }
}
impl From<std::io::Error> for ExecutionError {
    fn from(e: std::io::Error) -> Self {
        Self::new("journal_io", e)
    }
}
impl From<serde_json::Error> for ExecutionError {
    fn from(e: serde_json::Error) -> Self {
        Self::new("journal_encoding", e)
    }
}
impl From<crate::survival_cleanup::CleanupError> for ExecutionError {
    fn from(e: crate::survival_cleanup::CleanupError) -> Self {
        Self::new(e.code, e.detail)
    }
}
type Result<T> = std::result::Result<T, ExecutionError>;

#[derive(Clone, Debug, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum ExecutionProgress {
    StepCompleted {
        completed: usize,
    },
    MiningStarted {
        target: [i32; 3],
        selected_hotbar: u8,
        attempt: usize,
    },
    Replanned {
        previous_hotbar: u8,
        next_hotbar: u8,
        attempt: usize,
    },
    Completed,
}
struct PendingMining {
    intent: MiningIntent,
    edit: HypotheticalBlockEdit,
    slot: u8,
    attempt: usize,
}

/// Process-local execution only. Dropping/cancelling advance leaves a durable
/// uncertain intent and permanently blocks this executor from further dispatch.
pub struct SurvivalExecutor {
    bot: Client,
    observer: Client,
    reconnect: ConnectionConfig,
    plan: HypotheticalConstructionPlan,
    journal: Journal,
    expected: BTreeMap<[i32; 3], NativeBlockState>,
    temporary: BTreeMap<[i32; 3], NativeBlockState>,
    pending: Option<PendingMining>,
    retry: Option<(u8, usize)>,
    call_active: bool,
}
impl SurvivalExecutor {
    /// Caller explicitly authorizes this checked plan's edit/travel scope and
    /// supplied materials. It must not concurrently operate cloned source clients.
    /// A fresh directory is required. Existing records are diagnosis-only.
    pub async fn create(
        bot: Client,
        observer: Client,
        reconnect: ConnectionConfig,
        plan: HypotheticalConstructionPlan,
        directory: &Path,
    ) -> Result<Self> {
        let ops = bot.survival()?;
        let scene = ops
            .capture_survival_scene(native::region(plan.scope().observed))
            .await?;
        if scene.source().connection_id != plan.source().connection_id
            || scene.source().dimension != plan.source().dimension
            || scene.source().position != plan.source().position
        {
            return Err(ExecutionError::new(
                "plan_source_changed",
                "plan requires the original live starting context",
            ));
        }
        native::check_snapshot(&scene, plan.baseline())?;
        native::require_supplied(
            &ops.player_state().await?,
            &plan.materials().required_supplied,
        )?;
        let expected = native::scene_blocks(&scene)?;
        let journal = Journal::create(
            directory,
            json!({"plan":plan,"capabilities":ops.capabilities(),
                "declared_reconnect":{"host":reconnect.server.host,"port":reconnect.server.port,
                    "username":reconnect.username,"version":reconnect.version}}),
        )?;
        Ok(Self {
            bot,
            observer,
            reconnect,
            plan,
            journal,
            expected,
            temporary: BTreeMap::new(),
            pending: None,
            retry: None,
            call_active: false,
        })
    }
    pub fn record(&self) -> &ExecutionRecord {
        &self.journal.record
    }
    /// The source may have been replaced by a checked fresh native connection.
    /// Exposed for read-only diagnosis/trace retention; concurrent actions violate
    /// this executor's exclusive-source precondition.
    pub fn client(&self) -> &Client {
        &self.bot
    }
    /// Stop further dispatch. This is not a native abort/retirement receipt:
    /// an outstanding operation still requires inspection and explicit retirement.
    pub fn cancel(&mut self) -> Result<()> {
        self.call_active = true;
        self.journal.event("cancelled", self.journal.record.outcome.clone(), Continuation::Cancelled,
            json!({"native_operation_may_remain":true,"remaining_owned_temporary":self.temporary.keys().collect::<Vec<_>>()}))
    }
    /// One bounded transition. Mining START returns before waiting for its result;
    /// the next call collects the result and retires the old native connection.
    pub async fn advance(&mut self) -> Result<ExecutionProgress> {
        if self.call_active
            || matches!(
                self.record().continuation,
                Continuation::NeedsInspection | Continuation::Cancelled
            )
        {
            return Err(ExecutionError::new(
                "execution_needs_inspection",
                "previous call interrupted/refused; no automatic replay",
            ));
        }
        if self.record().continuation == Continuation::Completed {
            return Ok(ExecutionProgress::Completed);
        }
        self.call_active = true;
        let result = self.advance_inner().await;
        match result {
            Ok(progress) => {
                self.call_active = false;
                Ok(progress)
            }
            Err(error) => {
                // If this write also fails, the pre-dispatch uncertain intent remains.
                let _ = self.journal.event("stopped", self.journal.record.outcome.clone(), Continuation::NeedsInspection, json!({"error":error,"remaining_owned_temporary":self.temporary.keys().collect::<Vec<_>>()}));
                Err(error)
            }
        }
    }
    async fn current_scene(&self) -> Result<CapturedSurvivalScene> {
        let scene = self
            .bot
            .survival()?
            .capture_survival_scene(native::region(self.plan.scope().observed))
            .await?;
        let actual = native::scene_blocks(&scene)?;
        if actual != self.expected {
            return Err(ExecutionError::new(
                "site_changed",
                "fresh scene differs from exact last confirmed site; no further dispatch",
            ));
        }
        let idle = scene.scenario().preview_path(
            &[voxrig::checked_survival::SurvivalControl {
                yaw: 0.0,
                input: Default::default(),
            }; 3],
        )?;
        if !crate::survival_navigation::hypothetical_admissible(&idle, self.plan.scope().travel) {
            return Err(ExecutionError::new(
                "body_scope_changed",
                "fresh body/standing clearance leaves permitted travel scope",
            ));
        }
        Ok(scene)
    }
    async fn advance_inner(&mut self) -> Result<ExecutionProgress> {
        if let Some(pending) = self.pending.take() {
            return self.finish_mining(pending).await;
        }
        let index = self.record().completed_steps;
        if index == self.plan.steps().len() {
            let scene = self.current_scene().await?;
            native::check_snapshot(&scene, self.plan.expected())?;
            let p = scene.source().position;
            let retreat = self.plan.scope().retreat;
            if !self.temporary.is_empty()
                || (0..3).any(|i| p[i] < retreat.min[i] || p[i] > retreat.max[i])
            {
                return Err(ExecutionError::new(
                    "final_obligation_missing",
                    "temporary cleanup or safe retreat incomplete",
                ));
            }
            let independent = self.observer.observe_region(scene.region()).await?;
            if independent.blocks.len() != self.expected.len()
                || independent
                    .blocks
                    .iter()
                    .any(|b| b.state.as_ref() != self.expected.get(&b.position))
            {
                return Err(ExecutionError::new(
                    "final_observer_mismatch",
                    "independent final site differs",
                ));
            }
            self.journal.event("completed", OperationOutcome::Observed, Continuation::Completed,
                json!({"remaining_owned_temporary":[],"independently_checked_cells":independent.blocks.len(),"position":p}))?;
            return Ok(ExecutionProgress::Completed);
        }
        let scene = self.current_scene().await?;
        match self.plan.steps()[index].clone() {
            HypotheticalConstructionStep::Place { purpose, placement } => {
                self.place(&scene, &placement).await?;
                self.expected
                    .insert(placement.edit.position, placement.edit.after.clone());
                if matches!(purpose, PlacementPurpose::Temporary) {
                    self.temporary
                        .insert(placement.edit.position, placement.edit.after);
                }
                self.complete_step()
            }
            HypotheticalConstructionStep::Move { prediction } => {
                self.move_along(&prediction).await?;
                self.complete_step()
            }
            HypotheticalConstructionStep::RemoveTemporary {
                edit,
                face_id,
                rotation,
            } => {
                if self.temporary.get(&edit.position) != Some(&edit.before) {
                    return Err(ExecutionError::new(
                        "temporary_not_owned",
                        "only observed placements from this execution may be removed",
                    ));
                }
                self.start_mining(&scene, edit, face_id, rotation).await
            }
        }
    }
    fn complete_step(&mut self) -> Result<ExecutionProgress> {
        self.journal.record.completed_steps += 1;
        self.journal.event(
            "step_completed",
            OperationOutcome::Observed,
            Continuation::Revalidate,
            json!({"remaining_owned_temporary":self.temporary.keys().collect::<Vec<_>>()}),
        )?;
        Ok(ExecutionProgress::StepCompleted {
            completed: self.record().completed_steps,
        })
    }
}
