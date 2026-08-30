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
    pub result: Option<Value>,
}

#[derive(Clone, Debug)]
struct OperationEntry {
    record: OperationRecord,
    cancellation: CancellationToken,
}

#[derive(Clone, Debug, Default)]
pub struct OperationRegistry {
    entries: Arc<Mutex<HashMap<Uuid, OperationEntry>>>,
}

impl OperationRegistry {
    pub async fn create(&self, kind: OperationKind, message: impl Into<String>) -> Uuid {
        let id = Uuid::new_v4();
        let record = OperationRecord {
            id,
            kind,
            status: OperationStatus::Queued,
            progress_percent: 0,
            message: message.into(),
            created_at_unix_ms: SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .map_or(0, |duration| duration.as_millis()),
            result: None,
        };
        self.entries.lock().await.insert(
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
        }
    }

    pub async fn complete(&self, id: Uuid, result: Value) {
        if let Some(entry) = self.entries.lock().await.get_mut(&id) {
            entry.record.status = OperationStatus::Completed;
            entry.record.progress_percent = 100;
            entry.record.message = "completed".to_owned();
            entry.record.result = Some(result);
        }
    }

    pub async fn record_completed(&self, id: Uuid, kind: OperationKind, result: Value) {
        self.entries.lock().await.insert(
            id,
            OperationEntry {
                record: OperationRecord {
                    id,
                    kind,
                    status: OperationStatus::Completed,
                    progress_percent: 100,
                    message: "completed".to_owned(),
                    created_at_unix_ms: SystemTime::now()
                        .duration_since(UNIX_EPOCH)
                        .map_or(0, |duration| duration.as_millis()),
                    result: Some(result),
                },
                cancellation: CancellationToken::new(),
            },
        );
    }

    pub async fn fail(&self, id: Uuid, message: impl Into<String>) {
        if let Some(entry) = self.entries.lock().await.get_mut(&id) {
            entry.record.status = OperationStatus::Failed;
            entry.record.message = message.into();
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
        entry.record.status = OperationStatus::Cancelled;
        entry.record.message = "cancelled by request".to_owned();
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
}
