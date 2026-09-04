use dustroute_minecraft::PistonAction;
use dustroute_minecraft::time::{PhysicsEngine, TraceStatus};
use dustroute_translate::{
    Block, BlockKind, Facing, NormalizedBlockState, NormalizedTransition,
    NormalizedTransitionTrace, Pos, RedstoneTickSimulator, SameTickOrderEvidence,
    TransitionEvidence, WireConnection, World, compare_transition_traces,
    normalize_observed_fixture, normalize_transition_trace, observed_fixture_from_json,
    update_wire_shapes,
};
use std::collections::{BTreeMap, BTreeSet};

const FIXTURES: &[(&str, &str)] = &[
    (
        "repeater_observer",
        include_str!("fixtures/scheduler_1_21_11_observed_repeater_observer.json"),
    ),
    (
        "piston",
        include_str!("fixtures/scheduler_1_21_11_observed_piston.json"),
    ),
];

#[test]
fn java_1_21_11_observations_project_without_claiming_scheduler_phase() {
    for (name, source) in FIXTURES {
        let fixture = observed_fixture_from_json(source)
            .unwrap_or_else(|error| panic!("{name} fixture must parse: {error}"));
        let normalized = normalize_observed_fixture(&fixture);
        assert!(normalized.complete, "{name}");
        assert_eq!(normalized.transitions.len(), fixture.events.len(), "{name}");
        assert!(
            normalized
                .transitions
                .iter()
                .all(|transition| transition.scheduler_phase.is_none()),
            "{name} must not promote packet evidence into a Vanilla phase"
        );
        for (event, transition) in fixture.events.iter().zip(&normalized.transitions) {
            assert_eq!(transition.relative_game_tick, event.relative_game_tick);
            assert_eq!(transition.position, event.position);
            assert_eq!(transition.changed, event.changed);
            assert_eq!(
                transition.same_tick_order,
                SameTickOrderEvidence::ObservedPacket(event.sub_tick_order)
            );
        }
    }
}

#[test]
fn observed_noop_remains_distinct_from_state_transitions() {
    let fixture = observed_fixture_from_json(FIXTURES[0].1).unwrap();
    let normalized = normalize_observed_fixture(&fixture);
    let no_op = normalized
        .transitions
        .iter()
        .find(|transition| !transition.changed)
        .expect("the observed packet no-op must be retained");
    assert_eq!(no_op.before, no_op.after);
}

fn observed_without_input(fixture_index: usize, input: Pos) -> NormalizedTransitionTrace {
    let fixture = observed_fixture_from_json(FIXTURES[fixture_index].1).unwrap();
    let mut trace = normalize_observed_fixture(&fixture);
    trace.transitions.retain(|item| item.position != input);
    trace
        .unavailable_reasons
        .push("input activation boundary is outside the model transition trace".into());
    trace
}

fn block_state(
    block: &Block,
    state: &dustroute_translate::TickState,
    position: Pos,
) -> NormalizedBlockState {
    let mut properties = BTreeMap::new();
    let name = match block.kind {
        BlockKind::RedstoneWire => {
            properties.insert("power".into(), state.strength(position).to_string());
            for facing in [Facing::North, Facing::East, Facing::South, Facing::West] {
                let connection = block
                    .wire_connections
                    .as_ref()
                    .and_then(|items| items.get(&facing))
                    .copied()
                    .unwrap_or(WireConnection::None);
                properties.insert(
                    format!("{facing:?}").to_ascii_lowercase(),
                    format!("{connection:?}").to_ascii_lowercase(),
                );
            }
            "redstone_wire"
        }
        BlockKind::Repeater => {
            properties.insert("powered".into(), state.powered(position).to_string());
            properties.insert("locked".into(), "false".into());
            properties.insert("delay".into(), block.delay.unwrap_or(1).to_string());
            properties.insert("facing".into(), "west".into());
            "repeater"
        }
        BlockKind::Observer => {
            properties.insert("powered".into(), state.powered(position).to_string());
            properties.insert("facing".into(), "west".into());
            "observer"
        }
        BlockKind::RedstoneLamp => {
            properties.insert("lit".into(), state.powered(position).to_string());
            "redstone_lamp"
        }
        _ => panic!("unsupported observed scenario block: {:?}", block.kind),
    };
    NormalizedBlockState {
        name: name.into(),
        properties,
    }
}

fn repeater_observer_model_trace() -> NormalizedTransitionTrace {
    let lever_pos = Pos::new(0, 0, 0);
    let positions = BTreeSet::from([
        Pos::new(1, 0, 0),
        Pos::new(2, 0, 0),
        Pos::new(3, 0, 0),
        Pos::new(4, 0, 0),
        Pos::new(5, 0, 0),
    ]);
    let mut world = World::new();
    let mut lever = Block::new(BlockKind::Lever);
    lever.powered = Some(false);
    world.set(lever_pos, lever);
    world.set(Pos::new(1, 0, 0), Block::new(BlockKind::RedstoneWire));
    let mut repeater = Block::new(BlockKind::Repeater);
    repeater.facing = Some(Facing::East);
    repeater.delay = Some(2);
    world.set(Pos::new(2, 0, 0), repeater);
    world.set(Pos::new(3, 0, 0), Block::new(BlockKind::RedstoneWire));
    let mut observer = Block::new(BlockKind::Observer);
    observer.facing = Some(Facing::East);
    observer.powered = Some(false);
    world.set(Pos::new(4, 0, 0), observer);
    let mut lamp = Block::new(BlockKind::RedstoneLamp);
    lamp.powered = Some(false);
    world.set(Pos::new(5, 0, 0), lamp);
    update_wire_shapes(&mut world);

    let observed_world = world.clone();
    let mut simulator = RedstoneTickSimulator::new(world).unwrap();
    let before_activation = simulator.snapshot();
    let after_activation = simulator.set_lever_state(lever_pos, true).unwrap();
    let mut state_edges = vec![(
        dustroute_minecraft::time::PhysicsTime::default(),
        before_activation,
        after_activation,
    )];
    while simulator.time().game_tick < 30 {
        let edge = simulator.step_transition().unwrap();
        state_edges.push((edge.time, edge.from, edge.to));
    }

    let mut transitions = Vec::new();
    for (time, before, after) in state_edges {
        let changed: Vec<_> = positions
            .iter()
            .filter_map(|position| {
                let block = observed_world.get(*position).unwrap();
                let before = block_state(block, &before, *position);
                let after = block_state(block, &after, *position);
                (before != after).then_some((*position, before, after))
            })
            .collect();
        for (position, before, after) in &changed {
            transitions.push(NormalizedTransition {
                relative_game_tick: i64::try_from(time.game_tick).unwrap(),
                same_tick_order: if changed.len() == 1 {
                    SameTickOrderEvidence::ModelledScheduler(time.sub_tick_order)
                } else {
                    SameTickOrderEvidence::Unavailable
                },
                scheduler_phase: Some(format!("{:?}", time.phase).to_ascii_lowercase()),
                event_kind: None,
                position: *position,
                before: before.clone(),
                after: after.clone(),
                changed: true,
                evidence: TransitionEvidence::ModelledScheduler,
                scheduler_cause_sequence: None,
            });
        }
    }
    NormalizedTransitionTrace {
        transitions,
        neighbor_updates: Vec::new(),
        piston_states: Vec::new(),
        complete: true,
        unavailable_reasons: vec![
            "input activation boundary is outside the model transition trace".into(),
        ],
    }
}

#[test]
fn repeater_observer_fixture_is_connected_to_real_simulation() {
    let observed = observed_without_input(0, Pos::new(0, 0, 0));
    let modelled = repeater_observer_model_trace();
    let result = compare_transition_traces(&observed, &modelled);
    assert_eq!(result.compared_transitions, 7, "{result:#?}");
    assert_eq!(
        result.status,
        dustroute_translate::ConformanceStatus::Mismatch
    );
    assert_eq!(result.issues.len(), 4, "{result:#?}");
    assert_eq!(
        result
            .issues
            .iter()
            .filter(|issue| issue.field == dustroute_translate::ConformanceField::RelativeGameTick)
            .count(),
        1
    );
    assert_eq!(
        result
            .issues
            .iter()
            .filter(|issue| issue.field == dustroute_translate::ConformanceField::SameTickOrder)
            .count(),
        2
    );
}

#[test]
fn piston_fixture_is_connected_to_engine_transition_trace() {
    let piston_pos = Pos::new(3, 0, 0);
    let mut world = World::new();
    let mut piston = Block::new(BlockKind::Piston);
    piston.facing = Some(Facing::East);
    world.set(piston_pos, piston);
    let mut stone = Block::new(BlockKind::Solid);
    stone.observed_name = Some("minecraft:stone".into());
    world.set(Pos::new(4, 0, 0), stone);
    let mut engine = PhysicsEngine::new(world, 8);
    engine.schedule_piston_action(1, piston_pos, PistonAction::Extend);
    engine.run_piston_events().unwrap();
    assert_eq!(engine.transition_trace().status, TraceStatus::Complete);

    let observed = normalize_observed_fixture(&observed_fixture_from_json(FIXTURES[1].1).unwrap());
    let modelled = normalize_transition_trace(engine.transition_trace(), 0);
    println!("modelled transitions: {:#?}", modelled.transitions);
    let result = compare_transition_traces(&observed, &modelled);
    assert_eq!(result.compared_transitions, 3, "{result:#?}");
    assert_eq!(modelled.transitions[0].relative_game_tick, 1);
    assert_eq!(
        result.status,
        dustroute_translate::ConformanceStatus::Unavailable
    );
    assert!(
        !result
            .issues
            .iter()
            .any(|issue| issue.field == dustroute_translate::ConformanceField::AfterName)
    );
    assert!(result.issues.iter().any(|issue| {
        issue.field == dustroute_translate::ConformanceField::ChangeOrder
            && issue.status == dustroute_translate::ConformanceStatus::Unavailable
    }));
    assert!(result.issues.iter().any(|issue| {
        issue.field == dustroute_translate::ConformanceField::SameTickOrder
            && issue.status == dustroute_translate::ConformanceStatus::Unavailable
    }));
}

#[test]
fn piston_instrumentation_fixture_compares_typed_moving_and_stable_states() {
    let input_pos = Pos::new(652, 104, 0);
    let piston_pos = Pos::new(653, 104, 0);
    let mut world = World::new();
    let mut lever = Block::new(BlockKind::Lever);
    lever.powered = Some(false);
    world.set(input_pos, lever);
    let mut piston = Block::new(BlockKind::Piston);
    piston.facing = Some(Facing::East);
    world.set(piston_pos, piston);
    let mut stone = Block::new(BlockKind::Solid);
    stone.observed_name = Some("minecraft:stone".into());
    world.set(Pos::new(654, 104, 0), stone);
    let known_region =
        dustroute_minecraft::Region::new(Pos::new(651, 103, -1), Pos::new(655, 105, 1));
    let mut engine = PhysicsEngine::new(world, 16).with_piston_planning_region(known_region);
    engine.schedule_redstone_input(0, input_pos, true);
    engine.run_redstone_piston_events().unwrap();

    let artifact = dustroute_translate::parse_and_validate_instrumentation(include_str!(
        "fixtures/vanilla_1_21_11_offline_piston_input.json"
    ))
    .unwrap();
    let mut observed = dustroute_translate::normalize_vanilla_instrumentation_artifact(&artifact);
    observed
        .transitions
        .retain(|transition| transition.position != input_pos);
    let mut modelled = normalize_transition_trace(engine.transition_trace(), 0);
    modelled
        .transitions
        .retain(|transition| transition.position != input_pos);
    assert_eq!(observed.piston_states.len(), 3);
    assert_eq!(modelled.piston_states.len(), 3);
    let result = compare_transition_traces(&observed, &modelled);
    assert_ne!(
        result.status,
        dustroute_translate::ConformanceStatus::Mismatch,
        "{result:#?}"
    );
    assert_eq!(result.compared_transitions, 0, "{result:#?}");
    assert!(
        !result.issues.iter().any(|issue| {
            issue.field == dustroute_translate::ConformanceField::PistonState
                && issue.status == dustroute_translate::ConformanceStatus::Mismatch
        }),
        "{result:#?}"
    );
}
