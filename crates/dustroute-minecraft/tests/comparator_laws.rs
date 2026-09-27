use dustroute_minecraft::comparator_law::{
    CompatibilityComparatorLaw, builtin_comparator_law, builtin_program,
};
use dustroute_minecraft::law::{Expr, Instruction};

// Frozen pre-migration calculation, independent of the executable program.
fn previous_output(rear: u8, sides: [u8; 2], subtract: bool) -> u8 {
    let side = sides.into_iter().max().unwrap();
    if subtract {
        rear.saturating_sub(side)
    } else if rear >= side {
        rear
    } else {
        0
    }
}

#[test]
fn all_8192_signal_input_combinations_preserve_both_modes_and_side_selection() {
    let law = builtin_comparator_law();
    let mut count = 0;
    for rear in 0..=15 {
        for a in 0..=15 {
            for b in 0..=15 {
                for subtract in [false, true] {
                    assert_eq!(
                        law.evaluate(rear, [a, b], subtract).unwrap(),
                        previous_output(rear, [a, b], subtract),
                        "rear={rear}, side_a={a}, side_b={b}, subtract={subtract}"
                    );
                    count += 1;
                }
            }
        }
    }
    assert_eq!(count, 8192);
}

#[test]
fn raw_u8_compatibility_values_are_preserved_without_clamping_or_history() {
    let law = builtin_comparator_law();
    for rear in [0, 1, 7, 15, 16, 127, 255] {
        for a in [0, 1, 7, 15, 16, 127, 255] {
            for b in [0, 1, 7, 15, 16, 127, 255] {
                for subtract in [false, true] {
                    assert_eq!(
                        law.evaluate(rear, [a, b], subtract).unwrap(),
                        previous_output(rear, [a, b], subtract)
                    );
                    // Evaluation has no retained local state from the prior call.
                    assert_eq!(law.evaluate(0, [0, 0], false).unwrap(), 0);
                }
            }
        }
    }
}

#[test]
fn changed_programs_drive_outputs_and_incompatible_adapters_are_rejected() {
    let original = builtin_program().clone();
    let mut program = original.clone();
    program
        .handlers
        .get_mut("evaluate")
        .unwrap()
        .push(Instruction::Set {
            register: "strength".into(),
            value: Expr::Constant { value: 3 },
        });
    let changed = CompatibilityComparatorLaw::compile(&program).unwrap();
    assert_eq!(changed.evaluate(15, [0, 0], false).unwrap(), 3);
    assert_eq!(
        builtin_comparator_law()
            .evaluate(15, [0, 0], false)
            .unwrap(),
        15
    );
    assert_eq!(builtin_program(), &original);

    let mut program = original.clone();
    program.inputs.insert("rear".into(), 15);
    assert!(CompatibilityComparatorLaw::compile(&program).is_err());
    let mut program = original.clone();
    program
        .handlers
        .get_mut("evaluate")
        .unwrap()
        .push(Instruction::ScheduleIfAbsent {
            event: "evaluate".into(),
            after: 1,
        });
    assert!(CompatibilityComparatorLaw::compile(&program).is_err());
    let mut program = original;
    program
        .handlers
        .get_mut("evaluate")
        .unwrap()
        .push(Instruction::Set {
            register: "strength".into(),
            value: Expr::Constant { value: 256 },
        });
    let invalid_execution = CompatibilityComparatorLaw::compile(&program).unwrap();
    assert!(invalid_execution.evaluate(15, [0, 0], false).is_err());
    assert_eq!(
        builtin_comparator_law()
            .evaluate(15, [0, 0], false)
            .unwrap(),
        15
    );
}
