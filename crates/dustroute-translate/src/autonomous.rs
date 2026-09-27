//! Shared exact traversal for autonomous, single-output behavioral contracts.
//! Output predicates inspect the prefix and a closed complete-state cycle;
//! they never replace execution state equality with an output-only key.
use std::collections::BTreeMap;

use crate::behavior_type::{BehaviorBudget, BehaviorModel};

#[derive(Default)]
pub(crate) struct TraversalStats {
    pub reachable_states: usize,
    pub evaluated_steps: usize,
}

pub(crate) struct AutonomousTrace {
    pub outputs: Vec<bool>,
    pub cycle_entry: usize,
}

pub(crate) fn trace_autonomous<M: BehaviorModel>(
    model: &M,
    budget: BehaviorBudget,
    check_time: &impl Fn() -> Result<(), String>,
    stats: &mut TraversalStats,
) -> Result<AutonomousTrace, String> {
    check_time()?;
    let mut state = model.initial_state()?;
    let mut seen = BTreeMap::new();
    let mut outputs = Vec::new();
    loop {
        check_time()?;
        // A transition closing the cycle exactly at the budget is valid.
        if let Some(&cycle_entry) = seen.get(&state) {
            return Ok(AutonomousTrace {
                outputs,
                cycle_entry,
            });
        }
        if seen.len() >= budget.max_states {
            return Err("reachable-state budget exhausted before recurrence".into());
        }
        if model.with_inputs(&state, &[])? != state {
            return Err("an empty autonomous input assignment must preserve state".into());
        }
        let output = model.outputs(&state)?;
        if output.len() != 1 {
            return Err("an autonomous model must expose exactly one Boolean output".into());
        }
        seen.insert(state.clone(), outputs.len());
        outputs.push(output[0]);
        stats.reachable_states = seen.len();
        check_time()?;
        if stats.evaluated_steps >= budget.max_steps {
            return Err("model-step budget exhausted before recurrence".into());
        }
        stats.evaluated_steps += 1;
        state = model.step(&state)?;
    }
}
