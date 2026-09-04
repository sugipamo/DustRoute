use std::collections::BTreeMap;

use dustroute_translate::{PhysicsEventPhase, SchedulerProfile, SchedulerProfileId};
use serde::Deserialize;

#[derive(Debug, Deserialize)]
struct SchedulerFixture {
    schema_version: String,
    minecraft_version: String,
    profile_id: SchedulerProfileId,
    evidence: String,
    source: String,
    events: Vec<SchedulerEventFixture>,
    notes: Vec<String>,
}

#[derive(Debug, Deserialize)]
struct SchedulerEventFixture {
    kind: String,
    phase: PhysicsEventPhase,
    delay_game_ticks: u64,
}

#[test]
fn java_1_21_11_fixture_is_explicitly_modelled_and_schema_complete() {
    let fixture: SchedulerFixture =
        serde_json::from_str(include_str!("fixtures/scheduler_1_21_11.json"))
            .expect("scheduler fixture schema");
    assert_eq!(fixture.schema_version, "dustroute.scheduler-fixture.v1");
    assert_eq!(fixture.minecraft_version, "1.21.11");
    assert_eq!(
        fixture.profile_id,
        SchedulerProfileId::MinecraftJava1_21_11Modelled
    );
    assert_eq!(fixture.evidence, "modelled");
    assert_eq!(fixture.source, "DustRouteDeterministicV1");
    assert!(
        fixture
            .notes
            .iter()
            .any(|note| note.contains("not complete"))
    );

    let expected = [
        (
            "compatibility_boundary",
            PhysicsEventPhase::ScheduledTick,
            2,
        ),
        ("repeater_update", PhysicsEventPhase::ScheduledTick, 0),
        ("comparator_update", PhysicsEventPhase::ScheduledTick, 0),
        ("torch_update", PhysicsEventPhase::ScheduledTick, 0),
        ("observer_pulse_end", PhysicsEventPhase::BlockEvent, 0),
        ("observer_pulse_start", PhysicsEventPhase::BlockEvent, 0),
        ("signal_resolve", PhysicsEventPhase::Observation, 0),
        ("lamp_update", PhysicsEventPhase::Observation, 0),
    ];
    assert_eq!(fixture.events.len(), expected.len());
    for (event, (kind, phase, delay)) in fixture.events.iter().zip(expected) {
        assert_eq!(event.kind, kind);
        assert_eq!(event.phase, phase);
        assert_eq!(event.delay_game_ticks, delay);
    }

    let profile = SchedulerProfile::minecraft_java_1_21_11_modelled();
    assert!(profile.validate().is_ok());
    let mut phase_counts = BTreeMap::new();
    for event in &fixture.events {
        *phase_counts.entry(event.phase).or_insert(0_u8) += 1;
    }
    assert_eq!(phase_counts[&PhysicsEventPhase::ScheduledTick], 4);
    assert_eq!(phase_counts[&PhysicsEventPhase::BlockEvent], 2);
    assert_eq!(phase_counts[&PhysicsEventPhase::Observation], 2);
}
