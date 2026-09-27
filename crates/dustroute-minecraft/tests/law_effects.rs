use std::collections::BTreeMap;

use dustroute_minecraft::law::{Expr, Instruction, LawProgram, Register};

fn program() -> LawProgram {
    LawProgram {
        inputs: BTreeMap::new(),
        registers: BTreeMap::from([(
            "output".into(),
            Register {
                initial: 1,
                maximum: 1,
            },
        )]),
        histories: BTreeMap::new(),
        handlers: BTreeMap::from([
            (
                "start".into(),
                vec![Instruction::ScheduleIfAbsent {
                    event: "tick".into(),
                    after: 1,
                }],
            ),
            (
                "tick".into(),
                vec![
                    Instruction::Set {
                        register: "output".into(),
                        value: Expr::Constant { value: 0 },
                    },
                    Instruction::ScheduleIfAbsent {
                        event: "tick".into(),
                        after: 17,
                    },
                ],
            ),
            (
                "feedback".into(),
                vec![Instruction::ScheduleIfAbsent {
                    event: "tick".into(),
                    after: 3,
                }],
            ),
        ]),
    }
}

#[test]
fn synchronous_effect_schedules_before_the_interrupted_handler_resumes() {
    let law = program().compile().unwrap();
    let state = law
        .event(&law.initial_state(), "start", &BTreeMap::new())
        .unwrap();
    let original = state.clone();
    let atomic = law.advance(&state).unwrap();
    assert_eq!(atomic.pending("tick"), Some(17));
    let mut calls = 0;
    let resumed = law
        .advance_with_register_effects(&state, &mut |law, local, name| {
            calls += 1;
            assert_eq!(name, "output");
            assert_eq!(local.register("output"), Some(0));
            assert_eq!(local.pending("tick"), None);
            *local = law.event(local, "feedback", &BTreeMap::new())?;
            Ok(())
        })
        .unwrap();
    assert_eq!(calls, 1);
    assert_eq!(resumed.pending("tick"), Some(3));
    assert_eq!(state, original);
    assert_eq!(law.advance(&state).unwrap(), atomic);
}

#[test]
fn unchanged_assignments_do_not_invent_physical_notifications() {
    let law = program().compile().unwrap();
    let unlit = law
        .initialize_register(&law.initial_state(), "output", 0)
        .unwrap();
    let state = law.event(&unlit, "start", &BTreeMap::new()).unwrap();
    let next = law
        .advance_with_register_effects(&state, &mut |_, _, _| {
            panic!("no output change should deliver an effect")
        })
        .unwrap();
    assert_eq!(next.pending("tick"), Some(17));
}

#[test]
fn failed_effect_discards_tentative_state_and_pending_requests() {
    let law = program().compile().unwrap();
    let state = law
        .event(&law.initial_state(), "start", &BTreeMap::new())
        .unwrap();
    let original = state.clone();
    let result = law.advance_with_register_effects(&state, &mut |law, local, _| {
        *local = law.event(local, "feedback", &BTreeMap::new())?;
        Err("adapter could not resolve physical effects".into())
    });
    assert!(result.unwrap_err().contains("could not resolve"));
    assert_eq!(state, original);
    assert_eq!(law.advance(&state).unwrap().pending("tick"), Some(17));
}
