//! Source-derived expectations for the new bounded execution profile. These
//! are regression tests, not recorded live-world conformance observations.
use dustroute_minecraft::piston_motion_law::{ControlFacts, PistonBlockEvent, builtin_laws};
use dustroute_minecraft::time::piston_runtime::*;
use dustroute_minecraft::time::runtime::*;
use dustroute_minecraft::{
    Block, BlockKind, Facing, PistonState, PistonVariant, Pos, Region, World,
};

const BODY: Pos = Pos::new(0, 1, 0);
const FRONT: Pos = Pos::new(1, 1, 0);
const OUT: Pos = Pos::new(2, 1, 0);
const INPUT: Pos = Pos::new(-1, 1, 0);

fn scene(sticky: bool) -> (World, Region) {
    let mut world = World::new();
    world.fill(
        Pos::new(-1, 0, -1),
        Pos::new(3, 0, 1),
        Block::new(BlockKind::Solid),
    );
    let body = world.place(BlockKind::Piston, BODY);
    body.facing = Some(Facing::East);
    body.piston_variant = Some(if sticky {
        PistonVariant::Sticky
    } else {
        PistonVariant::Normal
    });
    body.piston_state = Some(PistonState::Retracted);
    world.place(BlockKind::Solid, FRONT);
    let input = world.place(BlockKind::Lever, INPUT);
    input.powered = Some(false);
    input.support_offset = Some(Pos::new(0, -1, 0));
    (world, Region::new(Pos::new(-3, -1, -3), Pos::new(5, 3, 3)))
}

fn runtime(sticky: bool) -> ElectricalPistonRuntime {
    let (world, region) = scene(sticky);
    new_piston_runtime(world, region, RuntimeLimits::default()).unwrap()
}

fn operate(off: u64, sticky: bool) -> ElectricalPistonRuntime {
    let mut run = runtime(sticky);
    schedule_electrical_input(&mut run, 1, INPUT, true).unwrap();
    schedule_electrical_input(&mut run, off, INPUT, false).unwrap();
    run.run_until_idle().unwrap();
    run
}

fn events(run: &ElectricalPistonRuntime) -> Vec<(u64, PistonBlockEvent)> {
    run.trace()
        .iter()
        .filter_map(|r| match r.invocation.call.payload {
            PistonEvent::Block { event, .. } if r.result == DeliveryResult::Executed => {
                Some((r.invocation.time.game_tick, event))
            }
            _ => None,
        })
        .collect()
}

#[test]
fn post_world_input_keeps_same_world_time_at_the_completion_boundary() {
    // The retained mixed-BA server trace requests RetractDrop after the second
    // carrier tick. Moving that input before the next tick changes the event
    // despite the same delivered-event tick and the same eventual geometry.
    let mut after = runtime(true);
    schedule_electrical_input_after_tick(&mut after, 1, INPUT, true).unwrap();
    schedule_electrical_input_after_tick(&mut after, 3, INPUT, false).unwrap();
    after.run_until_idle().unwrap();
    assert_eq!(
        events(&after),
        vec![
            (2, PistonBlockEvent::Extend),
            (4, PistonBlockEvent::RetractDrop)
        ]
    );

    let mut before = runtime(true);
    schedule_electrical_input(&mut before, 2, INPUT, true).unwrap();
    schedule_electrical_input(&mut before, 4, INPUT, false).unwrap();
    before.run_until_idle().unwrap();
    assert_eq!(
        events(&before),
        vec![
            (2, PistonBlockEvent::Extend),
            (4, PistonBlockEvent::Retract)
        ]
    );
    assert_eq!(after.view().world(), before.view().world());
}

#[test]
fn interruption_returns_body_but_drops_extending_payload() {
    let run = operate(2, true);
    assert_eq!(
        events(&run),
        vec![
            (1, PistonBlockEvent::Extend),
            (2, PistonBlockEvent::RetractDrop)
        ]
    );
    assert_eq!(
        run.view().block(BODY).unwrap().piston_state,
        Some(PistonState::Retracted)
    );
    assert_eq!(run.view().block(FRONT).unwrap().kind, BlockKind::Air);
    assert_eq!(run.view().block(OUT).unwrap().kind, BlockKind::Solid);
    assert_eq!(run.pending_count(), 0);
    assert!(
        run.trace()
            .iter()
            .any(|r| r.result == DeliveryResult::CarrierRetired)
    );
}

#[test]
fn ordinary_retract_also_finishes_a_still_extending_payload_without_pulling() {
    // At the external edge, progress is full but lastProgress is one half and
    // the saved tick is earlier. Event 1 still takes the moving-payload branch.
    let run = operate(3, true);
    assert_eq!(
        events(&run),
        vec![
            (1, PistonBlockEvent::Extend),
            (3, PistonBlockEvent::Retract)
        ]
    );
    assert_eq!(
        run.view().block(BODY).unwrap().piston_state,
        Some(PistonState::Retracted)
    );
    assert_eq!(run.view().block(OUT).unwrap().kind, BlockKind::Solid);
    assert_eq!(run.view().block(FRONT).unwrap().kind, BlockKind::Air);
}

#[test]
fn event_request_context_survives_until_block_event_delivery() {
    let run = operate(3, true);
    let request = run
        .trace()
        .iter()
        .find(|r| {
            matches!(
                r.invocation.call.payload,
                PistonEvent::Input { powered: false }
            )
        })
        .unwrap();
    assert_eq!(request.invocation.time.section, TickSection::External);
    let delivered = run
        .trace()
        .iter()
        .find(|r| {
            matches!(
                r.invocation.call.payload,
                PistonEvent::Block {
                    event: PistonBlockEvent::Retract,
                    ..
                }
            )
        })
        .unwrap();
    assert_eq!(delivered.invocation.time.section, TickSection::BlockEvents);
    // Reclassifying the retained event in the delivery section would turn
    // event 1 into event 2. The event argument must remain the request's value.
    assert!(delivered.invocation.time.in_block_tick());
}

#[test]
fn carrier_enrollment_and_notification_orders_are_separate_law_outputs() {
    let law = builtin_laws();
    assert_eq!(
        law.geometry(false, false, TickSection::BlockEvents, 0)
            .unwrap()
            .first_tick_delay,
        0
    );
    assert_eq!(
        law.geometry(false, false, TickSection::BlockEntities, 0)
            .unwrap()
            .first_tick_delay,
        1
    );
    let ordinary: Vec<_> = (0..6)
        .map(|i| {
            law.geometry(false, false, TickSection::External, i)
                .unwrap()
                .neighbor_offset
        })
        .collect();
    let shape: Vec<_> = (0..6)
        .map(|i| {
            law.geometry(false, false, TickSection::External, i)
                .unwrap()
                .shape_offset
        })
        .collect();
    assert_eq!(
        ordinary,
        vec![
            Pos::new(-1, 0, 0),
            Pos::new(1, 0, 0),
            Pos::new(0, -1, 0),
            Pos::new(0, 1, 0),
            Pos::new(0, 0, -1),
            Pos::new(0, 0, 1)
        ]
    );
    assert_eq!(
        shape,
        vec![
            Pos::new(-1, 0, 0),
            Pos::new(1, 0, 0),
            Pos::new(0, 0, -1),
            Pos::new(0, 0, 1),
            Pos::new(0, -1, 0),
            Pos::new(0, 1, 0)
        ]
    );
}

#[test]
fn settled_sticky_off_pulls_payload_with_independent_body_carrier() {
    let run = operate(8, true);
    assert_eq!(
        events(&run),
        vec![
            (1, PistonBlockEvent::Extend),
            (8, PistonBlockEvent::Retract)
        ]
    );
    assert_eq!(
        run.view().block(BODY).unwrap().piston_state,
        Some(PistonState::Retracted)
    );
    assert_eq!(run.view().block(FRONT).unwrap().kind, BlockKind::Solid);
    assert_eq!(run.view().block(OUT).unwrap().kind, BlockKind::Air);
    let moving_body = run
        .trace()
        .iter()
        .flat_map(|r| r.delta.iter())
        .flat_map(|d| &d.changes)
        .find(|c| c.position == BODY && c.after.piston_entity.is_some())
        .unwrap();
    let entity = moving_body.after.piston_entity.as_ref().unwrap();
    assert!(entity.source);
    assert!(!entity.extending);
    assert_eq!(
        entity.pushed_block.piston_state,
        Some(PistonState::Retracted)
    );
}

#[test]
fn settled_normal_off_leaves_payload() {
    let run = operate(8, false);
    assert_eq!(
        run.view().block(BODY).unwrap().piston_state,
        Some(PistonState::Retracted)
    );
    assert_eq!(run.view().block(FRONT).unwrap().kind, BlockKind::Air);
    assert_eq!(run.view().block(OUT).unwrap().kind, BlockKind::Solid);
}

#[test]
fn queued_extension_rechecks_power_after_same_tick_off() {
    let run = operate(1, true);
    assert_eq!(
        run.view().block(BODY).unwrap().piston_state,
        Some(PistonState::Retracted)
    );
    assert_eq!(run.view().block(FRONT).unwrap().kind, BlockKind::Solid);
    assert_eq!(run.view().block(OUT).unwrap().kind, BlockKind::Air);
    assert!(!run.trace().iter().any(|r| !r.carrier_changes.is_empty()));
}

#[test]
fn power_return_during_retraction_is_reconsidered_after_materialization() {
    let mut run = runtime(true);
    for (tick, power) in [(1, true), (8, false), (9, true)] {
        schedule_electrical_input(&mut run, tick, INPUT, power).unwrap();
    }
    run.run_until_idle().unwrap();
    assert_eq!(
        events(&run),
        vec![
            (1, PistonBlockEvent::Extend),
            (8, PistonBlockEvent::Retract),
            (11, PistonBlockEvent::Extend)
        ]
    );
    assert_eq!(
        run.view().block(BODY).unwrap().piston_state,
        Some(PistonState::Extended)
    );
    assert_eq!(run.view().block(FRONT).unwrap().kind, BlockKind::PistonHead);
    assert_eq!(run.view().block(OUT).unwrap().kind, BlockKind::Solid);
}

#[test]
fn control_reads_last_progress_tick_equality_and_enclosing_section() {
    let base = ControlFacts {
        extended: true,
        matching_carrier: true,
        last_progress: HalfProgress::Half,
        ..Default::default()
    };
    assert_eq!(
        builtin_laws().control(base).request,
        Some(PistonBlockEvent::Retract)
    );
    for changed in [
        ControlFacts {
            last_progress: HalfProgress::Zero,
            ..base
        },
        ControlFacts {
            same_tick: true,
            ..base
        },
        ControlFacts {
            in_block_tick: true,
            ..base
        },
    ] {
        assert_eq!(
            builtin_laws().control(changed).request,
            Some(PistonBlockEvent::RetractDrop)
        );
    }
    assert_eq!(
        builtin_laws()
            .control(ControlFacts {
                matching_carrier: false,
                in_block_tick: true,
                ..base
            })
            .request,
        Some(PistonBlockEvent::Retract)
    );
}

#[test]
fn normal_carrier_uses_three_calls_and_forced_source_disappears() {
    let law = builtin_laws();
    let first = law.carrier(MotionHistory::fresh(), false, false, 4);
    assert!(!first.complete);
    assert_eq!(
        first.history,
        MotionHistory {
            progress: HalfProgress::Half,
            last_progress: HalfProgress::Zero,
            saved_world_time: 4
        }
    );
    let second = law.carrier(first.history, false, false, 5);
    assert!(!second.complete);
    assert_eq!(second.history.progress, HalfProgress::Full);
    assert_eq!(second.history.last_progress, HalfProgress::Half);
    let third = law.carrier(second.history, false, false, 6);
    assert!(third.complete);
    assert!(!third.discard);
    assert_eq!(third.history.saved_world_time, 6);
    let forced = law.carrier(second.history, true, true, 6);
    assert!(forced.complete && forced.discard);
    assert_eq!(forced.history.saved_world_time, 5);
}

#[test]
fn checkpoints_resume_inside_notifications_with_identical_future() {
    let mut run = runtime(true);
    schedule_electrical_input(&mut run, 1, INPUT, true).unwrap();
    schedule_electrical_input(&mut run, 2, INPUT, false).unwrap();
    while !run.view().carrier_staged(BODY) {
        assert!(run.microstep().unwrap().is_some());
    }
    assert!(!run.at_input_boundary());
    assert_eq!(
        run.view().block(BODY).unwrap().kind,
        BlockKind::MovingPiston
    );
    assert_eq!(run.view().block(OUT).unwrap().kind, BlockKind::MovingPiston);
    assert!(matches!(
        run.input_now(INPUT, true),
        Err(RuntimeError::InputInsideCallback)
    ));
    let mut restored = ElectricalPistonRuntime::from_checkpoint(&run.checkpoint()).unwrap();
    assert_eq!(restored.state_key(), run.state_key());
    run.run_until_idle().unwrap();
    restored.run_until_idle().unwrap();
    assert_eq!(restored.state_key(), run.state_key());
    assert_eq!(restored.trace(), run.trace());
}

#[test]
fn all_horizontal_rotations_preserve_settled_and_interrupted_results() {
    use dustroute_minecraft::RotationY;
    for rotation in [
        RotationY::R0,
        RotationY::R90,
        RotationY::R180,
        RotationY::R270,
    ] {
        for off in [2, 8] {
            let (original, _) = scene(true);
            let mut world = World::new();
            for (p, b) in original.iter() {
                world.set(rotation.pos(*p), rotation.block(b));
            }
            let mut run = new_piston_runtime(
                world,
                Region::new(Pos::new(-5, -2, -5), Pos::new(5, 4, 5)),
                RuntimeLimits::default(),
            )
            .unwrap();
            schedule_electrical_input(&mut run, 1, rotation.pos(INPUT), true).unwrap();
            schedule_electrical_input(&mut run, off, rotation.pos(INPUT), false).unwrap();
            run.run_until_idle().unwrap();
            assert_eq!(
                run.view().block(rotation.pos(BODY)).unwrap().piston_state,
                Some(PistonState::Retracted)
            );
            assert_eq!(
                run.view().block(rotation.pos(BODY)).unwrap().facing,
                Some(rotation.facing(Facing::East))
            );
            assert_eq!(
                run.view().block(rotation.pos(OUT)).unwrap().kind,
                if off == 2 {
                    BlockKind::Solid
                } else {
                    BlockKind::Air
                }
            );
        }
    }
}

fn vertical_scene(direction: Facing, sticky: bool) -> (World, Region, Pos, Pos, Pos, Pos) {
    let body = Pos::new(0, 3, 0);
    let step = direction.offset();
    let front = body.offset(step.x, step.y, step.z);
    let out = front.offset(step.x, step.y, step.z);
    let input = Pos::new(-1, 3, 0);
    let mut world = World::new();
    let piston = world.place(BlockKind::Piston, body);
    piston.facing = Some(direction);
    piston.piston_variant = Some(if sticky {
        PistonVariant::Sticky
    } else {
        PistonVariant::Normal
    });
    piston.piston_state = Some(PistonState::Retracted);
    world.place(BlockKind::Solid, front);
    world.place(BlockKind::Solid, Pos::new(-1, 2, 0));
    let lever = world.place(BlockKind::Lever, input);
    lever.powered = Some(false);
    lever.support_offset = Some(Pos::new(0, -1, 0));
    (
        world,
        Region::new(Pos::new(-3, -1, -3), Pos::new(3, 7, 3)),
        body,
        front,
        out,
        input,
    )
}

#[test]
fn vertical_pistons_push_pull_and_preserve_interruption_for_both_directions() {
    for direction in [Facing::Up, Facing::Down] {
        for (off, expected_front, expected_out) in [
            (2, BlockKind::Air, BlockKind::Solid),
            (8, BlockKind::Solid, BlockKind::Air),
        ] {
            let (world, region, body, front, out, input) = vertical_scene(direction, true);
            let mut run = new_piston_runtime(world, region, RuntimeLimits::default()).unwrap();
            schedule_electrical_input(&mut run, 1, input, true).unwrap();
            schedule_electrical_input(&mut run, off, input, false).unwrap();
            run.run_until_idle().unwrap();
            assert_eq!(
                run.view().block(body).unwrap().piston_state,
                Some(PistonState::Retracted)
            );
            assert_eq!(run.view().block(front).unwrap().kind, expected_front);
            assert_eq!(run.view().block(out).unwrap().kind, expected_out);
            assert_eq!(run.execution_context().profile, dustroute_minecraft::execution_context::WorldExecutionProfile::UnifiedPistonElectricalCallbacksJava12111V8);
        }
    }
}

#[test]
fn a_powered_side_does_not_hide_an_unknown_side() {
    for facing in [Facing::East, Facing::Up, Facing::Down] {
        let mut world = World::new();
        let body = Pos::new(0, 0, 0);
        let piston = world.place(BlockKind::Piston, body);
        piston.facing = Some(facing);
        piston.piston_state = Some(PistonState::Retracted);
        piston.piston_variant = Some(PistonVariant::Normal);
        // A known powered source cannot certify the unknown opposite side.
        world.place(BlockKind::RedstoneBlock, Pos::new(0, 0, -1));
        let region = Region::new(Pos::new(0, -2, -2), Pos::new(2, 2, 2));
        let error = new_piston_runtime(world, region, RuntimeLimits::default())
            .err()
            .unwrap();
        assert!(matches!(error, RuntimeError::UnknownSpace(_)), "{error}");
    }
}

#[test]
fn vertical_checkpoints_and_behavior_states_resume_interrupted_execution() {
    for direction in [Facing::Up, Facing::Down] {
        for off in [1, 2, 3, 8] {
            let (world, region, _, _, _, input) = vertical_scene(direction, true);
            let mut original = new_piston_runtime(world, region, RuntimeLimits::default()).unwrap();
            schedule_electrical_input(&mut original, 1, input, true).unwrap();
            schedule_electrical_input(&mut original, off, input, false).unwrap();
            while original.pending_count() > 0 {
                let state = original.behavior_state().unwrap();
                let mut representative =
                    ElectricalPistonRuntime::from_behavior_state(&state).unwrap();
                loop {
                    let mut exact =
                        ElectricalPistonRuntime::from_checkpoint(&original.checkpoint()).unwrap();
                    let a = original.microstep().unwrap();
                    assert_eq!(a, exact.microstep().unwrap());
                    assert_eq!(original.checkpoint(), exact.checkpoint());
                    let b = representative.microstep().unwrap();
                    assert_eq!(a.is_some(), b.is_some());
                    if let (Some(a), Some(b)) = (a, b) {
                        assert_eq!(a.result, b.result);
                        assert_eq!(a.invocation.kind, b.invocation.kind);
                        assert_eq!(a.invocation.time.section, b.invocation.time.section);
                        assert_eq!(a.delta, b.delta);
                    }
                    assert_eq!(original.view().world(), representative.view().world());
                    assert_eq!(
                        original.at_input_boundary(),
                        representative.at_input_boundary()
                    );
                    if original.at_input_boundary() {
                        break;
                    }
                }
                assert_eq!(
                    original.behavior_state().unwrap(),
                    representative.behavior_state().unwrap()
                );
            }
        }
    }
}

#[test]
fn vertical_motion_preserves_normal_payload_and_enforces_twelve_block_limit() {
    for direction in [Facing::Up, Facing::Down] {
        for count in [12, 13] {
            let (mut world, region, body, front, _, input) = vertical_scene(direction, false);
            let step = direction.offset();
            for distance in 1..=count {
                world.place(
                    BlockKind::Solid,
                    body.offset(step.x * distance, step.y * distance, step.z * distance),
                );
            }
            let expanded = Region::new(
                Pos::new(region.min.x, -12, region.min.z),
                Pos::new(region.max.x, 18, region.max.z),
            );
            let mut run = new_piston_runtime(world, expanded, RuntimeLimits::default()).unwrap();
            schedule_electrical_input(&mut run, 1, input, true).unwrap();
            schedule_electrical_input(&mut run, 8, input, false).unwrap();
            run.run_until_idle().unwrap();
            assert_eq!(
                run.view().block(body).unwrap().piston_state,
                Some(PistonState::Retracted)
            );
            assert_eq!(
                run.view().block(front).unwrap().kind,
                if count == 12 {
                    BlockKind::Air
                } else {
                    BlockKind::Solid
                }
            );
            let far = body.offset(
                step.x * (count + 1),
                step.y * (count + 1),
                step.z * (count + 1),
            );
            assert_eq!(
                run.view().block(far).unwrap().kind,
                if count == 12 {
                    BlockKind::Solid
                } else {
                    BlockKind::Air
                }
            );
        }
    }
}

#[test]
fn maximum_linear_chain_moves_far_first_but_over_limit_is_blocked() {
    for count in [12, 13] {
        let (mut world, _) = scene(true);
        for x in 1..=count {
            world.place(BlockKind::Solid, Pos::new(x, 1, 0));
        }
        let mut run = new_piston_runtime(
            world,
            Region::new(Pos::new(-3, -1, -3), Pos::new(16, 3, 3)),
            RuntimeLimits::default(),
        )
        .unwrap();
        schedule_electrical_input(&mut run, 1, INPUT, true).unwrap();
        run.run_until_idle().unwrap();
        assert_eq!(
            run.view().block(BODY).unwrap().piston_state,
            Some(if count == 12 {
                PistonState::Extended
            } else {
                PistonState::Retracted
            })
        );
        let moves: Vec<_> = run
            .trace()
            .iter()
            .flat_map(|r| &r.delta)
            .flat_map(|d| &d.changes)
            .filter(|c| {
                c.before.piston_entity.is_none()
                    && c.after.piston_entity.as_ref().is_some_and(|e| !e.source)
            })
            .collect();
        if count == 12 {
            assert_eq!(
                moves.iter().map(|m| m.position.x - 1).collect::<Vec<_>>(),
                (1..=12).rev().collect::<Vec<_>>()
            );
            assert_eq!(
                run.view().block(Pos::new(13, 1, 0)).unwrap().kind,
                BlockKind::Solid
            );
        } else {
            assert!(moves.is_empty());
        }
    }
}

#[test]
fn unknown_boundary_and_unsupported_support_destruction_never_complete() {
    let (mut world, _) = scene(true);
    // A valid initial wire supported by a moving payload needs a destruction
    // law outside this profile. Keeping it suspended would be a false result.
    let wire = world.place(BlockKind::RedstoneWire, Pos::new(1, 2, 0));
    wire.power_level = Some(0);
    wire.wire_connections = Some(
        dustroute_minecraft::piston_electrical::HORIZONTAL
            .into_iter()
            .map(|d| (d, dustroute_minecraft::WireConnection::Side))
            .collect(),
    );
    let region = Region::new(Pos::new(-4, -2, -4), Pos::new(5, 4, 4));
    let mut run = new_piston_runtime(world, region, RuntimeLimits::default()).unwrap();
    schedule_electrical_input(&mut run, 1, INPUT, true).unwrap();
    let error = run.run_until_idle().unwrap_err();
    assert!(error.to_string().contains("component support"), "{error}");
    assert!(matches!(
        run.status(),
        dustroute_minecraft::time::TraceStatus::Failed { .. }
    ));

    let (world, _) = scene(true);
    assert!(matches!(
        new_piston_runtime(
            world,
            Region::new(Pos::new(-1, 0, -1), Pos::new(3, 2, 1)),
            RuntimeLimits::default()
        ),
        Err(RuntimeError::UnknownSpace(_))
    ));
}

#[test]
fn initial_gate_rejects_orphaned_heads_and_transient_state() {
    for kind in 1..3 {
        let (mut world, region) = scene(true);
        match kind {
            1 => {
                world.set(
                    FRONT,
                    Block::piston_head(Facing::East, PistonVariant::Sticky, false),
                );
            }
            _ => {
                world.get_mut(BODY).unwrap().piston_state = Some(PistonState::Retracting);
            }
        }
        assert!(new_piston_runtime(world, region, RuntimeLimits::default()).is_err());
    }
}

#[test]
fn canceled_retraction_preserves_extended_body_and_stable_payload() {
    let mut run = runtime(true);
    for (tick, powered) in [(1, true), (8, false), (8, true)] {
        schedule_electrical_input(&mut run, tick, INPUT, powered).unwrap();
    }
    run.run_until_idle().unwrap();
    assert_eq!(
        run.view().block(BODY).unwrap().piston_state,
        Some(PistonState::Extended)
    );
    assert_eq!(run.view().block(FRONT).unwrap().kind, BlockKind::PistonHead);
    assert_eq!(run.view().block(OUT).unwrap().kind, BlockKind::Solid);
    assert!(
        !run.trace()
            .iter()
            .flat_map(|r| &r.delta)
            .flat_map(|d| &d.changes)
            .any(|c| c.position == BODY && c.after.kind == BlockKind::MovingPiston)
    );
}

#[test]
fn observed_piston_properties_must_agree_with_typed_state() {
    let (mut world, region) = scene(true);
    let body = world.get_mut(BODY).unwrap();
    body.observed_name = Some("minecraft:sticky_piston".into());
    body.observation_classification = dustroute_minecraft::ObservationClassification::Exact;
    body.observed_properties
        .insert("extended".into(), "false".into());
    body.observed_properties
        .insert("facing".into(), "west".into());
    assert!(new_piston_runtime(world, region, RuntimeLimits::default()).is_err());
}

#[test]
fn a_lever_on_the_front_side_is_not_a_piston_input() {
    let (mut world, region) = scene(true);
    let front = world.get(INPUT).unwrap().clone();
    world.set(FRONT, front);
    let mut run = new_piston_runtime(world, region, RuntimeLimits::default()).unwrap();
    schedule_electrical_input(&mut run, 1, FRONT, true).unwrap();
    run.run_until_idle().unwrap();
    assert!(events(&run).is_empty());
    assert_eq!(
        run.view().block(BODY).unwrap().piston_state,
        Some(PistonState::Retracted)
    );
}

#[test]
fn fresh_construction_cannot_skip_initial_power_notification() {
    let (mut world, region) = scene(true);
    world.get_mut(INPUT).unwrap().powered = Some(true);
    let mut run = new_piston_runtime(world, region, RuntimeLimits::default()).unwrap();
    assert!(run.pending_count() > 0);
    run.run_until_idle().unwrap();
    assert_eq!(events(&run), vec![(0, PistonBlockEvent::Extend)]);
    assert_eq!(run.view().block(OUT).unwrap().kind, BlockKind::Solid);
}
