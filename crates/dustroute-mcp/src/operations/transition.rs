//! One transition attempt's evidence and cleanup. Simulation results are
//! separate from live execution, restoration and replay authority.
use super::mutation::Success;
use crate::api::{TRANSITION_SCHEMA_V1, TransitionTraceResponse};
use crate::bridge::LeverActivation;
use crate::failure::{ExecutionProgress, FailureReport};
use dustroute_ir::{BehaviorTrace, TransientAssessment, TransitionTrace};
use dustroute_translate::scenario::{Scenario, ScenarioDifference, ScenarioRun, ScenarioTrace};
use serde::{Serialize, Serializer};
use uuid::Uuid;

#[derive(Clone, Debug, PartialEq)]
pub(crate) enum TransitionOutcome {
    Verified(ExecutionProgress),
    Failed(Box<FailureReport>),
}
impl TransitionOutcome {
    pub fn from_attempt(progress: ExecutionProgress, failure: Option<FailureReport>) -> Self {
        match failure {
            Some(mut report) => {
                *report.progress = progress;
                Self::Failed(Box::new(report))
            }
            None => Self::Verified(progress),
        }
    }
    fn failed(&self) -> bool {
        matches!(self, Self::Failed(_))
    }
    fn progress(&self) -> &ExecutionProgress {
        match self {
            Self::Verified(progress) => progress,
            Self::Failed(report) => &report.progress,
        }
    }
}
impl Serialize for TransitionOutcome {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        #[derive(Serialize)]
        struct Verified<'a> {
            schema_version: &'static str,
            ok: Success,
            execution_progress: &'a ExecutionProgress,
        }
        match self {
            Self::Verified(progress) => Verified {
                schema_version: TRANSITION_SCHEMA_V1,
                ok: Success,
                execution_progress: progress,
            }
            .serialize(serializer),
            Self::Failed(report) => report.as_response().serialize(serializer),
        }
    }
}
#[derive(Clone, Debug, PartialEq)]
pub(crate) enum ScenarioSimulation {
    Succeeded(Box<ScenarioRun>),
    Failed(String),
}
impl From<Result<ScenarioRun, String>> for ScenarioSimulation {
    fn from(result: Result<ScenarioRun, String>) -> Self {
        match result {
            Ok(run) => Self::Succeeded(Box::new(run)),
            Err(error) => Self::Failed(error),
        }
    }
}
impl Serialize for ScenarioSimulation {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        #[derive(Serialize)]
        struct Run<'a> {
            label: &'a str,
            safety: dustroute_translate::scenario::ScenarioSafety,
            trace: TransitionTraceResponse,
            differences: &'a [ScenarioDifference],
        }
        #[derive(Serialize)]
        struct Succeeded<'a> {
            ok: Success,
            run: Run<'a>,
        }
        #[derive(Serialize)]
        struct Failed<'a> {
            ok: bool,
            error: &'a str,
        }
        match self {
            Self::Succeeded(run) => Succeeded {
                ok: Success,
                run: Run {
                    label: &run.label,
                    safety: run.safety,
                    trace: TransitionTraceResponse::from(&run.trace),
                    differences: &run.differences,
                },
            }
            .serialize(serializer),
            Self::Failed(error) => Failed { ok: false, error }.serialize(serializer),
        }
    }
}
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct ScenarioVerification {
    pub scenario: Scenario,
    pub simulated: ScenarioSimulation,
    pub live_trace: ScenarioTrace,
    pub differences: Option<Vec<ScenarioDifference>>,
    pub trace_equivalent: bool,
    pub steady_state_equivalent: bool,
}
impl Serialize for ScenarioVerification {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        #[derive(Serialize)]
        struct View<'a> {
            scenario: &'a Scenario,
            simulated: &'a ScenarioSimulation,
            live_trace: TransitionTraceResponse,
            differences: &'a Option<Vec<ScenarioDifference>>,
            trace_equivalent: bool,
            steady_state_equivalent: bool,
        }
        View {
            scenario: &self.scenario,
            simulated: &self.simulated,
            live_trace: TransitionTraceResponse::from(&self.live_trace),
            differences: &self.differences,
            trace_equivalent: self.trace_equivalent,
            steady_state_equivalent: self.steady_state_equivalent,
        }
        .serialize(serializer)
    }
}
#[derive(Clone, Debug, PartialEq, Serialize)]
pub(crate) struct RecordingSummary {
    pub started_game_tick: u64,
    pub stopped_game_tick: u64,
    pub seen_events: usize,
    pub stored_events: usize,
    pub truncated: bool,
}
#[derive(Clone, Debug, PartialEq, Serialize)]
pub(crate) struct RestorationSummary {
    pub lever_restored: bool,
    pub region_restored: bool,
    pub verified: bool,
    pub activation_error: Option<String>,
    pub wait_error: Option<String>,
    pub block_read_error: Option<String>,
    pub region_read_error: Option<String>,
}
#[derive(Clone, Debug, PartialEq, Serialize)]
pub(crate) struct TransitionRunDetails {
    pub operation_id: Uuid,
    pub activation: LeverActivation,
    pub observation_ticks: u16,
    pub recording: RecordingSummary,
    pub trace: BehaviorTrace,
    pub transition_trace: TransitionTrace,
    pub transient_assessment: TransientAssessment,
    pub scenario_verification: ScenarioVerification,
    pub restoration: RestorationSummary,
    pub wait_error: Option<String>,
    pub guidance: &'static str,
    #[serde(flatten)]
    pub outcome: TransitionOutcome,
}
#[derive(Clone, Debug, PartialEq)]
pub struct TransitionRunResult(TransitionRunBody);
#[derive(Clone, Debug, PartialEq)]
enum TransitionRunBody {
    Completed(Box<TransitionRunDetails>),
    Failed {
        report: Box<FailureReport>,
        restoration_verified: Option<bool>,
    },
}
impl TransitionRunResult {
    pub(crate) fn completed(details: TransitionRunDetails) -> Self {
        Self(TransitionRunBody::Completed(Box::new(details)))
    }
    pub(crate) fn failed_attempt(
        report: FailureReport,
        restoration_verified: Option<bool>,
    ) -> Self {
        Self(TransitionRunBody::Failed {
            report: Box::new(report),
            restoration_verified,
        })
    }
    pub fn failed(&self) -> bool {
        match &self.0 {
            TransitionRunBody::Completed(details) => details.outcome.failed(),
            TransitionRunBody::Failed { .. } => true,
        }
    }
    pub fn progress(&self) -> &ExecutionProgress {
        match &self.0 {
            TransitionRunBody::Completed(details) => details.outcome.progress(),
            TransitionRunBody::Failed { report, .. } => &report.progress,
        }
    }
}
impl Serialize for TransitionRunResult {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        #[derive(Serialize)]
        struct Failed<'a> {
            #[serde(flatten)]
            report: crate::failure::AttemptResponse<'a>,
            #[serde(skip_serializing_if = "Option::is_none")]
            restoration_verified: &'a Option<bool>,
        }
        match &self.0 {
            TransitionRunBody::Completed(details) => details.serialize(serializer),
            TransitionRunBody::Failed {
                report,
                restoration_verified,
            } => Failed {
                report: report.as_response(),
                restoration_verified,
            }
            .serialize(serializer),
        }
    }
}
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct TransitionRestoreResult {
    operation_id: Uuid,
    restoration_verified: bool,
    activation_error: Option<String>,
    natural_restore_verified: bool,
    snapshot_restore_attempted: bool,
    snapshot_restore_error: Option<String>,
    #[serde(flatten)]
    outcome: TransitionOutcome,
}
impl TransitionRestoreResult {
    pub(crate) fn completed(
        operation_id: Uuid,
        restoration_verified: bool,
        activation_error: Option<String>,
        natural_restore_verified: bool,
        snapshot_restore: Option<Result<crate::bridge_protocol::CommandSubmission, String>>,
        outcome: TransitionOutcome,
    ) -> Self {
        Self {
            operation_id,
            restoration_verified,
            activation_error,
            natural_restore_verified,
            snapshot_restore_attempted: snapshot_restore.is_some(),
            snapshot_restore_error: snapshot_restore.and_then(Result::err),
            outcome,
        }
    }
    pub fn failed(&self) -> bool {
        self.outcome.failed()
    }
    pub fn progress(&self) -> &ExecutionProgress {
        self.outcome.progress()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::failure::{CauseKind, FailureCause, FailurePhase, WorldOutcome};
    use dustroute_physical::Pos;
    use dustroute_translate::snapshot::MinecraftSnapshot;
    use serde_json::json;
    use std::collections::BTreeMap;

    #[test]
    fn early_failure_keeps_cleanup_and_restoration_facts_without_inventing_detail() {
        let progress = ExecutionProgress {
            operation_consumed: true,
            world: WorldOutcome::Verified,
            verified_steps: 2,
            total_changes: Some(2),
            ..Default::default()
        };
        let mut report = progress.cause(
            FailureCause::new(CauseKind::Connection, "recording lost")
                .at(FailurePhase::AfterReadback),
        );
        report.secondary(
            FailureCause::new(CauseKind::Timeout, "cleanup timed out").at(FailurePhase::Restore),
        );
        let with = TransitionRunResult::failed_attempt(report.clone(), Some(true));
        assert!(with.failed());
        assert_eq!(with.progress().verified_steps, 2);
        let wire = serde_json::to_value(with).unwrap();
        assert_eq!(wire["ok"], false);
        assert_eq!(wire["restoration_verified"], true);
        assert_eq!(wire["failure"]["primary"]["message"], "recording lost");
        assert_eq!(
            wire["failure"]["secondary"][0]["message"],
            "cleanup timed out"
        );
        assert_eq!(wire["retry_allowed"], false);
        for missing in ["operation_id", "recording", "scenario_verification"] {
            assert!(wire.get(missing).is_none());
        }
        let absent =
            serde_json::to_value(TransitionRunResult::failed_attempt(report, None)).unwrap();
        assert!(absent.get("restoration_verified").is_none());
    }

    #[test]
    fn simulation_failure_remains_separate_from_verified_live_execution() {
        let position = Pos::new(1, 64, -2);
        let scenario = Scenario {
            label: "toggle".into(),
            initial: MinecraftSnapshot {
                min: position,
                max: position,
                blocks: vec![],
            },
            actions: vec![],
            observe: [position].into(),
            duration_redstone_ticks: 1,
            required_capabilities: vec![],
            expectation: Default::default(),
        };
        let result = TransitionRunResult::completed(TransitionRunDetails {
            operation_id: Uuid::from_u128(1),
            activation: LeverActivation {
                pos: position,
                before_powered: false,
                after_powered: true,
                bot_approached: false,
            },
            observation_ticks: 2,
            recording: RecordingSummary {
                started_game_tick: 1,
                stopped_game_tick: 3,
                seen_events: 0,
                stored_events: 0,
                truncated: false,
            },
            trace: Default::default(),
            transition_trace: Default::default(),
            transient_assessment: Default::default(),
            scenario_verification: ScenarioVerification {
                scenario,
                simulated: ScenarioSimulation::Failed("model unavailable".into()),
                live_trace: ScenarioTrace {
                    final_strengths: BTreeMap::from([(position, 15)]),
                    final_powered: BTreeMap::from([(position, true)]),
                    ..Default::default()
                },
                differences: None,
                trace_equivalent: false,
                steady_state_equivalent: false,
            },
            restoration: RestorationSummary {
                lever_restored: true,
                region_restored: true,
                verified: true,
                activation_error: None,
                wait_error: None,
                block_read_error: None,
                region_read_error: None,
            },
            wait_error: None,
            guidance: "inspect",
            outcome: TransitionOutcome::Verified(ExecutionProgress {
                operation_consumed: true,
                world: WorldOutcome::Verified,
                verified_steps: 2,
                total_changes: Some(2),
                ..Default::default()
            }),
        });
        assert!(!result.failed());
        let wire = serde_json::to_value(result).unwrap();
        assert_eq!(wire["ok"], true);
        assert_eq!(wire["schema_version"], TRANSITION_SCHEMA_V1);
        let simulation = &wire["scenario_verification"];
        assert_eq!(
            simulation["simulated"],
            json!({"ok":false,"error":"model unavailable"})
        );
        assert_eq!(simulation["differences"], json!(null));
        assert_eq!(simulation["trace_equivalent"], false);
        assert_eq!(
            simulation["live_trace"]["final_strengths"],
            json!([{"position":position,"strength":15}])
        );
        assert_eq!(
            simulation["live_trace"]["final_powered"],
            json!([{"position":position,"powered":true}])
        );
        for missing in ["error", "status", "retry_allowed", "failure"] {
            assert!(wire.get(missing).is_none());
        }
    }
}
