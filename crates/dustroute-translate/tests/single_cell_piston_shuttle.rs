use dustroute_minecraft::time::{
    EventExecutionStatus, PhysicsEngine, PhysicsEventKind, TraceStatus,
};
use dustroute_minecraft::{
    Block, BlockKind, Facing, PistonState, Pos, Region, World, piston_state,
};
use serde::Deserialize;

const FIXTURE: &str = include_str!("fixtures/single_cell_piston_shuttle.json");

#[derive(Debug, Deserialize)]
struct Fixture {
    schema_version: String,
    minecraft_version: String,
    id: String,
    evidence: String,
    coordinates: Coordinates,
    piston_facing: PistonFacing,
    repeater_delay_redstone_ticks: u8,
    pulse_width_game_ticks: u64,
    expected: Expected,
    scope: Scope,
}

#[derive(Debug, Deserialize)]
struct Coordinates {
    lever: Pos,
    door_block_closed: Pos,
    door_block_open: Pos,
    open_piston: Pos,
    open_repeater: Pos,
    open_wire: Pos,
    open_source: Pos,
    close_piston: Pos,
    close_repeater: Pos,
    close_wire: Pos,
    close_source: Pos,
}

#[derive(Debug, Deserialize)]
struct PistonFacing {
    open: Facing,
    close: Facing,
}

#[derive(Debug, Deserialize)]
struct Expected {
    closed_lever_powered: bool,
    open_lever_powered: bool,
    stable_pistons_retracted: bool,
    pending_events_after_settle: usize,
    open_settle_game_tick: u64,
    closed_settle_game_tick: u64,
}

#[derive(Debug, Deserialize)]
struct Scope {
    supported: Vec<String>,
    out_of_scope: Vec<String>,
}

fn fixture() -> Fixture {
    serde_json::from_str(FIXTURE).expect("single-cell fixture must be valid JSON")
}

fn build_world(fixture: &Fixture) -> (World, Region) {
    let coordinates = &fixture.coordinates;
    let mut world = World::new();
    for z in -4..=5 {
        world.set(Pos::new(0, 0, z), Block::new(BlockKind::Solid));
    }
    world.set(Pos::new(2, 0, 0), Block::new(BlockKind::Solid));

    let mut lever = Block::new(BlockKind::Lever);
    lever.powered = Some(fixture.expected.closed_lever_powered);
    world.set(coordinates.lever, lever);
    world.set(coordinates.door_block_closed, Block::new(BlockKind::Solid));

    let mut open_piston = Block::new(BlockKind::Piston);
    open_piston.facing = Some(fixture.piston_facing.open);
    open_piston.piston_state = Some(PistonState::Retracted);
    world.set(coordinates.open_piston, open_piston);

    let mut close_piston = Block::new(BlockKind::Piston);
    close_piston.facing = Some(fixture.piston_facing.close);
    close_piston.piston_state = Some(PistonState::Retracted);
    world.set(coordinates.close_piston, close_piston);

    let mut open_repeater = Block::new(BlockKind::Repeater);
    open_repeater.facing = Some(Facing::South);
    open_repeater.delay = Some(fixture.repeater_delay_redstone_ticks);
    open_repeater.powered = Some(false);
    world.set(coordinates.open_repeater, open_repeater);
    world.set(coordinates.open_wire, Block::new(BlockKind::RedstoneWire));

    let mut close_repeater = Block::new(BlockKind::Repeater);
    close_repeater.facing = Some(Facing::North);
    close_repeater.delay = Some(fixture.repeater_delay_redstone_ticks);
    close_repeater.powered = Some(false);
    world.set(coordinates.close_repeater, close_repeater);
    world.set(coordinates.close_wire, Block::new(BlockKind::RedstoneWire));

    let mut open_source = Block::new(BlockKind::RedstoneBlock);
    open_source.powered = Some(false);
    world.set(coordinates.open_source, open_source);
    let mut close_source = Block::new(BlockKind::RedstoneBlock);
    close_source.powered = Some(false);
    world.set(coordinates.close_source, close_source);

    // Include all source, repeater, piston, and one-cell movement endpoints in
    // the complete observation region. Coordinates outside remain unknown and
    // must still fail closed.
    let known_region = Region::new(Pos::new(-1, 0, -5), Pos::new(3, 2, 6));
    (world, known_region)
}

fn assert_piston_retracted(fixture: &Fixture, engine: &PhysicsEngine) {
    for position in [
        fixture.coordinates.open_piston,
        fixture.coordinates.close_piston,
    ] {
        assert_eq!(
            piston_state(engine.world().get(position).expect("piston")),
            PistonState::Retracted,
            "piston at {position:?} should be retracted"
        );
    }
}

fn assert_sources_low(fixture: &Fixture, engine: &PhysicsEngine) {
    for position in [
        fixture.coordinates.open_source,
        fixture.coordinates.close_source,
    ] {
        assert_eq!(
            engine.world().get(position).and_then(|block| block.powered),
            Some(false),
            "pulse source at {position:?} should be low after settling"
        );
    }
}

fn assert_open(fixture: &Fixture, engine: &PhysicsEngine) {
    assert_eq!(
        engine
            .world()
            .kind_at(fixture.coordinates.door_block_closed),
        BlockKind::Air
    );
    assert_eq!(
        engine.world().kind_at(fixture.coordinates.door_block_open),
        BlockKind::Solid
    );
    assert_piston_retracted(fixture, engine);
    assert_sources_low(fixture, engine);
    assert_eq!(
        engine
            .world()
            .get(fixture.coordinates.lever)
            .and_then(|block| block.powered),
        Some(fixture.expected.open_lever_powered)
    );
}

fn assert_closed(fixture: &Fixture, engine: &PhysicsEngine) {
    assert_eq!(
        engine
            .world()
            .kind_at(fixture.coordinates.door_block_closed),
        BlockKind::Solid
    );
    assert_eq!(
        engine.world().kind_at(fixture.coordinates.door_block_open),
        BlockKind::Air
    );
    assert_piston_retracted(fixture, engine);
    assert_sources_low(fixture, engine);
    assert_eq!(
        engine
            .world()
            .get(fixture.coordinates.lever)
            .and_then(|block| block.powered),
        Some(fixture.expected.closed_lever_powered)
    );
}

fn assert_motion_trace(engine: &PhysicsEngine, piston: Pos, head: Pos) {
    let records = &engine.transition_trace().records;
    assert!(records.iter().any(|record| {
        record.changes.iter().any(|change| {
            change.position == piston && change.after.piston_state == Some(PistonState::Extending)
        })
    }));
    assert!(records.iter().any(|record| {
        record
            .changes
            .iter()
            .any(|change| change.position == head && change.after.kind == BlockKind::MovingPiston)
    }));
    assert!(records.iter().any(|record| {
        record
            .changes
            .iter()
            .any(|change| change.position == head && change.after.kind == BlockKind::PistonHead)
    }));
    assert!(records.iter().any(|record| {
        record.changes.iter().any(|change| {
            change.position == piston && change.after.piston_state == Some(PistonState::Retracting)
        })
    }));
}

fn assert_pulse_edges(engine: &PhysicsEngine, source: Pos, high_tick: u64, low_tick: u64) {
    assert!(engine.event_trace().records.iter().any(|record| {
        record.event.target == source
            && record.event.time.game_tick == high_tick
            && matches!(
                record.event.kind,
                PhysicsEventKind::RedstoneInput { powered: true }
            )
    }));
    assert!(engine.event_trace().records.iter().any(|record| {
        record.event.target == source
            && record.event.time.game_tick == low_tick
            && matches!(
                record.event.kind,
                PhysicsEventKind::RedstoneInput { powered: false }
            )
    }));
}

#[test]
fn fixture_freezes_the_single_cell_control_contract() {
    let fixture = fixture();
    assert_eq!(
        fixture.schema_version,
        "dustroute.single-cell-piston-shuttle.v1"
    );
    assert_eq!(fixture.minecraft_version, "1.21.11");
    assert_eq!(
        fixture.id,
        "lever_controlled_single_cell_normal_piston_shuttle"
    );
    assert_eq!(fixture.evidence, "designed_control_contract");
    assert_eq!(fixture.repeater_delay_redstone_ticks, 1);
    assert!(fixture.pulse_width_game_ticks > 0);
    assert!(fixture.expected.stable_pistons_retracted);
    assert_eq!(fixture.expected.pending_events_after_settle, 0);
    assert!(fixture.expected.open_settle_game_tick < fixture.expected.closed_settle_game_tick);
    assert!(
        fixture
            .scope
            .supported
            .iter()
            .any(|item| item == "wire_to_repeater_to_normal_piston")
    );
    for item in [
        "observer",
        "comparator",
        "repeater_lock",
        "qc_bud",
        "nine_cell_fanout",
    ] {
        assert!(
            fixture
                .scope
                .out_of_scope
                .iter()
                .any(|candidate| candidate == item)
        );
    }
    assert_eq!(fixture.piston_facing.open, Facing::South);
    assert_eq!(fixture.piston_facing.close, Facing::North);
}

#[test]
fn lever_on_drives_one_cell_open_pulse_through_wire_repeater_and_piston() {
    let fixture = fixture();
    let (world, known_region) = build_world(&fixture);
    let mut engine = PhysicsEngine::new(world, 256).with_piston_planning_region(known_region);
    let input_id = engine.schedule_lever_pulse_sequence(
        0,
        fixture.coordinates.lever,
        true,
        [fixture.coordinates.open_source],
        [fixture.coordinates.close_source],
        fixture.pulse_width_game_ticks,
    );

    engine
        .run_redstone_propagation()
        .expect("lever ON pulse should complete");

    assert_open(&fixture, &engine);
    assert_eq!(
        engine.time().game_tick,
        fixture.expected.open_settle_game_tick
    );
    assert_pulse_edges(
        &engine,
        fixture.coordinates.open_source,
        0,
        fixture.pulse_width_game_ticks,
    );
    assert_motion_trace(
        &engine,
        fixture.coordinates.open_piston,
        fixture.coordinates.door_block_closed,
    );
    assert_eq!(
        engine.pending_event_count(),
        fixture.expected.pending_events_after_settle
    );
    assert_eq!(*engine.trace_status(), TraceStatus::Complete);
    assert!(engine.event_trace().records.iter().any(|record| {
        record.event.id == input_id
            && matches!(
                record.event.kind,
                PhysicsEventKind::LeverPulseSequence { powered: true, .. }
            )
    }));
    assert!(engine.event_trace().records.iter().any(|record| {
        matches!(
            record.event.kind,
            PhysicsEventKind::RepeaterTick {
                expected_powered: true
            }
        )
    }));
    assert!(engine.event_trace().records.iter().any(|record| {
        matches!(
            record.event.kind,
            PhysicsEventKind::BlockEvent {
                event: dustroute_minecraft::time::BlockEventKind::PistonExtend
            }
        )
    }));
    assert!(
        engine
            .event_trace()
            .records
            .iter()
            .any(|record| { matches!(record.status, EventExecutionStatus::Transition { .. }) })
    );
}

#[test]
fn lever_off_drives_the_return_pulse_and_round_trips_to_closed() {
    let fixture = fixture();
    let (world, known_region) = build_world(&fixture);
    let mut engine = PhysicsEngine::new(world, 512).with_piston_planning_region(known_region);

    engine.schedule_lever_pulse_sequence(
        0,
        fixture.coordinates.lever,
        true,
        [fixture.coordinates.open_source],
        [fixture.coordinates.close_source],
        fixture.pulse_width_game_ticks,
    );
    engine.run_redstone_propagation().unwrap();
    let off_tick = engine.time().game_tick + 1;
    let off_id = engine.schedule_lever_pulse_sequence(
        off_tick,
        fixture.coordinates.lever,
        false,
        [fixture.coordinates.open_source],
        [fixture.coordinates.close_source],
        fixture.pulse_width_game_ticks,
    );

    engine
        .run_redstone_propagation()
        .expect("lever OFF pulse should complete");

    assert_closed(&fixture, &engine);
    assert_eq!(
        engine.time().game_tick,
        fixture.expected.closed_settle_game_tick
    );
    assert_pulse_edges(
        &engine,
        fixture.coordinates.close_source,
        off_tick,
        off_tick + fixture.pulse_width_game_ticks,
    );
    assert_motion_trace(
        &engine,
        fixture.coordinates.close_piston,
        fixture.coordinates.door_block_open,
    );
    assert_eq!(
        engine.pending_event_count(),
        fixture.expected.pending_events_after_settle
    );
    assert_eq!(*engine.trace_status(), TraceStatus::Complete);
    assert!(engine.event_trace().records.iter().any(|record| {
        record.event.id == off_id
            && matches!(
                record.event.kind,
                PhysicsEventKind::LeverPulseSequence { powered: false, .. }
            )
    }));
    assert!(engine.event_trace().records.iter().any(|record| {
        matches!(
            record.event.kind,
            PhysicsEventKind::BlockEvent {
                event: dustroute_minecraft::time::BlockEventKind::PistonRetract
            }
        )
    }));
}

#[test]
fn stable_lever_edge_is_idempotent_without_a_second_pulse() {
    let fixture = fixture();
    let (world, known_region) = build_world(&fixture);
    let mut engine = PhysicsEngine::new(world, 512).with_piston_planning_region(known_region);
    engine.schedule_lever_pulse_sequence(
        0,
        fixture.coordinates.lever,
        true,
        [fixture.coordinates.open_source],
        [fixture.coordinates.close_source],
        fixture.pulse_width_game_ticks,
    );
    engine.run_redstone_propagation().unwrap();
    assert_open(&fixture, &engine);
    let transitions_before = engine.transition_trace().len();
    let events_before = engine.event_trace().len();

    let repeat_id = engine.schedule_lever_pulse_sequence(
        engine.time().game_tick + 1,
        fixture.coordinates.lever,
        true,
        [fixture.coordinates.open_source],
        [fixture.coordinates.close_source],
        fixture.pulse_width_game_ticks,
    );
    engine.run_redstone_propagation().unwrap();

    assert_open(&fixture, &engine);
    assert_eq!(engine.transition_trace().len(), transitions_before);
    assert_eq!(
        engine.event_trace().records[events_before].event.id,
        repeat_id
    );
    assert_eq!(
        engine.event_trace().records[events_before].status,
        EventExecutionStatus::NoTransition
    );
    assert_eq!(
        engine.pending_event_count(),
        fixture.expected.pending_events_after_settle
    );
}

#[test]
fn malformed_pulse_width_fails_closed_before_changing_the_world() {
    let fixture = fixture();
    let (world, known_region) = build_world(&fixture);
    let before = world.clone();
    let mut engine = PhysicsEngine::new(world, 64).with_piston_planning_region(known_region);
    let event_id = engine.schedule_lever_pulse_sequence(
        0,
        fixture.coordinates.lever,
        true,
        [fixture.coordinates.open_source],
        [fixture.coordinates.close_source],
        0,
    );

    let error = engine
        .run_redstone_propagation()
        .expect_err("zero-width pulses must be rejected");
    assert!(matches!(
        error,
        dustroute_minecraft::time::PhysicsEngineError::InvalidLeverPulseSequence { .. }
    ));
    assert_eq!(engine.world(), &before);
    assert_eq!(engine.pending_event_count(), 1);
    assert_eq!(engine.event_trace().records.len(), 0);
    assert_eq!(
        engine
            .checkpoint()
            .pending_events()
            .next()
            .map(|event| event.id),
        Some(event_id)
    );
    assert!(engine.trace_status().is_failed());
}

#[test]
fn lever_pulse_sequence_is_not_accepted_by_the_direct_piston_runner() {
    let fixture = fixture();
    let (world, known_region) = build_world(&fixture);
    let mut engine = PhysicsEngine::new(world, 64).with_piston_planning_region(known_region);
    let event_id = engine.schedule_lever_pulse_sequence(
        0,
        fixture.coordinates.lever,
        true,
        [fixture.coordinates.open_source],
        [fixture.coordinates.close_source],
        fixture.pulse_width_game_ticks,
    );

    let error = engine
        .run_redstone_piston_events()
        .expect_err("pulse sequences require the propagation runner");
    assert!(matches!(
        error,
        dustroute_minecraft::time::PhysicsEngineError::UnsupportedEvent {
            kind: PhysicsEventKind::LeverPulseSequence { .. },
            ..
        }
    ));
    assert_eq!(engine.pending_event_count(), 1);
    assert_eq!(
        engine
            .checkpoint()
            .pending_events()
            .next()
            .map(|event| event.id),
        Some(event_id)
    );
    assert_eq!(engine.event_trace().records.len(), 0);
}
