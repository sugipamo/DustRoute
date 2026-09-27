#[path = "support/comparator_models.rs"]
mod comparator_models;

use dustroute_minecraft::{Block, BlockKind, Facing, Pos, RotationY, World};
use dustroute_translate::sim::{RedstoneTickSimulator, SimulationEventKind};

#[test]
fn comparator_traces_match_the_pre_migration_capture() {
    let expected: Vec<serde_json::Value> = include_str!("fixtures/comparator_model_v1.jsonl")
        .lines()
        .map(|line| serde_json::from_str(line).unwrap())
        .collect();
    let actual = comparator_models::capture();
    assert_eq!(actual.len(), 48);
    assert_eq!(actual.len(), expected.len());
    for (actual, expected) in actual.iter().zip(expected) {
        assert_eq!(
            *actual, expected,
            "mode={}, initial={}, rotation={}",
            actual["mode"], actual["initial"], actual["rotation"]
        );
    }
}

#[test]
fn sampled_inputs_and_mode_commit_at_the_existing_event_boundary() {
    let main = Pos::new(0, 1, 0);
    let mut changed = comparator_models::world(Some("compare"), 0, RotationY::R0)
        .get(main)
        .unwrap()
        .clone();
    changed
        .observed_properties
        .insert("mode".into(), "subtract".into());
    let mut sim =
        RedstoneTickSimulator::new(comparator_models::world(Some("compare"), 0, RotationY::R0))
            .unwrap();
    sim.set_pressure_plate_level(Pos::new(-1, 1, 0), 10)
        .unwrap();
    sim.set_pressure_plate_level(Pos::new(0, 1, -1), 9).unwrap();
    let boundary = sim.step_event().unwrap();
    assert_eq!(
        boundary.event_kind,
        SimulationEventKind::CompatibilityBoundary
    );
    assert_eq!(boundary.to.comparator_output[&main], 0);
    assert!(sim.has_pending_events());
    sim.set_pressure_plate_level(Pos::new(-1, 1, 0), 7).unwrap();
    sim.set_pressure_plate_level(Pos::new(0, 1, -1), 3).unwrap();
    sim.set_block_state(main, changed).unwrap();
    let update = sim.step_event().unwrap();
    assert_eq!(update.event_kind, SimulationEventKind::ComparatorUpdate);
    assert_eq!(update.time.game_tick, 2);
    assert_eq!(update.to.comparator_output[&main], 10);
    while sim.pending_scheduler_events() != 0 {
        sim.step_event().unwrap();
    }
    assert_eq!(sim.snapshot().comparator_output[&main], 10);
    assert_eq!(sim.advance_tick().unwrap().comparator_output[&main], 4);
}

#[test]
fn initial_output_and_raw_u8_levels_keep_the_existing_compatibility_policy() {
    for (level, powered, expected) in [
        (None, None, 0),
        (None, Some(false), 0),
        (None, Some(true), 15),
        (Some(7), Some(false), 7),
        (Some(255), Some(false), 255),
    ] {
        let mut world = World::new();
        for x in 0..2 {
            let comparator = world.place(BlockKind::Comparator, Pos::new(x, 1, 0));
            comparator.facing = Some(Facing::East);
            comparator.power_level = if x == 0 { level } else { Some(0) };
            comparator.powered = if x == 0 { powered } else { Some(false) };
        }
        let mut sim = RedstoneTickSimulator::new(world).unwrap();
        assert_eq!(
            sim.snapshot().comparator_output[&Pos::new(0, 1, 0)],
            expected
        );
        assert_eq!(
            sim.advance_tick().unwrap().comparator_output[&Pos::new(1, 1, 0)],
            expected
        );
    }
}

#[test]
fn observation_removal_keeps_the_existing_queue_lifecycle() {
    let main = Pos::new(0, 1, 0);
    let original = comparator_models::world(Some("compare"), 7, RotationY::R0);
    let block = original.get(main).unwrap().clone();
    let mut sim = RedstoneTickSimulator::new(original).unwrap();
    sim.set_block_state(main, Block::new(BlockKind::Air))
        .unwrap();
    assert!(
        !sim.advance_tick()
            .unwrap()
            .comparator_output
            .contains_key(&main)
    );
    assert!(sim.has_pending_events());
    sim.set_block_state(main, block).unwrap();
    sim.set_pressure_plate_level(Pos::new(-1, 1, 0), 9).unwrap();
    assert_eq!(sim.advance_tick().unwrap().comparator_output[&main], 9);
}
