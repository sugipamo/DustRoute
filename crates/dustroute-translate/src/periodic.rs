//! Exact recurrence checks for an autonomous deterministic model.
//! A repeated output alone is not evidence: the complete execution state must
//! recur. Proofs are relative to the model, its initial state and environment.
use std::time::Instant;

use dustroute_library::blueprint::{TypeContract, TypeRevision, TypeRevisionId};
use serde::Serialize;

use crate::autonomous::{TraversalStats, trace_autonomous};
use crate::behavior_type::{BehaviorBudget, BehaviorModel};
use crate::promotion::CheckStatus;

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct PeriodicCycle {
    /// Steps before entry into the recurring complete-state cycle. This is a
    /// sufficient startup prefix, not necessarily the earliest output recurrence.
    pub startup_steps: usize,
    pub state_period_steps: usize,
    pub output_period_steps: usize,
    /// One minimal output period starting at state-cycle entry. The type does
    /// not require this phase or these durations in another realization.
    pub waveform: Vec<bool>,
}

/// Diagnostics, never deserializable proof or adoption authority.
#[derive(Clone, Debug, Serialize)]
pub struct PeriodicTypeReport {
    pub type_revision: TypeRevisionId,
    pub status: CheckStatus,
    pub reachable_states: usize,
    pub evaluated_steps: usize,
    pub detail: String,
    /// Also present for a closed constant-output cycle, which fails the type.
    pub cycle: Option<PeriodicCycle>,
}

pub fn verify_periodic<M: BehaviorModel>(
    definition: &TypeRevision,
    model: &M,
    budget: BehaviorBudget,
) -> PeriodicTypeReport {
    let mut report = PeriodicTypeReport {
        type_revision: definition.id.clone(),
        status: CheckStatus::Undetermined,
        reachable_states: 0,
        evaluated_steps: 0,
        detail: String::new(),
        cycle: None,
    };
    let start = Instant::now();
    let check_time = || {
        if start.elapsed() >= budget.max_elapsed {
            Err("elapsed verification budget exhausted; no startup deadline was imposed".to_owned())
        } else {
            Ok(())
        }
    };
    let mut stats = TraversalStats::default();
    let result = (|| -> Result<PeriodicCycle, String> {
        let TypeContract::Periodic { requirement } = &definition.contract else {
            return Err("selected type does not describe autonomous periodic behavior".into());
        };
        requirement.validate().map_err(str::to_owned)?;
        let trace = trace_autonomous(model, budget, &check_time, &mut stats)?;
        let waveform = &trace.outputs[trace.cycle_entry..];
        let period = minimal_period(waveform, &check_time)?;
        check_time()?;
        Ok(PeriodicCycle {
            startup_steps: trace.cycle_entry,
            state_period_steps: waveform.len(),
            output_period_steps: period,
            waveform: waveform[..period].to_vec(),
        })
    })();
    report.reachable_states = stats.reachable_states;
    report.evaluated_steps = stats.evaluated_steps;
    match result {
        Ok(cycle) => {
            if cycle.waveform.contains(&true) && cycle.waveform.contains(&false) {
                report.status = CheckStatus::Passed;
                report.detail = "complete state recurs with a nonconstant output waveform under the declared initial state and fixed environment".into();
            } else {
                report.status = CheckStatus::Failed;
                report.detail = "the recurring state cycle has a constant output; transient pulses do not satisfy the periodic type".into();
            }
            report.cycle = Some(cycle);
        }
        Err(error) => report.detail = error,
    }
    report
}

/// Linear-time prefix matching avoids quadratic work for long recovery cycles.
fn minimal_period(
    waveform: &[bool],
    check_time: &impl Fn() -> Result<(), String>,
) -> Result<usize, String> {
    let mut prefixes = vec![0; waveform.len()];
    for i in 1..waveform.len() {
        check_time()?;
        let mut matched = prefixes[i - 1];
        while matched > 0 && waveform[i] != waveform[matched] {
            matched = prefixes[matched - 1];
        }
        if waveform[i] == waveform[matched] {
            matched += 1;
        }
        prefixes[i] = matched;
    }
    let candidate = waveform.len() - prefixes[waveform.len() - 1];
    Ok(if waveform.len() % candidate == 0 {
        candidate
    } else {
        waveform.len()
    })
}
