use std::collections::BTreeMap;

use dustroute_library::PortDirection;
use dustroute_library::behavior_type::{BooleanRow, RepeatedSettling};
use dustroute_library::blueprint::*;
use dustroute_library::builtin_laws::torch_law_revision;
use dustroute_library::location_observation::{LocationPredicate, ObservedPort};
use dustroute_minecraft::time::piston_runtime::{
    ElectricalPistonRuntime, PistonEvent, new_piston_runtime, schedule_electrical_input,
};
use dustroute_minecraft::time::runtime::{HalfProgress, RuntimeError, RuntimeLimits};
use dustroute_minecraft::{BlockKind, Facing, PistonState, PistonVariant, Pos, Region, World};

const INPUT: Pos = Pos::new(-1, 1, 0);
const BODY: Pos = Pos::new(0, 1, 0);
const OUT: Pos = Pos::new(2, 1, 0);

fn runtime() -> ElectricalPistonRuntime {
    let mut world = World::new();
    world.place(BlockKind::Solid, Pos::new(-1, 0, 0));
    let lever = world.place(BlockKind::Lever, INPUT);
    lever.support_offset = Some(Pos::new(0, -1, 0));
    lever.powered = Some(false);
    let body = world.place(BlockKind::Piston, BODY);
    body.facing = Some(Facing::East);
    body.piston_variant = Some(PistonVariant::Sticky);
    body.piston_state = Some(PistonState::Retracted);
    world.place(BlockKind::Solid, Pos::new(1, 1, 0));
    new_piston_runtime(
        world,
        Region::new(Pos::new(-4, -2, -4), Pos::new(5, 4, 4)),
        RuntimeLimits::default(),
    )
    .unwrap()
}

fn revision() -> BlueprintRevision {
    let mut revision = torch_law_revision().clone();
    revision.id = BlueprintRevisionId::new("test.location-source.v1").unwrap();
    revision.name = "An interpretation of fixed locations".into();
    revision.law = None;
    revision.ports = [
        ("input", INPUT, PortDirection::Input),
        ("output", OUT, PortDirection::Output),
    ]
    .into_iter()
    .map(|(name, position, direction)| BlueprintPort {
        name: name.into(),
        position,
        direction,
        kind: BlueprintPortKind::BlockState,
        facing: None,
        required_source_types: vec![],
    })
    .collect();
    revision.behavior_bindings = vec![BehaviorBinding::Observed {
        behavior_type: definition().id,
        observed_inputs: BTreeMap::from([(
            "in".into(),
            ObservedPort::Location {
                port: "input".into(),
                predicate: LocationPredicate::Powered {
                    block_kind: BlockKind::Lever,
                    powered: true,
                },
            },
        )]),
        observed_outputs: BTreeMap::from([(
            "out".into(),
            ObservedPort::Location {
                port: "output".into(),
                predicate: LocationPredicate::Present,
            },
        )]),
    }];
    revision
}

fn definition() -> TypeRevision {
    TypeRevision {
        id: TypeRevisionId::new("test.location-relation.v1").unwrap(),
        name: "Identity, independent of placement".into(),
        contract: TypeContract::RepeatedSettling {
            relation: RepeatedSettling {
                inputs: vec!["in".into()],
                outputs: vec!["out".into()],
                rows: [false, true]
                    .into_iter()
                    .map(|level| BooleanRow {
                        inputs: vec![level],
                        outputs: vec![level],
                    })
                    .collect(),
            },
        },
    }
}

fn catalog() -> BlueprintCatalog {
    let mut catalog = BlueprintCatalog::default();
    catalog.insert_type(definition()).unwrap();
    catalog.insert_revision(revision()).unwrap();
    catalog
}

#[test]
fn fixed_location_retains_air_carrier_payload_and_runtime_history() {
    let mut run = runtime();
    let air = run.view().observe_location(OUT).unwrap();
    assert!(!LocationPredicate::Present.evaluate(&air).unwrap());
    assert!(
        LocationPredicate::BlockKind {
            block_kind: BlockKind::Air
        }
        .evaluate(&air)
        .unwrap()
    );
    schedule_electrical_input(&mut run, 1, INPUT, true).unwrap();
    while run.view().block(OUT).unwrap().kind != BlockKind::MovingPiston {
        assert!(run.microstep().unwrap().is_some());
    }
    // The staged write is observable before its entity is registered. This is
    // already a moving block, but must not invent payload/progress evidence.
    let staged = run.view().observe_location(OUT).unwrap();
    assert!(staged.block().piston_entity.is_none());
    assert!(staged.carrier().is_none());
    while run.view().carrier(OUT).is_none() {
        assert!(run.microstep().unwrap().is_some());
    }
    let start = run.view().observe_location(OUT).unwrap();
    assert_eq!(start.position(), OUT);
    assert!(LocationPredicate::Present.evaluate(&start).unwrap());
    assert!(
        !LocationPredicate::BlockKind {
            block_kind: BlockKind::Solid
        }
        .evaluate(&start)
        .unwrap()
    );
    assert_eq!(
        start
            .block()
            .piston_entity
            .as_ref()
            .unwrap()
            .pushed_block
            .kind,
        BlockKind::Solid
    );
    assert!(
        LocationPredicate::MotionProgress {
            progress: HalfProgress::Zero
        }
        .evaluate(&start)
        .unwrap()
    );
    while run.view().carrier(OUT).unwrap().history.progress != HalfProgress::Half {
        assert!(run.microstep().unwrap().is_some());
    }
    let half = run.view().observe_location(OUT).unwrap();
    assert_eq!(
        start.block(),
        half.block(),
        "the old serialized progress marker is unchanged"
    );
    assert_ne!(start.carrier(), half.carrier());
    assert!(
        LocationPredicate::MotionProgress {
            progress: HalfProgress::Half
        }
        .evaluate(&half)
        .unwrap()
    );
    run.run_until_idle().unwrap();
    let solid = run.view().observe_location(OUT).unwrap();
    assert_eq!(solid.position(), OUT);
    assert!(solid.carrier().is_none());
    assert!(
        LocationPredicate::BlockKind {
            block_kind: BlockKind::Solid
        }
        .evaluate(&solid)
        .unwrap()
    );
    assert!(
        !LocationPredicate::MotionProgress {
            progress: HalfProgress::Full
        }
        .evaluate(&solid)
        .unwrap()
    );
}

#[test]
fn a_retracting_body_is_observed_as_its_actual_carrier() {
    let mut run = runtime();
    schedule_electrical_input(&mut run, 1, INPUT, true).unwrap();
    schedule_electrical_input(&mut run, 8, INPUT, false).unwrap();
    while !run
        .trace()
        .iter()
        .any(|r| matches!(r.invocation.call.payload, PistonEvent::RetractBody { .. }))
    {
        run.microstep().unwrap().unwrap();
    }
    // RetractBody now begins a staged continuation; its event boundary is not
    // the subsequent moving-entity registration boundary.
    while run.view().carrier(BODY).is_none() {
        assert!(run.microstep().unwrap().is_some());
    }
    let sample = run.view().observe_location(BODY).unwrap();
    assert!(LocationPredicate::Present.evaluate(&sample).unwrap());
    assert!(
        !LocationPredicate::PistonState {
            state: PistonState::Retracted
        }
        .evaluate(&sample)
        .unwrap()
    );
    assert_eq!(
        sample
            .block()
            .piston_entity
            .as_ref()
            .unwrap()
            .pushed_block
            .piston_state,
        Some(PistonState::Retracted)
    );
    run.run_until_idle().unwrap();
    assert!(
        LocationPredicate::PistonState {
            state: PistonState::Retracted
        }
        .evaluate(&run.view().observe_location(BODY).unwrap())
        .unwrap()
    );
}

#[test]
fn unknown_coordinates_and_missing_properties_are_not_false_or_air() {
    let run = runtime();
    assert!(matches!(
        run.view().observe_location(Pos::new(100, 1, 0)),
        Err(RuntimeError::UnknownSpace(_))
    ));
    let predicate = LocationPredicate::Powered {
        block_kind: BlockKind::Lever,
        powered: true,
    };
    assert!(
        !predicate
            .evaluate(&run.view().observe_location(INPUT).unwrap())
            .unwrap()
    );
    let mut world = run.view().world().clone();
    world.get_mut(INPUT).unwrap().powered = None;
    // The electrical runtime rejects incomplete device evidence at admission.
    let error = new_piston_runtime(world, run.view().known_region(), RuntimeLimits::default())
        .err()
        .unwrap();
    assert!(
        matches!(error, RuntimeError::Handler(ref message) if message.contains("explicit device power")),
        "{error}"
    );
    assert!(
        LocationPredicate::Powered {
            block_kind: BlockKind::Solid,
            powered: false
        }
        .validate()
        .is_err()
    );
}

#[test]
fn explicit_bindings_roundtrip_in_a_new_archive_without_rewriting_old_types() {
    let catalog = catalog();
    let saved = catalog.to_json().unwrap();
    assert!(saved.contains("dustroute.blueprint-catalog.v13"));
    let loaded = BlueprintCatalog::from_json(&saved).unwrap();
    assert_eq!(
        loaded.revision(&revision().id),
        catalog.revision(&revision().id)
    );
    assert_eq!(loaded.type_revision(&definition().id), Some(&definition()));
    for version in 1..=9 {
        assert!(
            BlueprintCatalog::from_json(
                &saved.replace("catalog.v13", &format!("catalog.v{version}"))
            )
            .is_err()
        );
    }
    let before = catalog.to_json().unwrap();
    let mut run = runtime();
    schedule_electrical_input(&mut run, 1, INPUT, true).unwrap();
    schedule_electrical_input(&mut run, 2, INPUT, false).unwrap();
    run.run_until_idle().unwrap();
    assert_eq!(catalog.to_json().unwrap(), before);
    assert_eq!(
        catalog.revision(&revision().id).unwrap().ports[1].position,
        OUT
    );
}

#[test]
fn mappings_must_cover_the_type_and_preserve_terminal_kinds_and_directions() {
    for change in 0..6 {
        let mut catalog = BlueprintCatalog::default();
        catalog.insert_type(definition()).unwrap();
        let mut source = revision();
        let BehaviorBinding::Observed {
            observed_inputs,
            observed_outputs,
            ..
        } = &mut source.behavior_bindings[0]
        else {
            unreachable!()
        };
        match change {
            0 => {
                observed_outputs.clear();
            }
            1 => {
                observed_outputs.insert(
                    "extra".into(),
                    ObservedPort::Location {
                        port: "output".into(),
                        predicate: LocationPredicate::Present,
                    },
                );
            }
            2 => {
                observed_inputs.insert(
                    "in".into(),
                    ObservedPort::Location {
                        port: "output".into(),
                        predicate: LocationPredicate::Present,
                    },
                );
            }
            3 => {
                source.ports[1].kind = BlueprintPortKind::Wire;
            }
            4 => {
                observed_outputs.insert(
                    "out".into(),
                    ObservedPort::Signal {
                        port: "output".into(),
                    },
                );
            }
            _ => {
                observed_outputs.insert(
                    "out".into(),
                    ObservedPort::Location {
                        port: "missing".into(),
                        predicate: LocationPredicate::Present,
                    },
                );
            }
        }
        assert!(catalog.insert_revision(source).is_err(), "case {change}");
    }
}

#[test]
fn legacy_binding_forms_still_reject_block_state_terminals() {
    let mut catalog = BlueprintCatalog::default();
    catalog.insert_type(definition()).unwrap();
    let mut source = revision();
    source.behavior_bindings = vec![BehaviorBinding::RepeatedSettling {
        behavior_type: definition().id,
        inputs: BTreeMap::from([("in".into(), "input".into())]),
        outputs: BTreeMap::from([("out".into(), "output".into())]),
    }];
    assert!(catalog.insert_revision(source).is_err());
}
