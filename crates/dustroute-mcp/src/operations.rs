use std::collections::HashMap;
use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};
use serde_json::Value;
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
    TransitionProposal,
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

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct OperationRecord {
    pub id: Uuid,
    pub kind: OperationKind,
    pub status: OperationStatus,
    pub progress_percent: u8,
    pub message: String,
    pub created_at_unix_ms: u128,
    pub updated_at_unix_ms: u128,
    pub completed_at_unix_ms: Option<u128>,
    pub result: Option<Value>,
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
        }
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
            entry.record.status = status;
            entry.record.progress_percent = progress_percent.min(100);
            entry.record.message = message.into();
            entry.record.updated_at_unix_ms = now_unix_ms();
        }
    }

    pub async fn complete(&self, id: Uuid, result: Value) {
        if let Some(entry) = self.entries.lock().await.get_mut(&id) {
            let now = now_unix_ms();
            entry.record.status = OperationStatus::Completed;
            entry.record.progress_percent = 100;
            entry.record.message = "completed".to_owned();
            entry.record.result = Some(result);
            entry.record.updated_at_unix_ms = now;
            entry.record.completed_at_unix_ms = Some(now);
        }
    }

    pub async fn record_completed(&self, id: Uuid, kind: OperationKind, result: Value) {
        let now = now_unix_ms();
        let mut entries = self.entries.lock().await;
        prune_terminal_entries(&mut entries, self.max_entries.saturating_sub(1));
        entries.insert(
            id,
            OperationEntry {
                record: OperationRecord {
                    id,
                    kind,
                    status: OperationStatus::Completed,
                    progress_percent: 100,
                    message: "completed".to_owned(),
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
        if matches!(
            entry.record.status,
            OperationStatus::Completed | OperationStatus::Failed | OperationStatus::Cancelled
        ) {
            return false;
        }
        entry.cancellation.cancel();
        let now = now_unix_ms();
        entry.record.status = OperationStatus::Cancelled;
        entry.record.message = "cancelled by request".to_owned();
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
    use super::*;

    #[tokio::test]
    async fn records_progress_completion_and_cancellation() {
        let registry = OperationRegistry::default();
        let completed = registry
            .create(OperationKind::AnalyzeRegion, "queued")
            .await;
        registry
            .update(completed, OperationStatus::Running, 50, "analyzing")
            .await;
        registry
            .complete(completed, serde_json::json!({ "ok": true }))
            .await;
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
                .create(OperationKind::RepairProposal, "queued")
                .await;
            registry
                .complete(id, serde_json::json!({ "ok": true }))
                .await;
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
