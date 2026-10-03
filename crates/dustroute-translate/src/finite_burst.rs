//! Two or more falling edges followed by eventual permanent OFF, proved under
//! a fixed deterministic model. No claim of restartability or physical burnout.
use std::time::Instant;

use dustroute_library::blueprint::{TypeContract, TypeRevision, TypeRevisionId};
use serde::{Deserialize, Serialize};

use crate::autonomous::{TraversalStats, trace_autonomous};
use crate::behavior_type::{BehaviorBudget, BehaviorModel};
use crate::promotion::CheckStatus;

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct FiniteBurstCessation {
    /// Actual ON-to-OFF transitions from the declared initial state.
    pub falling_edges: usize,
    /// Earliest step after which output remains OFF, not a type deadline.
    pub off_from_step: usize,
    /// Internal memory may keep evolving after the output has stopped.
    pub state_cycle_start_steps: usize,
    pub state_period_steps: usize,
}

/// Fresh model diagnostics only, never deserializable adoption authority.
#[derive(Clone, Debug, Serialize)]
pub struct FiniteBurstTypeReport {
    pub type_revision: TypeRevisionId,
    pub status: CheckStatus,
    pub reachable_states: usize,
    pub evaluated_steps: usize,
    pub detail: String,
    /// Also present for an OFF cycle reached with too few falling edges.
    pub cessation: Option<FiniteBurstCessation>,
}

pub fn verify_finite_burst<M: BehaviorModel>(
    definition: &TypeRevision,
    model: &M,
    budget: BehaviorBudget,
) -> FiniteBurstTypeReport {
    let mut report = FiniteBurstTypeReport {
        type_revision: definition.id.clone(),
        status: CheckStatus::Undetermined,
        reachable_states: 0,
        evaluated_steps: 0,
        detail: String::new(),
        cessation: None,
    };
    let start = Instant::now();
    let check_time = || {
        if start.elapsed() >= budget.max_elapsed {
            Err(
                "elapsed verification budget exhausted; no stopping deadline was imposed"
                    .to_owned(),
            )
        } else {
            Ok(())
        }
    };
    let mut stats = TraversalStats::default();
    let result = (|| -> Result<Option<FiniteBurstCessation>, String> {
        let TypeContract::FiniteBurst { requirement } = &definition.contract else {
            return Err("selected type does not describe autonomous finite-burst behavior".into());
        };
        requirement.validate().map_err(str::to_owned)?;
        let trace = trace_autonomous(model, budget, &check_time, &mut stats)?;
        if trace.outputs[trace.cycle_entry..].contains(&true) {
            check_time()?;
            return Ok(None);
        }
        let falling_edges = trace
            .outputs
            .windows(2)
            .filter(|pair| pair[0] && !pair[1])
            .count();
        let off_from_step = trace
            .outputs
            .iter()
            .rposition(|&level| level)
            .map_or(0, |step| step + 1);
        check_time()?;
        Ok(Some(FiniteBurstCessation {
            falling_edges,
            off_from_step,
            state_cycle_start_steps: trace.cycle_entry,
            state_period_steps: trace.outputs.len() - trace.cycle_entry,
        }))
    })();
    report.reachable_states = stats.reachable_states;
    report.evaluated_steps = stats.evaluated_steps;
    match result {
        Ok(Some(cessation)) => {
            report.status = if cessation.falling_edges >= 2 {
                CheckStatus::Passed
            } else {
                CheckStatus::Failed
            };
            report.detail = if cessation.falling_edges >= 2 {
                "at least two ON-to-OFF transitions precede a complete-state cycle whose output stays OFF; restartability was not checked"
            } else {
                "output eventually stays OFF, but fewer than two ON-to-OFF transitions occurred"
            }.into();
            report.cessation = Some(cessation);
        }
        Ok(None) => {
            report.status = CheckStatus::Failed;
            report.detail =
                "the recurring complete-state cycle does not keep the output OFF".into();
        }
        Err(error) => report.detail = error,
    }
    report
}
