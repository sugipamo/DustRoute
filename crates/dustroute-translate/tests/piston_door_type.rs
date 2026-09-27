use dustroute_library::behavior_type::PistonDoor;
use dustroute_library::blueprint::{TypeContract, TypeRevision, TypeRevisionId};
use dustroute_translate::behavior_type::{BehaviorBudget, BehaviorModel, verify_repeated_settling};
use dustroute_translate::piston_door_type::verify_piston_door;
use dustroute_translate::promotion::CheckStatus;

fn definition() -> TypeRevision {
    TypeRevision {
        id: TypeRevisionId::new("test.ordinary-door.v1").unwrap(),
        name: "Ordinary door".into(),
        contract: TypeContract::PistonDoor {
            requirement: Box::new(PistonDoor::three_by_three()),
        },
    }
}

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
struct State {
    command: bool,
    aperture: bool,
    remaining: u8,
    operations: u8,
    broken: bool,
    clock: u8,
}
#[derive(Default)]
struct Door {
    clock: bool,
    fail_after: Option<u8>,
    fail_clock_phase: bool,
    recurring_pulse: bool,
    initially_wrong: bool,
}
fn aperture(closed: bool) -> Vec<bool> {
    (0..9).flat_map(|_| [!closed, closed]).collect()
}
impl BehaviorModel for Door {
    type State = State;
    fn initial_state(&self) -> Result<State, String> {
        Ok(State {
            command: false,
            aperture: self.initially_wrong,
            remaining: 0,
            operations: 0,
            broken: false,
            clock: 0,
        })
    }
    fn with_inputs(&self, state: &State, inputs: &[bool]) -> Result<State, String> {
        let mut next = state.clone();
        if next.command != inputs[0] {
            next.broken |= next.remaining > 0 || (self.fail_clock_phase && next.clock == 1);
            next.command = inputs[0];
            next.remaining = 2;
            next.operations = (next.operations + 1).min(6);
        }
        Ok(next)
    }
    fn step(&self, state: &State) -> Result<State, String> {
        let mut next = state.clone();
        if self.clock {
            next.clock = (next.clock + 1) % 3;
        }
        if next.remaining > 0 {
            next.remaining -= 1;
            if next.remaining == 0 {
                next.aperture =
                    if next.broken || self.fail_after.is_some_and(|n| next.operations >= n) {
                        !next.command
                    } else {
                        next.command
                    };
            } else {
                // A correct transient snapshot is not a completion certificate.
                next.aperture = next.command;
            }
        }
        Ok(next)
    }
    fn outputs(&self, state: &State) -> Result<Vec<bool>, String> {
        Ok(aperture(state.aperture))
    }
    fn step_with_observations(&self, state: &State) -> Result<(State, Vec<Vec<bool>>), String> {
        let samples = if self.recurring_pulse && state.remaining == 0 && state.command {
            vec![aperture(false), aperture(true)]
        } else {
            vec![]
        };
        Ok((self.step(state)?, samples))
    }
}

#[test]
fn completed_operations_pass_while_unrestricted_reversals_fail() {
    let door = Door::default();
    let result = verify_piston_door(&definition(), &door, false, BehaviorBudget::default());
    assert_eq!(result.status, CheckStatus::Passed, "{result:?}");
    assert!(result.graph_closed);
    let mut unrestricted = definition();
    unrestricted.contract = TypeContract::RepeatedSettling {
        relation: PistonDoor::three_by_three().relation(),
    };
    assert_eq!(
        verify_repeated_settling(&unrestricted, &door, BehaviorBudget::default()).status,
        CheckStatus::Failed
    );
}

#[test]
fn repeated_use_carries_memory_and_rejects_later_failure_despite_correct_transient() {
    let door = Door {
        fail_after: Some(5),
        ..Door::default()
    };
    let result = verify_piston_door(&definition(), &door, false, BehaviorBudget::default());
    assert_eq!(result.status, CheckStatus::Failed, "{result:?}");
    let witness = result.counterexample.unwrap();
    let mut state = door.initial_state().unwrap();
    for action in witness.prefix {
        state = match action {
            dustroute_translate::behavior_type::WitnessAction::Advance => {
                door.step(&state).unwrap()
            }
            dustroute_translate::behavior_type::WitnessAction::SetInputs { inputs } => {
                door.with_inputs(&state, &inputs).unwrap()
            }
        };
    }
    assert!(state.operations >= 5);
    assert_ne!(door.outputs(&state).unwrap(), witness.expected_outputs);
}

#[test]
fn unrelated_clock_may_keep_running_but_every_completed_phase_must_work() {
    let door = Door {
        clock: true,
        ..Door::default()
    };
    assert_eq!(
        verify_piston_door(&definition(), &door, false, BehaviorBudget::default()).status,
        CheckStatus::Passed
    );
    let fragile = Door {
        clock: true,
        fail_clock_phase: true,
        ..Door::default()
    };
    assert_eq!(
        verify_piston_door(&definition(), &fragile, false, BehaviorBudget::default()).status,
        CheckStatus::Failed
    );
}

#[test]
fn recurring_internal_pulses_and_incorrect_initial_aperture_are_failures() {
    for door in [
        Door {
            recurring_pulse: true,
            ..Door::default()
        },
        Door {
            initially_wrong: true,
            ..Door::default()
        },
    ] {
        assert_eq!(
            verify_piston_door(&definition(), &door, false, BehaviorBudget::default()).status,
            CheckStatus::Failed
        );
    }
}

#[test]
fn incomplete_exploration_is_never_a_pass() {
    for budget in [
        BehaviorBudget {
            max_states: 2,
            ..BehaviorBudget::default()
        },
        BehaviorBudget {
            max_steps: 1,
            ..BehaviorBudget::default()
        },
        BehaviorBudget {
            max_elapsed: std::time::Duration::ZERO,
            ..BehaviorBudget::default()
        },
    ] {
        let report = verify_piston_door(&definition(), &Door::default(), false, budget);
        assert_eq!(report.status, CheckStatus::Undetermined);
        assert!(!report.graph_closed);
    }
}
