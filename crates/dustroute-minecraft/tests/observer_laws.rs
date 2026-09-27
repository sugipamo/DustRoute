use dustroute_minecraft::law::{Expr, Instruction};
use dustroute_minecraft::observer_law::{
    CompatibilityObserverLaw, ObserverChanges, ObserverDeadline, ObserverPulseAction,
    builtin_observer_law, builtin_program,
};

#[test]
fn all_change_combinations_require_a_baseline_and_preserve_the_field_union() {
    for mask in 0..512 {
        let bit = |n| mask & (1 << n) != 0;
        let changes = ObserverChanges {
            block_record: bit(0),
            signal: bit(1),
            weak: bit(2),
            strong: bit(3),
            repeater: bit(4),
            torch: bit(5),
            comparator: bit(6),
            observer: bit(7),
            lamp: bit(8),
        };
        for known in [false, true] {
            assert_eq!(
                builtin_observer_law()
                    .should_notify(known, changes)
                    .unwrap(),
                known && mask != 0
            );
        }
    }
}

#[test]
fn pulse_actions_preserve_presence_deadline_and_one_boundary_duration() {
    let law = builtin_observer_law();
    for present in [false, true] {
        assert_eq!(
            law.start(present).unwrap(),
            present.then_some(ObserverPulseAction {
                powered: true,
                deadline: ObserverDeadline::SetAfter(1),
            })
        );
        for due in [false, true] {
            assert_eq!(
                law.end(present, due).unwrap(),
                (present && due).then_some(ObserverPulseAction {
                    powered: false,
                    deadline: ObserverDeadline::Clear,
                })
            );
        }
    }
}

#[test]
fn programs_control_detection_and_duration_without_changing_the_published_law() {
    let original = builtin_program().clone();
    let mut changed = original.clone();
    changed
        .handlers
        .get_mut("observe")
        .unwrap()
        .push(Instruction::Set {
            register: "notify".into(),
            value: Expr::Constant { value: 0 },
        });
    changed
        .handlers
        .get_mut("start")
        .unwrap()
        .push(Instruction::Set {
            register: "after".into(),
            value: Expr::Constant { value: 3 },
        });
    let law = CompatibilityObserverLaw::compile(&changed).unwrap();
    assert!(
        !law.should_notify(
            true,
            ObserverChanges {
                signal: true,
                ..Default::default()
            }
        )
        .unwrap()
    );
    assert_eq!(
        law.start(true).unwrap().unwrap().deadline,
        ObserverDeadline::SetAfter(3)
    );
    assert_eq!(builtin_program(), &original);

    changed
        .handlers
        .get_mut("start")
        .unwrap()
        .push(Instruction::ScheduleIfAbsent {
            event: "end".into(),
            after: 1,
        });
    assert!(CompatibilityObserverLaw::compile(&changed).is_err());
    let mut invalid = original;
    invalid
        .handlers
        .get_mut("end")
        .unwrap()
        .push(Instruction::Set {
            register: "deadline".into(),
            value: Expr::Constant { value: 3 },
        });
    assert!(
        CompatibilityObserverLaw::compile(&invalid)
            .unwrap()
            .end(true, true)
            .is_err()
    );
    assert_eq!(
        builtin_observer_law()
            .end(true, true)
            .unwrap()
            .unwrap()
            .deadline,
        ObserverDeadline::Clear
    );
}
