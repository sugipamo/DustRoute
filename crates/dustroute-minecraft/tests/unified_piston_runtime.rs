use dustroute_minecraft::execution_context::{WorldExecutionContext, WorldExecutionProfile};
use dustroute_minecraft::piston_motion_law::PistonBlockEvent;
use dustroute_minecraft::time::piston_runtime::*;
use dustroute_minecraft::time::runtime::*;
use dustroute_minecraft::{
    Block, BlockKind, Facing, PistonState, PistonVariant, Pos, Region, World,
};

const SIDES: [Facing; 6] = [
    Facing::North,
    Facing::East,
    Facing::South,
    Facing::West,
    Facing::Up,
    Facing::Down,
];
const BODY: Pos = Pos::new(0, 4, 0);

fn region() -> Region {
    Region::new(Pos::new(-10, -12, -10), Pos::new(30, 22, 10))
}
fn along(p: Pos, d: Facing, n: i32) -> Pos {
    let v = d.offset();
    p.offset(v.x * n, v.y * n, v.z * n)
}
fn piston(world: &mut World, p: Pos, d: Facing, sticky: bool) {
    let block = world.place(BlockKind::Piston, p);
    block.facing = Some(d);
    block.piston_state = Some(PistonState::Retracted);
    block.piston_variant = Some(if sticky {
        PistonVariant::Sticky
    } else {
        PistonVariant::Normal
    });
}
fn lever(world: &mut World, p: Pos) {
    world.place(BlockKind::Solid, p.offset(0, -1, 0));
    let block = world.place(BlockKind::Lever, p);
    block.powered = Some(false);
    block.support_offset = Some(Facing::Down.offset());
}
fn mechanism(world: &mut World, p: Pos, d: Facing, sticky: bool) -> Pos {
    piston(world, p, d, sticky);
    world.place(BlockKind::Solid, along(p, d, 1));
    let input = if matches!(d, Facing::North | Facing::South) {
        p.offset(-1, 0, 0)
    } else {
        p.offset(0, 0, -1)
    };
    lever(world, input);
    input
}
fn runtime(world: World) -> ElectricalPistonRuntime {
    new_piston_runtime(world, region(), RuntimeLimits::default()).unwrap()
}
fn events(run: &ElectricalPistonRuntime) -> Vec<(u64, Pos, PistonBlockEvent)> {
    run.trace()
        .iter()
        .filter_map(|r| match r.invocation.call.payload {
            PistonEvent::Block { event, .. } if r.result == DeliveryResult::Executed => {
                Some((r.invocation.time.game_tick, r.invocation.call.target, event))
            }
            _ => None,
        })
        .collect()
}
fn scope_error(world: World, expected: &str) {
    let error = new_piston_runtime(world, region(), RuntimeLimits::default())
        .err()
        .expect("unsupported scene admitted");
    assert!(error.to_string().contains(expected), "{error}");
}

#[test]
fn all_six_facings_check_power_and_keep_the_front() {
    for direction in SIDES {
        for source_side in SIDES {
            for sticky in [false, true] {
                let mut world = World::new();
                piston(&mut world, BODY, direction, sticky);
                let source = along(BODY, source_side, 1);
                world.place(BlockKind::RedstoneBlock, source);
                let mut run = runtime(world);
                run.run_until_idle().unwrap();
                assert_eq!(
                    run.view().block(BODY).unwrap().piston_state,
                    Some(if source_side == direction {
                        PistonState::Retracted
                    } else {
                        PistonState::Extended
                    }),
                    "{direction:?} {source_side:?} sticky={sticky}"
                );
                assert_eq!(
                    run.view().block(source).unwrap().kind,
                    BlockKind::RedstoneBlock
                );
                assert_eq!(run.view().adapter_revision(), ELECTRICAL_PROFILE);
            }
        }
    }
}

#[test]
fn all_facings_settle_interrupt_cancel_and_repower_with_one_runtime() {
    for direction in SIDES {
        for sticky in [false, true] {
            for (off, restart) in [(1, false), (2, false), (3, false), (8, false), (8, true)] {
                let mut world = World::new();
                let input = mechanism(&mut world, BODY, direction, sticky);
                let mut run = runtime(world);
                schedule_electrical_input(&mut run, 1, input, true).unwrap();
                schedule_electrical_input(&mut run, off, input, false).unwrap();
                if restart {
                    schedule_electrical_input(&mut run, 9, input, true).unwrap();
                }
                run.run_until_idle().unwrap();
                assert_eq!(run.pending_count(), 0);
                assert_eq!(
                    run.view().block(BODY).unwrap().piston_state,
                    Some(if restart {
                        PistonState::Extended
                    } else {
                        PistonState::Retracted
                    })
                );
                let returned = off == 1 || (sticky && off == 8 && !restart);
                assert_eq!(
                    run.view().block(along(BODY, direction, 2)).unwrap().kind,
                    if returned {
                        BlockKind::Air
                    } else {
                        BlockKind::Solid
                    }
                );
                assert_eq!(
                    run.view().block(along(BODY, direction, 1)).unwrap().kind,
                    if restart {
                        BlockKind::PistonHead
                    } else if returned {
                        BlockKind::Solid
                    } else {
                        BlockKind::Air
                    }
                );
                let events = events(&run);
                if off == 1 {
                    assert!(run.trace().iter().all(|r| r.carrier_changes.is_empty()));
                }
                if off == 2 {
                    assert!(events.contains(&(2, BODY, PistonBlockEvent::RetractDrop)));
                }
                if off == 3 {
                    assert!(events.contains(&(3, BODY, PistonBlockEvent::Retract)));
                }
            }
        }
    }
}

#[test]
fn mixed_facings_share_one_queue_with_simultaneous_and_independent_inputs() {
    for sticky in [false, true] {
        let mut world = World::new();
        let scenes: Vec<_> = [Facing::East, Facing::Up, Facing::Down]
            .into_iter()
            .enumerate()
            .map(|(i, d)| {
                let p = BODY.offset(i as i32 * 8, 0, 0);
                let input = mechanism(&mut world, p, d, sticky);
                (p, d, input, [2, 3, 8][i])
            })
            .collect();
        let mut run = runtime(world);
        for &(_, _, input, off) in &scenes {
            schedule_electrical_input(&mut run, 1, input, true).unwrap();
            schedule_electrical_input(&mut run, off, input, false).unwrap();
        }
        run.run_until_idle().unwrap();
        assert_eq!(
            events(&run)
                .iter()
                .filter(|(tick, _, event)| *tick == 1 && *event == PistonBlockEvent::Extend)
                .count(),
            3
        );
        for &(p, d, _, off) in &scenes {
            assert_eq!(
                run.view().block(p).unwrap().piston_state,
                Some(PistonState::Retracted)
            );
            assert_eq!(
                run.view().block(along(p, d, 2)).unwrap().kind,
                if sticky && off == 8 {
                    BlockKind::Air
                } else {
                    BlockKind::Solid
                }
            );
        }
        assert_eq!(
            run.execution_context(),
            WorldExecutionContext::for_profile(
                WorldExecutionProfile::UnifiedPistonElectricalCallbacksJava12111V6
            )
        );
        assert_eq!(
            run.trace()
                .iter()
                .map(|r| r.invocation.id)
                .collect::<std::collections::BTreeSet<_>>()
                .len(),
            run.trace().len()
        );
    }
}

#[test]
fn mixed_checkpoints_and_behavior_representatives_preserve_the_future() {
    let mut world = World::new();
    let inputs: Vec<_> = [Facing::East, Facing::Up, Facing::Down]
        .into_iter()
        .enumerate()
        .map(|(i, d)| mechanism(&mut world, BODY.offset(i as i32 * 8, 0, 0), d, true))
        .collect();
    let mut run = runtime(world);
    for (i, input) in inputs.into_iter().enumerate() {
        schedule_electrical_input(&mut run, 1, input, true).unwrap();
        schedule_electrical_input(&mut run, [2, 3, 8][i], input, false).unwrap();
    }
    let mut inside_callback = false;
    while run.pending_count() > 0 {
        let mut representative =
            ElectricalPistonRuntime::from_behavior_state(&run.behavior_state().unwrap()).unwrap();
        loop {
            inside_callback |= !run.at_input_boundary();
            let mut exact = ElectricalPistonRuntime::from_checkpoint(&run.checkpoint()).unwrap();
            let a = run.microstep().unwrap();
            assert_eq!(a, exact.microstep().unwrap());
            assert_eq!(run.checkpoint(), exact.checkpoint());
            let b = representative.microstep().unwrap();
            assert_eq!(a.is_some(), b.is_some());
            if let (Some(a), Some(b)) = (a, b) {
                assert_eq!(a.result, b.result);
                assert_eq!(a.delta, b.delta);
                assert_eq!(a.invocation.kind, b.invocation.kind);
                assert_eq!(a.invocation.time.section, b.invocation.time.section);
            }
            assert_eq!(run.view().world(), representative.view().world());
            assert_eq!(run.at_input_boundary(), representative.at_input_boundary());
            if run.at_input_boundary() {
                break;
            }
        }
        assert_eq!(
            run.behavior_state().unwrap(),
            representative.behavior_state().unwrap()
        );
    }
    assert!(inside_callback);
}

#[test]
fn unknown_electrical_space_cannot_be_hidden_by_a_powered_source() {
    let mut world = World::new();
    piston(&mut world, BODY, Facing::Up, false);
    world.place(BlockKind::RedstoneBlock, BODY.offset(-1, 0, 0));
    // All immediate neighbors known; the extra halo needed to exclude omitted
    // paths is not. No partial construction can certify this environment.
    let limited = Region::new(BODY.offset(-1, -1, -1), BODY.offset(1, 1, 1));
    assert!(matches!(
        new_piston_runtime(world, limited, RuntimeLimits::default()),
        Err(RuntimeError::UnknownSpace(_))
    ));
}

#[test]
fn multiple_direct_sources_preserve_power_until_the_last_input_is_removed() {
    let mut world = World::new();
    let first = mechanism(&mut world, BODY, Facing::Up, true);
    let second = BODY.offset(-1, 0, 0);
    lever(&mut world, second);
    let mut run = runtime(world);
    for input in [first, second] {
        schedule_electrical_input(&mut run, 1, input, true).unwrap();
    }
    schedule_electrical_input(&mut run, 8, first, false).unwrap();
    run.run_until_idle().unwrap();
    assert_eq!(
        run.view().block(BODY).unwrap().piston_state,
        Some(PistonState::Extended)
    );
    run.input_now(second, false).unwrap();
    run.run_until_idle().unwrap();
    assert_eq!(
        run.view().block(BODY).unwrap().piston_state,
        Some(PistonState::Retracted)
    );
}

#[test]
fn direct_sources_require_consistent_explicit_evidence() {
    let mut world = World::new();
    let input = mechanism(&mut world, BODY, Facing::East, true);
    world.get_mut(input).unwrap().powered = None;
    scope_error(world, "explicit device power");
    let mut world = World::new();
    piston(&mut world, BODY, Facing::East, true);
    let mut source = Block::new(BlockKind::RedstoneBlock);
    source.powered = Some(false);
    world.set(BODY.offset(-1, 0, 0), source);
    scope_error(world, "unsupported electrical evidence");
}

#[test]
fn lever_inputs_notify_both_the_source_and_support_in_six_way_order() {
    let mut world = World::new();
    let input = mechanism(&mut world, BODY, Facing::Down, true);
    let mut run = runtime(world);
    run.run_until_idle().unwrap();
    let start = run.trace().len();
    run.input_now(input, true).unwrap();
    run.step().unwrap();
    let batch = run.trace()[start..]
        .iter()
        .find_map(|r| {
            (r.invocation.kind == InvocationKind::Callback
                && matches!(r.invocation.call.payload, PistonEvent::Notify { .. }))
            .then(|| serde_json::to_value(&r.invocation.call.payload).unwrap())
        })
        .unwrap();
    // Flags-3 write: ordinary, shape, then explicit lever/support notifications.
    let expected: Vec<_> = [
        (input, false),
        (input, true),
        (input, false),
        (input.offset(0, -1, 0), false),
    ]
    .into_iter()
    .flat_map(|(source, shape)| {
        let sides = if shape {
            [
                Facing::West,
                Facing::East,
                Facing::North,
                Facing::South,
                Facing::Down,
                Facing::Up,
            ]
        } else {
            [
                Facing::West,
                Facing::East,
                Facing::Down,
                Facing::Up,
                Facing::North,
                Facing::South,
            ]
        };
        sides.map(|d| serde_json::json!({"source":source,"target":along(source,d,1),"shape":shape}))
    })
    .collect();
    assert_eq!(batch["Notify"]["jobs"], serde_json::json!(expected));
}

#[test]
fn vertical_linear_limits_keep_the_existing_payload_contract() {
    for direction in [Facing::Up, Facing::Down] {
        for sticky in [false, true] {
            for count in [12, 13] {
                let mut world = World::new();
                let input = mechanism(&mut world, BODY, direction, sticky);
                for n in 1..=count {
                    world.place(BlockKind::Solid, along(BODY, direction, n));
                }
                let mut run = runtime(world);
                schedule_electrical_input(&mut run, 1, input, true).unwrap();
                schedule_electrical_input(&mut run, 8, input, false).unwrap();
                run.run_until_idle().unwrap();
                assert_eq!(
                    run.view().block(BODY).unwrap().piston_state,
                    Some(PistonState::Retracted)
                );
                assert_eq!(
                    run.view()
                        .block(along(BODY, direction, count + 1))
                        .unwrap()
                        .kind,
                    if count == 12 {
                        BlockKind::Solid
                    } else {
                        BlockKind::Air
                    }
                );
                assert_eq!(
                    run.view().block(along(BODY, direction, 1)).unwrap().kind,
                    if count == 13 || sticky {
                        BlockKind::Solid
                    } else {
                        BlockKind::Air
                    }
                );
            }
        }
    }
}

#[test]
fn exact_lever_mount_and_power_evidence_survive_input_changes() {
    let mut world = World::new();
    let input = mechanism(&mut world, BODY, Facing::Up, true);
    let block = world.get_mut(input).unwrap();
    block.observed_name = Some("minecraft:lever".into());
    block.observation_classification = dustroute_minecraft::ObservationClassification::Exact;
    block.facing = Some(Facing::North);
    block.observed_properties = [("face", "floor"), ("facing", "north"), ("powered", "false")]
        .map(|(k, v)| (k.into(), v.into()))
        .into();
    let mut run = runtime(world.clone());
    schedule_electrical_input(&mut run, 1, input, true).unwrap();
    run.run_until_idle().unwrap();
    assert_eq!(
        run.view().block(BODY).unwrap().piston_state,
        Some(PistonState::Extended)
    );
    assert_eq!(
        run.view().block(input).unwrap().observed_properties["powered"],
        "true"
    );
    world
        .get_mut(input)
        .unwrap()
        .observed_properties
        .insert("face".into(), "ceiling".into());
    scope_error(world, "unsupported electrical evidence");
}
