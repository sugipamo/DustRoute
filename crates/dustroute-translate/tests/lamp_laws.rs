#[path = "support/lamp_models.rs"]
mod lamp_models;

use dustroute_minecraft::time::PhysicsEngine;
use dustroute_minecraft::{Block, BlockKind, Pos, Region};
use dustroute_translate::sim::{RedstoneTickSimulator, SimulationEventKind};
use lamp_models::{INPUT, LAMP};

#[test]
fn both_lamp_models_retain_their_initialization_and_distinct_off_timing() {
    let expected: Vec<serde_json::Value> = include_str!("fixtures/lamp_models_v1.jsonl")
        .lines()
        .map(|line| serde_json::from_str(line).unwrap())
        .collect();
    let actual = lamp_models::capture();
    assert_eq!(actual.len(), 24);
    assert_eq!(actual, expected);
}

#[test]
fn lamp_samples_current_power_at_update_and_repower_cancels_the_old_deadline() {
    let mut sim = RedstoneTickSimulator::new(lamp_models::world(false, Some(true))).unwrap();
    assert!(!sim.snapshot().lamp_lit[&LAMP]);
    assert_eq!(
        sim.step_event().unwrap().event_kind,
        SimulationEventKind::CompatibilityBoundary
    );
    sim.set_powered(INPUT, true).unwrap();
    lamp_models::drain(&mut sim);
    assert!(sim.snapshot().lamp_lit[&LAMP]);
    sim.set_powered(INPUT, false).unwrap();
    lamp_models::drain(&mut sim);
    assert!(sim.has_pending_events());
    sim.set_powered(INPUT, true).unwrap();
    lamp_models::drain(&mut sim);
    assert!(!sim.has_pending_events());
    sim.set_powered(INPUT, false).unwrap();
    for expected in [true, true, false] {
        lamp_models::drain(&mut sim);
        assert_eq!(sim.snapshot().lamp_lit[&LAMP], expected);
    }
    assert!(!sim.has_pending_events());
}

#[test]
fn observation_removal_and_reinsertion_retain_existing_per_position_lamp_state() {
    let mut sim = RedstoneTickSimulator::new(lamp_models::world(true, None)).unwrap();
    sim.set_powered(INPUT, false).unwrap();
    lamp_models::drain(&mut sim);
    sim.set_block_state(LAMP, Block::new(BlockKind::Air))
        .unwrap();
    for _ in 0..3 {
        lamp_models::drain(&mut sim);
    }
    // The compatibility model historically retains the cache and deadline;
    // migration does not silently invent a new restart/cleanup policy.
    assert!(sim.snapshot().lamp_lit[&LAMP]);
    assert!(!sim.has_pending_events());
    sim.set_block_state(LAMP, Block::new(BlockKind::RedstoneLamp))
        .unwrap();
    assert!(sim.snapshot().lamp_lit[&LAMP]);
    lamp_models::drain(&mut sim);
    assert!(!sim.snapshot().lamp_lit[&LAMP]);
    assert!(!sim.has_pending_events());
}

#[test]
fn bounded_lamp_unknown_observed_state_still_rejects_without_mutating_the_lamp() {
    let mut world = lamp_models::world(false, None);
    world.get_mut(LAMP).unwrap().observed_name = Some("minecraft:redstone_lamp".into());
    let before = world.clone();
    let mut engine = PhysicsEngine::new_diagnostic(world, 128)
        .with_piston_planning_region(Region::new(Pos::new(-1, 0, -1), Pos::new(3, 2, 1)));
    engine.schedule_redstone_input(0, INPUT, true);
    assert!(engine.run_redstone_propagation().is_err());
    assert_eq!(engine.world().get(LAMP), before.get(LAMP));
    // Atomicity belongs to each event delta, not the entire event sequence.
    // The earlier input/wire events remain committed when the lamp rejects.
    assert_eq!(engine.world().get(INPUT).unwrap().powered, Some(true));
    assert_eq!(
        engine.world().get(Pos::new(1, 1, 0)).unwrap().power_level,
        Some(15)
    );
}
