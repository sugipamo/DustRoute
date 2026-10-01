//! Execution-boundary regressions. The adapter is a deterministic test program,
//! not a substitute for the subsequent source-backed piston physics adapter.
use dustroute_minecraft::time::runtime::*;
use dustroute_minecraft::{
    Block, BlockChange, BlockKind, ChangeReason, DeltaCause, Facing, PistonBlockEntityState, Pos,
    Region, RegionSet, ValidatedWorld, World, WorldDelta,
};

const A: Pos = Pos::new(0, 1, 0);
const B: Pos = Pos::new(4, 1, 0);
const FLAG: Pos = Pos::new(2, 1, 0);

#[derive(Clone, Debug, Eq, PartialEq)]
enum Task {
    Seed,
    Arm,
    CompleteA,
    NeighborA,
    Nested,
    ResumeA,
    CompleteB,
    Noop,
    EmptyDelta,
    Fail,
    ParentFailure,
    Recurse,
    Forever,
    ReplaceA,
    RetireA,
    Clock(MotionHistory),
    RetireWithoutWorld,
    WriteOutside,
    ReadOutside,
    RequestBlockEvent,
    PowerGuard,
    TurnOff,
    SwapBlock,
    InvalidBundle,
    StageProbe(u8),
    RegisterWrongPayload,
    StoredOutput(u8, bool),
}

struct Adapter;

fn call(target: Pos, payload: Task) -> RuntimeCall<Task> {
    RuntimeCall { target, payload }
}

fn delta(view: RuntimeView<'_>, writes: Vec<(Pos, Block)>) -> WorldDelta {
    WorldDelta {
        parent_shape: view.world().shape_id(),
        changes: writes
            .into_iter()
            .map(|(position, after)| BlockChange {
                position,
                before: view
                    .world()
                    .get(position)
                    .cloned()
                    .unwrap_or_else(|| Block::new(BlockKind::Air)),
                after,
                reason: ChangeReason::NeighborUpdate,
            })
            .collect(),
        moves: Vec::new(),
        dirty_region: RegionSet::default(),
        cause: DeltaCause::NeighborUpdate,
    }
}

fn moving() -> Block {
    Block::moving_piston(PistonBlockEntityState {
        pushed_block: Box::new(Block::new(BlockKind::Solid)),
        facing: Facing::East,
        extending: true,
        source: false,
        progress: 0,
    })
}

impl RuntimeAdapter for Adapter {
    type Payload = Task;
    const REVISION: &'static str = "test.synchronous-adapter.v1";

    fn validate_initial(view: RuntimeView<'_>) -> Result<(), RuntimeError> {
        ValidatedWorld::try_from(view.world().clone())
            .map(|_| ())
            .map_err(|e| RuntimeError::Invalid(e.to_string()))
    }

    fn handle(
        event: &Invocation<Task>,
        view: RuntimeView<'_>,
    ) -> Result<RuntimeOutcome<Task>, RuntimeError> {
        let mut out = RuntimeOutcome::default();
        match event.call.payload {
            Task::StageProbe(mode) => {
                let complete = moving();
                let mut bare = complete.clone();
                if mode != 4 {
                    bare.piston_entity = None;
                }
                out.delta = Some(delta(view, vec![(A, bare)]));
                if mode != 0 {
                    out.carriers.push(CarrierEffect::Stage {
                        position: A,
                        block: Box::new(complete),
                    });
                }
                if mode != 1 {
                    out.continuation = Some(if mode == 3 {
                        Task::RegisterWrongPayload
                    } else {
                        Task::Noop
                    });
                }
            }
            Task::RegisterWrongPayload => {
                let mut wrong = moving();
                wrong.piston_entity.as_mut().unwrap().pushed_block.kind = BlockKind::Transparent;
                out.delta = Some(delta(view, vec![(A, wrong)]));
                out.carriers.push(CarrierEffect::Install {
                    position: A,
                    history: MotionHistory::fresh(),
                });
            }
            Task::Seed => {
                out.delta = Some(delta(view, vec![(A, moving()), (B, moving())]));
                out.carriers = [A, B]
                    .into_iter()
                    .map(|position| CarrierEffect::Install {
                        position,
                        history: MotionHistory::fresh(),
                    })
                    .collect();
            }
            Task::Arm => {
                out.queued = [(A, Task::CompleteA), (B, Task::CompleteB)]
                    .into_iter()
                    .map(|(pos, task)| QueueRequest::CarrierTick {
                        game_tick: 4,
                        carrier: view.carrier(pos).unwrap().id,
                        call: call(pos, task),
                    })
                    .collect();
            }
            Task::CompleteA | Task::CompleteB | Task::RetireA => {
                let position = event.call.target;
                out.delta = Some(delta(view, vec![(position, Block::new(BlockKind::Air))]));
                out.carriers.push(CarrierEffect::Retire {
                    position,
                    expected: *view.carrier(position).unwrap(),
                });
                if event.call.payload == Task::CompleteA {
                    out.callbacks.push(call(A, Task::NeighborA));
                    out.continuation = Some(Task::ResumeA);
                }
            }
            Task::NeighborA => {
                assert_eq!(
                    view.block(B)?.kind,
                    BlockKind::MovingPiston,
                    "B must not finish before A's callback"
                );
                assert!(!view.time().in_block_tick());
                out.delta = Some(delta(view, vec![(FLAG, Block::new(BlockKind::Solid))]));
                out.callbacks.push(call(FLAG, Task::Nested));
            }
            Task::Nested => {
                assert_eq!(view.block(FLAG)?.kind, BlockKind::Solid);
                out.delta = Some(delta(
                    view,
                    vec![(FLAG, Block::new(BlockKind::Transparent))],
                ));
            }
            Task::ResumeA => {
                assert_eq!(
                    view.block(FLAG)?.kind,
                    BlockKind::Transparent,
                    "continuation must see nested writes"
                );
                assert_eq!(view.block(B)?.kind, BlockKind::MovingPiston);
            }
            Task::ParentFailure => {
                out.delta = Some(delta(view, vec![(FLAG, Block::new(BlockKind::Solid))]));
                out.callbacks.push(call(A, Task::Fail));
                out.continuation = Some(Task::Noop);
            }
            Task::Fail => return Err(RuntimeError::Handler("unsupported child".into())),
            Task::Recurse => out.callbacks.push(event.call.clone()),
            Task::Forever => out.continuation = Some(Task::Forever),
            Task::ReplaceA => out.carriers.push(CarrierEffect::Replace {
                position: A,
                expected: *view.carrier(A).unwrap(),
                history: MotionHistory::fresh(),
            }),
            Task::Clock(history) => out.carriers.push(CarrierEffect::Update {
                position: A,
                expected: *view.carrier(A).unwrap(),
                history,
            }),
            Task::RetireWithoutWorld => out.carriers.push(CarrierEffect::Retire {
                position: A,
                expected: *view.carrier(A).unwrap(),
            }),
            Task::WriteOutside => {
                out.delta = Some(delta(
                    view,
                    vec![(Pos::new(100, 0, 0), Block::new(BlockKind::Solid))],
                ))
            }
            Task::ReadOutside => {
                view.block(Pos::new(100, 0, 0))?;
            }
            Task::RequestBlockEvent => out.queued.push(QueueRequest::BlockEvent {
                block: BlockIdentity::of(&view.block(A)?),
                call: call(A, Task::PowerGuard),
            }),
            Task::PowerGuard => {
                // Event-time power is a law/adapter check; block identity is a
                // delivery check. The runtime must expose the latest world.
                if view.block(A)?.powered == Some(true) {
                    out.delta = Some(delta(view, vec![(FLAG, Block::new(BlockKind::Solid))]));
                }
            }
            Task::TurnOff => {
                let mut after = view.block(A)?;
                after.powered = Some(false);
                out.delta = Some(delta(view, vec![(A, after)]));
            }
            Task::SwapBlock => {
                out.delta = Some(delta(view, vec![(A, Block::new(BlockKind::Transparent))]))
            }
            Task::InvalidBundle => {
                out.delta = Some(delta(view, vec![(FLAG, Block::new(BlockKind::Solid))]));
                out.carriers.push(CarrierEffect::Update {
                    position: A,
                    expected: *view.carrier(A).unwrap(),
                    history: MotionHistory {
                        saved_world_time: view.time().game_tick + 1,
                        ..MotionHistory::fresh()
                    },
                });
                out.queued.push(QueueRequest::ScheduledTick {
                    game_tick: view.time().game_tick + 2,
                    call: call(A, Task::Noop),
                });
            }
            Task::StoredOutput(value, mismatch) => {
                let target = Block::new(BlockKind::Solid);
                out.delta = Some(delta(view, vec![(FLAG, target.clone())]));
                out.outputs.push(OutputEffect {
                    position: FLAG,
                    block: BlockIdentity::of(&if mismatch {
                        Block::new(BlockKind::Transparent)
                    } else {
                        target
                    }),
                    value,
                });
            }
            Task::Noop => {}
            Task::EmptyDelta => out.delta = Some(delta(view, vec![])),
        }
        Ok(out)
    }
}

fn runtime(limits: RuntimeLimits) -> SynchronousWorldRuntime<Adapter> {
    SynchronousWorldRuntime::new(
        World::new(),
        Region::new(Pos::new(-2, 0, -2), Pos::new(6, 3, 2)),
        limits,
    )
    .unwrap()
}

fn input(rt: &mut SynchronousWorldRuntime<Adapter>, task: Task) {
    rt.input_now(call(A, task)).unwrap();
    assert!(rt.step().unwrap());
}

fn seeded() -> SynchronousWorldRuntime<Adapter> {
    let mut rt = runtime(RuntimeLimits::default());
    input(&mut rt, Task::Seed);
    rt
}

#[test]
fn checkpoints_share_read_only_worlds_and_detach_before_committed_writes() {
    let mut rt = seeded();
    let checkpoint = rt.checkpoint();
    let restored = SynchronousWorldRuntime::<Adapter>::from_checkpoint(&checkpoint).unwrap();
    for task in [Task::Noop, Task::EmptyDelta, Task::ReplaceA] {
        input(&mut rt, task);
        assert!(std::ptr::eq(rt.view().world(), restored.view().world()));
        assert_eq!(
            rt.trace().last().unwrap().world_changed_positions().count(),
            0
        );
    }
    rt.input_now(call(A, Task::ParentFailure)).unwrap();
    rt.microstep().unwrap().unwrap();
    assert!(!std::ptr::eq(rt.view().world(), restored.view().world()));
    assert_eq!(rt.view().block(FLAG).unwrap().kind, BlockKind::Solid);
    assert_eq!(restored.view().block(FLAG).unwrap().kind, BlockKind::Air);
    assert_eq!(
        rt.trace()
            .last()
            .unwrap()
            .world_changed_positions()
            .collect::<Vec<_>>(),
        [FLAG]
    );
    let committed = rt.checkpoint();
    let same = SynchronousWorldRuntime::<Adapter>::from_checkpoint(&committed).unwrap();
    assert!(rt.microstep().is_err());
    assert!(std::ptr::eq(rt.view().world(), same.view().world()));
    assert_eq!(rt.view().time(), same.view().time());
    assert_eq!(rt.pending_count(), same.pending_count());
    assert_eq!(rt.trace(), same.trace());
    assert_eq!(rt.view().carrier(A), same.view().carrier(A));
}

#[test]
fn completion_callbacks_and_resume_run_before_the_next_block_entity() {
    let mut rt = seeded();
    input(&mut rt, Task::Arm);
    rt.run_until_idle().unwrap();
    let tasks: Vec<_> = rt
        .trace()
        .iter()
        .map(|r| &r.invocation.call.payload)
        .collect();
    assert_eq!(
        tasks,
        [
            &Task::Seed,
            &Task::Arm,
            &Task::CompleteA,
            &Task::NeighborA,
            &Task::Nested,
            &Task::ResumeA,
            &Task::CompleteB
        ]
    );
    let chain = &rt.trace()[2..6];
    assert!(chain.iter().all(|r| r.invocation.time
        == RuntimeTime {
            game_tick: 4,
            section: TickSection::BlockEntities
        }));
    assert!(
        chain
            .iter()
            .all(|r| r.invocation.root == chain[0].invocation.id)
    );
    assert_eq!(
        chain.iter().map(|r| r.invocation.depth).collect::<Vec<_>>(),
        [0, 1, 2, 0]
    );
    assert!(rt.status().is_complete());
}

#[test]
fn checkpoint_inside_nested_work_restores_world_history_stack_order_and_trace() {
    let mut rt = seeded();
    input(&mut rt, Task::Arm);
    rt.microstep().unwrap(); // complete A; neighbor and resume remain
    rt.microstep().unwrap(); // neighbor A; nested write remains
    assert!(!rt.at_input_boundary());
    let checkpoint = rt.checkpoint();
    let mut restored = SynchronousWorldRuntime::<Adapter>::from_checkpoint(&checkpoint).unwrap();
    assert_eq!(rt.state_key(), restored.state_key());
    rt.run_until_idle().unwrap();
    restored.run_until_idle().unwrap();
    assert_eq!(rt.state_key(), restored.state_key());
    assert_eq!(rt.trace(), restored.trace());
}

#[test]
fn input_cannot_interrupt_callbacks_but_can_follow_a_complete_root() {
    let mut rt = seeded();
    input(&mut rt, Task::Arm);
    rt.microstep().unwrap();
    let before = rt.state_key();
    assert_eq!(
        rt.input_now(call(A, Task::Noop)),
        Err(RuntimeError::InputInsideCallback)
    );
    assert_eq!(before, rt.state_key());
    rt.step().unwrap();
    assert!(rt.at_input_boundary());
    input(&mut rt, Task::Noop);
    let event = &rt.trace().last().unwrap().invocation;
    assert_eq!(event.kind, InvocationKind::Input);
    assert_eq!(event.time.section, TickSection::BlockEntities);
    assert_eq!(rt.pending_count(), 1); // B still pending
}

#[test]
fn failed_child_keeps_accepted_prefix_pending_continuation_and_no_success() {
    let mut rt = runtime(RuntimeLimits::default());
    rt.input_now(call(A, Task::ParentFailure)).unwrap();
    rt.microstep().unwrap();
    let checkpoint = rt.checkpoint();
    let before_world = rt.view().world().clone();
    let before_time = rt.view().time();
    let pending = rt.pending_count();
    assert!(matches!(rt.microstep(), Err(RuntimeError::Handler(_))));
    assert_eq!(rt.view().world(), &before_world);
    assert_eq!(rt.view().time(), before_time);
    assert_eq!(rt.pending_count(), pending);
    assert_eq!(rt.trace().len(), 1);
    assert!(rt.status().is_failed());
    assert!(matches!(rt.run_until_idle(), Err(RuntimeError::Failed(_))));
    let mut restored = SynchronousWorldRuntime::<Adapter>::from_checkpoint(&checkpoint).unwrap();
    assert!(matches!(
        restored.microstep(),
        Err(RuntimeError::Handler(_))
    ));
    assert_eq!(rt.state_key(), restored.state_key());
}

#[test]
fn same_visible_world_with_different_motion_history_is_not_the_same_runtime_state() {
    let mut first = seeded();
    let mut second =
        SynchronousWorldRuntime::<Adapter>::from_checkpoint(&first.checkpoint()).unwrap();
    input(
        &mut first,
        Task::Clock(MotionHistory {
            progress: HalfProgress::Half,
            last_progress: HalfProgress::Zero,
            saved_world_time: 0,
        }),
    );
    input(
        &mut second,
        Task::Clock(MotionHistory {
            progress: HalfProgress::Half,
            last_progress: HalfProgress::Half,
            saved_world_time: 0,
        }),
    );
    assert_eq!(first.view().world(), second.view().world());
    assert_eq!(
        first.view().world().state_id(),
        second.view().world().state_id()
    );
    assert_ne!(first.state_key(), second.state_key());
    assert_ne!(first.view().carrier(A), second.view().carrier(A));
}

#[test]
fn old_tick_does_not_act_on_a_new_carrier_at_the_same_location() {
    let mut rt = seeded();
    let old = rt.view().carrier(A).unwrap().id;
    rt.enqueue(QueueRequest::CarrierTick {
        game_tick: 4,
        carrier: old,
        call: call(A, Task::Fail),
    })
    .unwrap();
    input(&mut rt, Task::ReplaceA);
    let new = rt.view().carrier(A).unwrap().id;
    assert_ne!(old, new);
    rt.step().unwrap();
    assert_eq!(
        rt.trace().last().unwrap().result,
        DeliveryResult::CarrierRetired
    );
    assert_eq!(rt.view().carrier(A).unwrap().id, new);
    input(&mut rt, Task::RetireA);
    rt.input_now(call(B, Task::CompleteB)).unwrap();
    rt.run_until_idle().unwrap();
    assert!(rt.status().is_complete());
}

#[test]
fn motion_cannot_disappear_from_history_while_the_carrier_block_remains() {
    let mut rt = seeded();
    let before = *rt.view().carrier(A).unwrap();
    rt.input_now(call(A, Task::RetireWithoutWorld)).unwrap();
    assert_eq!(rt.step(), Err(RuntimeError::CarrierConflict(A)));
    assert_eq!(rt.view().carrier(A), Some(&before));
    assert_eq!(rt.view().block(A).unwrap().kind, BlockKind::MovingPiston);
    assert_eq!(rt.pending_count(), 1);
}

#[test]
fn empty_queue_with_unfinished_carriers_is_not_completion() {
    let mut rt = seeded();
    assert_eq!(rt.pending_count(), 0);
    assert_eq!(rt.run_until_idle(), Err(RuntimeError::UnfinishedMotion));
    assert!(rt.status().is_failed());
}

#[test]
fn a_later_section_queues_block_events_for_the_next_pass() {
    let mut rt = seeded();
    input(&mut rt, Task::Arm);
    rt.step().unwrap(); // completion A and all its synchronous work, tick 4
    input(&mut rt, Task::RequestBlockEvent);
    rt.run_until_idle().unwrap();
    let guarded = rt
        .trace()
        .iter()
        .find(|r| r.invocation.kind == InvocationKind::BlockEvent)
        .unwrap();
    assert_eq!(
        guarded.invocation.time,
        RuntimeTime {
            game_tick: 5,
            section: TickSection::BlockEvents
        }
    );
    assert!(guarded.invocation.time.in_block_tick());
}

#[test]
fn event_delivery_separates_changed_power_from_replaced_block_identity() {
    for replace in [false, true] {
        let mut world = World::new();
        let mut block = Block::new(BlockKind::Solid);
        block.powered = Some(true);
        world.set(A, block);
        let mut rt = SynchronousWorldRuntime::<Adapter>::new(
            world,
            Region::new(Pos::new(-2, 0, -2), Pos::new(6, 3, 2)),
            RuntimeLimits::default(),
        )
        .unwrap();
        input(&mut rt, Task::RequestBlockEvent);
        input(
            &mut rt,
            if replace {
                Task::SwapBlock
            } else {
                Task::TurnOff
            },
        );
        rt.run_until_idle().unwrap();
        assert_eq!(rt.view().block(FLAG).unwrap().kind, BlockKind::Air);
        assert_eq!(
            rt.trace().last().unwrap().result,
            if replace {
                DeliveryResult::BlockReplaced
            } else {
                DeliveryResult::Executed
            }
        );
    }
}

#[test]
fn duplicate_block_requests_keep_first_queue_position_and_cause() {
    let mut rt = runtime(RuntimeLimits::default());
    let request = QueueRequest::BlockEvent {
        block: BlockIdentity::of(&Block::new(BlockKind::Air)),
        call: call(A, Task::Noop),
    };
    let id = rt.enqueue(request.clone()).unwrap().unwrap();
    let key = rt.state_key();
    assert_eq!(rt.enqueue(request), Ok(None));
    assert_eq!(rt.state_key(), key);
    rt.run_until_idle().unwrap();
    assert_eq!(rt.trace().len(), 1);
    assert_eq!(rt.trace()[0].invocation.id, id);
}

#[test]
fn prioritized_ticks_deduplicate_across_times_and_resume_in_priority_order() {
    let mut rt = runtime(RuntimeLimits::default());
    let request = |target, game_tick, priority| QueueRequest::PrioritizedBlockTick {
        game_tick,
        priority,
        block: BlockIdentity::of(&Block::new(BlockKind::Air)),
        call: call(target, Task::Noop),
    };
    rt.enqueue(request(A, 4, 2)).unwrap();
    let before = rt.state_key();
    assert_eq!(rt.enqueue(request(A, 1, 0)).unwrap(), None);
    assert_eq!(rt.state_key(), before);
    rt.enqueue(request(B, 4, 0)).unwrap();
    rt.enqueue(request(FLAG, 4, 0)).unwrap();
    let mut resumed =
        SynchronousWorldRuntime::<Adapter>::from_checkpoint(&rt.checkpoint()).unwrap();
    for run in [&mut rt, &mut resumed] {
        run.run_until_idle().unwrap();
        assert_eq!(
            run.trace()
                .iter()
                .map(|r| r.invocation.call.target)
                .collect::<Vec<_>>(),
            vec![B, FLAG, A]
        );
        assert!(run.trace().iter().all(|r| r.invocation.time.game_tick == 4));
        // Once delivered, the block can schedule its next tick.
        assert!(run.enqueue(request(A, 6, 1)).unwrap().is_some());
        run.run_until_idle().unwrap();
    }
    assert_eq!(rt.state_key(), resumed.state_key());
    assert_eq!(rt.trace(), resumed.trace());
}

#[test]
fn prioritized_ticks_preserve_block_identity_guard_and_legacy_fifo_requests() {
    let mut rt = runtime(RuntimeLimits::default());
    rt.enqueue(QueueRequest::PrioritizedBlockTick {
        game_tick: 2,
        priority: 0,
        block: BlockIdentity::of(&Block::new(BlockKind::Air)),
        call: call(A, Task::Noop),
    })
    .unwrap();
    rt.input_now(call(A, Task::SwapBlock)).unwrap();
    rt.step().unwrap();
    rt.run_until_idle().unwrap();
    assert_eq!(
        rt.trace().last().unwrap().result,
        DeliveryResult::BlockReplaced
    );
    let request = QueueRequest::ScheduledTick {
        game_tick: 4,
        call: call(A, Task::Noop),
    };
    assert!(rt.enqueue(request.clone()).unwrap().is_some());
    assert!(rt.enqueue(request).unwrap().is_some());
    assert_eq!(rt.pending_count(), 2);
}

#[test]
fn unknown_space_is_neither_air_nor_a_writable_destination() {
    for task in [Task::ReadOutside, Task::WriteOutside] {
        let mut rt = runtime(RuntimeLimits::default());
        assert_eq!(rt.view().block(A).unwrap().kind, BlockKind::Air);
        let before = rt.view().world().clone();
        rt.input_now(call(A, task)).unwrap();
        assert_eq!(
            rt.step(),
            Err(RuntimeError::UnknownSpace(Pos::new(100, 0, 0)))
        );
        assert_eq!(rt.view().world(), &before);
        assert_eq!(rt.pending_count(), 1);
    }
}

#[test]
fn finite_budgets_reject_recursion_and_infinite_continuations_without_fake_idle() {
    for (task, expected) in [(Task::Recurse, "call depth"), (Task::Forever, "microsteps")] {
        let mut rt = runtime(RuntimeLimits {
            max_microsteps: 8,
            max_pending: 16,
            max_call_depth: 2,
        });
        rt.input_now(call(A, task)).unwrap();
        assert_eq!(rt.run_until_idle(), Err(RuntimeError::Limit(expected)));
        assert!(rt.status().is_failed());
        assert!(rt.pending_count() > 0);
    }
}

#[test]
fn callback_context_tracks_enclosing_tick_section_not_callback_kind() {
    let mut rt = runtime(RuntimeLimits::default());
    rt.enqueue(QueueRequest::ScheduledTick {
        game_tick: 3,
        call: call(A, Task::ParentFailure),
    })
    .unwrap();
    rt.microstep().unwrap();
    assert!(rt.view().time().in_block_tick());
    assert!(matches!(rt.microstep(), Err(RuntimeError::Handler(_))));
    assert!(rt.view().time().in_block_tick());
    assert!(
        !RuntimeTime {
            game_tick: 3,
            section: TickSection::BlockEntities
        }
        .in_block_tick()
    );
}

#[test]
fn checkpoint_cannot_be_reinterpreted_under_a_different_adapter_revision() {
    struct Other;
    impl RuntimeAdapter for Other {
        type Payload = Task;
        const REVISION: &'static str = "test.other.v1";
        fn validate_initial(view: RuntimeView<'_>) -> Result<(), RuntimeError> {
            Adapter::validate_initial(view)
        }
        fn handle(
            _: &Invocation<Task>,
            _: RuntimeView<'_>,
        ) -> Result<RuntimeOutcome<Task>, RuntimeError> {
            Ok(RuntimeOutcome::default())
        }
    }
    assert!(matches!(
        SynchronousWorldRuntime::<Other>::from_checkpoint(&seeded().checkpoint()),
        Err(RuntimeError::Invalid(_))
    ));
}

#[test]
fn invalid_history_rejects_the_whole_world_and_queue_update() {
    let mut rt = seeded();
    rt.input_now(call(A, Task::InvalidBundle)).unwrap();
    let checkpoint = rt.checkpoint();
    let shared = SynchronousWorldRuntime::<Adapter>::from_checkpoint(&checkpoint).unwrap();
    let world = rt.view().world().clone();
    let carrier = *rt.view().carrier(A).unwrap();
    assert!(matches!(rt.microstep(), Err(RuntimeError::Invalid(_))));
    assert!(std::ptr::eq(rt.view().world(), shared.view().world()));
    assert_eq!(rt.view().world(), &world);
    assert_eq!(rt.view().carrier(A), Some(&carrier));
    assert_eq!(rt.pending_count(), 1); // original input, no speculative tick
    assert_eq!(rt.trace().len(), 1); // only the earlier successful seed
}

#[test]
fn partial_registration_cannot_be_imported_abandoned_or_substituted() {
    for mode in 0..=4 {
        let mut rt = runtime(RuntimeLimits::default());
        rt.input_now(call(A, Task::StageProbe(mode))).unwrap();
        if mode == 2 || mode == 3 {
            rt.microstep().unwrap();
            assert!(rt.view().carrier_staged(A));
            assert!(!rt.at_input_boundary());
        }
        let world = rt.view().world().clone();
        assert_eq!(
            rt.microstep().unwrap_err(),
            RuntimeError::CarrierConflict(A)
        );
        assert_eq!(rt.view().world(), &world);
    }
}

#[test]
fn enqueue_rejection_preserves_ids_order_and_the_prior_state() {
    let mut rt = runtime(RuntimeLimits {
        max_pending: 1,
        ..RuntimeLimits::default()
    });
    rt.enqueue(QueueRequest::ScheduledTick {
        game_tick: 2,
        call: call(A, Task::Noop),
    })
    .unwrap();
    let before = rt.state_key();
    assert_eq!(
        rt.enqueue(QueueRequest::ScheduledTick {
            game_tick: 3,
            call: call(B, Task::Noop)
        }),
        Err(RuntimeError::Limit("pending work"))
    );
    assert_eq!(rt.state_key(), before);
    rt.run_until_idle().unwrap();
    let before = rt.state_key();
    assert!(matches!(
        rt.enqueue(QueueRequest::External {
            game_tick: 2,
            call: call(B, Task::Noop)
        }),
        Err(RuntimeError::PastDelivery { .. })
    ));
    assert_eq!(rt.state_key(), before);
}

#[test]
fn block_identity_preserves_piston_variant_but_ignores_power_and_facing() {
    use dustroute_minecraft::PistonVariant;
    let mut normal = Block::new(BlockKind::Piston);
    normal.piston_variant = Some(PistonVariant::Normal);
    let mut changed = normal.clone();
    changed.powered = Some(true);
    changed.facing = Some(Facing::East);
    assert_eq!(BlockIdentity::of(&normal), BlockIdentity::of(&changed));
    changed.piston_variant = Some(PistonVariant::Sticky);
    assert_ne!(BlockIdentity::of(&normal), BlockIdentity::of(&changed));
}

#[test]
fn a_world_snapshot_cannot_supply_or_discard_running_motion_history() {
    use dustroute_minecraft::PistonState;
    let mut world = World::new();
    let mut block = Block::new(BlockKind::Piston);
    block.facing = Some(Facing::East);
    block.piston_state = Some(PistonState::Extending);
    world.set(A, block);
    let result = SynchronousWorldRuntime::<Adapter>::new(
        world,
        Region::new(Pos::new(-2, 0, -2), Pos::new(6, 3, 2)),
        RuntimeLimits::default(),
    );
    assert!(
        matches!(result, Err(RuntimeError::Invalid(reason)) if reason == "initial motion requires a runtime checkpoint")
    );
}

#[test]
fn new_world_construction_must_pass_the_selected_adapters_initial_gate() {
    struct Rejecting;
    impl RuntimeAdapter for Rejecting {
        type Payload = Task;
        const REVISION: &'static str = "test.rejecting-initial.v1";
        fn validate_initial(_: RuntimeView<'_>) -> Result<(), RuntimeError> {
            Err(RuntimeError::Invalid("unsupported initial evidence".into()))
        }
        fn handle(
            _: &Invocation<Task>,
            _: RuntimeView<'_>,
        ) -> Result<RuntimeOutcome<Task>, RuntimeError> {
            panic!("initial validation must run before any handler")
        }
    }
    let result = SynchronousWorldRuntime::<Rejecting>::new(
        World::new(),
        Region::new(A, B),
        RuntimeLimits::default(),
    );
    assert!(
        matches!(result, Err(RuntimeError::Invalid(reason)) if reason == "unsupported initial evidence")
    );
}

#[test]
fn opaque_checkpoints_also_pin_the_adapter_type_even_if_a_label_is_reused() {
    struct SameLabel;
    impl RuntimeAdapter for SameLabel {
        type Payload = Task;
        const REVISION: &'static str = Adapter::REVISION;
        fn validate_initial(view: RuntimeView<'_>) -> Result<(), RuntimeError> {
            Adapter::validate_initial(view)
        }
        fn handle(
            _: &Invocation<Task>,
            _: RuntimeView<'_>,
        ) -> Result<RuntimeOutcome<Task>, RuntimeError> {
            Ok(RuntimeOutcome::default())
        }
    }
    assert!(matches!(
        SynchronousWorldRuntime::<SameLabel>::from_checkpoint(&seeded().checkpoint()),
        Err(RuntimeError::Invalid(_))
    ));
}

#[test]
fn stored_output_validation_is_transactional_and_checkpointed() {
    let mut rt = runtime(Default::default());
    input(&mut rt, Task::StoredOutput(7, false));
    assert_eq!(rt.view().stored_output(FLAG).unwrap(), 7);
    let checkpoint = rt.checkpoint();
    let restored = SynchronousWorldRuntime::<Adapter>::from_checkpoint(&checkpoint).unwrap();
    assert_eq!(restored.state_key(), rt.state_key());
    for task in [Task::StoredOutput(16, false), Task::StoredOutput(3, true)] {
        let mut failed = SynchronousWorldRuntime::<Adapter>::from_checkpoint(&checkpoint).unwrap();
        let world = failed.view().world().clone();
        failed.input_now(call(A, task)).unwrap();
        assert!(failed.step().is_err());
        assert_eq!(failed.view().stored_output(FLAG).unwrap(), 7);
        assert_eq!(failed.view().world(), &world);
        assert_eq!(failed.trace(), rt.trace());
    }
}
