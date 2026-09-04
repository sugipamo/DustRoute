use std::collections::BTreeSet;

use dustroute_minecraft::time::{PhysicsEngine, PhysicsEngineError, TraceStatus};
use dustroute_minecraft::{
    Block, BlockKind, Facing, PistonAction, PistonState, PistonVariant, Pos, Region, World,
    piston_state,
};
use serde::Deserialize;

const FIXTURE: &str = include_str!("fixtures/reference_3x3_noncompact_piston_shuttle.json");

#[derive(Debug, Deserialize)]
struct ReferenceFixture {
    schema_version: String,
    minecraft_version: String,
    id: String,
    evidence: String,
    components: Components,
    states: States,
    timeline: Vec<TimelineStep>,
    control_contract: ControlContract,
    control_timeline: Vec<ControlTimelineStep>,
    mechanical_replay: MechanicalReplay,
    classification: Vec<Classification>,
    replay: ReplayContract,
}

#[derive(Debug, Deserialize)]
struct Components {
    door_blocks: Vec<Pos>,
    open_pushers: Vec<PistonSpec>,
    close_pushers: Vec<PistonSpec>,
    control_repeaters: ControlRepeaters,
}

#[derive(Debug, Deserialize)]
struct ControlRepeaters {
    open_outputs: Vec<RepeaterSpec>,
    close_outputs: Vec<RepeaterSpec>,
}

#[derive(Debug, Deserialize)]
struct RepeaterSpec {
    position: Pos,
    facing: FacingSpec,
    target_role: String,
}

#[derive(Debug, Deserialize)]
struct PistonSpec {
    position: Pos,
    facing: FacingSpec,
    variant: PistonVariant,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "lowercase")]
enum FacingSpec {
    North,
    South,
}

impl FacingSpec {
    fn facing(&self) -> Facing {
        match self {
            Self::North => Facing::North,
            Self::South => Facing::South,
        }
    }
}

#[derive(Debug, Deserialize)]
struct States {
    closed: StableState,
    open: StableState,
}

#[derive(Debug, Deserialize)]
struct StableState {
    door_block_z: i32,
    open_pushers: String,
    close_pushers: String,
    passage_plane_z: i32,
}

#[derive(Debug, Deserialize)]
struct TimelineStep {
    id: String,
    trigger: String,
    role: String,
    action: PistonAction,
    schedule_game_tick: u64,
    expected_completion_game_tick: u64,
    expected_stable_state: String,
}

#[derive(Debug, Deserialize)]
struct ControlContract {
    input: String,
    lever_edges: Vec<LeverEdge>,
    repeater_count: u64,
    delayed_return_repeater_ticks: u64,
    fanout_channels: u64,
    status: String,
    first_gap: String,
    reason: String,
}

#[derive(Debug, Deserialize)]
struct MechanicalReplay {
    mode: String,
    inter_action_gap_game_ticks: u64,
    same_tick_batch_status: String,
    same_tick_batch_reason: String,
}

#[derive(Debug, Deserialize)]
struct ControlTimelineStep {
    id: String,
    source: String,
    repeater_role: String,
    delay_redstone_ticks: u64,
    target_role: String,
    status: String,
}

#[derive(Debug, Deserialize)]
struct LeverEdge {
    transition: String,
    game_tick: u64,
}

#[derive(Debug, Deserialize)]
struct Classification {
    id: String,
    status: String,
    reason: String,
}

#[derive(Debug, Deserialize)]
struct ReplayContract {
    engine: String,
    method: String,
    profile_initial_delay_game_ticks: TickRange,
    profile_stable_completion_game_ticks: u64,
    mechanical_replay_status: String,
    electrical_replay_status: String,
}

#[derive(Debug, Deserialize)]
struct TickRange {
    minimum: u64,
    maximum: u64,
}

fn fixture() -> ReferenceFixture {
    serde_json::from_str(FIXTURE).expect("reference fixture must be valid JSON")
}

fn build_world(reference: &ReferenceFixture) -> (World, Region) {
    let mut world = World::new();
    for position in &reference.components.door_blocks {
        world.set(*position, Block::new(BlockKind::Solid));
    }
    for piston in reference
        .components
        .open_pushers
        .iter()
        .chain(&reference.components.close_pushers)
    {
        let mut block = Block::new(BlockKind::Piston);
        block.facing = Some(piston.facing.facing());
        block.piston_variant = Some(piston.variant);
        block.piston_state = Some(PistonState::Retracted);
        world.set(piston.position, block);
    }
    // The region includes every ray endpoint used by either direction. Empty
    // coordinates inside it are Air; anything beyond it must remain unknown.
    let known_region = Region::new(Pos::new(-1, -1, -2), Pos::new(3, 3, 3));
    (world, known_region)
}

fn role_positions<'a>(reference: &'a ReferenceFixture, role: &str) -> &'a [PistonSpec] {
    match role {
        "open_push" => &reference.components.open_pushers,
        "close_push" => &reference.components.close_pushers,
        other => panic!("unknown fixture role {other}"),
    }
}

fn assert_panel_position(reference: &ReferenceFixture, engine: &PhysicsEngine, z: i32) {
    for position in &reference.components.door_blocks {
        let expected = Pos::new(position.x, position.y, z);
        assert_eq!(
            engine.world().kind_at(expected),
            BlockKind::Solid,
            "door block should be at {expected:?}"
        );
    }
}

fn assert_passage_clear(reference: &ReferenceFixture, engine: &PhysicsEngine, door_z: i32) {
    let other_z = if door_z == 0 { 1 } else { 0 };
    for position in &reference.components.door_blocks {
        assert_eq!(
            engine
                .world()
                .kind_at(Pos::new(position.x, position.y, other_z)),
            BlockKind::Air,
            "passage should be clear at z={other_z} for {position:?}"
        );
    }
}

fn assert_pusher_state(specs: &[PistonSpec], engine: &PhysicsEngine, state: PistonState) {
    for piston in specs {
        assert_eq!(
            piston_state(engine.world().get(piston.position).expect("piston")),
            state,
            "piston at {:?} has unexpected state",
            piston.position
        );
    }
}

fn assert_stable_step(reference: &ReferenceFixture, engine: &PhysicsEngine, step: &TimelineStep) {
    match step.expected_stable_state.as_str() {
        "open_pushers_extended_door_at_z1" => {
            assert_panel_position(reference, engine, 1);
            assert_pusher_state(
                &reference.components.open_pushers,
                engine,
                PistonState::Extended,
            );
            assert_pusher_state(
                &reference.components.close_pushers,
                engine,
                PistonState::Retracted,
            );
            for piston in &reference.components.open_pushers {
                assert_eq!(
                    engine.world().kind_at(piston.position.offset(0, 0, 1)),
                    BlockKind::PistonHead
                );
            }
        }
        "open" => {
            assert_panel_position(reference, engine, 1);
            assert_passage_clear(reference, engine, 1);
            assert_pusher_state(
                &reference.components.open_pushers,
                engine,
                PistonState::Retracted,
            );
            assert_pusher_state(
                &reference.components.close_pushers,
                engine,
                PistonState::Retracted,
            );
        }
        "close_pushers_extended_door_at_z0" => {
            assert_panel_position(reference, engine, 0);
            assert_pusher_state(
                &reference.components.open_pushers,
                engine,
                PistonState::Retracted,
            );
            assert_pusher_state(
                &reference.components.close_pushers,
                engine,
                PistonState::Extended,
            );
            for piston in &reference.components.close_pushers {
                assert_eq!(
                    engine.world().kind_at(piston.position.offset(0, 0, -1)),
                    BlockKind::PistonHead
                );
            }
        }
        "closed" => {
            assert_panel_position(reference, engine, 0);
            assert_passage_clear(reference, engine, 0);
            assert_pusher_state(
                &reference.components.open_pushers,
                engine,
                PistonState::Retracted,
            );
            assert_pusher_state(
                &reference.components.close_pushers,
                engine,
                PistonState::Retracted,
            );
        }
        other => panic!("unknown expected stable state {other}"),
    }
}

fn run_mechanical_replay(reference: &ReferenceFixture) -> PhysicsEngine {
    let (world, known_region) = build_world(reference);
    let mut engine = PhysicsEngine::new(world, 512).with_piston_planning_region(known_region);
    for step in &reference.timeline {
        let mut next_schedule_tick = step.schedule_game_tick;
        for piston in role_positions(reference, &step.role) {
            // The current delta contract intentionally keeps a strict parent
            // ShapeId.  Serializing each cell avoids inventing a multi-piston
            // batch rebase while still replaying the complete reference.
            if !engine.event_trace().records.is_empty() {
                next_schedule_tick = next_schedule_tick.max(engine.time().game_tick + 1);
            }
            engine.schedule_piston_action(next_schedule_tick, piston.position, step.action);
            engine
                .run_piston_events()
                .unwrap_or_else(|error| panic!("{} replay failed: {error}", step.id));
            next_schedule_tick =
                engine.time().game_tick + reference.mechanical_replay.inter_action_gap_game_ticks;
        }
        assert_eq!(
            engine.time().game_tick,
            step.expected_completion_game_tick,
            "{} completion tick",
            step.id
        );
        assert_stable_step(reference, &engine, step);
    }
    engine
}

#[test]
fn reference_fixture_freezes_a_buildable_two_sided_3x3_mechanism() {
    let reference = fixture();
    assert_eq!(reference.schema_version, "dustroute.reference-3x3-door.v1");
    assert_eq!(reference.minecraft_version, "1.21.11");
    assert_eq!(reference.id, "noncompact_two_sided_normal_piston_shuttle");
    assert_eq!(reference.evidence, "designed_mechanism");

    assert_eq!(reference.components.door_blocks.len(), 9);
    assert_eq!(reference.components.open_pushers.len(), 9);
    assert_eq!(reference.components.close_pushers.len(), 9);
    assert_eq!(reference.components.control_repeaters.open_outputs.len(), 9);
    assert_eq!(
        reference.components.control_repeaters.close_outputs.len(),
        9
    );
    let door_positions: BTreeSet<_> = reference.components.door_blocks.iter().copied().collect();
    assert_eq!(door_positions.len(), 9);
    assert!(
        reference
            .components
            .door_blocks
            .iter()
            .all(|position| position.z == reference.states.closed.door_block_z)
    );
    assert!(reference.components.open_pushers.iter().all(|piston| {
        piston.facing.facing() == Facing::South && piston.variant == PistonVariant::Normal
    }));
    assert!(reference.components.close_pushers.iter().all(|piston| {
        piston.facing.facing() == Facing::North && piston.variant == PistonVariant::Normal
    }));
    assert!(
        reference
            .components
            .control_repeaters
            .open_outputs
            .iter()
            .all(|repeater| repeater.facing.facing() == Facing::South
                && repeater.target_role == "open_push")
    );
    assert!(
        reference
            .components
            .control_repeaters
            .close_outputs
            .iter()
            .all(|repeater| repeater.facing.facing() == Facing::North
                && repeater.target_role == "close_push")
    );
    for (piston, repeater) in reference
        .components
        .open_pushers
        .iter()
        .zip(&reference.components.control_repeaters.open_outputs)
    {
        assert_eq!(repeater.position, piston.position.offset(0, 0, -1));
    }
    for (piston, repeater) in reference
        .components
        .close_pushers
        .iter()
        .zip(&reference.components.control_repeaters.close_outputs)
    {
        assert_eq!(repeater.position, piston.position.offset(0, 0, 1));
    }

    assert_eq!(reference.states.closed.door_block_z, 0);
    assert_eq!(reference.states.closed.passage_plane_z, 0);
    assert_eq!(reference.states.open.door_block_z, 1);
    assert_eq!(reference.states.open.passage_plane_z, 0);
    assert_eq!(reference.states.closed.open_pushers, "retracted");
    assert_eq!(reference.states.closed.close_pushers, "retracted");
    assert_eq!(reference.states.open.open_pushers, "retracted");
    assert_eq!(reference.states.open.close_pushers, "retracted");

    let ids: BTreeSet<_> = reference
        .timeline
        .iter()
        .map(|step| step.id.as_str())
        .collect();
    assert_eq!(
        ids,
        BTreeSet::from([
            "open_extend",
            "open_retract",
            "close_extend",
            "close_retract"
        ])
    );
    assert_eq!(reference.timeline[0].trigger, "lever_on_derived_pulse");
    assert_eq!(reference.timeline[2].trigger, "lever_off_derived_pulse");
    assert_eq!(reference.timeline[0].action, PistonAction::Extend);
    assert_eq!(reference.timeline[1].action, PistonAction::Retract);
    assert_eq!(reference.timeline[2].action, PistonAction::Extend);
    assert_eq!(reference.timeline[3].action, PistonAction::Retract);
    assert_eq!(reference.timeline[0].schedule_game_tick, 0);
    assert_eq!(reference.timeline[1].schedule_game_tick, 27);
    assert_eq!(reference.timeline[2].schedule_game_tick, 54);
    assert_eq!(reference.timeline[3].schedule_game_tick, 81);

    assert_eq!(reference.control_contract.input, "lever");
    assert_eq!(reference.control_contract.lever_edges.len(), 2);
    assert_eq!(
        reference.control_contract.lever_edges[0].transition,
        "off_to_on"
    );
    assert_eq!(reference.control_contract.lever_edges[0].game_tick, 0);
    assert_eq!(
        reference.control_contract.lever_edges[1].transition,
        "on_to_off"
    );
    assert_eq!(reference.control_contract.lever_edges[1].game_tick, 54);
    assert_eq!(reference.control_contract.repeater_count, 18);
    assert_eq!(reference.control_contract.delayed_return_repeater_ticks, 2);
    assert_eq!(reference.control_contract.fanout_channels, 18);
    assert_eq!(reference.control_contract.status, "out_of_scope");
    assert_eq!(
        reference.control_contract.first_gap,
        "pulse_sequencer_and_18_way_fanout"
    );
    assert!(!reference.control_contract.reason.is_empty());
    assert_eq!(reference.control_timeline.len(), 4);
    assert_eq!(
        reference
            .control_timeline
            .iter()
            .map(|step| step.id.as_str())
            .collect::<BTreeSet<_>>(),
        BTreeSet::from([
            "open_extend_repeater_output",
            "open_retract_repeater_output",
            "close_extend_repeater_output",
            "close_retract_repeater_output"
        ])
    );
    assert!(reference.control_timeline.iter().all(|step| {
        !step.source.is_empty()
            && !step.repeater_role.is_empty()
            && !step.target_role.is_empty()
            && step.delay_redstone_ticks >= 1
            && step.status == "out_of_scope"
    }));
    assert_eq!(
        reference.replay.engine,
        "dustroute_minecraft::time::PhysicsEngine"
    );
    assert_eq!(reference.replay.method, "schedule_piston_action");
    assert_eq!(reference.replay.profile_initial_delay_game_ticks.minimum, 0);
    assert_eq!(reference.replay.profile_initial_delay_game_ticks.maximum, 1);
    assert_eq!(reference.replay.profile_stable_completion_game_ticks, 2);
    assert_eq!(
        reference.replay.mechanical_replay_status,
        "supported_serial_only"
    );
    assert_eq!(
        reference.replay.electrical_replay_status,
        "stopped_at_first_gap"
    );
    assert_eq!(reference.mechanical_replay.mode, "serial_per_cell");
    assert_eq!(reference.mechanical_replay.inter_action_gap_game_ticks, 1);
    assert_eq!(
        reference.mechanical_replay.same_tick_batch_status,
        "missing"
    );
    assert!(
        !reference
            .mechanical_replay
            .same_tick_batch_reason
            .is_empty()
    );
}

#[test]
fn reference_mechanical_replay_reaches_open_and_closed_states_deterministically() {
    let reference = fixture();
    let first = run_mechanical_replay(&reference);
    let second = run_mechanical_replay(&reference);

    assert_eq!(first.world(), second.world());
    assert_eq!(first.event_trace(), second.event_trace());
    assert_eq!(first.transition_trace(), second.transition_trace());
    assert_eq!(first.execution_state_key(), second.execution_state_key());
    assert_eq!(first.time().game_tick, 107);
    assert_eq!(first.event_trace().records.len(), 72);
    assert_eq!(first.transition_trace().len(), 72);
    assert_eq!(*first.trace_status(), TraceStatus::Complete);
    assert!(
        first
            .world()
            .iter()
            .filter(|(_, block)| block.kind == BlockKind::Piston)
            .all(|(_, block)| piston_state(block) == PistonState::Retracted)
    );
    assert!(
        first
            .world()
            .iter()
            .find(|(_, block)| block.kind == BlockKind::PistonHead)
            .is_none()
    );
}

#[test]
fn same_tick_nine_piston_batch_fails_closed_without_implicit_rebase() {
    let reference = fixture();
    let (world, known_region) = build_world(&reference);
    let mut engine = PhysicsEngine::new(world, 512).with_piston_planning_region(known_region);
    for piston in &reference.components.open_pushers {
        engine.schedule_piston_action(0, piston.position, PistonAction::Extend);
    }

    let error = engine
        .run_piston_events()
        .expect_err("same-tick completion must remain an explicit missing capability");
    assert!(matches!(error, PhysicsEngineError::WorldDelta(_)));
    assert!(engine.trace_status().is_failed());
    assert_eq!(
        reference.mechanical_replay.same_tick_batch_status,
        "missing"
    );
}

#[test]
fn reference_classification_stops_at_the_declared_electrical_gap() {
    let reference = fixture();
    let statuses: BTreeSet<_> = reference
        .classification
        .iter()
        .map(|item| (item.id.as_str(), item.status.as_str()))
        .collect();
    for id in [
        "open_extend",
        "open_retract",
        "close_extend",
        "close_retract",
    ] {
        assert!(
            statuses.contains(&(id, "supported_serial_only")),
            "{id} must be serially supported"
        );
    }
    assert!(statuses.contains(&("lever_to_pulse_sequence", "out_of_scope")));
    assert!(statuses.contains(&("18_way_repeater_fanout", "missing")));
    assert!(statuses.contains(&("nine_piston_same_tick_batch", "missing")));
    assert!(
        reference
            .classification
            .iter()
            .all(|item| !item.reason.is_empty())
    );
}
