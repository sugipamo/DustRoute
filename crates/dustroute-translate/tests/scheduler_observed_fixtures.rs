use std::collections::BTreeMap;

use serde::Deserialize;

#[derive(Debug, Deserialize)]
struct ObservedSchedulerFixture {
    schema_version: String,
    minecraft_version: String,
    profile_id: String,
    profile_evidence: String,
    evidence: String,
    source: String,
    scenario: String,
    source_artifact: String,
    clock: ObservationClock,
    input: ObservationInput,
    events: Vec<ObservedEvent>,
    measurements: BTreeMap<String, u64>,
    notes: Vec<String>,
}

#[derive(Debug, Deserialize)]
struct ObservationClock {
    unit: String,
    origin: String,
    absolute_ticks_omitted: bool,
    scheduler_phase: Option<String>,
    scheduler_phase_evidence: String,
}

#[derive(Debug, Deserialize)]
struct ObservationInput {
    kind: String,
    transition: String,
    activation_is_baseline: bool,
}

#[derive(Debug, Deserialize)]
struct ObservedEvent {
    sequence: u64,
    kind: String,
    position: Position,
    relative_game_tick: u64,
    sub_tick_order: u64,
    scheduler_phase: Option<String>,
    changed: bool,
    before: BlockState,
    after: BlockState,
}

#[derive(Debug, Deserialize)]
struct Position {
    x: i32,
    y: i32,
    z: i32,
}

#[derive(Debug, Deserialize)]
struct BlockState {
    name: String,
    properties: serde_json::Value,
}

const OBSERVED_FIXTURES: &[(&str, &str)] = &[
    (
        "repeater_observer",
        include_str!("fixtures/scheduler_1_21_11_observed_repeater_observer.json"),
    ),
    (
        "piston",
        include_str!("fixtures/scheduler_1_21_11_observed_piston.json"),
    ),
];

const OBSERVED_METADATA: &[(&str, &str, &str)] = &[
    (
        "repeater_observer",
        include_str!("fixtures/scheduler_1_21_11_observed_repeater_observer.meta.json"),
        "crates/dustroute-translate/tests/fixtures/scheduler_1_21_11_observed_repeater_observer.json",
    ),
    (
        "piston",
        include_str!("fixtures/scheduler_1_21_11_observed_piston.meta.json"),
        "crates/dustroute-translate/tests/fixtures/scheduler_1_21_11_observed_piston.json",
    ),
];

fn load_fixture(name: &str, source: &str) -> ObservedSchedulerFixture {
    serde_json::from_str(source).unwrap_or_else(|error| {
        panic!("observed scheduler fixture {name} must be valid JSON: {error}")
    })
}

fn measurement(fixture: &ObservedSchedulerFixture, key: &str) -> u64 {
    *fixture
        .measurements
        .get(key)
        .unwrap_or_else(|| panic!("{key} is required in {}", fixture.scenario))
}

fn assert_common_contract(name: &str, fixture: &ObservedSchedulerFixture) {
    assert_eq!(
        fixture.schema_version, "dustroute.scheduler-observation-fixture.v1",
        "{name} schema"
    );
    assert_eq!(fixture.minecraft_version, "1.21.11", "{name} version");
    assert_eq!(
        fixture.profile_id, "minecraft_java1_21_11_modelled",
        "{name} profile"
    );
    assert_eq!(
        fixture.profile_evidence, "modelled",
        "{name} profile evidence"
    );
    assert_eq!(fixture.evidence, "observed", "{name} fixture evidence");
    assert_eq!(fixture.source, "live_mineflayer", "{name} source");
    assert!(!fixture.scenario.is_empty(), "{name} scenario");
    assert!(
        !fixture.source_artifact.is_empty(),
        "{name} source artifact"
    );

    assert_eq!(fixture.clock.unit, "game_tick", "{name} clock unit");
    assert_eq!(
        fixture.clock.origin, "normal_player_activation",
        "{name} clock origin"
    );
    assert!(
        fixture.clock.absolute_ticks_omitted,
        "{name} must not freeze an absolute server tick"
    );
    assert_eq!(
        fixture.clock.scheduler_phase, None,
        "{name} must not claim a Vanilla phase"
    );
    assert_eq!(
        fixture.clock.scheduler_phase_evidence, "unknown",
        "{name} scheduler phase evidence"
    );
    assert_eq!(fixture.input.kind, "lever", "{name} input kind");
    assert_eq!(
        fixture.input.transition, "off_to_on",
        "{name} input transition"
    );
    assert!(
        fixture.input.activation_is_baseline,
        "{name} input baseline"
    );
    assert!(
        !fixture.events.is_empty(),
        "{name} must contain observed events"
    );
    assert!(
        !fixture.measurements.is_empty(),
        "{name} must contain measurements"
    );

    let mut previous_tick = 0;
    let mut previous_order = None;
    for (index, event) in fixture.events.iter().enumerate() {
        assert_eq!(event.sequence, index as u64 + 1, "{name} sequence");
        assert!(!event.kind.is_empty(), "{name} event kind");
        let _ = (event.position.x, event.position.y, event.position.z);
        assert!(
            event.relative_game_tick >= previous_tick,
            "{name} event ticks must be non-decreasing"
        );
        if event.relative_game_tick == previous_tick {
            assert!(
                previous_order.is_some_and(|order| event.sub_tick_order > order),
                "{name} same-game-tick packet order must increase"
            );
        }
        previous_tick = event.relative_game_tick;
        previous_order = Some(event.sub_tick_order);
        assert_eq!(
            event.scheduler_phase, None,
            "{name} event must not claim Vanilla phase"
        );
        assert!(!event.before.name.is_empty(), "{name} before block name");
        assert!(!event.after.name.is_empty(), "{name} after block name");
        let _ = (&event.before.properties, &event.after.properties);
    }
    assert!(
        fixture
            .notes
            .iter()
            .any(|note| note.contains("packet") && note.contains("order")),
        "{name} must document packet-order provenance"
    );
}

#[test]
fn observed_scheduler_fixtures_are_explicitly_separate_from_modelled_profile() {
    for (name, source) in OBSERVED_FIXTURES {
        let fixture = load_fixture(name, source);
        assert_common_contract(name, &fixture);
    }
}

#[test]
fn observed_scheduler_fixture_metadata_is_required_and_traceable() {
    for ((name, source), (metadata_name, metadata_source, expected_fixture)) in
        OBSERVED_FIXTURES.iter().zip(OBSERVED_METADATA)
    {
        assert_eq!(name, metadata_name);
        let fixture = load_fixture(name, source);
        let metadata: serde_json::Value = serde_json::from_str(metadata_source)
            .unwrap_or_else(|error| panic!("metadata for {name} must be valid JSON: {error}"));
        assert_eq!(
            metadata["schema_version"],
            "dustroute.scheduler-observation-metadata.v1"
        );
        assert_eq!(metadata["minecraft_version"], "1.21.11");
        assert_eq!(metadata["fixture"].as_str(), Some(*expected_fixture));
        assert_eq!(
            metadata["source_artifact"].as_str(),
            Some(fixture.source_artifact.as_str())
        );
        assert!(
            metadata["reason"]
                .as_str()
                .is_some_and(|reason| !reason.is_empty())
        );
    }
}

#[test]
fn repeater_observer_measurement_preserves_same_tick_order_and_delays() {
    let fixture = load_fixture("repeater_observer", OBSERVED_FIXTURES[0].1);
    assert_eq!(fixture.scenario, "observer_repeater_preview_only");
    assert_eq!(
        measurement(&fixture, "input_to_repeater_power_game_ticks"),
        4
    );
    assert_eq!(
        measurement(&fixture, "wire_to_repeater_power_game_ticks"),
        3
    );
    assert_eq!(measurement(&fixture, "input_to_lamp_on_game_ticks"), 6);
    assert_eq!(
        measurement(&fixture, "repeater_to_observer_start_game_ticks"),
        2
    );
    assert_eq!(measurement(&fixture, "observer_pulse_game_ticks"), 2);
    assert_eq!(
        measurement(&fixture, "observer_start_to_lamp_off_game_ticks"),
        6
    );
    assert_eq!(measurement(&fixture, "input_to_lamp_off_game_ticks"), 12);

    let repeater = fixture
        .events
        .iter()
        .find(|event| event.kind == "repeater_powered")
        .expect("repeater transition");
    let observer_start = fixture
        .events
        .iter()
        .find(|event| event.kind == "observer_pulse_start")
        .expect("observer start");
    assert_eq!(repeater.relative_game_tick, 4);
    assert_eq!(observer_start.relative_game_tick, 6);
    assert_eq!(observer_start.sub_tick_order, 1);

    let same_tick_at_start: Vec<_> = fixture
        .events
        .iter()
        .filter(|event| event.relative_game_tick == 6)
        .collect();
    assert_eq!(same_tick_at_start.len(), 2);
    assert_eq!(same_tick_at_start[0].kind, "lamp_lit");
    assert_eq!(same_tick_at_start[1].kind, "observer_pulse_start");

    let no_op = fixture
        .events
        .iter()
        .find(|event| event.kind == "lever_state_noop")
        .expect("no-op packet observation");
    assert!(!no_op.changed);
}

#[test]
fn piston_measurement_separates_start_from_stable_completion() {
    let fixture = load_fixture("piston", OBSERVED_FIXTURES[1].1);
    assert_eq!(fixture.scenario, "piston_motion_trace");
    assert_eq!(measurement(&fixture, "input_to_piston_start_game_ticks"), 1);
    assert_eq!(
        measurement(&fixture, "piston_start_to_stable_completion_game_ticks"),
        2
    );
    assert_eq!(measurement(&fixture, "completion_same_tick_event_count"), 2);

    let start = &fixture.events[0];
    assert_eq!(start.kind, "piston_start");
    assert_eq!(start.relative_game_tick, 1);
    let completion: Vec<_> = fixture
        .events
        .iter()
        .filter(|event| event.relative_game_tick == 3)
        .collect();
    assert_eq!(completion.len(), 2);
    assert_eq!(completion[0].kind, "piston_block_move");
    assert_eq!(completion[1].kind, "piston_head_completion");
    assert_eq!(completion[0].sub_tick_order, 0);
    assert_eq!(completion[1].sub_tick_order, 1);
}
