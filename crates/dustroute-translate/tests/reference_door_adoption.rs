#[path = "support/reference_door_blueprint.rs"]
mod fixture;

use dustroute_library::blueprint::{InstanceId, TypeContract};
use dustroute_minecraft::time::piston_runtime::{
    PistonEvent, new_piston_runtime, schedule_electrical_input_after_tick,
};
use dustroute_translate::assembly_transform::AssemblyTransform;
use dustroute_translate::behavior_type::BehaviorModel;
use dustroute_translate::blueprint_update::BlueprintUpdates;
use dustroute_translate::piston_construction::{ElectricalConstruction, electrical_snapshot};
use dustroute_translate::runtime_behavior::RuntimeBehaviorModel;
use dustroute_translate::{cells::RotationY, world::BlockKind, world::Pos};

#[test]
fn production_planner_builds_and_removes_the_exact_reference_door_after_relocation() {
    let f = fixture::fixture();
    let mut catalog = f.catalog;
    catalog
        .insert_revisions(f.request.revisions.clone())
        .unwrap();
    let original = catalog.clone();
    for (target_anchor, rotation) in [
        (Pos::default(), RotationY::R0),
        (Pos::new(50_000, 180, 1000), RotationY::R0),
        (Pos::new(50_000, 180, 1000), RotationY::R90),
        (Pos::new(50_000, 180, 1000), RotationY::R180),
        (Pos::new(50_000, 180, 1000), RotationY::R270),
    ] {
        let transform = AssemblyTransform {
            source_anchor: Pos::default(),
            target_anchor,
            rotation,
        };
        let (assembly, context) = transform
            .apply(&f.request.candidate_state.assembly, &f.context)
            .unwrap();
        let world = assembly.inspect(&catalog).unwrap().proposed_world();
        let expected = electrical_snapshot(&world, context.known_region).unwrap();
        let plan =
            ElectricalConstruction::new(&world, context.known_region, context.root_limits).unwrap();
        assert_eq!(plan.initial(), &expected);
        assert_eq!(plan.settled(), &expected, "{target_anchor:?} {rotation:?}");
        assert_eq!(plan.build_steps().len(), 43);
        assert!(plan.build_steps().iter().all(|s| s.wait_ticks == 4));
        assert_eq!(plan.build_steps().last().unwrap().expected, expected);
        assert_eq!(plan.remove_steps().len(), 43);
        assert!(
            plan.remove_steps()
                .last()
                .unwrap()
                .expected
                .materialize()
                .blocks
                .is_empty()
        );
        assert_eq!(
            catalog, original,
            "construction must not rewrite source references"
        );
    }
}

#[test]
fn candidate_preserves_literal_geometry_and_saved_source_references() {
    let f = fixture::fixture();
    assert_eq!(f.base.assembly.blocks.len(), 43);
    assert_eq!(
        f.base.assembly.blocks,
        f.request.candidate_state.assembly.blocks
    );
    let original = f.catalog.clone();
    let mut updates = BlueprintUpdates::new(f.catalog);
    updates.create(f.request.clone()).unwrap();
    let restored = BlueprintUpdates::from_json(&updates.to_json().unwrap()).unwrap();
    assert_eq!(restored.catalog(), &original);
    assert_eq!(
        restored.proposal(&f.request.id).unwrap().request(),
        &f.request
    );
    let snapshot = electrical_snapshot(&f.world, f.context.known_region).unwrap();
    let imported = dustroute_translate::snapshot::assembly_from_snapshot(
        &snapshot,
        "round trip",
        vec![f.context.known_region],
    )
    .unwrap();
    assert_eq!(imported.blocks, f.base.assembly.blocks);
}

#[test]
fn proposed_aperture_bindings_observe_a_complete_settled_close_open_cycle() {
    let f = fixture::fixture();
    let mut catalog = f.catalog;
    catalog.insert_revisions(f.request.revisions).unwrap();
    let mechanism = catalog.revision(&f.request.next_child).unwrap();
    let model = RuntimeBehaviorModel::from_fresh_assembly(
        &catalog,
        &f.request.candidate_state.assembly,
        &vec![
            InstanceId::new("root").unwrap(),
            InstanceId::new("mechanism").unwrap(),
        ],
        &mechanism.behavior_bindings[0],
        &f.context,
    )
    .unwrap();
    let TypeContract::RepeatedSettling { relation } = &model.definition().contract else {
        panic!("wrong candidate contract");
    };
    let mut state = model.initial_state().unwrap();
    // This only validates the new bindings on one ordinary cycle. It does not
    // establish the arbitrary-input contract or grant adoption.
    for closed in [false, true, false] {
        state = model.with_inputs(&state, &[closed]).unwrap();
        let mut idle = false;
        for _ in 0..4096 {
            let next = model.step(&state).unwrap();
            if next == state {
                idle = true;
                break;
            }
            state = next;
        }
        assert!(idle, "ordinary door phase did not settle");
        let row = relation.rows.iter().find(|r| r.inputs == [closed]).unwrap();
        assert_eq!(model.outputs(&state).unwrap(), row.outputs);
    }
}

#[test]
fn interrupted_close_disproves_the_proposed_unrestricted_input_contract_in_the_model() {
    let f = fixture::fixture();
    let mut catalog = f.catalog;
    catalog.insert_revisions(f.request.revisions).unwrap();
    let mechanism = catalog.revision(&f.request.next_child).unwrap();
    let model = RuntimeBehaviorModel::from_fresh_assembly(
        &catalog,
        &f.request.candidate_state.assembly,
        &vec![
            InstanceId::new("root").unwrap(),
            InstanceId::new("mechanism").unwrap(),
        ],
        &mechanism.behavior_bindings[0],
        &f.context,
    )
    .unwrap();
    let TypeContract::RepeatedSettling { relation } = &model.definition().contract else {
        panic!("wrong candidate contract");
    };
    let expected = &relation
        .rows
        .iter()
        .find(|r| r.inputs == [false])
        .unwrap()
        .outputs;
    // Two complete model advances, not two game ticks or a recorded server
    // input schedule. This is a model counterexample, not Vanilla evidence.
    let mut state = model
        .with_inputs(&model.initial_state().unwrap(), &[true])
        .unwrap();
    state = model.step(&state).unwrap();
    state = model.step(&state).unwrap();
    state = model.with_inputs(&state, &[false]).unwrap();
    let mut fixed = false;
    for _ in 0..64 {
        let next = model.step(&state).unwrap();
        if next == state {
            fixed = true;
            break;
        }
        state = next;
    }
    assert!(fixed, "witness must establish full-state recurrence");
    assert_eq!(model.with_inputs(&state, &[false]).unwrap(), state);
    let outputs = model.outputs(&state).unwrap();
    let mismatches: Vec<_> = outputs
        .iter()
        .zip(expected)
        .enumerate()
        .filter(|(_, (actual, wanted))| actual != wanted)
        .map(|(i, _)| relation.outputs[i].as_str())
        .collect();
    assert_eq!(mismatches, ["aperture_2_1_air", "aperture_2_1_solid"]);
    for _ in 0..2 {
        let (next, samples) = model.step_with_observations(&state).unwrap();
        assert_eq!(next, state);
        assert!(samples.iter().all(|sample| sample == &outputs));
    }
}

#[test]
fn one_tick_pulse_after_world_ticks_also_leaves_the_model_aperture_closed() {
    let f = fixture::fixture();
    let input = Pos::new(0, 11, 0);
    let mut run =
        new_piston_runtime(f.world, f.context.known_region, f.context.root_limits).unwrap();
    schedule_electrical_input_after_tick(&mut run, 0, input, true).unwrap();
    schedule_electrical_input_after_tick(&mut run, 1, input, false).unwrap();
    run.run_until_idle().unwrap();
    assert_eq!(run.pending_count(), 0);
    assert_eq!(run.view().block(input).unwrap().powered, Some(false));
    let delivered: Vec<_> = run
        .trace()
        .iter()
        .filter_map(|r| match &r.invocation.call.payload {
            PistonEvent::Input { powered } => Some((r.invocation.time.game_tick, *powered)),
            _ => None,
        })
        .collect();
    assert_eq!(delivered, [(0, true), (1, false)]);
    // This separately pins the existing server-thread scheduling model. The
    // retained short-input live comparison now covers this one-tick case.
    for y in 6..=8 {
        for z in -1..=1 {
            let block = run.view().block(Pos::new(0, y, z)).unwrap();
            assert_eq!(block.kind, BlockKind::Solid);
            assert_eq!(
                block.observed_name.as_deref(),
                Some("minecraft:smooth_quartz")
            );
        }
    }
}

#[test]
fn reconstruction_covers_every_stopped_build_and_removal_and_short_input_damage() {
    let f = fixture::fixture();
    let plan = ElectricalConstruction::new(&f.world, f.context.known_region, f.context.root_limits)
        .unwrap();
    let mut cases = plan
        .build_steps()
        .iter()
        .chain(plan.remove_steps())
        .map(|step| step.expected.materialize())
        .collect::<Vec<_>>();
    let mut missing = new_piston_runtime(
        f.world.clone(),
        f.context.known_region,
        f.context.root_limits,
    )
    .unwrap();
    missing.run_until_idle().unwrap();
    missing.remove_now(Pos::new(0, 6, -2)).unwrap();
    missing.run_until_idle().unwrap();
    cases.push(electrical_snapshot(missing.view().world(), f.context.known_region).unwrap());
    for ticks in [1, 3, 14] {
        let mut run = new_piston_runtime(
            f.world.clone(),
            f.context.known_region,
            f.context.root_limits,
        )
        .unwrap();
        let input = Pos::new(0, 11, 0);
        schedule_electrical_input_after_tick(&mut run, 0, input, true).unwrap();
        schedule_electrical_input_after_tick(&mut run, ticks, input, false).unwrap();
        run.run_until_idle().unwrap();
        cases.push(electrical_snapshot(run.view().world(), f.context.known_region).unwrap());
    }
    let mut failures = Vec::new();
    for (i, snapshot) in cases.iter().enumerate() {
        let steps = match plan.reconstruction_steps(snapshot, f.context.root_limits) {
            Ok(steps) => steps,
            Err(error) => {
                failures.push(format!("case {i}: {error}"));
                continue;
            }
        };
        assert_eq!(&steps.last().unwrap().expected, plan.settled(), "case {i}");
        let clearing = steps.len() - plan.build_steps().len();
        if clearing > 0 {
            assert!(steps[clearing - 1].expected.is_empty(), "case {i}");
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
    let mut foreign = plan.initial().clone();
    let mut extra = foreign.blocks[0].clone();
    extra.pos = Pos::new(1, 1, 1);
    foreign.blocks.push(extra);
    assert!(
        plan.reconstruction_steps(&foreign, f.context.root_limits)
            .unwrap_err()
            .contains("material conflict")
    );
    let mut incomplete = plan.initial().clone();
    incomplete.min.x += 1;
    assert!(
        plan.reconstruction_steps(&incomplete, f.context.root_limits)
            .is_err()
    );
}

#[test]
fn operating_reference_preserves_normal_open_and_closed_doors_after_rotation() {
    let f = fixture::fixture();
    let mut catalog = f.catalog;
    catalog.insert_revisions(f.request.revisions).unwrap();
    for rotation in [
        RotationY::R0,
        RotationY::R90,
        RotationY::R180,
        RotationY::R270,
    ] {
        let transform = AssemblyTransform {
            source_anchor: Pos::default(),
            target_anchor: Pos::new(-80, 180, 400),
            rotation,
        };
        let (assembly, context) = transform
            .apply(&f.request.candidate_state.assembly, &f.context)
            .unwrap();
        let plan = ElectricalConstruction::new(
            &assembly.inspect(&catalog).unwrap().proposed_world(),
            context.known_region,
            context.root_limits,
        )
        .unwrap();
        let input = context.input_levers[0];
        let open = plan
            .operating_reference(&[(input, false)], context.root_limits)
            .unwrap();
        assert_eq!(&open, plan.settled());
        let closed = plan
            .operating_reference(&[(input, true)], context.root_limits)
            .unwrap();
        assert_ne!(closed, open);
        for y in 6..=8 {
            for z in -1..=1 {
                let source = Pos::new(0, y, z);
                let cell = transform.position(source).unwrap();
                assert!(!open.blocks.iter().any(|b| b.pos == cell));
                assert!(
                    closed
                        .blocks
                        .iter()
                        .any(|b| b.pos == cell && b.name == "minecraft:smooth_quartz")
                );
            }
        }
        assert!(
            plan.operating_reference(&[(input, false), (input, true)], context.root_limits)
                .is_err()
        );
        assert!(
            plan.operating_reference(&[(context.known_region.min, true)], context.root_limits)
                .is_err()
        );
    }
}

#[test]
fn every_settled_reconstruction_boundary_can_be_replanned_for_admitted_reference_damage() {
    let f = fixture::fixture();
    let plan = ElectricalConstruction::new(&f.world, f.context.known_region, f.context.root_limits)
        .unwrap();
    let input = Pos::new(0, 11, 0);
    let mut starts = Vec::new();
    for ticks in [1, 3, 14] {
        let mut run = new_piston_runtime(
            f.world.clone(),
            f.context.known_region,
            f.context.root_limits,
        )
        .unwrap();
        schedule_electrical_input_after_tick(&mut run, 0, input, true).unwrap();
        schedule_electrical_input_after_tick(&mut run, ticks, input, false).unwrap();
        run.run_until_idle().unwrap();
        starts.push(electrical_snapshot(run.view().world(), f.context.known_region).unwrap());
    }
    let mut missing = plan.settled().clone();
    missing.blocks.retain(|b| b.pos != Pos::new(0, 6, -2));
    starts.push(missing);
    for (case, snapshot) in starts.iter().enumerate() {
        let reconstruction = plan
            .reconstruction_steps(snapshot, f.context.root_limits)
            .unwrap();
        for (index, step) in reconstruction.iter().enumerate() {
            let replanned = plan
                .reconstruction_steps(&step.expected.materialize(), f.context.root_limits)
                .unwrap_or_else(|e| {
                    panic!("damage {case}, reconstruction step {}: {e}", index + 1)
                });
            assert_eq!(&replanned.last().unwrap().expected, plan.settled());
        }
    }
}
