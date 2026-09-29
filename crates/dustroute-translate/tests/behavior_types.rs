use std::time::Duration;

use dustroute_library::PortDirection;
use dustroute_library::behavior_type::{BooleanRow, RepeatedSettling};
use dustroute_library::blueprint::*;
use dustroute_library::builtin_blueprints::*;
use dustroute_translate::behavior_type::*;
use dustroute_translate::promotion::CheckStatus;
use dustroute_translate::{world::Block, world::BlockKind};

fn not_type() -> TypeRevision {
    TypeRevision {
        id: TypeRevisionId::new("test.type.not-repeated.v1").unwrap(),
        name: "Repeated NOT".into(),
        contract: TypeContract::RepeatedSettling {
            relation: RepeatedSettling {
                inputs: vec!["a".into()],
                outputs: vec!["out".into()],
                rows: vec![
                    BooleanRow {
                        inputs: vec![false],
                        outputs: vec![true],
                    },
                    BooleanRow {
                        inputs: vec![true],
                        outputs: vec![false],
                    },
                ],
            },
        },
    }
}

#[derive(Clone, Debug, Eq, Ord, PartialEq, PartialOrd)]
struct State {
    input: bool,
    output: bool,
    pending: u8,
    changes: u8,
    internal_clock: bool,
}

struct Model {
    delay: u8,
    one_shot: bool,
    internal_clock: bool,
    oscillating_output: bool,
}
impl BehaviorModel for Model {
    type State = State;
    fn initial_state(&self) -> Result<State, String> {
        Ok(State {
            input: false,
            output: true,
            pending: 0,
            changes: 0,
            internal_clock: false,
        })
    }
    fn with_inputs(&self, state: &State, inputs: &[bool]) -> Result<State, String> {
        let mut result = state.clone();
        if result.input != inputs[0] {
            result.input = inputs[0];
            result.pending = self.delay;
            result.changes = result.changes.saturating_add(1).min(2);
        }
        Ok(result)
    }
    fn step(&self, state: &State) -> Result<State, String> {
        let mut result = state.clone();
        result.pending = result.pending.saturating_sub(1);
        if self.internal_clock {
            result.internal_clock = !result.internal_clock;
        }
        if self.oscillating_output {
            result.output = !result.output;
        } else if result.pending == 0 && !(self.one_shot && result.changes >= 2) {
            result.output = !result.input;
        }
        Ok(result)
    }
    fn outputs(&self, state: &State) -> Result<Vec<bool>, String> {
        Ok(vec![state.output])
    }
}
fn model() -> Model {
    Model {
        delay: 2,
        one_shot: false,
        internal_clock: false,
        oscillating_output: false,
    }
}

#[test]
fn recurring_internal_pulses_cannot_hide_behind_correct_root_outputs() {
    struct Pulsing;
    impl BehaviorModel for Pulsing {
        type State = bool;
        fn initial_state(&self) -> Result<bool, String> {
            Ok(false)
        }
        fn with_inputs(&self, _: &bool, inputs: &[bool]) -> Result<bool, String> {
            Ok(inputs[0])
        }
        fn step(&self, state: &bool) -> Result<bool, String> {
            Ok(*state)
        }
        fn outputs(&self, state: &bool) -> Result<Vec<bool>, String> {
            Ok(vec![!*state])
        }
        fn step_with_observations(&self, state: &bool) -> Result<(bool, Vec<Vec<bool>>), String> {
            Ok((*state, vec![vec![*state], vec![!*state]]))
        }
    }
    let report = verify_repeated_settling(&not_type(), &Pulsing, BehaviorBudget::default());
    assert_eq!(report.status, CheckStatus::Failed);
    assert!(
        report
            .counterexample
            .unwrap()
            .cycle_outputs
            .contains(&vec![false])
    );
}

#[test]
fn a_reachable_bad_cycle_refutes_a_type_without_closing_an_unbounded_branch() {
    struct Growing;
    impl BehaviorModel for Growing {
        type State = (bool, u64);
        fn initial_state(&self) -> Result<Self::State, String> {
            Ok((false, 0))
        }
        fn with_inputs(&self, state: &Self::State, inputs: &[bool]) -> Result<Self::State, String> {
            Ok((inputs[0], state.1))
        }
        fn step(&self, state: &Self::State) -> Result<Self::State, String> {
            Ok((state.0, state.1 + u64::from(state.0)))
        }
        fn outputs(&self, _: &Self::State) -> Result<Vec<bool>, String> {
            Ok(vec![false])
        }
    }
    let report = verify_repeated_settling(
        &not_type(),
        &Growing,
        BehaviorBudget {
            max_states: 16,
            ..BehaviorBudget::default()
        },
    );
    assert_eq!(report.status, CheckStatus::Failed);
    assert!(!report.graph_closed);
    let witness = report.counterexample.unwrap();
    assert_eq!(witness.held_inputs, vec![false]);
    assert_eq!(witness.cycle_outputs, vec![vec![false]]);
}

#[test]
fn new_behavior_type_round_trips_without_rebinding_connection_types_or_old_archives() {
    let mut catalog = builtin_blueprints().clone();
    let old = catalog.to_json().unwrap();
    let old_wire = catalog
        .type_revision(&TypeRevisionId::new(WIRE_TYPE_REVISION).unwrap())
        .unwrap()
        .clone();
    catalog.insert_type(not_type()).unwrap();
    assert_eq!(catalog.type_revision(&old_wire.id), Some(&old_wire));
    assert!(catalog.insert_type(not_type()).is_err());
    let saved = catalog.to_json().unwrap();
    assert!(saved.contains("dustroute.blueprint-catalog.v13"));
    assert_eq!(BlueprintCatalog::from_json(&saved).unwrap(), catalog);
    assert!(
        BlueprintCatalog::from_json(&saved.replace(
            "dustroute.blueprint-catalog.v13",
            "dustroute.blueprint-catalog.v2"
        ))
        .is_err()
    );
    assert_eq!(
        BlueprintCatalog::from_json(&old).unwrap(),
        *builtin_blueprints()
    );
}

#[test]
fn malformed_relations_and_snapshot_only_behavior_certification_are_refused() {
    let mut definition = not_type();
    let TypeContract::RepeatedSettling { relation } = &mut definition.contract else {
        unreachable!()
    };
    relation.rows[1].inputs = vec![false];
    let mut catalog = builtin_blueprints().clone();
    assert!(catalog.insert_type(definition).is_err());
    catalog.insert_type(not_type()).unwrap();
    let source = catalog
        .revision(&BlueprintRevisionId::new(NOT_TOP_REVISION).unwrap())
        .unwrap()
        .ports
        .iter()
        .find(|p| p.direction == PortDirection::Output)
        .unwrap()
        .clone();
    let mut consumer = source.clone();
    consumer.direction = PortDirection::Input;
    consumer.required_source_types = vec![not_type().id];
    assert!(matches!(
        catalog.check_source_requirements(&source, &consumer, |_| Some(Block::new(
            BlockKind::RedstoneWire
        ))),
        Err(BlueprintError::BehavioralEvidenceRequired(_))
    ));
}

#[test]
fn repeated_input_changes_preserve_internal_state_and_expose_a_one_shot_impostor() {
    let good = model();
    assert_eq!(
        verify_repeated_settling(&not_type(), &good, BehaviorBudget::default()).status,
        CheckStatus::Passed
    );
    let bad = Model {
        one_shot: true,
        ..model()
    };
    // Each reset-based truth-table row passes, which is insufficient for this type.
    for input in [false, true] {
        let mut state = bad
            .with_inputs(&bad.initial_state().unwrap(), &[input])
            .unwrap();
        for _ in 0..8 {
            state = bad.step(&state).unwrap();
        }
        assert_eq!(bad.outputs(&state).unwrap(), vec![!input]);
    }
    let report = verify_repeated_settling(&not_type(), &bad, BehaviorBudget::default());
    assert_eq!(report.status, CheckStatus::Failed);
    let witness = report.counterexample.unwrap();
    assert!(!witness.prefix.is_empty());
    let mut state = bad.initial_state().unwrap();
    for action in witness.prefix {
        state = match action {
            WitnessAction::SetInputs { inputs } => bad.with_inputs(&state, &inputs).unwrap(),
            WitnessAction::Advance => bad.step(&state).unwrap(),
        };
    }
    state = bad.with_inputs(&state, &witness.held_inputs).unwrap();
    for _ in 0..8 {
        state = bad.step(&state).unwrap();
    }
    assert_ne!(bad.outputs(&state).unwrap(), witness.expected_outputs);
}

#[test]
fn output_settling_does_not_require_internal_quiescence_or_a_fixed_delay() {
    let delayed = Model {
        delay: 24,
        internal_clock: true,
        ..model()
    };
    let report = verify_repeated_settling(&not_type(), &delayed, BehaviorBudget::default());
    assert_eq!(report.status, CheckStatus::Passed, "{report:?}");
    let unstable = Model {
        oscillating_output: true,
        ..model()
    };
    assert_eq!(
        verify_repeated_settling(&not_type(), &unstable, BehaviorBudget::default()).status,
        CheckStatus::Failed
    );
}

#[test]
fn incomplete_reachability_is_undetermined_even_when_samples_pass() {
    for budget in [
        BehaviorBudget {
            max_states: 1,
            ..BehaviorBudget::default()
        },
        BehaviorBudget {
            max_steps: 1,
            ..BehaviorBudget::default()
        },
        BehaviorBudget {
            max_elapsed: Duration::ZERO,
            ..BehaviorBudget::default()
        },
    ] {
        let report = verify_repeated_settling(&not_type(), &model(), budget);
        assert_eq!(report.status, CheckStatus::Undetermined);
        assert!(report.counterexample.is_none());
    }
}

#[test]
fn behavioral_types_can_have_multiple_inputs_and_outputs_without_gate_names() {
    struct Pair;
    impl BehaviorModel for Pair {
        type State = Vec<bool>;
        fn initial_state(&self) -> Result<Self::State, String> {
            Ok(vec![false, false])
        }
        fn with_inputs(&self, _: &Self::State, input: &[bool]) -> Result<Self::State, String> {
            Ok(input.to_vec())
        }
        fn step(&self, s: &Self::State) -> Result<Self::State, String> {
            Ok(s.clone())
        }
        fn outputs(&self, s: &Self::State) -> Result<Vec<bool>, String> {
            Ok(vec![s[0] ^ s[1], s[0] && s[1]])
        }
    }
    let definition = TypeRevision {
        id: TypeRevisionId::new("pair.v1").unwrap(),
        name: "Named relation".into(),
        contract: TypeContract::RepeatedSettling {
            relation: RepeatedSettling {
                inputs: vec!["left".into(), "right".into()],
                outputs: vec!["first".into(), "second".into()],
                rows: (0..4)
                    .map(|bits| {
                        let input = vec![bits & 1 != 0, bits & 2 != 0];
                        BooleanRow {
                            outputs: Pair.outputs(&input).unwrap(),
                            inputs: input,
                        }
                    })
                    .collect(),
            },
        },
    };
    assert_eq!(
        verify_repeated_settling(&definition, &Pair, BehaviorBudget::default()).status,
        CheckStatus::Passed
    );
}
