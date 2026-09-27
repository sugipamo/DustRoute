use std::collections::{BTreeSet, VecDeque};

use dustroute_minecraft::law::{Expr, Instruction};
use dustroute_minecraft::repeater_law::{
    BoundedRepeaterLaw, CompatibilityRepeaterLaw, builtin_bounded_law, builtin_compatibility_law,
    builtin_programs,
};

#[test]
fn every_reachable_compatibility_queue_preserves_output_locking_and_pending_work() {
    let law = builtin_compatibility_law();
    for delay in 1..=4 {
        for powered in [false, true] {
            let initial = law.initial(delay, powered).unwrap();
            assert_eq!(initial.powered(), powered);
            assert!(!initial.has_pending_output());
            let mut pending =
                VecDeque::from([(initial, VecDeque::from(vec![powered; usize::from(delay)]))]);
            let mut visited = BTreeSet::new();
            let mut queues = BTreeSet::new();
            while let Some((state, queue)) = pending.pop_front() {
                if !visited.insert((state.clone(), queue.clone())) {
                    continue;
                }
                assert!(
                    visited.len() < 4096,
                    "the retained Boolean queue has a finite state space"
                );
                queues.insert(queue.clone());
                for rear in [false, true] {
                    for locked in [false, true] {
                        // Frozen pre-migration VecDeque rule, independent of the program.
                        let mut expected = queue.clone();
                        if !locked {
                            expected.pop_front();
                            expected.push_back(rear);
                        }
                        let before = state.clone();
                        let next = law.boundary(&state, rear, locked).unwrap();
                        assert_eq!(state, before);
                        assert_eq!(next.powered(), expected[0]);
                        assert_eq!(
                            next.has_pending_output(),
                            expected.iter().any(|v| *v != expected[0])
                        );
                        pending.push_back((next, expected));
                    }
                }
            }
            assert_eq!(queues.len(), 1 << delay);
        }
    }
}

#[test]
fn bounded_decisions_preserve_delay_and_stale_callback_contracts() {
    let law = builtin_bounded_law();
    for powered in [false, true] {
        for rear in [false, true] {
            assert_eq!(law.needs_update(powered, rear), powered != rear);
            for expected in [false, true] {
                assert_eq!(
                    law.scheduled_output(rear, expected),
                    (rear == expected).then_some(expected)
                );
            }
        }
    }
    for delay in 1..=4 {
        assert_eq!(law.delay_game_ticks(delay), Some(u64::from(delay) * 2));
    }
    for delay in [0, 5, 255] {
        assert_eq!(law.delay_game_ticks(delay), None);
        assert!(builtin_compatibility_law().initial(delay, false).is_err());
    }
}

#[test]
fn changed_programs_drive_outputs_instead_of_selecting_hardcoded_device_handlers() {
    let originals = builtin_programs().clone();
    let mut compat = originals[0].clone();
    compat
        .handlers
        .get_mut("boundary")
        .unwrap()
        .push(Instruction::Set {
            register: "powered".into(),
            value: Expr::Constant { value: 0 },
        });
    let changed = CompatibilityRepeaterLaw::compile(&compat).unwrap();
    let seed = changed.initial(1, false).unwrap();
    assert!(!changed.boundary(&seed, true, false).unwrap().powered());
    assert!(
        builtin_compatibility_law()
            .boundary(&seed, true, false)
            .unwrap()
            .powered()
    );

    let mut bounded = originals[1].clone();
    for (register, value) in [
        ("needs_update", 0),
        ("apply_tick", 1),
        ("next_powered", 1),
        ("delay_game_ticks", 7),
    ] {
        bounded
            .handlers
            .get_mut("evaluate")
            .unwrap()
            .push(Instruction::Set {
                register: register.into(),
                value: Expr::Constant { value },
            });
    }
    let changed = BoundedRepeaterLaw::compile(&bounded).unwrap();
    assert!(!changed.needs_update(false, true));
    assert_eq!(changed.delay_game_ticks(1), Some(7));
    assert_eq!(changed.scheduled_output(false, true), Some(true));
    assert_eq!(builtin_programs(), &originals);
}

#[test]
fn adapters_reject_incompatible_programs_and_failed_execution_preserves_input_state() {
    let mut program = builtin_programs()[0].clone();
    program.inputs.insert("unknown".into(), 1);
    assert!(CompatibilityRepeaterLaw::compile(&program).is_err());
    let mut program = builtin_programs()[0].clone();
    program
        .handlers
        .get_mut("boundary")
        .unwrap()
        .push(Instruction::ScheduleIfAbsent {
            event: "boundary".into(),
            after: 1,
        });
    assert!(CompatibilityRepeaterLaw::compile(&program).is_err());

    let mut program = builtin_programs()[0].clone();
    program
        .handlers
        .get_mut("boundary")
        .unwrap()
        .push(Instruction::Set {
            register: "powered".into(),
            value: Expr::Constant { value: 2 },
        });
    let law = CompatibilityRepeaterLaw::compile(&program).unwrap();
    let seed = law.initial(1, false).unwrap();
    let before = seed.clone();
    assert!(law.boundary(&seed, true, false).is_err());
    assert_eq!(seed, before);

    let mut program = builtin_programs()[1].clone();
    program
        .handlers
        .get_mut("evaluate")
        .unwrap()
        .push(Instruction::Set {
            register: "delay_game_ticks".into(),
            value: Expr::Constant { value: 9 },
        });
    assert!(BoundedRepeaterLaw::compile(&program).is_err());
}
