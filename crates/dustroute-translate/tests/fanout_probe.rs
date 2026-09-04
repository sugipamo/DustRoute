use std::collections::BTreeSet;

use dustroute_minecraft::time::{
    BlockEventKind, EventExecutionStatus, PhysicsEngineError, PhysicsEventKind, TraceStatus,
};
use dustroute_minecraft::{Block, BlockKind, PistonAction, PistonState, Pos};
use dustroute_translate::{PistonDoorScenario, PistonDoorScenarioError};
use serde::Deserialize;

const FIXTURE: &str = include_str!("fixtures/3x3_piston_shuttle_fanout.json");
const REFERENCE_FIXTURE: &str =
    include_str!("fixtures/reference_3x3_noncompact_piston_shuttle.json");

#[derive(Debug, Deserialize)]
struct ReferenceFixture {
    components: ReferenceComponents,
}

#[derive(Debug, Deserialize)]
struct ReferenceComponents {
    door_blocks: Vec<Pos>,
    open_pushers: Vec<ReferencePiston>,
    close_pushers: Vec<ReferencePiston>,
}

#[derive(Debug, Deserialize)]
struct ReferencePiston {
    position: Pos,
}

fn scenario() -> PistonDoorScenario {
    PistonDoorScenario::from_json(FIXTURE).expect("3x3 fanout fixture must be valid")
}

fn reference_fixture() -> ReferenceFixture {
    serde_json::from_str(REFERENCE_FIXTURE)
        .expect("reference 3x3 piston fixture must be valid JSON")
}

fn piston_position(cell: &dustroute_translate::PistonDoorCell, z: i32) -> Pos {
    Pos::new(cell.x, cell.y, z)
}

fn assert_door_state(
    scenario: &PistonDoorScenario,
    engine: &dustroute_minecraft::time::PhysicsEngine,
    z: i32,
) {
    for cell in &scenario.cells {
        assert_eq!(
            engine.world().kind_at(piston_position(cell, z)),
            BlockKind::Solid,
            "door block should be at ({}, {}, {})",
            cell.x,
            cell.y,
            z
        );
        let other_z = if z == scenario.expected.closed_door_z {
            scenario.expected.open_door_z
        } else {
            scenario.expected.closed_door_z
        };
        assert_eq!(
            engine.world().kind_at(piston_position(cell, other_z)),
            BlockKind::Air,
            "the other door plane should be empty for ({}, {})",
            cell.x,
            cell.y
        );
    }
}

fn assert_retracted(
    scenario: &PistonDoorScenario,
    engine: &dustroute_minecraft::time::PhysicsEngine,
) {
    for cell in &scenario.cells {
        for z in [
            scenario.expected.closed_door_z - 1,
            scenario.expected.open_door_z + 1,
        ] {
            assert_eq!(
                engine
                    .world()
                    .get(piston_position(cell, z))
                    .map(dustroute_minecraft::piston_state),
                Some(PistonState::Retracted),
                "piston at ({}, {}, {}) should be retracted",
                cell.x,
                cell.y,
                z
            );
        }
    }
}

fn assert_open(scenario: &PistonDoorScenario, engine: &dustroute_minecraft::time::PhysicsEngine) {
    assert_door_state(scenario, engine, scenario.expected.open_door_z);
    assert_retracted(scenario, engine);
    assert_eq!(
        engine.pending_event_count(),
        scenario.expected.pending_events_after_settle
    );
    assert_eq!(*engine.trace_status(), TraceStatus::Complete);
}

fn assert_closed(scenario: &PistonDoorScenario, engine: &dustroute_minecraft::time::PhysicsEngine) {
    assert_door_state(scenario, engine, scenario.expected.closed_door_z);
    assert_retracted(scenario, engine);
    assert_eq!(
        engine.pending_event_count(),
        scenario.expected.pending_events_after_settle
    );
    assert_eq!(*engine.trace_status(), TraceStatus::Complete);
}

fn piston_block_event_times(
    engine: &dustroute_minecraft::time::PhysicsEngine,
    action: BlockEventKind,
) -> Vec<(Pos, u64)> {
    engine
        .event_trace()
        .records
        .iter()
        .filter_map(|record| match &record.event.kind {
            PhysicsEventKind::BlockEvent { event } if *event == action => {
                Some((record.event.target, record.event.time.game_tick))
            }
            _ => None,
        })
        .collect()
}

#[test]
fn fixture_is_loaded_and_validated_by_the_common_scenario_api() {
    let scenario = scenario();
    assert_eq!(
        scenario.schema_version,
        "dustroute.3x3-piston-shuttle-fanout.v1"
    );
    assert_eq!(scenario.minecraft_version, "1.21.11");
    assert_eq!(
        scenario.id,
        "lever_controlled_one_to_three_to_nine_piston_shuttle"
    );
    assert_eq!(scenario.evidence, "designed_control_contract");
    assert_eq!(scenario.cells.len(), 9);
    assert_eq!(scenario.control.fanout_levels, [1, 3, 9]);
    assert_eq!(scenario.control.row_order, [0, 1, 2]);
    assert_eq!(scenario.control.open_source, Pos::new(-22, 1, -6));
    assert_eq!(scenario.control.close_source, Pos::new(-22, 1, 7));
    assert_eq!(scenario.control.branch_lane_z_open, [-7, -6, -9]);
    assert_eq!(scenario.control.branch_lane_z_close, [8, 7, 10]);
    assert_eq!(scenario.control.trunk_repeater_counts, [9, 15, 21]);
    assert!(
        scenario
            .scope
            .out_of_scope
            .iter()
            .any(|item| item == "same_tick_multi_piston_completion_rebase")
    );
}

#[test]
fn fanout_cells_match_the_reference_3x3_mechanical_fixture() {
    let scenario = scenario();
    let reference = reference_fixture();
    let door_positions = scenario
        .cells
        .iter()
        .map(|cell| Pos::new(cell.x, cell.y, scenario.expected.closed_door_z))
        .collect::<BTreeSet<_>>();
    let open_positions = scenario
        .cells
        .iter()
        .map(|cell| Pos::new(cell.x, cell.y, scenario.expected.closed_door_z - 1))
        .collect::<BTreeSet<_>>();
    let close_positions = scenario
        .cells
        .iter()
        .map(|cell| Pos::new(cell.x, cell.y, scenario.expected.open_door_z + 1))
        .collect::<BTreeSet<_>>();
    assert_eq!(
        door_positions,
        reference
            .components
            .door_blocks
            .into_iter()
            .collect::<BTreeSet<_>>()
    );
    assert_eq!(
        open_positions,
        reference
            .components
            .open_pushers
            .into_iter()
            .map(|piston| piston.position)
            .collect::<BTreeSet<_>>()
    );
    assert_eq!(
        close_positions,
        reference
            .components
            .close_pushers
            .into_iter()
            .map(|piston| piston.position)
            .collect::<BTreeSet<_>>()
    );
}

#[test]
fn common_runner_drives_all_nine_cells_open_through_two_level_fanout() {
    let scenario = scenario();
    let engine = scenario.run_open().expect("open fanout should settle");
    assert_open(&scenario, &engine);
    assert_eq!(
        engine.world().get(scenario.control.lever).unwrap().powered,
        Some(true)
    );
    assert_eq!(
        engine
            .event_trace()
            .records
            .iter()
            .filter(|record| {
                record.event.target == scenario.control.open_source
                    && matches!(
                        record.event.kind,
                        PhysicsEventKind::RedstoneInput { powered: true }
                    )
            })
            .count(),
        1
    );
    for cell in &scenario.cells {
        let final_repeater = Pos::new(cell.x, cell.y, scenario.control.leaf_repeater_z_open[3]);
        assert!(engine.event_trace().records.iter().any(|record| {
            record.event.target == final_repeater
                && matches!(
                    record.event.kind,
                    PhysicsEventKind::RepeaterTick {
                        expected_powered: true
                    }
                )
        }));
        assert!(engine.transition_trace().records.iter().any(|record| {
            record.changes.iter().any(|change| {
                change.position == piston_position(cell, scenario.expected.open_door_z)
                    && change.after.kind == BlockKind::Solid
            })
        }));
    }
}

#[test]
fn common_runner_keeps_piston_starts_and_completions_serial() {
    let scenario = scenario();
    let engine = scenario.run_open().expect("open fanout should settle");
    let starts = piston_block_event_times(&engine, BlockEventKind::PistonExtend);
    assert_eq!(starts.len(), scenario.cells.len());
    assert_eq!(
        starts
            .iter()
            .map(|(position, _)| *position)
            .collect::<Vec<_>>(),
        scenario
            .cells
            .iter()
            .map(|cell| piston_position(cell, scenario.expected.closed_door_z - 1))
            .collect::<Vec<_>>()
    );
    assert!(starts.windows(2).all(|window| window[0].1 < window[1].1));
    let completions = engine
        .event_trace()
        .records
        .iter()
        .filter_map(|record| match &record.event.kind {
            PhysicsEventKind::PistonComplete {
                action: PistonAction::Extend,
                ..
            } => Some(record.event.time.game_tick),
            _ => None,
        })
        .collect::<Vec<_>>();
    assert_eq!(completions.len(), scenario.cells.len());
    assert!(completions.windows(2).all(|window| window[0] < window[1]));
}

#[test]
fn common_runner_round_trips_all_nine_cells() {
    let scenario = scenario();
    let engine = scenario.run_cycle().expect("fanout cycle should settle");
    assert_closed(&scenario, &engine);
    assert_eq!(
        engine.world().get(scenario.control.lever).unwrap().powered,
        Some(false)
    );
    assert_eq!(
        engine
            .event_trace()
            .records
            .iter()
            .filter(|record| {
                record.event.target == scenario.control.close_source
                    && matches!(
                        record.event.kind,
                        PhysicsEventKind::RedstoneInput { powered: true }
                    )
            })
            .count(),
        1
    );
}

#[test]
fn translated_scenario_reuses_the_same_execution_path() {
    let scenario = scenario().translated(Pos::new(37, 11, 29));
    let engine = scenario
        .run_cycle()
        .expect("translated fanout should settle");
    assert_closed(&scenario, &engine);
    assert_eq!(scenario.control.lever, Pos::new(15, 15, 29));
    assert_eq!(scenario.cells[0].x, 37);
    assert_eq!(scenario.cells[0].y, 11);
}

#[test]
fn fanout_replay_is_deterministic() {
    let scenario = scenario();
    let first = scenario.run_cycle().unwrap();
    let second = scenario.run_cycle().unwrap();
    assert_eq!(first.world(), second.world());
    assert_eq!(first.event_trace(), second.event_trace());
    assert_eq!(first.transition_trace(), second.transition_trace());
}

#[test]
fn stable_lever_edge_is_idempotent_after_open() {
    let scenario = scenario();
    let mut engine = scenario.run_open().unwrap();
    let transitions_before = engine.transition_trace().len();
    let events_before = engine.event_trace().len();
    let event_id = engine.schedule_lever_pulse_sequence(
        engine.time().game_tick + 1,
        scenario.control.lever,
        true,
        [scenario.control.open_source],
        [scenario.control.close_source],
        scenario.control.pulse_width_game_ticks,
    );
    engine.run_redstone_propagation().unwrap();
    assert_open(&scenario, &engine);
    assert_eq!(engine.transition_trace().len(), transitions_before);
    assert_eq!(
        engine.event_trace().records[events_before].event.id,
        event_id
    );
    assert_eq!(
        engine.event_trace().records[events_before].status,
        EventExecutionStatus::NoTransition
    );
}

#[test]
fn malformed_source_fails_closed_before_lever_mutation() {
    let scenario = scenario();
    let materialized = scenario.build_world().unwrap();
    let (mut world, known_region) = materialized.into_parts();
    let mut unobserved_source = Block::new(BlockKind::RedstoneBlock);
    unobserved_source.powered = Some(false);
    let invalid_source = Pos::new(50, 1, 0);
    world.set(invalid_source, unobserved_source);
    let before = world.clone();
    let mut engine = dustroute_minecraft::time::PhysicsEngine::new(world, 256)
        .with_piston_planning_region(known_region);
    let event_id = engine.schedule_lever_pulse_sequence(
        0,
        scenario.control.lever,
        true,
        [invalid_source],
        [scenario.control.close_source],
        scenario.control.pulse_width_game_ticks,
    );
    let error = engine
        .run_redstone_propagation()
        .expect_err("an unobserved source must fail closed");
    assert!(matches!(
        error,
        PhysicsEngineError::Redstone(error)
            if matches!(
                error.as_ref(),
                dustroute_minecraft::RedstonePropagationError::UnknownState { .. }
            )
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
fn malformed_scenario_is_rejected_without_falling_back_to_a_different_layout() {
    let mut scenario = scenario();
    scenario.control.leaf_delay_redstone_ticks[0][0][0] = 0;
    let error = scenario
        .run_open()
        .expect_err("zero repeater delay must fail closed");
    assert!(matches!(error, PistonDoorScenarioError::Invalid { .. }));
}
