use dustroute_minecraft::piston_electrical::{ElectricalWorld, SIDES, conducts, full_face};
use dustroute_minecraft::time::piston_runtime::*;
use dustroute_minecraft::time::runtime::*;
use dustroute_minecraft::{Block, BlockKind, Facing, PistonState, Pos, Region, World};

const CENTER: Pos = Pos::new(0, 4, 0);
fn region() -> Region {
    Region::new(Pos::new(-8, -4, -8), Pos::new(8, 14, 8))
}
fn along(p: Pos, d: Facing, n: i32) -> Pos {
    let d = d.offset();
    p.offset(d.x * n, d.y * n, d.z * n)
}
fn lever(w: &mut World, p: Pos, support: Facing) {
    w.place(BlockKind::Solid, along(p, support, 1));
    let b = w.place(BlockKind::Lever, p);
    b.powered = Some(false);
    b.support_offset = Some(support.offset());
}
fn device(w: &mut World, p: Pos, kind: BlockKind, output: Facing) {
    let b = w.place(kind, p);
    b.powered = Some(false);
    if kind == BlockKind::Observer {
        b.facing = Some(output);
    }
}
fn transitions(rt: &ElectricalPistonRuntime, pos: Pos) -> Vec<(u64, bool)> {
    rt.trace()
        .iter()
        .flat_map(|r| {
            r.delta
                .iter()
                .flat_map(|d| &d.changes)
                .filter(move |c| c.position == pos && c.before.powered != c.after.powered)
                .map(move |c| (r.invocation.time.game_tick, c.after.powered.unwrap()))
        })
        .collect()
}

#[test]
fn six_direction_observer_pulses_drive_lamps_and_survive_both_restore_boundaries() {
    for output in SIDES {
        let input = along(CENTER, output, -1);
        let lamp = along(CENTER, output, 1);
        let support = SIDES
            .into_iter()
            .find(|s| *s != output && *s != output.opposite())
            .unwrap();
        let mut w = World::new();
        lever(&mut w, input, support);
        device(&mut w, CENTER, BlockKind::Observer, output);
        device(&mut w, lamp, BlockKind::RedstoneLamp, output);
        let mut rt = new_piston_runtime(w, region(), Default::default()).unwrap();
        schedule_electrical_input_after_tick(&mut rt, 1, input, true).unwrap();
        schedule_electrical_input_after_tick(&mut rt, 2, input, false).unwrap();
        rt.step().unwrap(); // fresh initialization
        rt.step().unwrap(); // input, observer pulse now pending
        let saved = rt.behavior_state().unwrap();
        while rt.microstep().unwrap().is_some() {
            if !rt.at_input_boundary() && rt.view().block(CENTER).unwrap().powered == Some(true) {
                break;
            }
        }
        let suspended = rt.checkpoint();
        rt.run_until_idle().unwrap();
        assert_eq!(
            transitions(&rt, CENTER),
            [(3, true), (5, false)],
            "{output:?}"
        );
        assert_eq!(
            transitions(&rt, lamp),
            [(3, true), (9, false)],
            "{output:?}"
        );
        let mut exact = ElectricalPistonRuntime::from_checkpoint(&suspended).unwrap();
        exact.run_until_idle().unwrap();
        assert_eq!(exact.state_key(), rt.state_key());
        let mut resumed = ElectricalPistonRuntime::from_behavior_state(&saved).unwrap();
        resumed.run_until_idle().unwrap();
        assert_eq!(
            resumed.behavior_state().unwrap(),
            rt.behavior_state().unwrap()
        );
    }
}

#[test]
fn lamp_rechecks_power_when_its_first_delayed_off_tick_arrives() {
    let mut w = World::new();
    let input = CENTER.offset(-1, 0, 0);
    lever(&mut w, input, Facing::Down);
    device(&mut w, CENTER, BlockKind::RedstoneLamp, Facing::East);
    let mut rt = new_piston_runtime(w, region(), Default::default()).unwrap();
    for (tick, powered) in [(1, true), (2, false), (4, true), (7, false)] {
        schedule_electrical_input_after_tick(&mut rt, tick, input, powered).unwrap();
    }
    rt.run_until_idle().unwrap();
    assert_eq!(transitions(&rt, CENTER), [(1, true), (11, false)]);
}

#[test]
fn observer_is_a_full_support_but_not_a_conductor_and_emits_only_at_its_output() {
    for output in SIDES {
        let mut w = World::new();
        device(&mut w, CENTER, BlockKind::Observer, output);
        w.get_mut(CENTER).unwrap().powered = Some(true);
        let q = ElectricalWorld::new(&w, region()).unwrap();
        assert!(!conducts(w.get(CENTER).unwrap()));
        for query in SIDES {
            assert!(full_face(w.get(CENTER).unwrap(), query));
            let power = q.emission(CENTER, query, true).unwrap();
            assert_eq!(
                (power.weak, power.strong),
                if query == output.opposite() {
                    (15, 15)
                } else {
                    (0, 0)
                }
            );
        }
        assert!(
            new_piston_runtime(w, region(), Default::default()).is_err(),
            "unrecorded pending pulse is not a fresh state"
        );
    }
}

#[test]
fn command_observer_queues_before_write_and_restores_every_suspended_boundary() {
    for output in SIDES {
        let mut rt = new_piston_runtime(World::new(), region(), Default::default()).unwrap();
        rt.run_until_idle().unwrap();
        let mut block = Block::new(BlockKind::Observer);
        block.facing = Some(output);
        block.powered = Some(false);
        let identity = BlockIdentity::of(&block);
        rt.install_now(CENTER, block).unwrap();
        let mut checkpoints = Vec::new();
        let mut before_write_queued = false;
        while rt.microstep().unwrap().is_some() {
            before_write_queued |= rt.view().block(CENTER).unwrap().kind == BlockKind::Air
                && rt.view().block_tick_queued(CENTER, &identity);
            checkpoints.push(rt.checkpoint());
        }
        assert!(before_write_queued, "{output:?}");
        assert_eq!(
            transitions(&rt, CENTER),
            [(0, false), (2, true), (4, false)]
        );
        for checkpoint in checkpoints {
            let mut restored = ElectricalPistonRuntime::from_checkpoint(&checkpoint).unwrap();
            restored.run_until_idle().unwrap();
            assert_eq!(restored.state_key(), rt.state_key(), "{output:?}");
        }
    }
}

#[test]
fn command_runtime_rejects_the_pre_command_adapter_checkpoint() {
    struct PreviousAdapter;
    impl RuntimeAdapter for PreviousAdapter {
        type Payload = PistonEvent;
        const REVISION: &'static str = "dustroute.piston-electrical-callbacks.java-1-21-11.v5";
        fn validate_initial(_: RuntimeView<'_>) -> Result<(), RuntimeError> {
            Ok(())
        }
        fn handle(
            _: &Invocation<PistonEvent>,
            _: RuntimeView<'_>,
        ) -> Result<RuntimeOutcome<PistonEvent>, RuntimeError> {
            Ok(RuntimeOutcome::default())
        }
    }
    let previous =
        SynchronousWorldRuntime::<PreviousAdapter>::new(World::new(), region(), Default::default())
            .unwrap();
    assert!(ElectricalPistonRuntime::from_checkpoint(&previous.checkpoint()).is_err());
}

#[test]
fn powered_observer_command_add_resets_without_creating_a_pulse() {
    let mut rt = new_piston_runtime(World::new(), region(), Default::default()).unwrap();
    rt.run_until_idle().unwrap();
    let mut block = Block::new(BlockKind::Observer);
    block.facing = Some(Facing::Up);
    block.powered = Some(true);
    rt.install_now(CENTER, block).unwrap();
    rt.run_until_idle().unwrap();
    assert_eq!(rt.view().block(CENTER).unwrap().powered, Some(false));
    assert_eq!(rt.view().time().game_tick, 0);
    assert_eq!(rt.pending_count(), 0);
}

#[test]
fn back_face_lever_survives_source_body_retraction_without_becoming_a_payload() {
    let mut w = World::new();
    let input = CENTER.offset(0, 1, 0);
    lever(&mut w, input, Facing::Down);
    let body = w.get_mut(CENTER).unwrap();
    body.kind = BlockKind::Piston;
    body.facing = Some(Facing::Down);
    body.piston_state = Some(PistonState::Retracted);
    let initial = w.clone();
    let mut rt = new_piston_runtime(w, region(), Default::default()).unwrap();
    schedule_electrical_input_after_tick(&mut rt, 1, input, true).unwrap();
    schedule_electrical_input_after_tick(&mut rt, 10, input, false).unwrap();
    rt.run_until_idle().unwrap();
    assert_eq!(
        rt.view().block(input).unwrap(),
        *initial.get(input).unwrap()
    );
    assert_eq!(
        rt.view().block(CENTER).unwrap().piston_state,
        Some(PistonState::Retracted)
    );
    assert!(rt.trace().iter().all(|r| {
        r.delta.as_ref().is_none_or(|d| {
            d.changes
                .iter()
                .filter(|c| c.position == input)
                .all(|c| c.before.kind == BlockKind::Lever && c.after.kind == BlockKind::Lever)
        })
    }));
}

#[derive(Clone, Debug, Eq, PartialEq)]
enum Task {
    First,
    Second,
}
struct QueueProbe;
impl RuntimeAdapter for QueueProbe {
    type Payload = Task;
    const REVISION: &'static str = "test.native-ready-batch.v1";
    fn validate_initial(_: RuntimeView<'_>) -> Result<(), RuntimeError> {
        Ok(())
    }
    fn handle(
        i: &Invocation<Task>,
        v: RuntimeView<'_>,
    ) -> Result<RuntimeOutcome<Task>, RuntimeError> {
        let mut out = RuntimeOutcome::default();
        let block = BlockIdentity::of(&v.block(CENTER)?);
        if i.call.payload == Task::First {
            assert!(!v.block_tick_queued(CENTER, &block));
            assert!(v.block_tick_ticking(CENTER, &block));
            out.queued.push(QueueRequest::PrioritizedBlockTick {
                game_tick: 4,
                priority: 3,
                block,
                call: RuntimeCall {
                    target: CENTER,
                    payload: Task::Second,
                },
            });
        } else {
            assert!(
                !v.block_tick_ticking(CENTER, &block),
                "executing callback has left ready batch"
            );
            assert_eq!(v.block_tick_queued(CENTER, &block), i.time.game_tick == 2);
        }
        Ok(out)
    }
}

#[test]
fn due_ticks_are_ticking_but_no_longer_queued_and_admit_a_future_tick() {
    let mut w = World::new();
    w.place(BlockKind::Solid, CENTER);
    w.place(BlockKind::Solid, CENTER.offset(1, 0, 0));
    let mut rt =
        SynchronousWorldRuntime::<QueueProbe>::new(w, region(), Default::default()).unwrap();
    for (target, payload, priority) in [
        (CENTER.offset(1, 0, 0), Task::First, 1),
        (CENTER, Task::Second, 3),
    ] {
        rt.enqueue(QueueRequest::PrioritizedBlockTick {
            game_tick: 2,
            priority,
            block: BlockIdentity::of(&rt.view().block(target).unwrap()),
            call: RuntimeCall { target, payload },
        })
        .unwrap();
    }
    assert!(rt.view().block_tick_queued(
        CENTER,
        &BlockIdentity::of(&rt.view().block(CENTER).unwrap())
    ));
    rt.step().unwrap();
    let checkpoint = rt.checkpoint();
    rt.run_until_idle().unwrap();
    assert_eq!(
        rt.trace()
            .iter()
            .map(|r| r.invocation.time.game_tick)
            .collect::<Vec<_>>(),
        [2, 2, 4]
    );
    let mut resumed = SynchronousWorldRuntime::<QueueProbe>::from_checkpoint(&checkpoint).unwrap();
    resumed.run_until_idle().unwrap();
    assert_eq!(resumed.state_key(), rt.state_key());
    let before = rt.state_key();
    assert!(
        rt.enqueue(QueueRequest::PrioritizedBlockTick {
            game_tick: 4,
            priority: 3,
            block: BlockIdentity::of(&rt.view().block(CENTER).unwrap()),
            call: RuntimeCall {
                target: CENTER,
                payload: Task::Second
            },
        })
        .is_err()
    );
    assert_eq!(
        rt.state_key(),
        before,
        "unsupported same-tick enrollment is atomic"
    );
}

#[test]
fn movement_shapes_precede_registration_and_notify_observers_at_the_start_tick() {
    let mut w = World::new();
    let input = CENTER.offset(-1, 0, 0);
    let observed = CENTER.offset(3, 0, 0);
    let observer = observed.offset(0, 1, 0);
    lever(&mut w, input, Facing::Down);
    let b = w.place(BlockKind::Piston, CENTER);
    b.facing = Some(Facing::East);
    b.piston_state = Some(PistonState::Retracted);
    w.place(BlockKind::Solid, CENTER.offset(1, 0, 0));
    w.place(BlockKind::Solid, CENTER.offset(2, 0, 0));
    device(&mut w, observer, BlockKind::Observer, Facing::Up);
    let mut rt = new_piston_runtime(w, region(), Default::default()).unwrap();
    schedule_electrical_input_after_tick(&mut rt, 0, input, true).unwrap();
    let mut checkpoints = Vec::new();
    let mut saw_queued_before_registration = false;
    while rt.microstep().unwrap().is_some() {
        if rt.view().carrier_staged(observed) {
            let sample = rt.view().observe_location(observed).unwrap();
            assert_eq!(sample.block().kind, BlockKind::MovingPiston);
            assert!(sample.block().piston_entity.is_none());
            assert!(sample.carrier().is_none());
            // Both original sources remain until their own destination writes.
            for n in [1, 2] {
                assert_eq!(
                    rt.view().block(CENTER.offset(n, 0, 0)).unwrap().kind,
                    BlockKind::Solid
                );
            }
            assert!(ElectricalWorld::new(rt.view().world(), region()).is_err());
            assert!(
                new_piston_runtime(rt.view().world().clone(), region(), Default::default())
                    .is_err()
            );
            assert_eq!(
                rt.behavior_state().unwrap_err(),
                RuntimeError::InputInsideCallback
            );
            assert_eq!(
                rt.input_now(input, false).unwrap_err(),
                RuntimeError::InputInsideCallback
            );
            saw_queued_before_registration |= rt.view().block_tick_queued(
                observer,
                &BlockIdentity::of(&rt.view().block(observer).unwrap()),
            );
            checkpoints.push(rt.checkpoint());
        }
    }
    assert!(saw_queued_before_registration);
    assert_eq!(transitions(&rt, observer), [(3, true), (5, false)]);
    assert!(!checkpoints.is_empty());
    for checkpoint in checkpoints {
        let mut restored = ElectricalPistonRuntime::from_checkpoint(&checkpoint).unwrap();
        restored.run_until_idle().unwrap();
        assert_eq!(restored.state_key(), rt.state_key());
        assert_eq!(restored.trace(), rt.trace());
    }
}

#[test]
fn moved_observer_arrival_pulses_and_pending_ticks_stay_at_their_original_cells() {
    for powered_payload in [false, true] {
        let mut w = World::new();
        let input = CENTER.offset(-1, 0, 0);
        let origin = CENTER.offset(1, 0, 0);
        let destination = CENTER.offset(2, 0, 0);
        let trigger = origin.offset(0, 0, -1);
        lever(&mut w, input, Facing::Down);
        lever(&mut w, trigger, Facing::Down);
        let b = w.place(BlockKind::Piston, CENTER);
        b.facing = Some(Facing::East);
        b.piston_variant = Some(dustroute_minecraft::PistonVariant::Sticky);
        b.piston_state = Some(PistonState::Retracted);
        device(&mut w, origin, BlockKind::Observer, Facing::South);
        let mut rt = new_piston_runtime(w, region(), Default::default()).unwrap();
        if powered_payload {
            schedule_electrical_input_after_tick(&mut rt, 0, trigger, true).unwrap();
        }
        schedule_electrical_input_after_tick(
            &mut rt,
            if powered_payload { 2 } else { 0 },
            input,
            true,
        )
        .unwrap();
        schedule_electrical_input_after_tick(&mut rt, 12, input, false).unwrap();
        let mut suspended = None;
        while let Some(r) = rt.microstep().unwrap() {
            if suspended.is_none() && rt.view().carrier_staged(origin) {
                suspended = Some(rt.checkpoint());
            }
            if let Some(d) = r.delta {
                for c in d.changes {
                    if let Some(e) = c.after.piston_entity {
                        if e.pushed_block.kind == BlockKind::Observer {
                            assert_eq!(e.pushed_block.facing, Some(Facing::South));
                        }
                    }
                }
            }
        }
        let at_destination: Vec<_> = rt
            .trace()
            .iter()
            .filter_map(|r| r.delta.as_ref().map(|d| (r, d)))
            .flat_map(|(r, d)| {
                d.changes
                    .iter()
                    .filter(move |c| {
                        c.position == destination
                            && c.before.kind == BlockKind::Observer
                            && c.after.kind == BlockKind::Observer
                    })
                    .map(move |c| (r.invocation.time.game_tick, c.after.powered.unwrap()))
            })
            .collect();
        assert_eq!(
            at_destination,
            if powered_payload {
                vec![(5, false)]
            } else {
                vec![(5, true), (7, false)]
            }
        );
        assert_eq!(rt.view().block(origin).unwrap().kind, BlockKind::Observer);
        assert_eq!(rt.view().block(origin).unwrap().powered, Some(false));
        assert_eq!(rt.view().block(destination).unwrap().kind, BlockKind::Air);
        if powered_payload {
            assert!(
                rt.trace()
                    .iter()
                    .any(|r| r.result == DeliveryResult::BlockReplaced
                        && r.invocation.call.target == origin
                        && r.invocation.call.payload == PistonEvent::ObserverTick)
            );
        }
        let mut restored = ElectricalPistonRuntime::from_checkpoint(&suspended.unwrap()).unwrap();
        restored.run_until_idle().unwrap();
        assert_eq!(restored.state_key(), rt.state_key());
    }
}

#[test]
fn observer_payloads_push_and_pull_in_all_six_directions() {
    for direction in SIDES {
        let output = SIDES
            .into_iter()
            .find(|d| *d != direction && *d != direction.opposite())
            .unwrap();
        let input = along(CENTER, direction, -1);
        let origin = along(CENTER, direction, 1);
        let destination = along(CENTER, direction, 2);
        let mut w = World::new();
        lever(&mut w, input, output);
        let b = w.place(BlockKind::Piston, CENTER);
        b.facing = Some(direction);
        b.piston_variant = Some(dustroute_minecraft::PistonVariant::Sticky);
        b.piston_state = Some(PistonState::Retracted);
        device(&mut w, origin, BlockKind::Observer, output);
        let initial_observer = w.get(origin).unwrap().clone();
        let mut rt = new_piston_runtime(w, region(), Default::default()).unwrap();
        schedule_electrical_input_after_tick(&mut rt, 0, input, true).unwrap();
        schedule_electrical_input_after_tick(&mut rt, 12, input, false).unwrap();
        rt.run_until_idle().unwrap();
        assert_eq!(
            rt.view().block(origin).unwrap(),
            initial_observer,
            "{direction:?}"
        );
        assert_eq!(rt.view().block(destination).unwrap().kind, BlockKind::Air);
        let ticks: Vec<_> = rt
            .trace()
            .iter()
            .filter(|r| {
                r.result == DeliveryResult::Executed
                    && r.invocation.call.payload == PistonEvent::ObserverTick
            })
            .map(|r| (r.invocation.time.game_tick, r.invocation.call.target))
            .collect();
        assert_eq!(
            ticks,
            [
                (5, destination),
                (7, destination),
                (17, origin),
                (19, origin)
            ],
            "{direction:?}"
        );
    }
}
