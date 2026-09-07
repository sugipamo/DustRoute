use dustroute_minecraft::time::{PhysicsEngine, TraceStatus};
use dustroute_minecraft::{BlockKind, Region, piston_state, piston_variant};
use dustroute_translate::{MinecraftSnapshot, MinecraftSnapshotBlock, Pos, world_from_snapshot};
use serde_json::Value;

#[test]
fn isolated_retracted_piston_round_trip_matches_recorded_java_states() {
    let case: Value = serde_json::from_str(include_str!(
        "fixtures/piston-low-layer/03-sticky-piston.json"
    ))
    .unwrap();
    let observation: Value = serde_json::from_str(include_str!(
        "fixtures/piston-low-layer-push-observation.json"
    ))
    .unwrap();
    let initial: MinecraftSnapshot = serde_json::from_value(case["initial"].clone()).unwrap();
    let mut engine = PhysicsEngine::new_diagnostic(world_from_snapshot(&initial).unwrap(), 512)
        .with_piston_planning_region(Region::new(initial.min, initial.max));
    let original_payload = engine.world().get(Pos::new(1, 0, 0)).unwrap().clone();
    engine.schedule_redstone_input(
        1,
        serde_json::from_value(case["input"].clone()).unwrap(),
        true,
    );
    engine.run_redstone_propagation().unwrap();
    assert_eq!(engine.trace_status(), &TraceStatus::Complete);
    assert_eq!(engine.pending_event_count(), 0);
    assert_eq!(
        engine.world().get(Pos::new(2, 0, 0)),
        Some(&original_payload)
    );

    assert_observation(&engine, &initial, &observation);
    engine.schedule_redstone_input(
        engine.time().game_tick + 1,
        serde_json::from_value(case["input"].clone()).unwrap(),
        false,
    );
    engine.run_redstone_propagation().unwrap();
    assert_eq!(engine.trace_status(), &TraceStatus::Complete);
    assert_eq!(engine.pending_event_count(), 0);
    assert_eq!(
        engine.world().get(Pos::new(1, 0, 0)),
        Some(&original_payload)
    );
    let pulled: Value = serde_json::from_str(include_str!(
        "fixtures/piston-low-layer-pull-observation.json"
    ))
    .unwrap();
    assert_observation(&engine, &initial, &pulled);
}

#[test]
fn moved_piston_actuation_matches_recorded_java_sequence() {
    let case: Value = serde_json::from_str(include_str!(
        "fixtures/piston-low-layer/04-moved-piston-actuation.json"
    ))
    .unwrap();
    let observed: Value = serde_json::from_str(include_str!(
        "fixtures/piston-low-layer-compound-observation.json"
    ))
    .unwrap();
    let initial: MinecraftSnapshot = serde_json::from_value(case["initial"].clone()).unwrap();
    let mut engine = PhysicsEngine::new_diagnostic(world_from_snapshot(&initial).unwrap(), 512)
        .with_piston_planning_region(Region::new(initial.min, initial.max));
    let actions = case["actions"].as_array().unwrap();
    let phases = observed["phases"].as_array().unwrap();
    assert_eq!(actions.len(), phases.len());
    for (action, observation) in actions.iter().zip(phases) {
        assert_eq!(action["input"], observation["input"]);
        assert_eq!(action["powered"], observation["powered"]);
        engine.schedule_redstone_input(
            engine.time().game_tick + 1,
            serde_json::from_value(action["input"].clone()).unwrap(),
            action["powered"].as_bool().unwrap(),
        );
        engine.run_redstone_propagation().unwrap();
        assert_eq!(engine.trace_status(), &TraceStatus::Complete);
        assert_eq!(engine.pending_event_count(), 0);
        assert_observation(&engine, &initial, observation);
    }
}

#[test]
fn two_row_passage_matches_recorded_java_sequence() {
    let case: Value = serde_json::from_str(include_str!(
        "fixtures/piston-low-layer/06-two-row-passage.json"
    ))
    .unwrap();
    let observed: Value = serde_json::from_str(include_str!(
        "fixtures/piston-low-layer-two-row-observation.json"
    ))
    .unwrap();
    let initial: MinecraftSnapshot = serde_json::from_value(case["initial"].clone()).unwrap();
    let mut engine = PhysicsEngine::new_diagnostic(world_from_snapshot(&initial).unwrap(), 512)
        .with_piston_planning_region(Region::new(initial.min, initial.max));
    let actions = case["actions"].as_array().unwrap();
    let phases = observed["phases"].as_array().unwrap();
    assert_eq!(actions.len(), phases.len());
    for (index, (action, observation)) in actions.iter().zip(phases).enumerate() {
        assert_eq!(action["input"], observation["input"]);
        assert_eq!(action["powered"], observation["powered"]);
        engine.schedule_redstone_input(
            engine.time().game_tick + 1,
            serde_json::from_value(action["input"].clone()).unwrap(),
            action["powered"].as_bool().unwrap(),
        );
        engine.run_redstone_propagation().unwrap();
        assert_eq!(engine.trace_status(), &TraceStatus::Complete);
        assert_eq!(engine.pending_event_count(), 0);
        assert_observation(&engine, &initial, observation);
        let clear = (0..=1).all(|y| engine.world().get(Pos::new(2, y, 0)).is_none());
        assert_eq!(
            clear,
            case["passage"]["open_by_phase"][index].as_bool().unwrap()
        );
        if index == 1 || index == 5 {
            for y in 0..=1 {
                assert_eq!(
                    engine.world().get(Pos::new(2, y, 0)).unwrap().kind,
                    BlockKind::Solid
                );
            }
        }
    }
}

#[test]
fn single_input_two_row_matches_recorded_java_sequence() {
    let case: Value = serde_json::from_str(include_str!(
        "fixtures/piston-low-layer/07-single-input-two-row.json"
    ))
    .unwrap();
    let observed: Value = serde_json::from_str(include_str!(
        "fixtures/piston-low-layer-single-input-observation.json"
    ))
    .unwrap();
    let initial: MinecraftSnapshot = serde_json::from_value(case["initial"].clone()).unwrap();
    let mut engine = PhysicsEngine::new_diagnostic(world_from_snapshot(&initial).unwrap(), 3 * 512)
        .with_piston_planning_region(Region::new(initial.min, initial.max));
    let actions = case["actions"].as_array().unwrap();
    let phases = observed["phases"].as_array().unwrap();
    assert_eq!(actions.len(), phases.len());
    for (index, (action, observation)) in actions.iter().zip(phases).enumerate() {
        assert_eq!(case["input"], observation["input"]);
        assert_eq!(*action, observation["powered"]);
        engine.schedule_redstone_input(
            engine.time().game_tick + 1,
            serde_json::from_value(case["input"].clone()).unwrap(),
            action.as_bool().unwrap(),
        );
        engine.run_redstone_propagation().unwrap();
        assert_eq!(engine.trace_status(), &TraceStatus::Complete);
        assert_eq!(engine.pending_event_count(), 0);
        assert_observation(&engine, &initial, observation);
        let clear = (0..=1).all(|y| engine.world().get(Pos::new(2, y, 0)).is_none());
        assert_eq!(
            clear,
            case["passage"]["open_by_phase"][index].as_bool().unwrap()
        );
        if index == 0 || index == 2 {
            for y in 0..=1 {
                assert_eq!(
                    engine.world().get(Pos::new(2, y, 0)).unwrap().kind,
                    BlockKind::Solid
                );
            }
        }
    }
}

#[test]
fn passage_close_open_close_matches_recorded_java_states() {
    let case: Value = serde_json::from_str(include_str!(
        "fixtures/piston-low-layer/05-single-piston-passage.json"
    ))
    .unwrap();
    let observed: Value = serde_json::from_str(include_str!(
        "fixtures/piston-low-layer-passage-observation.json"
    ))
    .unwrap();
    let initial: MinecraftSnapshot = serde_json::from_value(case["initial"].clone()).unwrap();
    let mut engine = PhysicsEngine::new_diagnostic(world_from_snapshot(&initial).unwrap(), 512)
        .with_piston_planning_region(Region::new(initial.min, initial.max));
    let actions = case["actions"].as_array().unwrap();
    let phases = observed["phases"].as_array().unwrap();
    assert_eq!(actions.len(), phases.len());
    for (action, observation) in actions.iter().zip(phases) {
        assert_eq!(case["input"], observation["input"]);
        assert_eq!(*action, observation["powered"]);
        engine.schedule_redstone_input(
            engine.time().game_tick + 1,
            serde_json::from_value(case["input"].clone()).unwrap(),
            action.as_bool().unwrap(),
        );
        engine.run_redstone_propagation().unwrap();
        assert_eq!(engine.trace_status(), &TraceStatus::Complete);
        assert_eq!(engine.pending_event_count(), 0);
        assert_observation(&engine, &initial, observation);
        // Every floor and headroom cell on the walking path is checked.
        for z in -2..=2 {
            assert_eq!(
                engine.world().get(Pos::new(2, -1, z)).unwrap().kind,
                BlockKind::Solid
            );
            assert!(engine.world().get(Pos::new(2, 1, z)).is_none());
            let obstacle = engine.world().get(Pos::new(2, 0, z));
            if z == 0 && action.as_bool().unwrap() {
                assert_eq!(obstacle.unwrap().kind, BlockKind::Solid);
            } else {
                assert!(obstacle.is_none());
            }
        }
    }
}

fn assert_observation(engine: &PhysicsEngine, initial: &MinecraftSnapshot, observation: &Value) {
    let expected = MinecraftSnapshot {
        min: initial.min,
        max: initial.max,
        blocks: observation["expected"]
            .as_array()
            .unwrap()
            .iter()
            .map(|r| MinecraftSnapshotBlock {
                pos: serde_json::from_value(r["pos"].clone()).unwrap(),
                name: r["name"].as_str().unwrap().into(),
                properties: r["properties"]
                    .as_object()
                    .unwrap()
                    .iter()
                    .map(|(k, v)| {
                        (
                            k.clone(),
                            v.as_str().map_or_else(|| v.to_string(), str::to_owned),
                        )
                    })
                    .collect(),
            })
            .collect(),
    };
    let expected_world = world_from_snapshot(&expected).unwrap();
    for r in &expected.blocks {
        let actual = engine.world().get(r.pos);
        let expected = expected_world.get(r.pos);
        assert_eq!(
            actual.map(|b| b.kind),
            expected.map(|b| b.kind),
            "identity at {:?}",
            r.pos
        );
        if let (Some(actual), Some(expected)) = (actual, expected) {
            match actual.kind {
                BlockKind::Piston | BlockKind::PistonHead => {
                    assert_eq!(actual.facing, expected.facing);
                    assert_eq!(piston_variant(actual), piston_variant(expected));
                    if actual.kind == BlockKind::Piston {
                        assert_eq!(piston_state(actual), piston_state(expected));
                    }
                }
                BlockKind::RedstoneWire => assert_eq!(actual.power_level, expected.power_level),
                BlockKind::Lever => assert_eq!(actual.powered, expected.powered),
                _ => assert_eq!(actual.observed_name, expected.observed_name),
            }
        }
    }
}
