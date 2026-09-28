use dustroute_library::blueprint::{BlueprintCatalog, BlueprintRevisionId};
use dustroute_library::builtin_laws::{builtin_laws, torch_law_revision};
use dustroute_minecraft::law::{Expr, Instruction};
use std::collections::BTreeMap;

#[test]
fn motion_time_laws_are_immutable_executable_blueprint_revisions() {
    use dustroute_minecraft::execution_context::{WorldExecutionContext, WorldExecutionProfile};
    use dustroute_minecraft::piston_motion_law::{LAW_IDS, PistonMotionLaws, builtin_programs};
    use dustroute_minecraft::time::runtime::{HalfProgress, MotionHistory};
    let mut catalog = builtin_laws().clone();
    let programs = builtin_programs().clone();
    for (id, program) in LAW_IDS.into_iter().zip(&programs) {
        assert_eq!(
            catalog
                .revision(&BlueprintRevisionId::new(id).unwrap())
                .unwrap()
                .law
                .as_ref(),
            Some(program)
        );
    }
    let original = catalog
        .revision(&BlueprintRevisionId::new(LAW_IDS[1]).unwrap())
        .unwrap()
        .clone();
    let mut changed = original.clone();
    changed
        .law
        .as_mut()
        .unwrap()
        .handlers
        .get_mut("evaluate")
        .unwrap()
        .push(Instruction::Set {
            register: "next_progress".into(),
            value: Expr::Constant { value: 0 },
        });
    assert!(catalog.insert_revision(changed.clone()).is_err());
    changed.id = BlueprintRevisionId::new("test.piston.carrier.hold.v1").unwrap();
    changed.parents = vec![original.id.clone()];
    catalog.insert_revision(changed.clone()).unwrap();
    let loaded = BlueprintCatalog::from_json(&catalog.to_json().unwrap()).unwrap();
    assert_eq!(loaded.revision(&changed.id), Some(&changed));
    let mut alternative = programs.clone();
    alternative[1] = loaded.revision(&changed.id).unwrap().law.clone().unwrap();
    assert_eq!(
        PistonMotionLaws::compile(&programs)
            .unwrap()
            .carrier(MotionHistory::fresh(), false, false, 1)
            .history
            .progress,
        HalfProgress::Half
    );
    assert_eq!(
        PistonMotionLaws::compile(&alternative)
            .unwrap()
            .carrier(MotionHistory::fresh(), false, false, 1)
            .history
            .progress,
        HalfProgress::Zero
    );
    let context = WorldExecutionContext::for_profile(
        WorldExecutionProfile::UnifiedPistonElectricalCallbacksJava12111V12,
    );
    assert_eq!(
        context.synchronous_runtime_profile(),
        Some(dustroute_minecraft::time::runtime::PROFILE)
    );
    assert!(!context.is_proof_profile());
    let mut impostor = changed;
    impostor.id = original.id;
    impostor.parents.clear();
    let mut conflict = BlueprintCatalog::default();
    conflict.insert_revision(impostor).unwrap();
    assert!(
        dustroute_library::execution_context::resolve_law_references(&conflict, &context)
            .unwrap_err()
            .contains("immutable law")
    );
}

#[test]
fn observer_law_round_trips_without_rebinding_detection_or_pulse_rules() {
    use dustroute_minecraft::observer_law::{
        CompatibilityObserverLaw, OBSERVER_LAW_REVISION, ObserverDeadline, builtin_program,
    };
    let mut catalog = builtin_laws().clone();
    let original = catalog
        .revision(&BlueprintRevisionId::new(OBSERVER_LAW_REVISION).unwrap())
        .unwrap()
        .clone();
    assert_eq!(original.law.as_ref(), Some(builtin_program()));
    let mut changed = original.clone();
    changed.id = BlueprintRevisionId::new("test.observer.three-boundaries.v1").unwrap();
    changed.parents = vec![original.id.clone()];
    changed
        .law
        .as_mut()
        .unwrap()
        .handlers
        .get_mut("start")
        .unwrap()
        .push(Instruction::Set {
            register: "after".into(),
            value: Expr::Constant { value: 3 },
        });
    catalog.insert_revision(changed.clone()).unwrap();
    let mut conflict = changed.clone();
    conflict.id = original.id.clone();
    assert!(catalog.insert_revision(conflict).is_err());
    let loaded = BlueprintCatalog::from_json(&catalog.to_json().unwrap()).unwrap();
    for (source, duration) in [(&original, 1), (&changed, 3)] {
        assert_eq!(loaded.revision(&source.id), Some(source));
        let law = CompatibilityObserverLaw::compile(
            loaded.revision(&source.id).unwrap().law.as_ref().unwrap(),
        )
        .unwrap();
        assert_eq!(
            law.start(true).unwrap().unwrap().deadline,
            ObserverDeadline::SetAfter(duration)
        );
    }
}

#[test]
fn comparator_law_round_trips_without_rebinding_the_published_revision() {
    use dustroute_minecraft::comparator_law::{
        COMPARATOR_LAW_REVISION, CompatibilityComparatorLaw, builtin_program,
    };
    let mut catalog = builtin_laws().clone();
    let id = BlueprintRevisionId::new(COMPARATOR_LAW_REVISION).unwrap();
    let original = catalog.revision(&id).unwrap().clone();
    assert_eq!(original.law.as_ref(), Some(builtin_program()));
    let mut changed = original.clone();
    changed.id = BlueprintRevisionId::new("test.comparator.constant.v1").unwrap();
    changed.parents = vec![id.clone()];
    changed
        .law
        .as_mut()
        .unwrap()
        .handlers
        .get_mut("evaluate")
        .unwrap()
        .push(Instruction::Set {
            register: "strength".into(),
            value: Expr::Constant { value: 3 },
        });
    catalog.insert_revision(changed.clone()).unwrap();
    let mut conflicting = changed.clone();
    conflicting.id = id;
    assert!(catalog.insert_revision(conflicting).is_err());
    let loaded = BlueprintCatalog::from_json(&catalog.to_json().unwrap()).unwrap();
    assert_eq!(loaded.revision(&original.id), Some(&original));
    assert_eq!(loaded.revision(&changed.id), Some(&changed));
    for (id, expected) in [(&original.id, 12), (&changed.id, 3)] {
        let law =
            CompatibilityComparatorLaw::compile(loaded.revision(id).unwrap().law.as_ref().unwrap())
                .unwrap();
        assert_eq!(law.evaluate(15, [1, 3], true).unwrap(), expected);
    }
}

#[test]
fn repeater_laws_round_trip_as_distinct_immutable_executable_revisions() {
    use dustroute_minecraft::repeater_law::{
        BoundedRepeaterLaw, CompatibilityRepeaterLaw, REPEATER_LAW_IDS, builtin_programs,
    };
    let mut catalog = builtin_laws().clone();
    let ids = REPEATER_LAW_IDS.map(|id| BlueprintRevisionId::new(id).unwrap());
    for (id, program) in ids.iter().zip(builtin_programs()) {
        assert_eq!(catalog.revision(id).unwrap().law.as_ref(), Some(program));
    }
    let original = catalog.revision(&ids[0]).unwrap().clone();
    let mut changed = original.clone();
    changed.id = BlueprintRevisionId::new("test.repeater.always-off.v1").unwrap();
    changed.parents = vec![original.id.clone()];
    changed
        .law
        .as_mut()
        .unwrap()
        .handlers
        .get_mut("boundary")
        .unwrap()
        .push(Instruction::Set {
            register: "powered".into(),
            value: Expr::Constant { value: 0 },
        });
    catalog.insert_revision(changed.clone()).unwrap();
    let mut conflicting = changed.clone();
    conflicting.id = original.id.clone();
    assert!(catalog.insert_revision(conflicting).is_err());
    let loaded = BlueprintCatalog::from_json(&catalog.to_json().unwrap()).unwrap();
    assert_eq!(loaded.revision(&original.id), Some(&original));
    assert_eq!(loaded.revision(&changed.id), Some(&changed));
    for (id, expected) in [(&original.id, true), (&changed.id, false)] {
        let law =
            CompatibilityRepeaterLaw::compile(loaded.revision(id).unwrap().law.as_ref().unwrap())
                .unwrap();
        let seed = law.initial(1, false).unwrap();
        assert_eq!(
            law.boundary(&seed, true, false).unwrap().powered(),
            expected
        );
    }
    let bounded =
        BoundedRepeaterLaw::compile(loaded.revision(&ids[1]).unwrap().law.as_ref().unwrap())
            .unwrap();
    assert_eq!(bounded.scheduled_output(false, true), None);
}

#[test]
fn spatial_blueprint_revisions_round_trip_and_drive_world_support_without_rebinding() {
    use dustroute_library::builtin_laws::spatial_law_revisions;
    use dustroute_minecraft::spatial::SpatialLaws;
    use dustroute_minecraft::{Block, BlockKind, Pos, World};
    let mut catalog = builtin_laws().clone();
    let mut ids = spatial_law_revisions().clone();
    let original = catalog.revision(&ids[0]).unwrap().clone();
    let mut revised = original.clone();
    revised.id = BlueprintRevisionId::new("test.spatial.no-support.v1").unwrap();
    revised.parents = vec![original.id.clone()];
    revised
        .law
        .as_mut()
        .unwrap()
        .handlers
        .get_mut("evaluate")
        .unwrap()
        .push(Instruction::Set {
            register: "dust_support".into(),
            value: Expr::Constant { value: 0 },
        });
    ids[0] = revised.id.clone();
    catalog.insert_revision(revised.clone()).unwrap();
    assert!(catalog.insert_revision(original.clone()).is_err());
    let loaded = BlueprintCatalog::from_json(&catalog.to_json().unwrap()).unwrap();
    assert_eq!(loaded.revision(&original.id), Some(&original));
    assert_eq!(loaded.revision(&revised.id), Some(&revised));
    let programs = ids.map(|id| loaded.revision(&id).unwrap().law.clone().unwrap());
    let compiled = SpatialLaws::compile(&programs).unwrap();
    let mut world = World::new();
    world.set(Pos::default(), Block::new(BlockKind::Solid));
    world.place(BlockKind::RedstoneWire, Pos::new(0, 1, 0));
    let unchanged = world.clone();
    assert!(world.support_issues().is_empty());
    assert_eq!(
        world.support_issues_with_laws(&compiled),
        vec![(
            Pos::new(0, 1, 0),
            BlockKind::RedstoneWire,
            Some(Pos::default())
        )]
    );
    assert_eq!(world, unchanged);
}

#[test]
fn law_revisions_round_trip_and_modified_rules_execute_without_rebinding_the_source() {
    let mut catalog = builtin_laws().clone();
    let original = torch_law_revision().clone();
    let before = original.law.as_ref().unwrap().compile().unwrap();
    let mut changed = original.clone();
    changed.id = BlueprintRevisionId::new("test.noninverting-law.v1").unwrap();
    changed.parents = vec![original.id.clone()];
    // The interpreter has no hardcoded torch/NOT meaning: replace the handler
    // with an intentionally incompatible rule and observe its actual execution.
    changed.law.as_mut().unwrap().handlers.insert(
        "scheduled_tick".into(),
        vec![Instruction::Set {
            register: "lit".into(),
            value: Expr::Input {
                name: "powered".into(),
            },
        }],
    );
    catalog.insert_revision(changed.clone()).unwrap();
    assert!(catalog.insert_revision(original.clone()).is_err());
    let saved = catalog.to_json().unwrap();
    assert!(saved.contains("dustroute.blueprint-catalog.v3"));
    assert!(BlueprintCatalog::from_json(&saved.replace("catalog.v3", "catalog.v2")).is_err());
    let loaded = BlueprintCatalog::from_json(&saved).unwrap();
    assert_eq!(loaded.revision(&original.id), Some(&original));
    assert_eq!(loaded.revision(&changed.id), Some(&changed));
    for (law, expected) in [(before, 0), (changed.law.unwrap().compile().unwrap(), 1)] {
        let s = law
            .event(
                &law.initial_state(),
                "neighbor_update",
                &BTreeMap::from([("powered".into(), 1)]),
            )
            .unwrap();
        let s = law.advance(&law.advance(&s).unwrap()).unwrap();
        assert_eq!(s.register("lit"), Some(expected));
    }
}

#[test]
fn invalid_programs_and_failed_execution_cannot_mutate_the_catalog_or_state() {
    let mut catalog = builtin_laws().clone();
    let saved = catalog.to_json().unwrap();
    let mut bad = torch_law_revision().clone();
    bad.id = BlueprintRevisionId::new("bad-law.v1").unwrap();
    bad.law.as_mut().unwrap().handlers.insert(
        "bad".into(),
        vec![Instruction::ScheduleIfAbsent {
            event: "missing".into(),
            after: 0,
        }],
    );
    assert!(catalog.insert_revision(bad).is_err());
    assert_eq!(catalog.to_json().unwrap(), saved);

    let mut program = torch_law_revision().law.clone().unwrap();
    program.handlers.insert(
        "bad".into(),
        vec![
            Instruction::Set {
                register: "lit".into(),
                value: Expr::Constant { value: 0 },
            },
            Instruction::Set {
                register: "lit".into(),
                value: Expr::Constant { value: 2 },
            },
        ],
    );
    let law = program.compile().unwrap();
    let initial = law.initial_state();
    assert!(
        law.event(&initial, "bad", &BTreeMap::from([("powered".into(), 0)]))
            .is_err()
    );
    assert_eq!(initial.register("lit"), Some(1));
    assert!(
        law.event(&initial, "neighbor_update", &BTreeMap::new())
            .is_err()
    );
    assert!(
        law.event(
            &initial,
            "neighbor_update",
            &BTreeMap::from([("powered".into(), 2)])
        )
        .is_err()
    );

    let mut foreign = program.clone();
    foreign.histories.clear();
    foreign.handlers = BTreeMap::from([("nothing".into(), vec![])]);
    let state = foreign.compile().unwrap().initial_state();
    assert!(law.advance(&state).is_err());

    let mut deep = Expr::Constant { value: 0 };
    for _ in 0..34 {
        deep = Expr::Not {
            value: Box::new(deep),
        };
    }
    program.handlers.insert(
        "deep".into(),
        vec![Instruction::Set {
            register: "lit".into(),
            value: deep,
        }],
    );
    assert!(program.compile().is_err());
}

#[test]
fn law_requirements_are_immutable_dependencies_and_cannot_be_silently_downgraded() {
    let mut catalog = builtin_laws().clone();
    let mut source = torch_law_revision().clone();
    source.id = BlueprintRevisionId::new("test.declaration.v1").unwrap();
    source.law = None;
    source.required_laws = vec![torch_law_revision().id.clone()];
    catalog.insert_revision(source.clone()).unwrap();
    let saved = catalog.to_json().unwrap();
    assert!(saved.contains("dustroute.blueprint-catalog.v9"));
    assert_eq!(
        BlueprintCatalog::from_json(&saved)
            .unwrap()
            .revision(&source.id),
        Some(&source)
    );
    for version in 1..9 {
        assert!(
            BlueprintCatalog::from_json(
                &saved.replace("catalog.v9", &format!("catalog.v{version}"))
            )
            .is_err()
        );
    }
    for requirements in [
        vec![BlueprintRevisionId::new("missing.v1").unwrap()],
        vec![source.id.clone()],
        vec![torch_law_revision().id.clone(); 2],
    ] {
        let mut invalid = source.clone();
        invalid.id = BlueprintRevisionId::new("test.invalid.v1").unwrap();
        invalid.required_laws = requirements;
        assert!(catalog.insert_revision(invalid).is_err());
        assert_eq!(catalog.to_json().unwrap(), saved);
    }
}

#[test]
fn mutually_required_laws_are_declarations_not_recursive_inclusions() {
    let mut catalog = builtin_laws().clone();
    let mut left = torch_law_revision().clone();
    let mut right = left.clone();
    left.id = BlueprintRevisionId::new("test.left.v1").unwrap();
    right.id = BlueprintRevisionId::new("test.right.v1").unwrap();
    left.required_laws = vec![right.id.clone()];
    right.required_laws = vec![left.id.clone()];
    catalog
        .insert_revisions(vec![left.clone(), right.clone()])
        .unwrap();
    assert_eq!(catalog.expand(&left.id).unwrap().occurrences.len(), 1);
    let saved = catalog.to_json().unwrap();
    assert_eq!(
        BlueprintCatalog::from_json(&saved)
            .unwrap()
            .to_json()
            .unwrap(),
        saved
    );
}

#[test]
fn lamp_law_revisions_preserve_both_models_and_executable_archive_data() {
    use dustroute_minecraft::lamp_law::{
        CompatibilityLampLaw, LAMP_LAW_IDS, LampDeadline, builtin_programs,
    };
    let mut catalog = builtin_laws().clone();
    for (id, program) in LAMP_LAW_IDS.into_iter().zip(builtin_programs()) {
        let source = catalog
            .revision(&BlueprintRevisionId::new(id).unwrap())
            .unwrap();
        assert_eq!(source.law.as_ref(), Some(program));
    }
    let old = catalog
        .revision(&BlueprintRevisionId::new(LAMP_LAW_IDS[0]).unwrap())
        .unwrap()
        .clone();
    let mut changed = old.clone();
    changed
        .law
        .as_mut()
        .unwrap()
        .handlers
        .get_mut("evaluate")
        .unwrap()
        .push(Instruction::Set {
            register: "after".into(),
            value: Expr::Constant { value: 5 },
        });
    assert!(catalog.insert_revision(changed.clone()).is_err());
    changed.id = BlueprintRevisionId::new("test.lamp.five-boundaries.v1").unwrap();
    changed.parents = vec![old.id.clone()];
    catalog.insert_revision(changed.clone()).unwrap();
    let restored = BlueprintCatalog::from_json(&catalog.to_json().unwrap()).unwrap();
    assert_eq!(restored.revision(&old.id), Some(&old));
    assert_eq!(restored.revision(&changed.id), Some(&changed));
    for (source, delay) in [(&old, 2), (&changed, 5)] {
        let program = restored.revision(&source.id).unwrap().law.as_ref().unwrap();
        let law = CompatibilityLampLaw::compile(program).unwrap();
        assert_eq!(
            law.boundary(false, true, None, false).deadline,
            LampDeadline::SetAfter(delay)
        );
    }
}

#[test]
fn piston_laws_round_trip_as_executable_immutable_revisions_without_rebinding() {
    use dustroute_minecraft::PistonAction;
    use dustroute_minecraft::piston_law::{PISTON_LAW_IDS, PistonLaws, builtin_programs};
    let mut catalog = builtin_laws().clone();
    let mut ids = PISTON_LAW_IDS.map(|id| BlueprintRevisionId::new(id).unwrap());
    for (id, program) in ids.iter().zip(builtin_programs()) {
        assert_eq!(catalog.revision(id).unwrap().law.as_ref(), Some(program));
    }
    let original = catalog.revision(&ids[1]).unwrap().clone();
    let mut changed = original.clone();
    changed
        .law
        .as_mut()
        .unwrap()
        .handlers
        .get_mut("evaluate")
        .unwrap()
        .extend([
            Instruction::Set {
                register: "head".into(),
                value: Expr::Constant { value: 2 },
            },
            Instruction::Set {
                register: "completion".into(),
                value: Expr::Constant { value: 5 },
            },
        ]);
    assert!(catalog.insert_revision(changed.clone()).is_err());
    changed.id = BlueprintRevisionId::new("test.piston.changed-motion.v1").unwrap();
    changed.parents = vec![original.id.clone()];
    ids[1] = changed.id.clone();
    catalog.insert_revision(changed.clone()).unwrap();
    let loaded = BlueprintCatalog::from_json(&catalog.to_json().unwrap()).unwrap();
    assert_eq!(loaded.revision(&original.id), Some(&original));
    assert_eq!(loaded.revision(&changed.id), Some(&changed));
    let programs = ids.map(|id| loaded.revision(&id).unwrap().law.clone().unwrap());
    let laws = PistonLaws::compile(&programs).unwrap();
    assert_eq!(laws.motion_effects(PistonAction::Extend).head, 2);
    assert_eq!(laws.default_motion_profile().movement_game_ticks, 5);
    let old = PistonLaws::compile(builtin_programs()).unwrap();
    assert_eq!(old.motion_effects(PistonAction::Extend).head, 1);
    assert_eq!(old.default_motion_profile().movement_game_ticks, 2);
}
