use dustroute_library::behavior_type::SingleOperation;
use dustroute_library::blueprint::{TypeContract, TypeRevision, TypeRevisionId};
use dustroute_translate::behavior_type::{BehaviorBudget, BehaviorModel, verify_repeated_settling};
use dustroute_translate::piston_door_type::verify_single_operation;
use dustroute_translate::promotion::CheckStatus;

fn definition() -> TypeRevision {
    TypeRevision {
        id: TypeRevisionId::new("test.single-operation.v1").unwrap(),
        name: "Single operation".into(),
        contract: TypeContract::SingleOperation {
            requirement: SingleOperation {
                input: "start".into(),
                outputs: vec!["arrived".into()],
                initial: vec![false],
                completed: vec![true],
            },
        },
    }
}

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
struct State {
    input: bool,
    position: u8,
    clock: bool,
    broken: bool,
}
#[derive(Default)]
struct Machine {
    fail_ready_phase: bool,
    pulse: bool,
    transient_only: bool,
    initially_arrived: bool,
}
impl BehaviorModel for Machine {
    type State = State;
    fn initial_state(&self) -> Result<State, String> {
        Ok(State {
            input: false,
            position: if self.initially_arrived { 2 } else { 0 },
            clock: false,
            broken: false,
        })
    }
    fn with_inputs(&self, state: &State, inputs: &[bool]) -> Result<State, String> {
        let mut next = state.clone();
        if !next.input && inputs[0] {
            next.broken |= self.fail_ready_phase && next.clock;
        }
        next.input = inputs[0];
        Ok(next)
    }
    fn step(&self, state: &State) -> Result<State, String> {
        let mut next = state.clone();
        next.clock = !next.clock;
        if next.input {
            next.position = (next.position + 1).min(2);
        }
        Ok(next)
    }
    fn outputs(&self, state: &State) -> Result<Vec<bool>, String> {
        Ok(vec![
            state.position > 0 && !(state.position == 2 && (state.broken || self.transient_only)),
        ])
    }
    fn step_with_observations(&self, state: &State) -> Result<(State, Vec<Vec<bool>>), String> {
        let inside = if self.pulse && state.position == 2 {
            vec![vec![false], vec![true]]
        } else {
            vec![]
        };
        Ok((self.step(state)?, inside))
    }
}

#[test]
fn one_irreversible_operation_passes_without_promising_reset_or_repeated_use() {
    let machine = Machine::default();
    let definition = definition();
    let report = verify_single_operation(&definition, &machine, false, BehaviorBudget::default());
    assert_eq!(report.status, CheckStatus::Passed, "{report:?}");
    assert!(report.graph_closed);
    let TypeContract::SingleOperation { requirement } = definition.contract.clone() else {
        unreachable!()
    };
    let repeated = TypeRevision {
        contract: TypeContract::RepeatedSettling {
            relation: requirement.relation(),
        },
        ..definition
    };
    assert_eq!(
        verify_repeated_settling(&repeated, &machine, BehaviorBudget::default()).status,
        CheckStatus::Failed
    );
}

#[test]
fn all_ready_phases_and_recurrent_internal_observations_are_checked() {
    for machine in [
        Machine {
            fail_ready_phase: true,
            ..Default::default()
        },
        Machine {
            pulse: true,
            ..Default::default()
        },
        Machine {
            transient_only: true,
            ..Default::default()
        },
        Machine {
            initially_arrived: true,
            ..Default::default()
        },
    ] {
        let report =
            verify_single_operation(&definition(), &machine, false, BehaviorBudget::default());
        assert_eq!(report.status, CheckStatus::Failed, "{report:?}");
    }
}

#[test]
fn incomplete_exploration_and_an_already_active_initial_input_do_not_pass() {
    assert_eq!(
        verify_single_operation(
            &definition(),
            &Machine::default(),
            false,
            BehaviorBudget {
                max_states: 1,
                ..Default::default()
            }
        )
        .status,
        CheckStatus::Undetermined
    );
    assert_eq!(
        verify_single_operation(
            &definition(),
            &Machine::default(),
            true,
            BehaviorBudget::default()
        )
        .status,
        CheckStatus::Undetermined
    );
}
