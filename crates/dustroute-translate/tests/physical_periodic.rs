use std::collections::BTreeMap;

use dustroute_library::PortDirection;
use dustroute_library::assembly::{Assembly, AssemblyRevision};
use dustroute_library::assembly::{AssemblyConnection, AssemblyPortRef, BlueprintGrouping};
use dustroute_library::behavior_type::{
    BehaviorInitialCondition, FiniteBurst, Periodic, PhysicalBehaviorContext,
    PhysicalBehaviorProfile,
};
use dustroute_library::blueprint::*;
use dustroute_library::builtin_laws::{
    DUST_LAW_REVISION, TORCH_LAW_REVISION, builtin_laws, torch_law_revision,
};
use dustroute_minecraft::law::{Expr, Instruction};
use dustroute_minecraft::{Block, BlockKind, Facing, Pos, Region, RotationY, World};
use dustroute_translate::assembly::{validate_assembly, validate_assembly_in_context};
use dustroute_translate::behavior_type::{BehaviorBudget, BehaviorModel};
use dustroute_translate::blueprint_update::{
    BlueprintUpdateRequest, BlueprintUpdates, UpdateStatus,
};
use dustroute_translate::physical_behavior::*;
use dustroute_translate::promotion::{
    CheckStatus, PromotionCandidate, review_assembly, review_assembly_in_context,
};

fn clock() -> (BlueprintCatalog, PhysicalBehaviorSelection) {
    // The wall torch powers the block above it, then dust on its own support.
    // In this execution profile, feedback causes burnout and recovery. Live
    // divergence is retained separately in periodic_clock_observation.rs.
    let mut world = World::new();
    world.set(Pos::new(0, 0, 0), Block::new(BlockKind::Solid));
    world.set(Pos::new(1, 1, 0), Block::new(BlockKind::Solid));
    let torch = world.place(BlockKind::RedstoneTorch, Pos::new(1, 0, 0));
    torch.facing = Some(Facing::East);
    torch.support_offset = Some(Pos::new(-1, 0, 0));
    world.place(BlockKind::RedstoneWire, Pos::new(0, 1, 0));
    let mut catalog = builtin_laws().clone();
    let behavior_type = TypeRevisionId::new("clock.type.v1").unwrap();
    catalog
        .insert_type(TypeRevision {
            id: behavior_type.clone(),
            name: "Recurring nonconstant output".into(),
            contract: TypeContract::Periodic {
                requirement: Periodic {
                    output: "out".into(),
                },
            },
        })
        .unwrap();
    let assembly = AssemblyRevisionId::new("clock.actual.v1").unwrap();
    catalog
        .insert_assembly(AssemblyRevision {
            id: assembly.clone(),
            parents: vec![],
            assembly: Assembly {
                name: "Four-block torch feedback candidate".into(),
                instances: vec![],
                blocks: world
                    .iter()
                    .map(|(position, block)| PositionedBlock {
                        position: *position,
                        block: block.clone(),
                    })
                    .collect(),
                known_regions: vec![Region {
                    min: Pos::new(-2, -2, -2),
                    max: Pos::new(3, 3, 2),
                }],
                connections: vec![],
                boundaries: vec![],
            },
        })
        .unwrap();
    (
        catalog,
        PhysicalBehaviorSelection {
            assembly,
            behavior_type,
            dust_law: BlueprintRevisionId::new(DUST_LAW_REVISION).unwrap(),
            torch_law: BlueprintRevisionId::new(TORCH_LAW_REVISION).unwrap(),
            inputs: BTreeMap::new(),
            outputs: BTreeMap::from([(
                "out".into(),
                PhysicalOutput::Signal {
                    position: Pos::new(0, 1, 0),
                },
            )]),
            max_electrical_iterations: 128,
        },
    )
}

#[test]
fn physical_feedback_recurrence_includes_burnout_and_recovery_memory() {
    let (catalog, selected) = clock();
    let model = PhysicalBehaviorModel::from_fresh_assembly(&catalog, selected.clone()).unwrap();
    let report = model.verify_periodic(BehaviorBudget::default());
    assert_eq!(report.behavior.status, CheckStatus::Passed, "{report:?}");
    assert_eq!(report.profile, PHYSICAL_BEHAVIOR_PROFILE);
    assert_eq!(
        report.assembly,
        *catalog.assembly(&selected.assembly).unwrap()
    );
    let cycle = report.behavior.cycle.unwrap();
    assert_eq!(cycle.startup_steps, 30);
    assert_eq!(report.behavior.reachable_states, 220);
    assert_eq!(cycle.state_period_steps, 190);
    assert_eq!(cycle.output_period_steps, 190);
    assert_eq!(cycle.waveform.iter().filter(|&&b| b).count(), 16);
    let mut state = model.initial_state().unwrap();
    for _ in 0..cycle.startup_steps {
        state = model.step(&state).unwrap();
    }
    let beginning = state.clone();
    assert_eq!(
        model
            .torch_state(&state, Pos::new(1, 0, 0))
            .unwrap()
            .register("burnout_seen"),
        Some(1)
    );
    for i in 0..cycle.state_period_steps * 3 {
        assert_eq!(
            model.outputs(&state).unwrap(),
            [cycle.waveform[i % cycle.output_period_steps]]
        );
        state = model.step(&state).unwrap();
    }
    assert_eq!(state, beginning);
    assert_eq!(
        model
            .verify_periodic(BehaviorBudget {
                max_states: 20,
                ..BehaviorBudget::default()
            })
            .behavior
            .status,
        CheckStatus::Undetermined
    );
}

#[test]
fn changed_laws_and_surroundings_require_their_own_periodic_verification() {
    let (mut catalog, selected) = clock();
    let mut changed = torch_law_revision().clone();
    changed.id = BlueprintRevisionId::new("torch.stuck.v1").unwrap();
    changed.parents = vec![torch_law_revision().id.clone()];
    changed.law.as_mut().unwrap().handlers.insert(
        "scheduled_tick".into(),
        vec![Instruction::Set {
            register: "lit".into(),
            value: Expr::Constant { value: 1 },
        }],
    );
    catalog.insert_revision(changed.clone()).unwrap();
    let mut changed_selection = selected.clone();
    changed_selection.torch_law = changed.id;
    let changed_model =
        PhysicalBehaviorModel::from_fresh_assembly(&catalog, changed_selection).unwrap();
    assert_eq!(
        changed_model
            .verify_periodic(BehaviorBudget::default())
            .behavior
            .status,
        CheckStatus::Failed
    );

    let mut interrupted = catalog.assembly(&selected.assembly).unwrap().clone();
    interrupted.id = AssemblyRevisionId::new("clock.interrupted.v1").unwrap();
    interrupted.parents = vec![selected.assembly.clone()];
    interrupted
        .assembly
        .blocks
        .retain(|b| b.position != Pos::new(1, 1, 0));
    let mut moved = selected.clone();
    moved.assembly = interrupted.id.clone();
    catalog.insert_assembly(interrupted).unwrap();
    let model = PhysicalBehaviorModel::from_fresh_assembly(&catalog, moved).unwrap();
    assert_eq!(
        model
            .verify_periodic(BehaviorBudget::default())
            .behavior
            .status,
        CheckStatus::Failed
    );
    let old = PhysicalBehaviorModel::from_fresh_assembly(&catalog, selected).unwrap();
    assert_eq!(
        old.verify_periodic(BehaviorBudget::default())
            .behavior
            .status,
        CheckStatus::Passed
    );
}

#[test]
fn periodic_bindings_reject_external_inputs_missing_outputs_and_unknown_context() {
    let (mut catalog, selected) = clock();
    let mut driven = selected.clone();
    driven.inputs.insert("enable".into(), Pos::default());
    assert!(PhysicalBehaviorModel::from_fresh_assembly(&catalog, driven).is_err());
    let mut missing = selected.clone();
    missing.outputs.clear();
    assert!(PhysicalBehaviorModel::from_fresh_assembly(&catalog, missing).is_err());
    let mut state = catalog.assembly(&selected.assembly).unwrap().clone();
    state.id = AssemblyRevisionId::new("clock.unknown.v1").unwrap();
    state.assembly.known_regions.clear();
    let mut unknown = selected;
    unknown.assembly = state.id.clone();
    catalog.insert_assembly(state).unwrap();
    assert!(PhysicalBehaviorModel::from_fresh_assembly(&catalog, unknown).is_err());
}

fn context() -> PhysicalBehaviorContext {
    PhysicalBehaviorContext {
        profile: PhysicalBehaviorProfile::DustTorchSynchronousGameTickV1,
        initial_condition: BehaviorInitialCondition::FreshConstruction,
        dust_law: BlueprintRevisionId::new(DUST_LAW_REVISION).unwrap(),
        torch_law: BlueprintRevisionId::new(TORCH_LAW_REVISION).unwrap(),
        max_electrical_iterations: 128,
        input_drivers: vec![],
    }
}

#[test]
fn block_effects_match_fresh_saved_queue_and_do_not_recover_without_a_notification() {
    let (catalog, selected) = clock();
    let profile = PhysicalBehaviorProfile::DustSingleTorchBlockEffectsV1;
    let model =
        PhysicalBehaviorModel::from_fresh_assembly_with_profile(&catalog, selected, profile)
            .unwrap();
    let observed: serde_json::Value =
        serde_json::from_str(include_str!("fixtures/periodic_clock_queue_1_21_11.json")).unwrap();
    let checkpoints = observed["queue_checkpoints"].as_array().unwrap();
    let position = Pos::new(1, 0, 0);
    let mut state = model.initial_state().unwrap();
    for tick in 0..=220 {
        let local = model.torch_state(&state, position).unwrap();
        for checkpoint in checkpoints
            .iter()
            .filter(|c| c["game_tick"] == tick && c["current_queue_verified"] == true)
        {
            assert_eq!(
                serde_json::json!(local.pending("scheduled_tick")),
                checkpoint["ticks"][0]["stored_delay"]
            );
        }
        if tick >= 32 {
            assert_eq!(local.pending("scheduled_tick"), None);
            assert_eq!(local.register("lit"), Some(0));
        }
        if tick < 220 {
            state = model.step(&state).unwrap();
        }
    }
    let notified = model.notify_torch_neighbors(&state).unwrap();
    assert_eq!(
        model
            .torch_state(&notified, position)
            .unwrap()
            .pending("scheduled_tick"),
        Some(2)
    );
    let relit = model.step(&model.step(&notified).unwrap()).unwrap();
    assert_eq!(
        model.torch_state(&relit, position).unwrap().register("lit"),
        Some(1)
    );
    assert_eq!(
        model
            .torch_state(&state, position)
            .unwrap()
            .pending("scheduled_tick"),
        None
    );
    assert_eq!(
        model
            .verify_periodic(BehaviorBudget::default())
            .behavior
            .status,
        CheckStatus::Failed
    );
}

#[test]
fn block_effects_refuse_unmodeled_multi_torch_order_and_nested_neighbor_effects() {
    let (mut catalog, selected) = clock();
    let profile = PhysicalBehaviorProfile::DustSingleTorchBlockEffectsV1;
    let mut multiple = catalog.assembly(&selected.assembly).unwrap().clone();
    multiple.id = AssemblyRevisionId::new("clock.two-torches.v1").unwrap();
    let mut torch = multiple
        .assembly
        .blocks
        .iter()
        .find(|b| b.block.kind == BlockKind::RedstoneTorch)
        .unwrap()
        .clone();
    torch.position = Pos::new(10, 0, 0);
    multiple.assembly.blocks.push(torch);
    let mut more = selected.clone();
    more.assembly = multiple.id.clone();
    catalog.insert_assembly(multiple).unwrap();
    assert!(
        PhysicalBehaviorModel::from_fresh_assembly_with_profile(&catalog, more, profile)
            .unwrap_err()
            .contains("at most one torch")
    );
    let mut changed_law = torch_law_revision().clone();
    changed_law.id = blueprint_id("torch.nested-effect.v1");
    changed_law.parents = vec![torch_law_revision().id.clone()];
    changed_law
        .law
        .as_mut()
        .unwrap()
        .handlers
        .get_mut("neighbor_update")
        .unwrap()
        .push(Instruction::Set {
            register: "lit".into(),
            value: Expr::Constant { value: 0 },
        });
    let mut unsupported = selected;
    unsupported.torch_law = changed_law.id.clone();
    catalog.insert_revision(changed_law).unwrap();
    assert!(
        PhysicalBehaviorModel::from_fresh_assembly_with_profile(&catalog, unsupported, profile)
            .unwrap_err()
            .contains("no lit writes")
    );
}

#[test]
fn effects_context_rejects_clock_promotion_without_changing_old_pins() {
    let (mut catalog, assembly) = declared_clock();
    let before = catalog.to_json().unwrap();
    let legacy = context();
    let mut effects = legacy.clone();
    effects.profile = PhysicalBehaviorProfile::DustSingleTorchBlockEffectsV1;
    let encoded = serde_json::to_string(&effects).unwrap();
    assert_eq!(
        serde_json::from_str::<PhysicalBehaviorContext>(&encoded).unwrap(),
        effects
    );
    let report = review_assembly_in_context(
        &catalog,
        &assembly,
        Some(&effects),
        BehaviorBudget::default(),
    )
    .unwrap();
    assert_eq!(report.behavior_status(), Some(CheckStatus::Failed));
    assert!(
        validate_assembly_in_context(
            &catalog,
            &assembly,
            Some(&effects),
            BehaviorBudget::default()
        )
        .is_err()
    );
    let candidate =
        PromotionCandidate::prepare(&catalog, &assembly, grouping("effects.rejected.v1")).unwrap();
    let review = candidate
        .validate_in_context(&catalog, effects, BehaviorBudget::default())
        .unwrap();
    assert_eq!(review.report().behavior_status(), Some(CheckStatus::Failed));
    assert!(review.adopt(&mut catalog).is_err());
    assert_eq!(catalog.to_json().unwrap(), before);
    assert_eq!(
        review_assembly_in_context(
            &catalog,
            &assembly,
            Some(&legacy),
            BehaviorBudget::default()
        )
        .unwrap()
        .behavior_status(),
        Some(CheckStatus::Passed)
    );
}

fn blueprint_id(name: &str) -> BlueprintRevisionId {
    BlueprintRevisionId::new(name).unwrap()
}
fn path(names: &[&str]) -> InstancePath {
    names
        .iter()
        .map(|name| InstanceId::new(*name).unwrap())
        .collect()
}
fn include(name: &str, revision: &str) -> BlueprintInclusion {
    BlueprintInclusion {
        instance: InstanceId::new(name).unwrap(),
        revision: blueprint_id(revision),
        origin: Pos::default(),
        rotation: RotationY::R0,
    }
}

fn declared_clock() -> (BlueprintCatalog, Assembly) {
    let (mut catalog, selected) = clock();
    let mut assembly = catalog
        .assembly(&selected.assembly)
        .unwrap()
        .assembly
        .clone();
    let blueprint = BlueprintRevision {
        id: blueprint_id("clock.v1"),
        parents: vec![],
        name: "Autonomous pulse source".into(),
        classifications: vec![],
        blocks: assembly.blocks.clone(),
        law: None,
        initial_layout: None,
        inclusions: vec![],
        connections: vec![],
        port_bindings: vec![],
        required_laws: vec![],
        static_type_bindings: vec![],
        behavior_bindings: vec![BehaviorBinding::Autonomous {
            behavior_type: selected.behavior_type,
            output_port: "pulse".into(),
        }],
        ports: vec![BlueprintPort {
            name: "pulse".into(),
            direction: PortDirection::Output,
            position: Pos::new(0, 1, 0),
            kind: BlueprintPortKind::Wire,
            facing: None,
            required_source_types: vec![],
        }],
        provenance: torch_law_revision().provenance.clone(),
    };
    catalog.insert_revision(blueprint).unwrap();
    assembly.instances = vec![include("clock", "clock.v1")];
    (catalog, assembly)
}

fn grouping(name: &str) -> BlueprintGrouping {
    BlueprintGrouping {
        id: blueprint_id(name),
        name: name.into(),
        classifications: vec![],
        provenance: torch_law_revision().provenance.clone(),
    }
}

fn declared_burst() -> (BlueprintCatalog, Assembly, PhysicalBehaviorSelection) {
    let (mut catalog, mut assembly) = declared_clock();
    let (_, mut selected) = clock();
    selected.behavior_type = TypeRevisionId::new("burst.type.v1").unwrap();
    catalog
        .insert_type(TypeRevision {
            id: selected.behavior_type.clone(),
            name: "Finite pulse train followed by OFF".into(),
            contract: TypeContract::FiniteBurst {
                requirement: FiniteBurst {
                    output: "out".into(),
                },
            },
        })
        .unwrap();
    let mut burst = catalog.revision(&blueprint_id("clock.v1")).unwrap().clone();
    burst.id = blueprint_id("burst.v1");
    burst.name = "Four-block finite burst candidate".into();
    burst.behavior_bindings = vec![BehaviorBinding::Autonomous {
        behavior_type: selected.behavior_type.clone(),
        output_port: "pulse".into(),
    }];
    catalog.insert_revision(burst).unwrap();
    assembly.instances = vec![include("burst", "burst.v1")];
    (catalog, assembly, selected)
}

fn burst_context() -> PhysicalBehaviorContext {
    PhysicalBehaviorContext {
        profile: PhysicalBehaviorProfile::DustSingleTorchBlockEffectsV1,
        ..context()
    }
}

#[test]
fn observed_feedback_satisfies_finite_burst_but_not_periodicity_in_the_effects_profile() {
    let (catalog, _, selected) = declared_burst();
    let before = catalog.to_json().unwrap();
    let model = PhysicalBehaviorModel::from_fresh_assembly_with_profile(
        &catalog,
        selected.clone(),
        burst_context().profile,
    )
    .unwrap();
    let report = model.verify_finite_burst(BehaviorBudget::default());
    assert_eq!(report.behavior.status, CheckStatus::Passed, "{report:?}");
    assert_eq!(report.profile, PHYSICAL_BEHAVIOR_EFFECTS_PROFILE);
    let cessation = report.behavior.cessation.unwrap();
    assert_eq!(
        (
            cessation.falling_edges,
            cessation.off_from_step,
            cessation.state_cycle_start_steps,
            cessation.state_period_steps
        ),
        (8, 30, 91, 1)
    );
    let old = PhysicalBehaviorModel::from_fresh_assembly(&catalog, selected.clone()).unwrap();
    assert_eq!(
        old.verify_finite_burst(BehaviorBudget::default())
            .behavior
            .status,
        CheckStatus::Failed
    );
    let mut periodic = selected.clone();
    periodic.behavior_type = TypeRevisionId::new("clock.type.v1").unwrap();
    let clock = PhysicalBehaviorModel::from_fresh_assembly_with_profile(
        &catalog,
        periodic,
        burst_context().profile,
    )
    .unwrap();
    assert_eq!(
        clock
            .verify_periodic(BehaviorBudget::default())
            .behavior
            .status,
        CheckStatus::Failed
    );
    let mut driven = selected;
    driven.inputs.insert("trigger".into(), Pos::default());
    assert!(
        PhysicalBehaviorModel::from_fresh_assembly_with_profile(
            &catalog,
            driven,
            burst_context().profile
        )
        .is_err()
    );
    assert_eq!(catalog.to_json().unwrap(), before);
}

#[test]
fn finite_burst_obligations_and_consumer_requirements_need_fresh_context_for_promotion() {
    let (mut catalog, mut assembly, selected) = declared_burst();
    let original = catalog.revision(&blueprint_id("burst.v1")).unwrap().clone();
    let mut consumer = original.clone();
    consumer.id = blueprint_id("burst.consumer.v1");
    consumer.behavior_bindings.clear();
    consumer.blocks.retain(|b| b.position == Pos::new(0, 1, 0));
    consumer.ports[0].name = "input".into();
    consumer.ports[0].direction = PortDirection::Input;
    consumer.ports[0].required_source_types = vec![selected.behavior_type];
    catalog.insert_revision(consumer).unwrap();
    assembly
        .instances
        .push(include("consumer", "burst.consumer.v1"));
    assembly.connections.push(AssemblyConnection {
        source: AssemblyPortRef {
            instance: path(&["burst"]),
            port: "pulse".into(),
        },
        sink: AssemblyPortRef {
            instance: path(&["consumer"]),
            port: "input".into(),
        },
        path: vec![Pos::new(0, 1, 0)],
    });
    assert!(validate_assembly(&catalog, &assembly).is_err());
    assert!(dustroute_translate::blueprint::blueprint_cell(&catalog, &original.id).is_err());
    let unknown = review_assembly(&catalog, &assembly).unwrap();
    assert_eq!(unknown.behavior_status(), Some(CheckStatus::Undetermined));
    assert!(
        validate_assembly_in_context(
            &catalog,
            &assembly,
            Some(&burst_context()),
            BehaviorBudget::default()
        )
        .is_ok()
    );
    let candidate =
        PromotionCandidate::prepare(&catalog, &assembly, grouping("burst.group.v1")).unwrap();
    let exhausted = candidate
        .validate_in_context(
            &catalog,
            burst_context(),
            BehaviorBudget {
                max_states: 1,
                ..BehaviorBudget::default()
            },
        )
        .unwrap();
    assert_eq!(
        exhausted.report().behavior_status(),
        Some(CheckStatus::Undetermined)
    );
    assert!(exhausted.adopt(&mut catalog).is_err());
    let candidate =
        PromotionCandidate::prepare(&catalog, &assembly, grouping("burst.group.v1")).unwrap();
    let review = candidate
        .validate_in_context(&catalog, burst_context(), BehaviorBudget::default())
        .unwrap();
    assert_eq!(review.report().behavior_status(), Some(CheckStatus::Passed));
    review.adopt(&mut catalog).unwrap();
    assert_eq!(catalog.revision(&original.id), Some(&original));
    let saved = catalog.to_json().unwrap();
    assert!(saved.contains("dustroute.blueprint-catalog.v13"));
    let loaded = BlueprintCatalog::from_json(&saved).unwrap();
    assert!(
        validate_assembly_in_context(
            &loaded,
            &assembly,
            Some(&burst_context()),
            BehaviorBudget::default()
        )
        .is_ok()
    );
    let stopped = review_assembly_in_context(
        &loaded,
        &assembly,
        Some(&context()),
        BehaviorBudget::default(),
    )
    .unwrap();
    assert_eq!(stopped.behavior_status(), Some(CheckStatus::Failed));
}

#[test]
fn passing_burst_parent_cannot_hide_failing_periodic_child_or_change_its_pin() {
    let (mut catalog, mut assembly, _) = declared_burst();
    let child = catalog.revision(&blueprint_id("clock.v1")).unwrap().clone();
    let mut parent = catalog.revision(&blueprint_id("burst.v1")).unwrap().clone();
    parent.id = blueprint_id("burst.parent.v1");
    parent.blocks.clear();
    parent.inclusions = vec![include("child", "clock.v1")];
    catalog.insert_revision(parent).unwrap();
    assembly.instances = vec![include("root", "burst.parent.v1")];
    let original = catalog.to_json().unwrap();
    let candidate =
        PromotionCandidate::prepare(&catalog, &assembly, grouping("burst.rejected.v1")).unwrap();
    let review = candidate
        .validate_in_context(&catalog, burst_context(), BehaviorBudget::default())
        .unwrap();
    assert_eq!(
        review.report().occurrences[&path(&["root", "root"])].status(),
        CheckStatus::Passed
    );
    assert_eq!(
        review.report().occurrences[&path(&["root", "root", "child"])].status(),
        CheckStatus::Failed
    );
    assert!(review.adopt(&mut catalog).is_err());
    assert_eq!(catalog.to_json().unwrap(), original);
    assert_eq!(catalog.revision(&child.id), Some(&child));
}

#[test]
fn declared_clock_requires_fresh_context_for_promotion_and_rotated_reuse() {
    let (mut catalog, assembly) = declared_clock();
    let source = catalog.revision(&blueprint_id("clock.v1")).unwrap().clone();
    assert!(dustroute_translate::blueprint::blueprint_cell(&catalog, &source.id).is_err());
    assert_eq!(
        review_assembly(&catalog, &assembly).unwrap().status(),
        CheckStatus::Undetermined
    );
    assert!(validate_assembly(&catalog, &assembly).is_err());
    let candidate = PromotionCandidate::prepare(&catalog, &assembly, grouping("group.v1")).unwrap();
    assert!(
        candidate
            .validate(&catalog)
            .unwrap()
            .adopt(&mut catalog)
            .is_err()
    );
    let review = candidate
        .validate_in_context(&catalog, context(), BehaviorBudget::default())
        .unwrap();
    assert_eq!(
        review.report().status(),
        CheckStatus::Passed,
        "{:?}",
        review.report()
    );
    assert_eq!(review.report().behavior_status(), Some(CheckStatus::Passed));
    review.adopt(&mut catalog).unwrap();
    assert_eq!(catalog.revision(&source.id), Some(&source));

    let mut moved = assembly.clone();
    let origin = Pos::new(12, 3, -7);
    moved.instances[0].rotation = RotationY::R90;
    moved.instances[0].origin = origin;
    moved.blocks.iter_mut().for_each(|b| {
        b.position = RotationY::R90
            .pos(b.position)
            .offset(origin.x, origin.y, origin.z);
        b.block = RotationY::R90.block(&b.block);
    });
    moved.known_regions = moved
        .blocks
        .iter()
        .map(|b| Region::around(b.position, 2))
        .collect();
    let loaded = BlueprintCatalog::from_json(&catalog.to_json().unwrap()).unwrap();
    assert!(
        validate_assembly_in_context(&loaded, &moved, Some(&context()), BehaviorBudget::default())
            .is_ok()
    );

    let mut incompatible: serde_json::Value =
        serde_json::from_str(&catalog.to_json().unwrap()).unwrap();
    let torch = incompatible["revisions"]
        .as_array_mut()
        .unwrap()
        .iter_mut()
        .find(|r| r["id"] == TORCH_LAW_REVISION)
        .unwrap();
    torch["law"]["handlers"]["scheduled_tick"] = serde_json::json!([]);
    let mut incompatible = BlueprintCatalog::from_json(&incompatible.to_string()).unwrap();
    // Even an identically spelled law ID cannot replace the reviewed definition.
    let other = PromotionCandidate::prepare(&catalog, &assembly, grouping("other.v1")).unwrap();
    let reviewed = other
        .validate_in_context(&catalog, context(), BehaviorBudget::default())
        .unwrap();
    assert!(reviewed.adopt(&mut incompatible).is_err());
}

#[test]
fn connection_requirements_and_children_are_checked_in_the_actual_shared_context() {
    let (mut catalog, mut assembly) = declared_clock();
    let mut consumer = catalog.revision(&blueprint_id("clock.v1")).unwrap().clone();
    consumer.id = blueprint_id("consumer.v1");
    consumer.behavior_bindings.clear();
    consumer.blocks.clear();
    consumer.ports[0].name = "input".into();
    consumer.ports[0].direction = PortDirection::Input;
    consumer.ports[0].required_source_types = vec![TypeRevisionId::new("clock.type.v1").unwrap()];
    catalog.insert_revision(consumer).unwrap();
    assembly.instances.push(include("consumer", "consumer.v1"));
    assembly.connections.push(AssemblyConnection {
        source: AssemblyPortRef {
            instance: path(&["clock"]),
            port: "pulse".into(),
        },
        sink: AssemblyPortRef {
            instance: path(&["consumer"]),
            port: "input".into(),
        },
        path: vec![Pos::new(0, 1, 0)],
    });
    assert!(validate_assembly(&catalog, &assembly).is_err());
    assert!(
        validate_assembly_in_context(
            &catalog,
            &assembly,
            Some(&context()),
            BehaviorBudget::default()
        )
        .is_ok()
    );
    let mut broken = assembly.clone();
    broken
        .blocks
        .retain(|block| block.position != Pos::new(1, 1, 0));
    let original = catalog.to_json().unwrap();
    let candidate =
        PromotionCandidate::prepare(&catalog, &broken, grouping("broken.group.v1")).unwrap();
    let review = candidate
        .validate_in_context(&catalog, context(), BehaviorBudget::default())
        .unwrap();
    assert_eq!(
        review.report().occurrences[&path(&["root"])].status(),
        CheckStatus::Passed
    );
    assert_eq!(
        review.report().occurrences[&path(&["root", "clock"])].status(),
        CheckStatus::Failed
    );
    assert_eq!(
        review.report().occurrences[&path(&["root", "consumer"])].status(),
        CheckStatus::Failed
    );
    assert!(review.adopt(&mut catalog).is_err());
    assert_eq!(catalog.to_json().unwrap(), original);

    let no_budget = review_assembly_in_context(
        &catalog,
        &assembly,
        Some(&context()),
        BehaviorBudget {
            max_states: 0,
            ..BehaviorBudget::default()
        },
    )
    .unwrap();
    assert_eq!(no_budget.status(), CheckStatus::Undetermined);
    // An exposed external input is not silently frozen for this first contract.
    assembly.boundaries.push(BlueprintPortBinding {
        name: "control".into(),
        port: AssemblyPortRef {
            instance: path(&["consumer"]),
            port: "input".into(),
        },
    });
    assert_eq!(
        review_assembly_in_context(
            &catalog,
            &assembly,
            Some(&context()),
            BehaviorBudget::default()
        )
        .unwrap()
        .status(),
        CheckStatus::Undetermined
    );
}

fn update_fixture() -> (BlueprintUpdates, BlueprintUpdateRequest) {
    let (mut catalog, mut assembly) = declared_clock();
    let mut next_child = catalog.revision(&blueprint_id("clock.v1")).unwrap().clone();
    next_child.id = blueprint_id("clock.v2");
    next_child.parents = vec![blueprint_id("clock.v1")];
    catalog.insert_revision(next_child).unwrap();
    let mut parent = catalog.revision(&blueprint_id("clock.v1")).unwrap().clone();
    parent.id = blueprint_id("parent.v1");
    parent.blocks.clear();
    parent.ports.clear();
    parent.behavior_bindings.clear();
    parent.inclusions = vec![include("clock", "clock.v1")];
    catalog.insert_revision(parent.clone()).unwrap();
    assembly.instances = vec![include("root", "parent.v1")];
    let base = AssemblyRevision {
        id: AssemblyRevisionId::new("base.v1").unwrap(),
        parents: vec![],
        assembly,
    };
    catalog.insert_assembly(base.clone()).unwrap();
    let mut candidate = base.clone();
    candidate.id = AssemblyRevisionId::new("candidate.v1").unwrap();
    candidate.parents = vec![base.id.clone()];
    candidate.assembly.instances[0].revision = blueprint_id("parent.v2");
    parent.id = blueprint_id("parent.v2");
    parent.parents = vec![blueprint_id("parent.v1")];
    parent.inclusions[0].revision = blueprint_id("clock.v2");
    let request = BlueprintUpdateRequest {
        id: BlueprintUpdateId::new("update.clock.1").unwrap(),
        title: "Explicit clock revision update".into(),
        description: String::new(),
        base_state: base.id,
        base_parent: blueprint_id("parent.v1"),
        candidate_parent: parent.id.clone(),
        parent_instance: path(&["root"]),
        child_before: path(&["clock"]),
        previous_child: blueprint_id("clock.v1"),
        child_after: path(&["clock"]),
        next_child: blueprint_id("clock.v2"),
        revisions: vec![parent],
        candidate_state: candidate,
        behavior_context: Some(context().into()),
    };
    (BlueprintUpdates::new(catalog), request)
}

#[test]
fn restored_update_rechecks_periodicity_and_cannot_adopt_a_forged_saved_success() {
    let (mut updates, request) = update_fixture();
    let old = updates
        .catalog()
        .revision(&blueprint_id("clock.v1"))
        .unwrap()
        .clone();
    updates.create(request.clone()).unwrap();
    assert_eq!(
        updates.validate(&request.id).unwrap().status(),
        CheckStatus::Passed
    );
    let saved = updates.to_json().unwrap();
    let mut valid = BlueprintUpdates::from_json(&saved).unwrap();
    valid.adopt(&request.id).unwrap();
    assert_eq!(
        valid.proposal(&request.id).unwrap().status(),
        UpdateStatus::Adopted
    );
    assert_eq!(valid.catalog().revision(&old.id), Some(&old));
    let mut forged: serde_json::Value = serde_json::from_str(&saved).unwrap();
    forged["proposals"][0]["request"]["candidate_state"]["assembly"]["blocks"]
        .as_array_mut()
        .unwrap()
        .retain(|b| b["position"] != serde_json::json!({"x":1,"y":1,"z":0}));
    let mut forged = BlueprintUpdates::from_json(&forged.to_string()).unwrap();
    let catalog_before = forged.catalog().to_json().unwrap();
    assert!(forged.adopt(&request.id).is_err());
    assert_eq!(forged.catalog().to_json().unwrap(), catalog_before);
    assert_eq!(
        forged.proposal(&request.id).unwrap().status(),
        UpdateStatus::Open
    );
    assert_eq!(
        forged.review(&request.id).unwrap().status(),
        CheckStatus::Failed
    );
}

#[test]
fn behavior_bindings_cannot_drop_names_directions_or_type_kinds() {
    let (mut catalog, _) = declared_clock();
    let source = catalog.revision(&blueprint_id("clock.v1")).unwrap().clone();
    for mode in 0..3 {
        let mut bad = source.clone();
        bad.id = blueprint_id(&format!("bad-{mode}.v1"));
        match mode {
            0 => {
                bad.behavior_bindings[0] = BehaviorBinding::Autonomous {
                    behavior_type: bad.behavior_bindings[0].behavior_type().clone(),
                    output_port: "absent".into(),
                }
            }
            1 => bad.ports[0].direction = PortDirection::Input,
            _ => bad.behavior_bindings.push(bad.behavior_bindings[0].clone()),
        }
        assert!(catalog.insert_revision(bad).is_err());
    }
    let id = TypeRevisionId::new("signal.v1").unwrap();
    catalog
        .insert_type(TypeRevision {
            id: id.clone(),
            name: "Wire".into(),
            contract: TypeContract::Signal {
                port_kind: BlueprintPortKind::Wire,
            },
        })
        .unwrap();
    let mut bad = source;
    bad.id = blueprint_id("bad-kind.v1");
    bad.behavior_bindings[0] = BehaviorBinding::Autonomous {
        behavior_type: id,
        output_port: "pulse".into(),
    };
    assert!(catalog.insert_revision(bad).is_err());
    assert!(
        serde_json::from_value::<TypeContract>(
            serde_json::json!({"kind":"periodic","requirement":{"output":"out"},"period":4})
        )
        .is_err()
    );
}
