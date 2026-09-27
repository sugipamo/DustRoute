use std::time::Duration;

use dustroute_library::behavior_type::Periodic;
use dustroute_library::blueprint::{TypeContract, TypeRevision, TypeRevisionId};
use dustroute_translate::behavior_type::{BehaviorBudget, BehaviorModel};
use dustroute_translate::periodic::verify_periodic;
use dustroute_translate::promotion::CheckStatus;

fn definition() -> TypeRevision {
    TypeRevision {
        id: TypeRevisionId::new("clock.type.v1").unwrap(),
        name: "Autonomous waveform".into(),
        contract: TypeContract::Periodic {
            requirement: Periodic {
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
    fn with_inputs(&self, s: &usize, input: &[bool]) -> Result<usize, String> {
        assert!(input.is_empty());
        Ok(*s)
    }
    fn step(&self, s: &usize) -> Result<usize, String> {
        Ok(if s + 1 == self.outputs.len() {
            self.cycle_entry
        } else {
            s + 1
        })
    }
    fn outputs(&self, s: &usize) -> Result<Vec<bool>, String> {
        Ok(vec![self.outputs[*s]])
    }
}

#[test]
fn startup_and_hidden_state_period_do_not_constrain_the_output_period() {
    let model = Trace {
        outputs: vec![true, true, false, true, false, true, false, true],
        cycle_entry: 2,
    };
    // Closure at the exact budget boundary must be accepted.
    let report = verify_periodic(
        &definition(),
        &model,
        BehaviorBudget {
            max_states: 8,
            max_steps: 8,
            ..BehaviorBudget::default()
        },
    );
    assert_eq!(report.status, CheckStatus::Passed, "{report:?}");
    assert_eq!(report.reachable_states, 8);
    let cycle = report.cycle.unwrap();
    assert_eq!(
        (
            cycle.startup_steps,
            cycle.state_period_steps,
            cycle.output_period_steps
        ),
        (2, 6, 2)
    );
    assert_eq!(cycle.waveform, [false, true]);
}

#[test]
fn bursts_and_recovery_pauses_are_allowed_but_transient_pulses_are_insufficient() {
    let burst = vec![true, false, true, false, false, false, false];
    let report = verify_periodic(
        &definition(),
        &Trace {
            outputs: burst.clone(),
            cycle_entry: 0,
        },
        BehaviorBudget::default(),
    );
    assert_eq!(report.status, CheckStatus::Passed);
    assert_eq!(report.cycle.unwrap().waveform, burst);
    for level in [false, true] {
        let report = verify_periodic(
            &definition(),
            &Trace {
                outputs: vec![false, true, false, true, level, level],
                cycle_entry: 4,
            },
            BehaviorBudget::default(),
        );
        assert_eq!(report.status, CheckStatus::Failed);
        assert_eq!(report.cycle.unwrap().waveform, [level]);
    }
}

struct Unclosed;
impl BehaviorModel for Unclosed {
    type State = usize;
    fn initial_state(&self) -> Result<usize, String> {
        Ok(0)
    }
    fn with_inputs(&self, s: &usize, _: &[bool]) -> Result<usize, String> {
        Ok(*s)
    }
    fn step(&self, s: &usize) -> Result<usize, String> {
        Ok(s + 1)
    }
    fn outputs(&self, s: &usize) -> Result<Vec<bool>, String> {
        Ok(vec![s % 2 == 0])
    }
}

#[test]
fn repeated_samples_and_resource_exhaustion_never_become_proof() {
    for budget in [
        BehaviorBudget {
            max_states: 16,
            ..BehaviorBudget::default()
        },
        BehaviorBudget {
            max_steps: 16,
            ..BehaviorBudget::default()
        },
        BehaviorBudget {
            max_elapsed: Duration::ZERO,
            ..BehaviorBudget::default()
        },
    ] {
        let report = verify_periodic(&definition(), &Unclosed, budget);
        assert_eq!(report.status, CheckStatus::Undetermined, "{report:?}");
        assert!(report.cycle.is_none());
    }
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
    fn with_inputs(&self, s: &usize, _: &[bool]) -> Result<usize, String> {
        Ok(if self.0 == 1 { s + 1 } else { *s })
    }
    fn step(&self, _: &usize) -> Result<usize, String> {
        Err("unsupported transition".into())
    }
    fn outputs(&self, _: &usize) -> Result<Vec<bool>, String> {
        Ok(vec![false; if self.0 == 2 { 2 } else { 1 }])
    }
}

#[test]
fn unknown_or_invalid_execution_cannot_pass_or_claim_a_counterexample() {
    for model in (0..4).map(Broken) {
        let report = verify_periodic(&definition(), &model, BehaviorBudget::default());
        assert_eq!(report.status, CheckStatus::Undetermined, "{report:?}");
        assert!(report.cycle.is_none());
    }
}
