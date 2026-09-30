use std::collections::BTreeMap;

use dustroute_library::PortDirection;
use dustroute_library::assembly::Assembly;
use dustroute_library::behavior_type::{BooleanRow, RepeatedSettling};
use dustroute_library::blueprint::*;
use dustroute_library::builtin_laws::torch_law_revision;
use dustroute_library::location_observation::{LocationPredicate, ObservedPort};
use dustroute_minecraft::time::piston_runtime::{new_piston_runtime, schedule_electrical_input};
use dustroute_minecraft::time::runtime::RuntimeLimits;
use dustroute_minecraft::{
    BlockKind, Facing, PistonState, PistonVariant, Pos, Region, RotationY, World,
};
use dustroute_translate::location_behavior::LocationBehaviorBinding;

fn fixture(
    rotation: RotationY,
) -> (
    BlueprintCatalog,
    Assembly,
    InstancePath,
    BehaviorBinding,
    Region,
) {
    let mut local = World::new();
    local.place(BlockKind::Solid, Pos::new(-1, 0, 0));
    let input = local.place(BlockKind::Lever, Pos::new(-1, 1, 0));
    input.support_offset = Some(Pos::new(0, -1, 0));
    input.powered = Some(false);
    let piston = local.place(BlockKind::Piston, Pos::new(0, 1, 0));
    piston.facing = Some(Facing::East);
    piston.piston_state = Some(PistonState::Retracted);
    piston.piston_variant = Some(PistonVariant::Sticky);
    local.place(BlockKind::Solid, Pos::new(1, 1, 0));
    let definition = TypeRevision {
        id: TypeRevisionId::new("test.location.behavior.v1").unwrap(),
        name: "Boolean observations".into(),
        contract: TypeContract::RepeatedSettling {
            relation: RepeatedSettling {
                inputs: vec!["control".into()],
                outputs: vec!["occupied".into(), "solid".into()],
                rows: [false, true]
                    .into_iter()
                    .map(|b| BooleanRow {
                        inputs: vec![b],
                        outputs: vec![b, b],
                    })
                    .collect(),
            },
        },
    };
    let binding = BehaviorBinding::Observed {
        behavior_type: definition.id.clone(),
        observed_inputs: BTreeMap::from([(
            "control".into(),
            ObservedPort::Location {
                port: "input".into(),
                predicate: LocationPredicate::Powered {
                    block_kind: BlockKind::Lever,
                    powered: true,
                },
            },
        )]),
        observed_outputs: BTreeMap::from([
            (
                "occupied".into(),
                ObservedPort::Location {
                    port: "output".into(),
                    predicate: LocationPredicate::Present,
                },
            ),
            (
                "solid".into(),
                ObservedPort::Location {
                    port: "output".into(),
                    predicate: LocationPredicate::BlockKind {
                        block_kind: BlockKind::Solid,
                    },
                },
            ),
        ]),
    };
    let mut source = torch_law_revision().clone();
    source.id = BlueprintRevisionId::new("test.location.body.v1").unwrap();
    source.law = None;
    source.blocks = local
        .iter()
        .map(|(p, b)| PositionedBlock {
            position: *p,
            block: b.clone(),
        })
        .collect();
    source.ports = [
        ("input", Pos::new(-1, 1, 0), PortDirection::Input),
        ("output", Pos::new(2, 1, 0), PortDirection::Output),
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
    source.behavior_bindings = vec![binding.clone()];
    let instance = InstanceId::new("mechanism").unwrap();
    let origin = Pos::new(7, 2, -4);
    let region = Region::new(Pos::new(-20, -1, -20), Pos::new(20, 10, 20));
    let assembly = Assembly {
        name: "Concrete rotated placement".into(),
        instances: vec![BlueprintInclusion {
            instance: instance.clone(),
            revision: source.id.clone(),
            origin,
            rotation,
        }],
        blocks: local
            .iter()
            .map(|(p, b)| {
                let p = rotation.pos(*p);
                PositionedBlock {
                    position: p.offset(origin.x, origin.y, origin.z),
                    block: rotation.block(b),
                }
            })
            .collect(),
        known_regions: vec![region],
        connections: vec![],
        boundaries: vec![],
    };
    let mut catalog = BlueprintCatalog::default();
    catalog.insert_type(definition).unwrap();
    catalog.insert_revision(source).unwrap();
    (catalog, assembly, vec![instance], binding, region)
}

#[test]
fn bound_coordinates_stay_fixed_while_multiple_named_observations_see_motion() {
    for rotation in [
        RotationY::R0,
        RotationY::R90,
        RotationY::R180,
        RotationY::R270,
    ] {
        let (catalog, assembly, path, binding, region) = fixture(rotation);
        let original = assembly.clone();
        let saved = catalog.to_json().unwrap();
        let view = assembly.inspect(&catalog).unwrap();
        let input = view
            .resolved_port(&BlueprintPortRef {
                instance: path.clone(),
                port: "input".into(),
            })
            .unwrap()
            .0
            .position;
        let output = view
            .resolved_port(&BlueprintPortRef {
                instance: path.clone(),
                port: "output".into(),
            })
            .unwrap()
            .0
            .position;
        let bound = LocationBehaviorBinding::resolve(&catalog, &assembly, &path, &binding).unwrap();
        let mut run =
            new_piston_runtime(view.proposed_world(), region, RuntimeLimits::default()).unwrap();
        let sample = bound.sample(&run).unwrap();
        assert!(!sample.inputs["control"].value);
        assert!(!sample.outputs["occupied"].value);
        schedule_electrical_input(&mut run, 1, input, true).unwrap();
        while run.view().block(output).unwrap().kind != BlockKind::MovingPiston {
            run.microstep().unwrap().unwrap();
        }
        let sample = bound.sample(&run).unwrap();
        assert!(sample.inputs["control"].value);
        assert!(sample.outputs["occupied"].value);
        assert!(!sample.outputs["solid"].value);
        assert_eq!(sample.outputs["occupied"].state.position(), output);
        assert!(sample.outputs["occupied"].state.carrier().is_none());
        // Moving occupancy is visible during the staged write, before the
        // carrier entity exists. Sample again at its registration boundary.
        while run.view().carrier(output).is_none() {
            run.microstep().unwrap().unwrap();
        }
        let sample = bound.sample(&run).unwrap();
        assert!(sample.outputs["occupied"].value);
        assert!(!sample.outputs["solid"].value);
        assert_eq!(sample.outputs["occupied"].state.position(), output);
        assert!(sample.outputs["occupied"].state.carrier().is_some());
        run.run_until_idle().unwrap();
        let sample = bound.sample(&run).unwrap();
        assert!(sample.outputs["solid"].value);
        assert_eq!(sample.outputs["solid"].state.position(), output);
        let tick = run.view().time().game_tick + 1;
        schedule_electrical_input(&mut run, tick, input, false).unwrap();
        run.run_until_idle().unwrap();
        let sample = bound.sample(&run).unwrap();
        assert!(!sample.outputs["occupied"].value);
        assert!(!sample.outputs["solid"].value);
        assert_eq!(sample.outputs["solid"].state.position(), output);
        assert_eq!(catalog.to_json().unwrap(), saved);
        assert_eq!(assembly, original);
    }
}

#[test]
fn sampling_rejects_an_undeclared_reinterpretation_of_the_same_location() {
    let (catalog, assembly, path, mut binding, _) = fixture(RotationY::R0);
    let BehaviorBinding::Observed {
        observed_outputs, ..
    } = &mut binding
    else {
        unreachable!()
    };
    observed_outputs.insert(
        "occupied".into(),
        ObservedPort::Location {
            port: "output".into(),
            predicate: LocationPredicate::BlockKind {
                block_kind: BlockKind::Air,
            },
        },
    );
    assert!(
        LocationBehaviorBinding::resolve(&catalog, &assembly, &path, &binding)
            .unwrap_err()
            .contains("does not declare")
    );
}

fn runtime_context(
    assembly: &Assembly,
    region: Region,
) -> dustroute_library::runtime_behavior::RuntimeBehaviorContext {
    dustroute_library::runtime_behavior::RuntimeBehaviorContext {
        profile:dustroute_library::runtime_behavior::RuntimeBehaviorProfile::UnifiedPistonElectricalRootExplorationV19,
        initial_condition:dustroute_library::behavior_type::BehaviorInitialCondition::FreshConstruction,
        known_region:region,
        input_levers:assembly.blocks.iter().filter(|r|r.block.kind==BlockKind::Lever).map(|r|r.position).collect(),
        root_limits:RuntimeLimits::default(),
    }
}

#[test]
fn reachable_motion_interruption_refutes_the_location_relation() {
    use dustroute_translate::behavior_type::{BehaviorBudget, BehaviorModel, WitnessAction};
    use dustroute_translate::promotion::CheckStatus;
    use dustroute_translate::runtime_behavior::RuntimeBehaviorModel;
    let (catalog, assembly, path, binding, region) = fixture(RotationY::R0);
    let context = runtime_context(&assembly, region);
    let model =
        RuntimeBehaviorModel::from_fresh_assembly(&catalog, &assembly, &path, &binding, &context)
            .unwrap();
    let report = model.verify(BehaviorBudget::default());
    assert_eq!(report.status, CheckStatus::Failed, "{report:?}");
    let witness = report.counterexample.unwrap();
    let mut state = model.initial_state().unwrap();
    for action in witness.prefix {
        state = match action {
            WitnessAction::SetInputs { inputs } => model.with_inputs(&state, &inputs).unwrap(),
            WitnessAction::Advance => model.step(&state).unwrap(),
        };
    }
    for _ in 0..30 {
        state = model.with_inputs(&state, &witness.held_inputs).unwrap();
        state = model.step(&state).unwrap();
    }
    assert_ne!(model.outputs(&state).unwrap(), witness.expected_outputs);
}

#[test]
fn vertical_runtime_context_executes_a_blueprint_observation_binding() {
    use dustroute_translate::behavior_type::BehaviorModel;
    use dustroute_translate::runtime_behavior::RuntimeBehaviorModel;

    let (base_catalog, _, _, binding, _) = fixture(RotationY::R0);
    let mut source = base_catalog
        .revision(&BlueprintRevisionId::new("test.location.body.v1").unwrap())
        .unwrap()
        .clone();
    source.id = BlueprintRevisionId::new("test.location.vertical-body.v1").unwrap();
    let mut world = World::new();
    world.place(BlockKind::Solid, Pos::new(-1, 2, 0));
    let lever = world.place(BlockKind::Lever, Pos::new(-1, 3, 0));
    lever.support_offset = Some(Pos::new(0, -1, 0));
    lever.powered = Some(false);
    let piston = world.place(BlockKind::Piston, Pos::new(0, 3, 0));
    piston.facing = Some(Facing::Up);
    piston.piston_state = Some(PistonState::Retracted);
    piston.piston_variant = Some(PistonVariant::Sticky);
    world.place(BlockKind::Solid, Pos::new(0, 4, 0));
    source.blocks = world
        .iter()
        .map(|(position, block)| PositionedBlock {
            position: *position,
            block: block.clone(),
        })
        .collect();
    source.ports[0].position = Pos::new(-1, 3, 0);
    source.ports[1].position = Pos::new(0, 5, 0);
    let instance = InstanceId::new("vertical").unwrap();
    let region = Region::new(Pos::new(-3, -1, -3), Pos::new(3, 8, 3));
    let assembly = Assembly {
        name: "Vertical piston observation".into(),
        instances: vec![BlueprintInclusion {
            instance: instance.clone(),
            revision: source.id.clone(),
            origin: Pos::default(),
            rotation: RotationY::R0,
        }],
        blocks: source.blocks.clone(),
        known_regions: vec![region],
        connections: vec![],
        boundaries: vec![],
    };
    let definition = base_catalog
        .type_revision(binding.behavior_type())
        .unwrap()
        .clone();
    let mut catalog = BlueprintCatalog::default();
    catalog.insert_type(definition).unwrap();
    catalog.insert_revision(source).unwrap();
    let context = dustroute_library::runtime_behavior::RuntimeBehaviorContext {
        profile: dustroute_library::runtime_behavior::RuntimeBehaviorProfile::UnifiedPistonElectricalRootExplorationV19,
        initial_condition: dustroute_library::behavior_type::BehaviorInitialCondition::FreshConstruction,
        known_region: region,
        input_levers: vec![Pos::new(-1, 3, 0)],
        root_limits: RuntimeLimits::default(),
    };
    let model = RuntimeBehaviorModel::from_fresh_assembly(
        &catalog,
        &assembly,
        &vec![instance],
        &binding,
        &context,
    )
    .unwrap();
    let mut state = model.initial_state().unwrap();
    state = model.with_inputs(&state, &[true]).unwrap();
    for _ in 0..30 {
        state = model.step(&state).unwrap();
    }
    assert_eq!(model.outputs(&state).unwrap(), vec![true, true]);
}

fn body_fixture(
    rotation: RotationY,
) -> (
    BlueprintCatalog,
    Assembly,
    InstancePath,
    BehaviorBinding,
    Region,
) {
    let (mut catalog, assembly, path, _, region) = fixture(rotation);
    let mut source = catalog
        .revision(&assembly.instances[0].revision)
        .unwrap()
        .clone();
    source.id = BlueprintRevisionId::new("test.body-state.v1").unwrap();
    source
        .ports
        .iter_mut()
        .find(|p| p.name == "output")
        .unwrap()
        .position = Pos::new(0, 1, 0);
    let BehaviorBinding::Observed {
        observed_outputs, ..
    } = &mut source.behavior_bindings[0]
    else {
        unreachable!()
    };
    for observation in observed_outputs.values_mut() {
        *observation = ObservedPort::Location {
            port: "output".into(),
            predicate: LocationPredicate::PistonState {
                state: PistonState::Extended,
            },
        };
    }
    let binding = source.behavior_bindings[0].clone();
    let mut assembly = assembly;
    assembly.instances[0].revision = source.id.clone();
    catalog.insert_revision(source).unwrap();
    (catalog, assembly, path, binding, region)
}

#[test]
fn model_closes_a_body_state_relation_and_keeps_computation_failure_unknown() {
    use dustroute_translate::behavior_type::BehaviorBudget;
    use dustroute_translate::promotion::CheckStatus;
    use dustroute_translate::runtime_behavior::RuntimeBehaviorModel;
    let (catalog, assembly, path, binding, region) = body_fixture(RotationY::R0);
    let context = runtime_context(&assembly, region);
    let model =
        RuntimeBehaviorModel::from_fresh_assembly(&catalog, &assembly, &path, &binding, &context)
            .unwrap();
    let report = model.verify(BehaviorBudget::default());
    assert_eq!(report.status, CheckStatus::Passed, "{report:?}");
    assert!(report.reachable_states > 10);
    let report = model.verify(BehaviorBudget {
        max_states: 1,
        ..BehaviorBudget::default()
    });
    assert_eq!(report.status, CheckStatus::Undetermined);
}

#[test]
fn whole_review_keeps_parent_pass_and_child_motion_failure_separate() {
    use dustroute_translate::behavior_type::BehaviorBudget;
    use dustroute_translate::promotion::{CheckKind, CheckStatus};
    use dustroute_translate::runtime_review::review_assembly_in_runtime_context;
    for (rotation, air) in [(RotationY::R0, false), (RotationY::R90, true)] {
        let (mut catalog, mut assembly, path, _, region) = body_fixture(rotation);
        let mut parent = catalog
            .revision(&assembly.instances[0].revision)
            .unwrap()
            .clone();
        let mut child = parent.clone();
        child.id = BlueprintRevisionId::new("test.child.v1").unwrap();
        child.behavior_bindings.clear();
        child.ports.retain(|p| p.name == "output");
        let identity = TypeRevision {
            id: TypeRevisionId::new("test.piston-identity.v1").unwrap(),
            name: "Actual piston identity".into(),
            contract: TypeContract::BlockKind {
                block_kind: BlockKind::Piston,
            },
        };
        if air {
            child.blocks = vec![PositionedBlock {
                position: Pos::new(2, 1, 0),
                block: dustroute_minecraft::Block::new(BlockKind::Air),
            }];
            child.ports[0].position = Pos::new(2, 1, 0);
        } else {
            child.blocks.retain(|r| r.block.kind == BlockKind::Piston);
            child.static_type_bindings = vec![StaticTypeBinding {
                port: "output".into(),
                type_revision: identity.id.clone(),
            }];
        }
        catalog.insert_type(identity).unwrap();
        parent.id = BlueprintRevisionId::new("test.parent-with-child.v1").unwrap();
        let child_id = InstanceId::new("retained").unwrap();
        parent.inclusions.push(BlueprintInclusion {
            instance: child_id.clone(),
            revision: child.id.clone(),
            origin: Pos::default(),
            rotation: RotationY::R0,
        });
        catalog.insert_revision(child).unwrap();
        assembly.instances[0].revision = parent.id.clone();
        catalog.insert_revision(parent).unwrap();
        let before = catalog.to_json().unwrap();
        let original = assembly.clone();
        let report = review_assembly_in_runtime_context(
            &catalog,
            &assembly,
            &runtime_context(&assembly, region),
            BehaviorBudget::default(),
        )
        .unwrap();
        assert_eq!(report.status(), CheckStatus::Failed, "{report:?}");
        // The existing source-claim index also retains inherited explicit Air
        // on the parent. It must not be weakened for movement. A child's own
        // static type, by contrast, remains an independent child obligation.
        assert_eq!(
            report.occurrences[&path].status(),
            if air {
                CheckStatus::Failed
            } else {
                CheckStatus::Passed
            },
            "{report:?}"
        );
        let child_path = path.iter().cloned().chain([child_id]).collect::<Vec<_>>();
        assert_eq!(
            report.occurrences[&child_path].status(),
            CheckStatus::Failed
        );
        assert!(
            report.occurrences[&child_path]
                .checks
                .iter()
                .any(|c| c.status == CheckStatus::Failed
                    && c.kind
                        == if air {
                            CheckKind::Placement
                        } else {
                            CheckKind::StaticType
                        }
                    && c.detail.contains("runtime"))
        );
        assert!(
            report
                .behavior
                .iter()
                .all(|b| b.report.status == CheckStatus::Passed)
        );
        assert_eq!(catalog.to_json().unwrap(), before);
        assert_eq!(assembly, original);
    }
}

#[test]
fn native_review_can_pass_without_weakened_children_or_a_saved_certificate() {
    use dustroute_translate::behavior_type::BehaviorBudget;
    use dustroute_translate::promotion::CheckStatus;
    use dustroute_translate::runtime_review::review_assembly_in_runtime_context;
    let (catalog, assembly, _, _, region) = body_fixture(RotationY::R270);
    let context = runtime_context(&assembly, region);
    let report = review_assembly_in_runtime_context(
        &catalog,
        &assembly,
        &context,
        BehaviorBudget::default(),
    )
    .unwrap();
    assert_eq!(report.status(), CheckStatus::Passed, "{report:?}");
    let report = review_assembly_in_runtime_context(
        &catalog,
        &assembly,
        &context,
        BehaviorBudget {
            max_states: 1,
            ..BehaviorBudget::default()
        },
    )
    .unwrap();
    assert_eq!(report.status(), CheckStatus::Undetermined);
    let mut changed = assembly.clone();
    changed
        .blocks
        .iter_mut()
        .find(|b| b.block.kind == BlockKind::Piston)
        .unwrap()
        .block
        .piston_variant = Some(PistonVariant::Normal);
    // A new review executes this different realization afresh.
    let report =
        review_assembly_in_runtime_context(&catalog, &changed, &context, BehaviorBudget::default())
            .unwrap();
    assert_eq!(
        report.status(),
        CheckStatus::Passed,
        "body state does not require pulling: {report:?}"
    );
}

#[test]
fn partial_recorded_wire_states_cannot_start_behavior_exploration() {
    use dustroute_translate::runtime_behavior::RuntimeBehaviorModel;
    let case: serde_json::Value = serde_json::from_str(include_str!(
        "fixtures/piston-low-layer/07-single-input-two-row.json"
    ))
    .unwrap();
    let snapshot: dustroute_translate::snapshot::MinecraftSnapshot =
        serde_json::from_value(case["initial"].clone()).unwrap();
    let world = dustroute_translate::snapshot::world_from_snapshot(&snapshot).unwrap();
    let input: Pos = serde_json::from_value(case["input"].clone()).unwrap();
    let (mut catalog, mut assembly, path, _, _) = fixture(RotationY::R0);
    let mut source = catalog
        .revision(&assembly.instances[0].revision)
        .unwrap()
        .clone();
    source.id = BlueprintRevisionId::new("test.recorded.two-row.v1").unwrap();
    source.blocks = world
        .iter()
        .map(|(p, b)| PositionedBlock {
            position: *p,
            block: b.clone(),
        })
        .collect();
    source.ports[0].position = input;
    source.ports[1].position = Pos::new(2, 0, 0);
    let mut upper = source.ports[1].clone();
    upper.name = "upper".into();
    upper.position = Pos::new(2, 1, 0);
    source.ports.push(upper);
    let BehaviorBinding::Observed {
        observed_outputs, ..
    } = &mut source.behavior_bindings[0]
    else {
        unreachable!()
    };
    observed_outputs.insert(
        "solid".into(),
        ObservedPort::Location {
            port: "upper".into(),
            predicate: LocationPredicate::Present,
        },
    );
    let binding = source.behavior_bindings[0].clone();
    assembly.blocks = source.blocks.clone();
    assembly.instances[0].revision = source.id.clone();
    assembly.instances[0].origin = Pos::default();
    let region = Region::new(snapshot.min, snapshot.max);
    assembly.known_regions = vec![region];
    catalog.insert_revision(source).unwrap();
    let context = runtime_context(&assembly, region);
    // This historical capture records wire power but omits all four connection
    // arms. It cannot prove or refute behavior in the electrical model.
    let error =
        RuntimeBehaviorModel::from_fresh_assembly(&catalog, &assembly, &path, &binding, &context)
            .err()
            .expect("partial observations must not start an exploration");
    assert!(
        error.contains("complete four-arm wire state required"),
        "{error}"
    );
}

#[test]
fn known_pattern_failure_survives_unknown_space_and_an_unfinished_exploration() {
    use dustroute_translate::behavior_type::BehaviorBudget;
    use dustroute_translate::promotion::{CheckKind, CheckStatus};
    use dustroute_translate::runtime_review::review_assembly_in_runtime_context;
    let (mut catalog, mut assembly, path, _, region) = body_fixture(RotationY::R90);
    let mut source = catalog
        .revision(&assembly.instances[0].revision)
        .unwrap()
        .clone();
    source.id = BlueprintRevisionId::new("test.pattern-and-unknown.v1").unwrap();
    let expected = source
        .blocks
        .iter()
        .find(|b| b.block.kind == BlockKind::Lever)
        .unwrap()
        .block
        .clone();
    let requirement = TypeRevision {
        id: TypeRevisionId::new("test.exact-lever.v1").unwrap(),
        name: "Literal lever state".into(),
        contract: TypeContract::BlockPattern {
            blocks: vec![PositionedBlock {
                position: Pos::default(),
                block: expected,
            }],
        },
    };
    source.static_type_bindings.push(StaticTypeBinding {
        port: "input".into(),
        type_revision: requirement.id.clone(),
    });
    let mut unknown = source.ports[1].clone();
    unknown.name = "unknown".into();
    unknown.position = Pos::new(100, 0, 0);
    source.ports.push(unknown);
    assembly.instances[0].revision = source.id.clone();
    assembly
        .blocks
        .iter_mut()
        .find(|b| b.block.kind == BlockKind::Lever)
        .unwrap()
        .block
        .powered = Some(true);
    catalog.insert_type(requirement).unwrap();
    catalog.insert_revision(source).unwrap();
    let report = review_assembly_in_runtime_context(
        &catalog,
        &assembly,
        &runtime_context(&assembly, region),
        BehaviorBudget {
            max_states: 1,
            ..BehaviorBudget::default()
        },
    )
    .unwrap();
    assert_eq!(report.status(), CheckStatus::Failed);
    let checks = &report.occurrences[&path].checks;
    assert!(
        checks
            .iter()
            .any(|c| c.kind == CheckKind::StaticType && c.status == CheckStatus::Failed)
    );
    assert!(
        checks
            .iter()
            .any(|c| c.kind == CheckKind::Port && c.status == CheckStatus::Undetermined)
    );
    assert!(
        report
            .behavior
            .iter()
            .all(|b| b.report.status == CheckStatus::Undetermined)
    );
}

#[test]
fn context_cannot_crop_the_world_or_transfer_running_state_between_models() {
    use dustroute_translate::behavior_type::BehaviorModel;
    use dustroute_translate::runtime_behavior::RuntimeBehaviorModel;
    let (catalog, assembly, path, binding, region) = body_fixture(RotationY::R0);
    let context = runtime_context(&assembly, region);
    let saved = serde_json::to_string(&context).unwrap();
    assert_eq!(
        serde_json::from_str::<dustroute_library::runtime_behavior::RuntimeBehaviorContext>(&saved)
            .unwrap(),
        context
    );
    assert!(
        serde_json::from_str::<dustroute_library::behavior_type::PhysicalBehaviorContext>(&saved)
            .is_err()
    );
    let model =
        RuntimeBehaviorModel::from_fresh_assembly(&catalog, &assembly, &path, &binding, &context)
            .unwrap();
    let other =
        RuntimeBehaviorModel::from_fresh_assembly(&catalog, &assembly, &path, &binding, &context)
            .unwrap();
    assert!(model.step(&other.initial_state().unwrap()).is_err());
    let mut cropped = context.clone();
    cropped.known_region = Region::new(Pos::new(0, 0, 0), Pos::new(1, 1, 1));
    assert!(
        RuntimeBehaviorModel::from_fresh_assembly(&catalog, &assembly, &path, &binding, &cropped)
            .is_err()
    );
    let mut incomplete = assembly.clone();
    incomplete.known_regions = vec![
        Region::new(region.min, Pos::new(0, 10, 20)),
        Region::new(Pos::new(2, -1, -20), region.max),
    ];
    assert!(
        RuntimeBehaviorModel::from_fresh_assembly(&catalog, &incomplete, &path, &binding, &context)
            .is_err()
    );
    let mut unsupported = assembly.clone();
    unsupported.blocks.push(PositionedBlock {
        position: Pos::new(0, 3, 0),
        block: dustroute_minecraft::Block::new(BlockKind::Observer),
    });
    assert!(
        RuntimeBehaviorModel::from_fresh_assembly(
            &catalog,
            &unsupported,
            &path,
            &binding,
            &context
        )
        .is_err()
    );
}

#[test]
fn multiple_actual_inputs_outputs_and_inverted_predicates_are_preserved() {
    use dustroute_translate::behavior_type::{BehaviorBudget, BehaviorModel};
    use dustroute_translate::promotion::CheckStatus;
    use dustroute_translate::runtime_behavior::RuntimeBehaviorModel;
    let (mut catalog, mut assembly, path, _, region) = fixture(RotationY::R0);
    let mut source = catalog
        .revision(&assembly.instances[0].revision)
        .unwrap()
        .clone();
    source.id = BlueprintRevisionId::new("test.independent.inputs.v1").unwrap();
    source.blocks.retain(|b| b.position.x == -1);
    let mut second = source.blocks.clone();
    for block in &mut second {
        block.position.x = 2;
    }
    source.blocks.extend(second);
    let definition = TypeRevision {
        id: TypeRevisionId::new("test.two-input-relation.v1").unwrap(),
        name: "Independent location values".into(),
        contract: TypeContract::RepeatedSettling {
            relation: RepeatedSettling {
                inputs: vec!["a".into(), "b".into()],
                outputs: vec!["out_a".into(), "out_b".into()],
                rows: (0..4)
                    .map(|n| BooleanRow {
                        inputs: vec![n & 1 != 0, n & 2 != 0],
                        outputs: vec![n & 1 != 0, n & 2 != 0],
                    })
                    .collect(),
            },
        },
    };
    source.ports.clear();
    let mut inputs = BTreeMap::new();
    let mut outputs = BTreeMap::new();
    for (name, x, positive) in [("a", -1, true), ("b", 2, false)] {
        for (prefix, direction) in [("in", PortDirection::Input), ("out", PortDirection::Output)] {
            source.ports.push(BlueprintPort {
                name: format!("{prefix}_{name}"),
                position: Pos::new(x, 1, 0),
                direction,
                kind: BlueprintPortKind::BlockState,
                facing: None,
                required_source_types: vec![],
            });
        }
        let observation = |prefix| ObservedPort::Location {
            port: format!("{prefix}_{name}"),
            predicate: LocationPredicate::Powered {
                block_kind: BlockKind::Lever,
                powered: positive,
            },
        };
        inputs.insert(name.into(), observation("in"));
        outputs.insert(format!("out_{name}"), observation("out"));
    }
    let binding = BehaviorBinding::Observed {
        behavior_type: definition.id.clone(),
        observed_inputs: inputs,
        observed_outputs: outputs,
    };
    source.behavior_bindings = vec![binding.clone()];
    assembly.instances[0].revision = source.id.clone();
    let origin = assembly.instances[0].origin;
    assembly.blocks = source
        .blocks
        .iter()
        .map(|r| PositionedBlock {
            position: r.position.offset(origin.x, origin.y, origin.z),
            block: r.block.clone(),
        })
        .collect();
    catalog.insert_type(definition).unwrap();
    catalog.insert_revision(source).unwrap();
    let model = RuntimeBehaviorModel::from_fresh_assembly(
        &catalog,
        &assembly,
        &path,
        &binding,
        &runtime_context(&assembly, region),
    )
    .unwrap();
    let mut state = model.initial_state().unwrap();
    for inputs in [[false, false], [true, true], [true, false], [false, true]] {
        state = model.with_inputs(&state, &inputs).unwrap();
        assert_eq!(model.outputs(&state).unwrap(), inputs);
        assert_eq!(model.with_inputs(&state, &inputs).unwrap(), state);
    }
    let report = model.verify(BehaviorBudget::default());
    assert_eq!(report.status, CheckStatus::Passed, "{report:?}");
    assert!(report.graph_closed);
}
