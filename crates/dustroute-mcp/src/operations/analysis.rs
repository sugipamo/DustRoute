//! Completed analysis is diagnostic history, never live execution progress or authority.
use super::mutation::UnrecordedFailure;
use crate::recorded_analysis::reports::{HierarchicalAnalysisReport, ReverseAnalysisReport};
use serde::Serialize;

#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct AnalysisResult {
    #[serde(flatten)]
    outcome: AnalysisOutcome,
}
#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(untagged)]
enum AnalysisOutcome {
    Flat(Box<ReverseAnalysisReport>),
    Hierarchical(Box<HierarchicalAnalysisReport>),
    Failed(UnrecordedFailure),
}
impl From<ReverseAnalysisReport> for AnalysisResult {
    fn from(report: ReverseAnalysisReport) -> Self {
        Self {
            outcome: AnalysisOutcome::Flat(Box::new(report)),
        }
    }
}
impl From<HierarchicalAnalysisReport> for AnalysisResult {
    fn from(report: HierarchicalAnalysisReport) -> Self {
        Self {
            outcome: AnalysisOutcome::Hierarchical(Box::new(report)),
        }
    }
}
impl From<crate::failure::FailureCause> for AnalysisResult {
    fn from(cause: crate::failure::FailureCause) -> Self {
        Self {
            outcome: AnalysisOutcome::Failed(cause.into()),
        }
    }
}
impl AnalysisResult {
    pub fn failed(&self) -> bool {
        matches!(self.outcome, AnalysisOutcome::Failed(_))
    }
}
#[derive(Debug, Serialize)]
pub(crate) struct QueuedAnalysis {
    pub ok: super::mutation::Success,
    pub operation_id: uuid::Uuid,
    pub status: super::OperationStatus,
    pub next_step: &'static str,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::failure::{CauseKind, FailureCause, FailurePhase};
    use crate::operations::{ActivityAction, OperationKind, OperationRegistry, OperationStatus};

    #[tokio::test]
    async fn native_analysis_history_keeps_diagnostic_failure_separate_from_execution() {
        let (bounds, mut translated) = crate::recorded_analysis::reports::tests::fixture(0);
        translated.truth_table_error = Some(
            dustroute_translate::world_reverse::TruthTableError::BudgetExceeded {
                rows: 4,
                max_rows: 1,
                estimated_work_units: 40,
                max_work_units: 1,
            },
        );
        let report = crate::recorded_analysis::reports::reverse_report(bounds, &translated);
        let expected = serde_json::to_value(&report).unwrap();
        let result = AnalysisResult::from(report);
        assert!(!result.failed());
        assert_eq!(serde_json::to_value(&result).unwrap(), expected);
        let registry = OperationRegistry::default();
        let id = registry
            .create(OperationKind::AnalyzeRegion, "queued")
            .await;
        let guard = registry
            .begin_activity(id, ActivityAction::AnalyzeRegion)
            .await
            .unwrap();
        registry.complete(id, result.into()).await;
        let stored = registry.get(id).await.unwrap();
        assert_eq!(stored.status, OperationStatus::Completed);
        assert_eq!(stored.progress_percent, 100);
        let result = stored.result.unwrap();
        assert!(result.progress().is_none());
        assert!(!result.consumed());
        assert!(serde_json::to_value(registry.activity(id).await.unwrap()).unwrap()["execution_progress"].is_null());
        drop(guard);
        assert!(!registry.activity(id).await.unwrap().active);

        for phase in [
            FailurePhase::BeforeReadback,
            FailurePhase::Normalization,
            FailurePhase::Analysis,
        ] {
            let id = registry
                .create(OperationKind::AnalyzeRegion, "queued")
                .await;
            registry
                .complete(
                    id,
                    AnalysisResult::from(
                        FailureCause::new(CauseKind::Unknown, "analysis unavailable").at(phase),
                    )
                    .into(),
                )
                .await;
            let stored = registry.get(id).await.unwrap();
            assert_eq!(stored.status, OperationStatus::Failed);
            assert_eq!(stored.progress_percent, 0);
            let result = stored.result.unwrap();
            assert!(result.progress().is_none());
            assert!(!result.consumed());
            assert!(serde_json::to_value(result).unwrap()["failure"]["progress"].is_null());
        }
    }
    #[tokio::test]
    async fn cancelled_analysis_ignores_late_completion_and_failure() {
        let registry = OperationRegistry::default();
        let id = registry
            .create(OperationKind::AnalyzeRegion, "queued")
            .await;
        assert!(registry.cancel(id).await);
        let before = registry.get(id).await.unwrap();
        let (bounds, translated) = crate::recorded_analysis::reports::tests::fixture(0);
        registry
            .complete(
                id,
                AnalysisResult::from(crate::recorded_analysis::reports::reverse_report(
                    bounds,
                    &translated,
                ))
                .into(),
            )
            .await;
        registry
            .complete(
                id,
                AnalysisResult::from(FailureCause::new(CauseKind::Timeout, "late reply")).into(),
            )
            .await;
        assert_eq!(registry.get(id).await.unwrap(), before);
        assert_eq!(before.status, OperationStatus::Cancelled);
        assert!(before.result.is_none());
    }
}
