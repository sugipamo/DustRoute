pub mod analysis;
pub mod construction;
pub mod mutation;
pub mod piston;
pub mod preview;
mod result;
pub mod transition;
pub use result::OperationResult;
#[path = "operation_activity.rs"]
mod activity;
pub(crate) use activity::{Activity, ActivityAction, ActivityGuard, ActivitySnapshot};
use std::collections::HashMap;
use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};
use tokio::sync::Mutex;
use tokio_util::sync::CancellationToken;
use uuid::Uuid;

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum OperationKind {
    AnalyzeRegion,
    PlacementPreview,
    PlacementApply,
    PlacementUndo,
    RepairProposal,
    RepairApply,
    RepairUndo,
    OptimizationProposal,
    TransitionProposal,
    PistonDoorProposal,
    PistonDoorRun,
    TransitionRun,
    TransitionRestore,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum OperationStatus {
    Queued,
    Running,
    Completed,
    Failed,
    Cancelled,
}

#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct OperationRecord {
    pub id: Uuid,
    pub kind: OperationKind,
    pub status: OperationStatus,
    pub progress_percent: u8,
    pub message: String,
    pub created_at_unix_ms: u128,
    pub updated_at_unix_ms: u128,
    pub completed_at_unix_ms: Option<u128>,
    pub result: Option<OperationResult>,
}

#[derive(Clone, Debug)]
struct OperationEntry {
    record: OperationRecord,
    cancellation: CancellationToken,
}

#[derive(Clone, Debug)]
pub struct OperationRegistry {
    entries: Arc<Mutex<HashMap<Uuid, OperationEntry>>>,
    max_entries: usize,
    activities: Arc<Mutex<HashMap<Uuid, Activity>>>,
}

impl Default for OperationRegistry {
    fn default() -> Self {
        Self::with_max_entries(256)
    }
}

fn now_unix_ms() -> u128 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |duration| duration.as_millis())
}

impl OperationRegistry {
    #[must_use]
    pub fn with_max_entries(max_entries: usize) -> Self {
        Self {
            entries: Arc::new(Mutex::new(HashMap::new())),
            max_entries: max_entries.max(1),
            activities: Arc::default(),
        }
    }

    pub(crate) async fn begin_activity(
        &self,
        id: Uuid,
        action: ActivityAction,
    ) -> Option<ActivityGuard> {
        let mut activities = self.activities.lock().await;
        if activities.get(&id).is_some_and(|a| a.snapshot().active) {
            return None; // A concurrent refusal must not replace the running attempt.
        }
        if activities.len() >= self.max_entries {
            activities.retain(|_, activity| activity.snapshot().active);
        }
        if activities.len() >= self.max_entries {
            return None;
        }
        let activity = Activity::new(action);
        activities.insert(id, activity.clone());
        Some(ActivityGuard(activity))
    }
    pub(crate) async fn activity(&self, id: Uuid) -> Option<ActivitySnapshot> {
        self.activities
            .lock()
            .await
            .get(&id)
            .map(Activity::snapshot)
    }

    pub async fn create(&self, kind: OperationKind, message: impl Into<String>) -> Uuid {
        let id = Uuid::new_v4();
        let now = now_unix_ms();
        let record = OperationRecord {
            id,
            kind,
            status: OperationStatus::Queued,
            progress_percent: 0,
            message: message.into(),
            created_at_unix_ms: now,
            updated_at_unix_ms: now,
            completed_at_unix_ms: None,
            result: None,
        };
        let mut entries = self.entries.lock().await;
        prune_terminal_entries(&mut entries, self.max_entries.saturating_sub(1));
        entries.insert(
            id,
            OperationEntry {
                record,
                cancellation: CancellationToken::new(),
            },
        );
        id
    }

    pub async fn update(
        &self,
        id: Uuid,
        status: OperationStatus,
        progress_percent: u8,
        message: impl Into<String>,
    ) {
        if let Some(entry) = self.entries.lock().await.get_mut(&id) {
            if entry.cancellation.is_cancelled() {
                return;
            }
            entry.record.status = status;
            entry.record.progress_percent = progress_percent.min(100);
            entry.record.message = message.into();
            entry.record.updated_at_unix_ms = now_unix_ms();
        }
    }

    pub async fn complete(&self, id: Uuid, result: OperationResult) {
        if let Some(entry) = self.entries.lock().await.get_mut(&id) {
            let now = now_unix_ms();
            if entry.cancellation.is_cancelled() {
                return;
            }
            let failed = result.failed();
            entry.record.status = if failed {
                OperationStatus::Failed
            } else {
                OperationStatus::Completed
            };
            entry.record.progress_percent = result.progress_percent();
            entry.record.message = if failed {
                "failed; inspect result before recovery"
            } else {
                "completed"
            }
            .to_owned();
            entry.record.result = Some(result);
            entry.record.updated_at_unix_ms = now;
            entry.record.completed_at_unix_ms = Some(now);
        }
    }

    pub async fn record_completed(&self, id: Uuid, kind: OperationKind, result: OperationResult) {
        let now = now_unix_ms();
        let mut entries = self.entries.lock().await;
        // A refusal to replay is not a new world attempt. Keep the original
        // mutation's facts under its ID rather than replacing them with an
        // admission error that knows nothing about earlier effects.
        if result.failed()
            && !result.consumed()
            && entries
                .get(&id)
                .and_then(|entry| entry.record.result.as_ref())
                .is_some_and(OperationResult::consumed)
        {
            return;
        }
        prune_terminal_entries(&mut entries, self.max_entries.saturating_sub(1));
        let failed = result.failed();
        let progress_percent = result.progress_percent();
        entries.insert(
            id,
            OperationEntry {
                record: OperationRecord {
                    id,
                    kind,
                    status: if failed {
                        OperationStatus::Failed
                    } else {
                        OperationStatus::Completed
                    },
                    progress_percent,
                    message: if failed {
                        "failed; inspect result before recovery"
                    } else {
                        "completed"
                    }
                    .to_owned(),
                    created_at_unix_ms: now,
                    updated_at_unix_ms: now,
                    completed_at_unix_ms: Some(now),
                    result: Some(result),
                },
                cancellation: CancellationToken::new(),
            },
        );
    }

    pub async fn fail(&self, id: Uuid, message: impl Into<String>) {
        if let Some(entry) = self.entries.lock().await.get_mut(&id) {
            let now = now_unix_ms();
            if entry.cancellation.is_cancelled() {
                return;
            }
            entry.record.status = OperationStatus::Failed;
            entry.record.message = message.into();
            entry.record.updated_at_unix_ms = now;
            entry.record.completed_at_unix_ms = Some(now);
        }
    }

    pub async fn cancel(&self, id: Uuid) -> bool {
        let mut entries = self.entries.lock().await;
        let Some(entry) = entries.get_mut(&id) else {
            return false;
        };
        if entry.record.kind != OperationKind::AnalyzeRegion {
            return false;
        }
        if matches!(
            entry.record.status,
            OperationStatus::Completed | OperationStatus::Failed | OperationStatus::Cancelled
        ) {
            return false;
        }
        entry.cancellation.cancel();
        let now = now_unix_ms();
        entry.record.status = OperationStatus::Cancelled;
        entry.record.message = "cancellation requested; in-flight work may finish".to_owned();
        entry.record.updated_at_unix_ms = now;
        entry.record.completed_at_unix_ms = Some(now);
        true
    }

    pub async fn is_cancelled(&self, id: Uuid) -> bool {
        self.entries
            .lock()
            .await
            .get(&id)
            .is_some_and(|entry| entry.cancellation.is_cancelled())
    }

    pub async fn get(&self, id: Uuid) -> Option<OperationRecord> {
        self.entries
            .lock()
            .await
            .get(&id)
            .map(|entry| entry.record.clone())
    }

    pub async fn list(&self) -> Vec<OperationRecord> {
        let mut records = self
            .entries
            .lock()
            .await
            .values()
            .map(|entry| entry.record.clone())
            .collect::<Vec<_>>();
        records.sort_by_key(|record| (record.created_at_unix_ms, record.id));
        records
    }
}

fn prune_terminal_entries(entries: &mut HashMap<Uuid, OperationEntry>, target_len: usize) {
    if entries.len() <= target_len {
        return;
    }
    let mut terminal = entries
        .values()
        .filter(|entry| {
            matches!(
                entry.record.status,
                OperationStatus::Completed | OperationStatus::Failed | OperationStatus::Cancelled
            )
        })
        .map(|entry| (entry.record.updated_at_unix_ms, entry.record.id))
        .collect::<Vec<_>>();
    terminal.sort_unstable();
    let remove_count = entries.len().saturating_sub(target_len).min(terminal.len());
    for (_, id) in terminal.into_iter().take(remove_count) {
        entries.remove(&id);
    }
}

#[cfg(test)]
mod tests {
    use super::mutation::{PlacementAttempt, PlacementOutcome, RepairAttempt, RepairOutcome};
    use super::*;
    use crate::failure::{
        CauseKind, ExecutionProgress, FailureCause, FailurePhase, PersistenceOutcome, WorldOutcome,
    };

    fn typed_failure(id: Uuid, progress: ExecutionProgress) -> OperationResult {
        PlacementAttempt {
            operation_id: id.to_string(),
            outcome: PlacementOutcome::failed(progress.cause(FailureCause::new(
                CauseKind::Timeout,
                "readback unavailable",
            ))),
        }
        .into()
    }

    #[tokio::test]
    async fn typed_results_preserve_verified_prefix_and_consumed_attempt_across_refusal() {
        let registry = OperationRegistry::default();
        let id = Uuid::new_v4();
        let original = typed_failure(
            id,
            ExecutionProgress {
                world: WorldOutcome::Unknown,
                submitted_changes: Some(10),
                total_changes: Some(10),
                verified_steps: 2,
                operation_consumed: true,
                ..Default::default()
            },
        );
        registry
            .record_completed(id, OperationKind::PlacementApply, original.clone())
            .await;
        let stored = registry.get(id).await.unwrap();
        assert_eq!(stored.status, OperationStatus::Failed);
        assert_eq!(stored.progress_percent, 20);
        let refusal = PlacementAttempt {
            operation_id: id.to_string(),
            outcome: PlacementOutcome::refused("already consumed"),
        };
        assert!(refusal.progress().is_none());
        registry
            .record_completed(id, OperationKind::PlacementApply, refusal.into())
            .await;
        assert_eq!(registry.get(id).await.unwrap().result, Some(original));
    }

    #[tokio::test]
    async fn verified_world_with_failed_final_save_is_not_a_completed_operation() {
        let registry = OperationRegistry::default();
        let id = Uuid::new_v4();
        let progress = ExecutionProgress {
            phase: FailurePhase::FinalSave,
            world: WorldOutcome::Verified,
            verified_steps: 2,
            total_changes: Some(2),
            durable_verified_steps: Some(1),
            persistence: PersistenceOutcome::Uncertain,
            operation_consumed: true,
            ..Default::default()
        };
        let record = RepairAttempt {
            operation_id: id.to_string(),
            outcome: RepairOutcome::failed(progress.cause(FailureCause::new(
                CauseKind::Persistence,
                "final save unavailable",
            ))),
        };
        registry
            .record_completed(id, OperationKind::RepairApply, record.into())
            .await;
        let stored = registry.get(id).await.unwrap();
        assert_eq!(stored.status, OperationStatus::Failed);
        assert_eq!(stored.progress_percent, 100);
        let progress = stored.result.as_ref().unwrap().progress().unwrap();
        assert_eq!(progress.world, WorldOutcome::Verified);
        assert_eq!(progress.durable_verified_steps, Some(1));
        assert_eq!(progress.persistence, PersistenceOutcome::Uncertain);
    }

    #[tokio::test]
    async fn cancellation_cannot_be_reclassified_by_a_late_typed_result() {
        let registry = OperationRegistry::default();
        let id = registry
            .create(OperationKind::AnalyzeRegion, "queued")
            .await;
        assert!(registry.cancel(id).await);
        registry
            .complete(id, typed_failure(id, ExecutionProgress::default()))
            .await;
        let stored = registry.get(id).await.unwrap();
        assert_eq!(stored.status, OperationStatus::Cancelled);
        assert!(stored.result.is_none());
    }

    #[tokio::test]
    async fn serializing_recorded_results_cannot_publish_a_fresh_attempt_progress() {
        let registry = OperationRegistry::default();
        let id = Uuid::new_v4();
        registry
            .record_completed(
                id,
                OperationKind::PlacementApply,
                typed_failure(
                    id,
                    ExecutionProgress {
                        operation_consumed: true,
                        verified_steps: 5,
                        world: WorldOutcome::Unknown,
                        ..Default::default()
                    },
                ),
            )
            .await;
        let guard = registry
            .begin_activity(id, ActivityAction::InvokeOperation)
            .await
            .unwrap();
        crate::performance::with_activity(Some(guard.0.clone()), async {
            let history = registry.get(id).await.unwrap();
            let wire = serde_json::to_value(history).unwrap();
            assert_eq!(wire["result"]["failure"]["progress"]["verified_steps"], 5);
            let live = serde_json::to_value(registry.activity(id).await.unwrap()).unwrap();
            assert!(live["execution_progress"].is_null());
        })
        .await;
    }

    #[tokio::test]
    async fn replay_refusal_does_not_replace_original_consumed_attempt() {
        let registry = OperationRegistry::default();
        let id = Uuid::new_v4();
        let original: OperationResult = RepairAttempt {
            operation_id: id.to_string(),
            outcome: RepairOutcome::failed(
                ExecutionProgress {
                    world: WorldOutcome::Unknown,
                    operation_consumed: true,
                    ..Default::default()
                }
                .cause(FailureCause::new(CauseKind::Timeout, "reply lost")),
            ),
        }
        .into();
        registry
            .record_completed(id, OperationKind::RepairApply, original.clone())
            .await;
        registry
            .record_completed(
                id,
                OperationKind::RepairApply,
                RepairAttempt {
                    operation_id: id.to_string(),
                    outcome: RepairOutcome::refused("already consumed"),
                }
                .into(),
            )
            .await;
        assert_eq!(registry.get(id).await.unwrap().result, Some(original));
    }

    fn completed_analysis() -> OperationResult {
        let bounds = dustroute_translate::world_reverse::RegionBounds::new(
            dustroute_physical::Pos::new(0, 0, 0),
            dustroute_physical::Pos::new(0, 0, 0),
        );
        let translated = dustroute_translate::api::Translator.reverse(
            &dustroute_physical::World::new(),
            dustroute_translate::api::ReverseRequest::new(bounds),
        );
        super::analysis::AnalysisResult::from(crate::recorded_analysis::reports::reverse_report(
            bounds,
            &translated,
        ))
        .into()
    }

    #[tokio::test]
    async fn completed_call_preserves_failure_and_only_verified_progress() {
        let registry = OperationRegistry::default();
        let id = Uuid::new_v4();
        let result = typed_failure(
            id,
            ExecutionProgress {
                submitted_changes: Some(10),
                verified_steps: 2,
                total_changes: Some(10),
                ..Default::default()
            },
        );
        registry
            .record_completed(id, OperationKind::PlacementApply, result.clone())
            .await;
        let stored = registry.get(id).await.unwrap();
        assert_eq!(stored.status, OperationStatus::Failed);
        assert_eq!(stored.progress_percent, 20);
        assert_eq!(stored.result, Some(result.clone()));
        let task = registry
            .create(OperationKind::AnalyzeRegion, "queued")
            .await;
        registry
            .complete(
                task,
                super::analysis::AnalysisResult::from(FailureCause::new(
                    CauseKind::Timeout,
                    "analysis unavailable",
                ))
                .into(),
            )
            .await;
        assert_eq!(
            registry.get(task).await.unwrap().status,
            OperationStatus::Failed
        );
    }

    #[tokio::test]
    async fn records_progress_completion_and_cancellation() {
        let registry = OperationRegistry::default();
        let completed = registry
            .create(OperationKind::AnalyzeRegion, "queued")
            .await;
        registry
            .update(completed, OperationStatus::Running, 50, "analyzing")
            .await;
        registry.complete(completed, completed_analysis()).await;
        assert_eq!(
            registry.get(completed).await.unwrap().status,
            OperationStatus::Completed
        );

        let cancelled = registry
            .create(OperationKind::AnalyzeRegion, "queued")
            .await;
        assert!(registry.cancel(cancelled).await);
        assert!(registry.is_cancelled(cancelled).await);
        assert_eq!(registry.list().await.len(), 2);
    }

    #[tokio::test]
    async fn bounds_terminal_audit_history_without_dropping_active_work() {
        let registry = OperationRegistry::with_max_entries(2);
        let active = registry
            .create(OperationKind::AnalyzeRegion, "active")
            .await;
        registry
            .update(active, OperationStatus::Running, 10, "running")
            .await;
        for _ in 0..3 {
            let id = registry
                .create(OperationKind::AnalyzeRegion, "queued")
                .await;
            registry.complete(id, completed_analysis()).await;
        }

        let records = registry.list().await;
        assert!(records.iter().any(|record| record.id == active));
        assert!(records.len() <= 2);
        assert!(
            records
                .iter()
                .all(|record| record.updated_at_unix_ms >= record.created_at_unix_ms)
        );
    }
}
