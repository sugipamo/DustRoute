//! Recorded verifier diagnostics, without a model, executable state or authority.
use crate::abstract_behavior::{AbstractBehaviorReport, HISTORY_ABSTRACTION_METHOD};
use crate::behavior_type::{BehaviorCounterexample, BehaviorTypeReport};
use crate::finite_burst::{FiniteBurstCessation, FiniteBurstTypeReport};
use crate::periodic::{PeriodicCycle, PeriodicTypeReport};
use crate::promotion::CheckStatus;
use dustroute_library::blueprint::TypeRevisionId;
use serde::{Deserialize, Serialize};

/// Different report shapes stay typed, while retaining their public field names.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(untagged)]
pub enum BehaviorDiagnostics {
    Concrete(Box<ConcreteReport>),
    FiniteBurst(Box<FiniteBurstReport>),
    Periodic(Box<PeriodicReport>),
    Abstract {
        abstraction_method: String,
        report: AbstractReport,
    },
}
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ConcreteReport {
    pub type_revision: TypeRevisionId,
    pub status: CheckStatus,
    pub reachable_states: usize,
    pub evaluated_steps: usize,
    pub graph_closed: bool,
    pub detail: String,
    #[serde(deserialize_with = "required_option")]
    pub counterexample: Option<BehaviorCounterexample>,
}
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct FiniteBurstReport {
    pub type_revision: TypeRevisionId,
    pub status: CheckStatus,
    pub reachable_states: usize,
    pub evaluated_steps: usize,
    pub detail: String,
    #[serde(deserialize_with = "required_option")]
    pub cessation: Option<FiniteBurstCessation>,
}
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct PeriodicReport {
    pub type_revision: TypeRevisionId,
    pub status: CheckStatus,
    pub reachable_states: usize,
    pub evaluated_steps: usize,
    pub detail: String,
    #[serde(deserialize_with = "required_option")]
    pub cycle: Option<PeriodicCycle>,
}
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct AbstractReport {
    pub type_revision: TypeRevisionId,
    pub proof_method: String,
    pub status: CheckStatus,
    pub abstract_states: usize,
    pub evaluated_transitions: usize,
    pub detail: String,
}
impl From<&BehaviorTypeReport> for BehaviorDiagnostics {
    fn from(report: &BehaviorTypeReport) -> Self {
        Self::Concrete(Box::new(ConcreteReport {
            type_revision: report.type_revision.clone(),
            status: report.status,
            reachable_states: report.reachable_states,
            evaluated_steps: report.evaluated_steps,
            graph_closed: report.graph_closed,
            detail: report.detail.clone(),
            counterexample: report.counterexample.clone(),
        }))
    }
}
impl From<&FiniteBurstTypeReport> for BehaviorDiagnostics {
    fn from(report: &FiniteBurstTypeReport) -> Self {
        Self::FiniteBurst(Box::new(FiniteBurstReport {
            type_revision: report.type_revision.clone(),
            status: report.status,
            reachable_states: report.reachable_states,
            evaluated_steps: report.evaluated_steps,
            detail: report.detail.clone(),
            cessation: report.cessation.clone(),
        }))
    }
}
impl From<&PeriodicTypeReport> for BehaviorDiagnostics {
    fn from(report: &PeriodicTypeReport) -> Self {
        Self::Periodic(Box::new(PeriodicReport {
            type_revision: report.type_revision.clone(),
            status: report.status,
            reachable_states: report.reachable_states,
            evaluated_steps: report.evaluated_steps,
            detail: report.detail.clone(),
            cycle: report.cycle.clone(),
        }))
    }
}
impl From<&AbstractBehaviorReport> for BehaviorDiagnostics {
    fn from(report: &AbstractBehaviorReport) -> Self {
        Self::Abstract {
            abstraction_method: HISTORY_ABSTRACTION_METHOD.into(),
            report: AbstractReport {
                type_revision: report.type_revision.clone(),
                proof_method: report.proof_method.into(),
                status: report.status,
                abstract_states: report.abstract_states,
                evaluated_transitions: report.evaluated_transitions,
                detail: report.detail.clone(),
            },
        }
    }
}
impl BehaviorDiagnostics {
    pub fn counterexample(&self) -> Option<&BehaviorCounterexample> {
        match self {
            Self::Concrete(report) => report.counterexample.as_ref(),
            _ => None,
        }
    }
}
impl std::fmt::Display for BehaviorDiagnostics {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Concrete(r) => write!(
                f,
                "{:?}; states={}, steps={}, graph_closed={}: {}",
                r.status, r.reachable_states, r.evaluated_steps, r.graph_closed, r.detail
            ),
            Self::FiniteBurst(r) => write!(
                f,
                "{:?}; states={}, steps={}: {}",
                r.status, r.reachable_states, r.evaluated_steps, r.detail
            ),
            Self::Periodic(r) => write!(
                f,
                "{:?}; states={}, steps={}: {}",
                r.status, r.reachable_states, r.evaluated_steps, r.detail
            ),
            Self::Abstract {
                abstraction_method,
                report: r,
            } => write!(
                f,
                "{:?}; method={}, states={}, transitions={}: {}",
                r.status, abstraction_method, r.abstract_states, r.evaluated_transitions, r.detail
            ),
        }
    }
}

fn required_option<'de, D, T>(decoder: D) -> Result<Option<T>, D::Error>
where
    D: serde::Deserializer<'de>,
    T: Deserialize<'de>,
{
    Option::<T>::deserialize(decoder)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    #[test]
    fn incomplete_or_mixed_verifier_shapes_are_not_recategorized() {
        let mut report = json!({"type_revision":"example.v1", "status":"undetermined",
            "reachable_states":0, "evaluated_steps":0, "detail":"budget exhausted"});
        assert!(serde_json::from_value::<BehaviorDiagnostics>(report.clone()).is_err());
        report["cycle"] = json!(null);
        assert!(matches!(
            serde_json::from_value::<BehaviorDiagnostics>(report.clone()).unwrap(),
            BehaviorDiagnostics::Periodic(_)
        ));
        report["cessation"] = json!(null);
        assert!(serde_json::from_value::<BehaviorDiagnostics>(report).is_err());
    }
}
