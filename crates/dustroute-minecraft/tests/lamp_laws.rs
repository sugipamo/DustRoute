use dustroute_minecraft::lamp_law::{
    BoundedLampLaw, CompatibilityLampLaw, LampDeadline, builtin_bounded_law,
    builtin_compatibility_law, builtin_programs,
};
use dustroute_minecraft::law::{Expr, Instruction};

#[test]
fn compatibility_effects_match_the_frozen_cache_and_saturating_deadline_rules() {
    for now in [0, 1, u64::MAX - 1, u64::MAX] {
        for deadline in [None, Some(0), Some(now), Some(u64::MAX)] {
            for powered in [false, true] {
                for lit in [None, Some(false), Some(true)] {
                    // Retained native algorithm, independent of the law data.
                    let mut expected_lit = lit;
                    let mut expected_deadline = deadline;
                    if powered {
                        expected_lit = Some(true);
                        expected_deadline = None;
                    } else if lit.unwrap_or(false) {
                        let expires = *expected_deadline.get_or_insert(now.saturating_add(2));
                        if now >= expires {
                            expected_lit = Some(false);
                            expected_deadline = None;
                        }
                    }
                    let action = builtin_compatibility_law().boundary(
                        powered,
                        lit.unwrap_or(false),
                        deadline.map(|t| t <= now),
                        now == u64::MAX,
                    );
                    let actual_deadline = match action.deadline {
                        LampDeadline::Keep => deadline,
                        LampDeadline::Clear => None,
                        LampDeadline::SetAfter(delay) => Some(now.saturating_add(u64::from(delay))),
                    };
                    assert_eq!(
                        (action.lit.or(lit), actual_deadline),
                        (expected_lit, expected_deadline),
                        "now={now}, deadline={deadline:?}, powered={powered}, lit={lit:?}"
                    );
                }
            }
        }
    }
    for powered in [false, true] {
        assert_eq!(
            builtin_compatibility_law().initially_lit(powered),
            Some(powered)
        );
        assert_eq!(builtin_bounded_law().lit(powered), powered);
    }
}

#[test]
fn executable_data_controls_deadlines_initialization_and_bounded_output() {
    let original = builtin_programs().clone();
    let mut program = original[0].clone();
    program.handlers.get_mut("evaluate").unwrap().extend([
        Instruction::Set {
            register: "after".into(),
            value: Expr::Constant { value: 5 },
        },
        Instruction::Set {
            register: "lit".into(),
            value: Expr::Constant { value: 0 },
        },
    ]);
    let changed = CompatibilityLampLaw::compile(&program).unwrap();
    assert_eq!(
        changed.boundary(false, true, None, false).deadline,
        LampDeadline::SetAfter(5)
    );
    assert_eq!(changed.initially_lit(true), Some(false));
    let mut program = original[1].clone();
    program
        .handlers
        .get_mut("evaluate")
        .unwrap()
        .push(Instruction::Set {
            register: "lit".into(),
            value: Expr::Not {
                value: Box::new(Expr::Input {
                    name: "powered".into(),
                }),
            },
        });
    let changed = BoundedLampLaw::compile(&program).unwrap();
    assert!(!changed.lit(true));
    assert!(changed.lit(false));
    assert_eq!(builtin_programs(), &original);
}

#[test]
fn law_adapters_reject_events_wrong_abis_and_invalid_effects() {
    let mut program = builtin_programs()[0].clone();
    program
        .handlers
        .get_mut("evaluate")
        .unwrap()
        .push(Instruction::ScheduleIfAbsent {
            event: "evaluate".into(),
            after: 1,
        });
    assert!(CompatibilityLampLaw::compile(&program).is_err());
    let mut program = builtin_programs()[0].clone();
    program
        .handlers
        .get_mut("evaluate")
        .unwrap()
        .push(Instruction::Set {
            register: "deadline".into(),
            value: Expr::Constant { value: 3 },
        });
    assert!(CompatibilityLampLaw::compile(&program).is_err());
    assert!(CompatibilityLampLaw::compile(&builtin_programs()[1]).is_err());
    assert!(BoundedLampLaw::compile(&builtin_programs()[0]).is_err());
}
