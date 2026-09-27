use std::time::Duration;

use dustroute_library::behavior_type::{BooleanRow, RepeatedSettling};
use dustroute_library::blueprint::{TypeContract, TypeRevision, TypeRevisionId};
use dustroute_translate::abstract_behavior::{
    AbstractBehaviorModel, verify_abstract_repeated_settling,
};
use dustroute_translate::behavior_type::BehaviorBudget;
use dustroute_translate::promotion::CheckStatus;

fn definition() -> TypeRevision {
    TypeRevision {
        id: TypeRevisionId::new("test.abstract.constant.v1").unwrap(),
        name: "Eventually ON".into(),
        contract: TypeContract::RepeatedSettling {
            relation: RepeatedSettling {
                inputs: vec!["input".into()],
                outputs: vec!["output".into()],
                rows: [false, true]
                    .into_iter()
                    .map(|input| BooleanRow {
                        inputs: vec![input],
                        outputs: vec![true],
                    })
                    .collect(),
            },
        },
    }
}

struct Branches {
    outputs: Vec<bool>,
    edges: Vec<Vec<usize>>,
}
impl AbstractBehaviorModel for Branches {
    type State = usize;
    fn initial_state(&self) -> Result<usize, String> {
        Ok(0)
    }
    fn with_inputs(&self, state: &usize, _: &[bool]) -> Result<usize, String> {
        Ok(*state)
    }
    fn outputs(&self, state: &usize) -> Result<Vec<bool>, String> {
        Ok(vec![self.outputs[*state]])
    }
    fn successors(&self, state: &usize, _: usize) -> Result<Vec<usize>, String> {
        Ok(self.edges[*state].clone())
    }
}

#[test]
fn all_nondeterministic_tails_must_settle_but_transients_are_unrestricted() {
    let graph = Branches {
        outputs: vec![false, true, false, true],
        edges: vec![vec![1, 2], vec![1], vec![3], vec![3]],
    };
    let report =
        verify_abstract_repeated_settling(&definition(), &graph, BehaviorBudget::default());
    assert_eq!(report.status, CheckStatus::Passed, "{report:?}");
    assert_eq!(report.abstract_states, 4);
    for graph in [
        Branches {
            outputs: vec![false, true],
            edges: vec![vec![0, 1], vec![1]],
        }, // Nonterminal bad SCC.
        Branches {
            outputs: vec![true, false, true],
            edges: vec![vec![1, 2], vec![0], vec![2]],
        },
        Branches {
            outputs: vec![true, false],
            edges: vec![vec![0, 1], vec![1]],
        }, // Bad alternative hidden behind good branch.
    ] {
        let report =
            verify_abstract_repeated_settling(&definition(), &graph, BehaviorBudget::default());
        assert_eq!(report.status, CheckStatus::Undetermined);
        assert!(report.detail.contains("no concrete counterexample"));
    }
}

#[test]
fn cycle_with_correct_outputs_can_keep_changing_hidden_state() {
    let graph = Branches {
        outputs: vec![true, true],
        edges: vec![vec![0, 1], vec![0]],
    };
    assert_eq!(
        verify_abstract_repeated_settling(&definition(), &graph, BehaviorBudget::default()).status,
        CheckStatus::Passed
    );
}

#[test]
fn closure_and_every_successor_are_required_at_resource_boundaries() {
    let graph = Branches {
        outputs: vec![true, true],
        edges: vec![vec![0, 1], vec![1]],
    };
    let exact = BehaviorBudget {
        max_states: 2,
        max_steps: 6,
        ..BehaviorBudget::default()
    };
    assert_eq!(
        verify_abstract_repeated_settling(&definition(), &graph, exact).status,
        CheckStatus::Passed
    );
    for budget in [
        BehaviorBudget {
            max_states: 1,
            ..exact
        },
        BehaviorBudget {
            max_steps: 5,
            ..exact
        },
        BehaviorBudget {
            max_elapsed: Duration::ZERO,
            ..exact
        },
    ] {
        assert_eq!(
            verify_abstract_repeated_settling(&definition(), &graph, budget).status,
            CheckStatus::Undetermined
        );
    }
    let dead_end = Branches {
        outputs: vec![true],
        edges: vec![vec![]],
    };
    assert_eq!(
        verify_abstract_repeated_settling(&definition(), &dead_end, exact).status,
        CheckStatus::Undetermined
    );
}

struct BeforeStep;
impl AbstractBehaviorModel for BeforeStep {
    type State = usize;
    fn initial_state(&self) -> Result<usize, String> {
        Ok(0)
    }
    fn with_inputs(&self, state: &usize, inputs: &[bool]) -> Result<usize, String> {
        Ok(match (*state, inputs[0]) {
            (0, true) => 1,
            (1, false) => 2,
            _ => *state,
        })
    }
    fn outputs(&self, state: &usize) -> Result<Vec<bool>, String> {
        Ok(vec![*state != 2])
    }
    fn successors(&self, state: &usize, _: usize) -> Result<Vec<usize>, String> {
        Ok(vec![if *state == 1 { 0 } else { *state }])
    }
}

#[test]
fn input_changes_without_an_intervening_tick_are_not_lost() {
    // The bad state requires ON then OFF before stepping. Fusing all input
    // assignments with a tick would miss this legitimate reachable state.
    let report =
        verify_abstract_repeated_settling(&definition(), &BeforeStep, BehaviorBudget::default());
    assert_eq!(report.status, CheckStatus::Undetermined);
    assert_eq!(report.abstract_states, 3);
}

struct Broken(u8);
impl AbstractBehaviorModel for Broken {
    type State = usize;
    fn initial_state(&self) -> Result<usize, String> {
        if self.0 == 0 {
            Err("unknown initial state".into())
        } else {
            Ok(0)
        }
    }
    fn with_inputs(&self, state: &usize, _: &[bool]) -> Result<usize, String> {
        Ok(if self.0 == 1 { state + 1 } else { *state })
    }
    fn outputs(&self, _: &usize) -> Result<Vec<bool>, String> {
        Ok(if self.0 == 2 { vec![] } else { vec![true] })
    }
    fn successors(&self, _: &usize, _: usize) -> Result<Vec<usize>, String> {
        Err("possible execution error".into())
    }
}

#[test]
fn unknown_execution_and_invalid_interfaces_cannot_establish_a_pass() {
    for error in 0..4 {
        let report = verify_abstract_repeated_settling(
            &definition(),
            &Broken(error),
            BehaviorBudget::default(),
        );
        assert_eq!(report.status, CheckStatus::Undetermined, "{report:?}");
    }
}
