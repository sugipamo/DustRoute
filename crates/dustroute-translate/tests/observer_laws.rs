#[path = "support/observer_models.rs"]
mod observer_models;

use dustroute_minecraft::{Block, BlockKind, Facing};
use dustroute_translate::sim::{RedstoneTickSimulator, SimulationEventKind};

#[test]
fn observer_traces_retain_all_six_directions_and_overlapping_pulse_events() {
    let expected: Vec<serde_json::Value> = include_str!("fixtures/observer_model_v1.jsonl")
        .lines()
        .map(|line| serde_json::from_str(line).unwrap())
        .collect();
    let actual = observer_models::capture();
    assert_eq!(actual.len(), 48);
    assert_eq!(actual, expected);
}

#[test]
fn observation_changes_between_boundary_and_start_keep_separate_pending_work() {
    let (world, observer, target) = observer_models::world(Facing::East, false);
    let mut sim = RedstoneTickSimulator::new(world).unwrap();
    sim.set_block_state(target, observer_models::changed("a"))
        .unwrap();
    assert_eq!(
        sim.step_event().unwrap().event_kind,
        SimulationEventKind::CompatibilityBoundary
    );
    sim.set_block_state(target, observer_models::changed("b"))
        .unwrap();
    observer_models::drain(&mut sim, observer);
    let events = observer_models::drain(&mut sim, observer);
    let kinds: Vec<_> = events.iter().map(|e| e[1].as_str().unwrap()).collect();
    assert_eq!(
        kinds,
        [
            "CompatibilityBoundary",
            "ObserverPulseEnd",
            "ObserverPulseStart"
        ]
    );
    assert!(sim.snapshot().observer_powered[&observer]);
    observer_models::drain(&mut sim, observer);
    assert!(!sim.snapshot().observer_powered[&observer]);
}

#[test]
fn removal_before_pulse_start_and_new_observer_baselines_keep_their_existing_behavior() {
    let (world, observer, target) = observer_models::world(Facing::East, false);
    let original = world.get(observer).unwrap().clone();
    let mut sim = RedstoneTickSimulator::new(world).unwrap();
    sim.set_block_state(target, observer_models::changed("a"))
        .unwrap();
    sim.step_event().unwrap();
    sim.set_block_state(observer, Block::new(BlockKind::Air))
        .unwrap();
    observer_models::drain(&mut sim, observer);
    assert!(!sim.snapshot().observer_powered.contains_key(&observer));
    assert!(!sim.has_pending_events());
    sim.set_block_state(observer, original).unwrap();
    observer_models::drain(&mut sim, observer);
    assert!(!sim.snapshot().observer_powered[&observer]);
    sim.set_block_state(target, observer_models::changed("b"))
        .unwrap();
    observer_models::drain(&mut sim, observer);
    assert!(sim.snapshot().observer_powered[&observer]);
}
