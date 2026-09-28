use dustroute_minecraft::execution_context::{
    InitializationPolicy, InputPolicy, LawRole, WorldExecutionContext, WorldExecutionProfile,
};
use dustroute_minecraft::time::{
    PhysicsEngine, PhysicsEventKind, SchedulerProfile, ZeroDelayPolicy,
};
use dustroute_minecraft::{Block, BlockKind, Facing, PistonMotionProfile, Pos, Region, World};
use dustroute_translate::sim::RedstoneTickSimulator;

fn circuit() -> World {
    let mut world = World::new();
    world.fill(
        Pos::new(-1, 0, -1),
        Pos::new(3, 0, 1),
        Block::new(BlockKind::Solid),
    );
    let lever = world.place(BlockKind::Lever, Pos::new(-1, 1, 0));
    lever.powered = Some(false);
    lever.support_offset = Some(Pos::new(0, -1, 0));
    world.place(BlockKind::RedstoneWire, Pos::new(0, 1, 0));
    let repeater = world.place(BlockKind::Repeater, Pos::new(1, 1, 0));
    repeater.facing = Some(Facing::East);
    repeater.delay = Some(2);
    repeater.powered = Some(false);
    world.place(BlockKind::RedstoneLamp, Pos::new(2, 1, 0));
    world
}

#[test]
fn context_construction_preserves_compatibility_states_events_and_driver_lifecycle() {
    let mut context =
        WorldExecutionContext::for_profile(WorldExecutionProfile::RedstoneCompatibilityBoundaryV1);
    let mut scheduler = SchedulerProfile::default();
    scheduler.phase_order.swap(2, 3);
    context.scheduler = Some(scheduler);
    let mut original = RedstoneTickSimulator::new(circuit())
        .unwrap()
        .with_scheduler_profile(scheduler);
    let mut selected = RedstoneTickSimulator::new_in_context(circuit(), &context).unwrap();
    let untouched = selected.clone();
    assert_eq!(selected.execution_context(), context);
    for powered in [true, false, true, true, false] {
        assert_eq!(
            original.set_powered(Pos::new(-1, 1, 0), powered).unwrap(),
            selected.set_powered(Pos::new(-1, 1, 0), powered).unwrap()
        );
        for _ in 0..20 {
            assert_eq!(
                original.step_event().unwrap(),
                selected.step_event().unwrap()
            );
        }
        assert_eq!(original.snapshot(), selected.snapshot());
        assert_eq!(original.has_pending_events(), selected.has_pending_events());
    }
    // A shared immutable adapter owns no mutable device state across worlds.
    assert_eq!(
        untouched.snapshot(),
        RedstoneTickSimulator::new(circuit()).unwrap().snapshot()
    );
    assert_eq!(untouched.pending_scheduler_events(), 0);
}

#[test]
fn bounded_context_retains_checkpoint_work_and_existing_scheduler_settings() {
    let scheduler = SchedulerProfile {
        zero_delay: ZeroDelayPolicy::NextGameTick,
        ..SchedulerProfile::default()
    };
    let motion = PistonMotionProfile {
        movement_game_ticks: 4,
        ..PistonMotionProfile::default()
    };
    let mut context =
        WorldExecutionContext::for_profile(WorldExecutionProfile::BoundedRedstoneEventsV1);
    context.scheduler = Some(scheduler);
    context.piston_motion = Some(motion);
    let new = || {
        PhysicsEngine::new_diagnostic(circuit(), 256)
            .with_piston_planning_region(Region::new(Pos::new(-3, -1, -3), Pos::new(5, 3, 3)))
    };
    let mut original = new()
        .with_scheduler_profile(scheduler)
        .with_piston_motion_profile(motion);
    let mut selected = new().with_execution_context(&context).unwrap();
    assert_eq!(selected.execution_context(), context);
    for engine in [&mut original, &mut selected] {
        engine.schedule_redstone_input(0, Pos::new(-1, 1, 0), true);
        engine.schedule_external(
            1,
            Pos::new(0, 1, 0),
            PhysicsEventKind::NeighborUpdate {
                source: Pos::new(-1, 1, 0),
            },
        );
    }
    let checkpoint = selected.checkpoint();
    let key = selected.execution_state_key();
    original.run_redstone_propagation().unwrap();
    selected.run_redstone_propagation().unwrap();
    assert_eq!(original.world(), selected.world());
    assert_eq!(original.event_trace(), selected.event_trace());
    assert_eq!(
        original.execution_state_key(),
        selected.execution_state_key()
    );
    selected.restore(&checkpoint);
    assert_eq!(selected.execution_state_key(), key);
    assert_eq!(selected.execution_context(), context);
    selected.run_redstone_propagation().unwrap();
    assert_eq!(original.event_trace(), selected.event_trace());
}

#[test]
fn mismatched_profiles_pins_initialization_and_invalid_order_are_rejected() {
    use WorldExecutionProfile::*;
    for profile in [
        DustTorchSynchronousGameTickV1,
        DustSingleTorchBlockEffectsV1,
        RedstoneCompatibilityBoundaryV1,
        BoundedRedstoneEventsV1,
        UnifiedPistonElectricalCallbacksJava12111V16,
    ] {
        let context = WorldExecutionContext::for_profile(profile);
        if profile != BoundedRedstoneEventsV1 {
            assert!(
                PhysicsEngine::new_diagnostic(World::new(), 8)
                    .with_execution_context(&context)
                    .is_err()
            );
        }
        if profile != RedstoneCompatibilityBoundaryV1 {
            assert!(RedstoneTickSimulator::new_in_context(World::new(), &context).is_err());
        }
        let mut changed = context.clone();
        changed.initialization = if context.is_proof_profile() {
            InitializationPolicy::CompatibilityDeviceSeeds
        } else {
            InitializationPolicy::FreshTorchConstruction
        };
        assert!(changed.validate().is_err());
        changed = context.clone();
        changed.inputs = if context.is_proof_profile() {
            InputPolicy::ExplicitScheduledWorldEvents
        } else {
            InputPolicy::PhysicalLeversBetweenModelSteps
        };
        assert!(changed.validate().is_err());
        changed = context.clone();
        changed.laws.remove(&LawRole::BlockTraits);
        assert!(changed.validate().is_err());
        changed = context.clone();
        if context.is_proof_profile() {
            changed.laws.insert(
                LawRole::PistonState,
                "dustroute.law.piston.state.bounded-v1".into(),
            );
            assert!(changed.validate().is_err());
        } else {
            changed.max_electrical_iterations = Some(42);
            assert!(changed.validate().is_err());
        }
        if let Some(mut motion) = context.piston_motion {
            motion.initial_delay_min_game_ticks = motion.initial_delay_max_game_ticks + 1;
            changed = context.clone();
            changed.piston_motion = Some(motion);
            assert!(
                PhysicsEngine::new_diagnostic(World::new(), 8)
                    .with_execution_context(&changed)
                    .is_err()
            );
        }
        if context.scheduler.is_some() {
            changed = context.clone();
            let scheduler = changed.scheduler.as_mut().unwrap();
            scheduler.phase_order[0] = scheduler.phase_order[1];
            assert!(changed.validate().is_err());
            if profile == BoundedRedstoneEventsV1 {
                assert!(
                    PhysicsEngine::new_diagnostic(World::new(), 8)
                        .with_execution_context(&changed)
                        .is_err()
                );
            } else {
                assert!(RedstoneTickSimulator::new_in_context(World::new(), &changed).is_err());
            }
        }
    }
}
