use dustroute_minecraft::dust_law::{DustStrengthLaw, builtin_dust_law, builtin_program};
use dustroute_minecraft::law::{Expr, Instruction};

#[test]
fn legal_domain_matches_the_old_calculation_and_outside_values_are_unavailable() {
    let law = builtin_dust_law();
    for direct in 0u8..=255 {
        for neighbor in 0u8..=255 {
            let expected =
                (direct <= 15 && neighbor <= 15).then(|| direct.max(neighbor.saturating_sub(1)));
            assert_eq!(law.strength(direct, neighbor), expected);
        }
    }
}

#[test]
fn shared_compiler_executes_program_changes_and_preserves_the_original_abi() {
    let original = builtin_program().clone();
    let mut changed = original.clone();
    changed
        .handlers
        .get_mut("evaluate")
        .unwrap()
        .push(Instruction::Set {
            register: "strength".into(),
            value: Expr::Constant { value: 3 },
        });
    let law = DustStrengthLaw::compile(&changed).unwrap();
    assert_eq!(law.strength(15, 15), Some(3));
    assert_eq!(law.strength(16, 15), None);
    assert_eq!(builtin_program(), &original);
    changed
        .handlers
        .get_mut("evaluate")
        .unwrap()
        .push(Instruction::ScheduleIfAbsent {
            event: "evaluate".into(),
            after: 1,
        });
    assert!(DustStrengthLaw::compile(&changed).is_err());
}
