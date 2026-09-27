use std::time::Duration;

use dustroute_library::behavior_type::FiniteBurst;
use dustroute_library::blueprint::{TypeContract, TypeRevision, TypeRevisionId};
use dustroute_translate::behavior_type::{BehaviorBudget, BehaviorModel};
use dustroute_translate::finite_burst::verify_finite_burst;
use dustroute_translate::promotion::CheckStatus;

fn definition() -> TypeRevision {
    TypeRevision {
        id: TypeRevisionId::new("burst.type.v1").unwrap(),
        name: "Two or more falling edges, then OFF".into(),
        contract: TypeContract::FiniteBurst {
            requirement: FiniteBurst {
                output: "out".into(),
            },
        },
    }
}

struct Trace {
    outputs: Vec<bool>,
    cycle_entry: usize,
}
impl BehaviorModel for Trace {
    type State = usize;
    fn initial_state(&self) -> Result<usize, String> {
        Ok(0)
    }
    fn with_inputs(&self, state: &usize, inputs: &[bool]) -> Result<usize, String> {
        assert!(inputs.is_empty());
        Ok(*state)
    }
    fn step(&self, state: &usize) -> Result<usize, String> {
        Ok(if state + 1 == self.outputs.len() {
            self.cycle_entry
        } else {
            state + 1
        })
    }
    fn outputs(&self, state: &usize) -> Result<Vec<bool>, String> {
        Ok(vec![self.outputs[*state]])
    }
}

#[test]
fn two_actual_falling_edges_pass_at_the_exact_resource_boundary() {
    let model = Trace {
        outputs: vec![true, false, true, false],
        cycle_entry: 3,
    };
    let report = verify_finite_burst(
        &definition(),
        &model,
        BehaviorBudget {
            max_states: 4,
            max_steps: 4,
            ..BehaviorBudget::default()
        },
    );
    assert_eq!(report.status, CheckStatus::Passed, "{report:?}");
    assert_eq!((report.reachable_states, report.evaluated_steps), (4, 4));
    let stopped = report.cessation.unwrap();
    assert_eq!(
        (
            stopped.falling_edges,
            stopped.off_from_step,
            stopped.state_cycle_start_steps,
            stopped.state_period_steps
        ),
        (2, 3, 3, 1)
    );
    for budget in [
        BehaviorBudget {
            max_states: 3,
            ..BehaviorBudget::default()
        },
        BehaviorBudget {
            max_steps: 3,
            ..BehaviorBudget::default()
        },
        BehaviorBudget {
            max_elapsed: Duration::ZERO,
            ..BehaviorBudget::default()
        },
    ] {
        let incomplete = verify_finite_burst(&definition(), &model, budget);
        assert_eq!(incomplete.status, CheckStatus::Undetermined);
        assert!(incomplete.cessation.is_none());
    }
}

#[test]
fn initial_level_is_not_an_invented_pulse_and_stable_or_single_pulse_outputs_fail() {
    for outputs in [
        vec![false],
        vec![true],
        vec![true, false],
        vec![false, true, false],
        vec![false, false, false],
    ] {
        let model = Trace {
            cycle_entry: outputs.len() - 1,
            outputs,
        };
        let report = verify_finite_burst(&definition(), &model, BehaviorBudget::default());
        assert_eq!(report.status, CheckStatus::Failed, "{report:?}");
    }
    let delayed = Trace {
        outputs: vec![false, false, true, false, false, true, true, false],
        cycle_entry: 7,
    };
    let report = verify_finite_burst(&definition(), &delayed, BehaviorBudget::default());
    assert_eq!(report.status, CheckStatus::Passed);
    assert_eq!(report.cessation.unwrap().falling_edges, 2);
}

#[test]
fn finite_burst_requires_permanent_off_instead_of_a_pause_or_permanent_on() {
    for model in [
        Trace {
            outputs: vec![true, false, true, false, false, false],
            cycle_entry: 0,
        },
        Trace {
            outputs: vec![true, false, true, false, true],
            cycle_entry: 4,
        },
    ] {
        let report = verify_finite_burst(&definition(), &model, BehaviorBudget::default());
        assert_eq!(report.status, CheckStatus::Failed);
        assert!(report.cessation.is_none());
    }
}

#[test]
fn hidden_state_can_keep_cycling_after_output_stops_and_exact_pulse_count_is_not_fixed() {
    let model = Trace {
        outputs: vec![true, false, true, false, true, false, false, false],
        cycle_entry: 6,
    };
    let report = verify_finite_burst(&definition(), &model, BehaviorBudget::default());
    assert_eq!(report.status, CheckStatus::Passed);
    let stopped = report.cessation.unwrap();
    assert_eq!(
        (
            stopped.falling_edges,
            stopped.off_from_step,
            stopped.state_cycle_start_steps,
            stopped.state_period_steps
        ),
        (3, 5, 6, 2)
    );
}

struct Unclosed;
impl BehaviorModel for Unclosed {
    type State = usize;
    fn initial_state(&self) -> Result<usize, String> {
        Ok(0)
    }
    fn with_inputs(&self, state: &usize, _: &[bool]) -> Result<usize, String> {
        Ok(*state)
    }
    fn step(&self, state: &usize) -> Result<usize, String> {
        Ok(state + 1)
    }
    fn outputs(&self, state: &usize) -> Result<Vec<bool>, String> {
        Ok(vec![matches!(*state, 0 | 2)])
    }
}

#[test]
fn many_off_samples_do_not_establish_future_cessation_without_state_closure() {
    let report = verify_finite_burst(
        &definition(),
        &Unclosed,
        BehaviorBudget {
            max_states: 32,
            ..BehaviorBudget::default()
        },
    );
    assert_eq!(report.status, CheckStatus::Undetermined);
    assert!(report.cessation.is_none());
}

struct Broken(u8);
impl BehaviorModel for Broken {
    type State = usize;
    fn initial_state(&self) -> Result<usize, String> {
        if self.0 == 0 {
            Err("unknown initial memory".into())
        } else {
            Ok(0)
        }
    }
    fn with_inputs(&self, state: &usize, _: &[bool]) -> Result<usize, String> {
        Ok(if self.0 == 1 { state + 1 } else { *state })
    }
    fn step(&self, _: &usize) -> Result<usize, String> {
        Err("unsupported transition".into())
    }
    fn outputs(&self, _: &usize) -> Result<Vec<bool>, String> {
        Ok(vec![false; if self.0 == 2 { 2 } else { 1 }])
    }
}

#[test]
fn invalid_execution_or_wrong_type_never_produces_a_pass_or_counterexample() {
    for model in (0..4).map(Broken) {
        let report = verify_finite_burst(&definition(), &model, BehaviorBudget::default());
        assert_eq!(report.status, CheckStatus::Undetermined, "{report:?}");
        assert!(report.cessation.is_none());
    }
    let mut invalid = definition();
    invalid.contract = TypeContract::Periodic {
        requirement: dustroute_library::behavior_type::Periodic {
            output: "out".into(),
        },
    };
    let report = verify_finite_burst(&invalid, &Unclosed, BehaviorBudget::default());
    assert_eq!(report.status, CheckStatus::Undetermined);
    assert_eq!(report.reachable_states, 0);
}
