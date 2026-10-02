use super::*;
use std::collections::BTreeMap;

fn play_state() -> State {
    let mut state = State {
        phase: Phase::Play,
        ..State::default()
    };
    state.world.select_dimension(
        "minecraft:overworld".into(),
        Dimension::new(-64, 384).unwrap(),
    );
    state
}

#[test]
fn initial_zero_tick_step_is_not_a_frozen_world() {
    let mut state = play_state();
    state
        .receive(ids::play_clientbound::STEP_TICK, &[0], 64)
        .unwrap();
    assert!(state.reconstruction.issue.is_none());
    state
        .receive(ids::play_clientbound::STEP_TICK, &[1], 64)
        .unwrap();
    assert!(matches!(
        state.reconstruction.issue,
        Some(super::super::reconstruction::ReconstructionIssue::UnsupportedTickControl)
    ));
}

#[test]
fn piston_packet_rejects_truncation_and_unknown_action_without_applying_it() {
    let mut valid = vec![0; 8];
    valid.extend([0, 5]);
    put_varint(&mut valid, 138);
    for length in 0..valid.len() {
        let mut state = play_state();
        assert!(
            state
                .receive(ids::play_clientbound::BLOCK_ACTION, &valid[..length], 64)
                .is_err()
        );
        assert!(state.failure.is_some());
    }
    valid[8] = 3;
    assert!(
        play_state()
            .receive(ids::play_clientbound::BLOCK_ACTION, &valid, 64)
            .is_err()
    );
}

#[test]
fn trace_overflow_never_reports_complete_or_resumes_after_a_gap() {
    let mut trace = TraceCapture {
        start: 10,
        bytes: 0,
        limit: 2,
        complete: true,
        records: vec![],
    };
    trace.record(11, 0, Phase::Play, 8, &[1, 2]);
    trace.record(12, 0, Phase::Play, 8, &[3]);
    trace.record(13, 0, Phase::Play, 0, &[]);
    assert!(!trace.complete);
    assert_eq!(trace.records.len(), 1);
    assert_eq!(trace.records[0].sequence, 11);
}

#[test]
fn failed_packet_poisoning_prevents_later_state_application() {
    let mut state = play_state();
    state.ready = true;
    state.world.seed_replay_cell([0, 0, 0], 1);
    let revision = state.world.revision;
    assert!(
        state
            .receive(ids::play_clientbound::BLOCK_CHANGE, &[0], 64)
            .is_err()
    );
    assert!(state.failure.is_some());
    assert!(!state.ready);
    let mut valid = vec![0; 8];
    valid.push(0);
    assert!(
        state
            .receive(ids::play_clientbound::BLOCK_CHANGE, &valid, 64)
            .is_err()
    );
    assert_eq!(state.world.block([0, 0, 0]), Some(1));
    assert_eq!(state.world.revision, revision);
    assert_eq!(state.sequence, 1);
}

#[test]
fn reconfiguration_discards_world_and_requires_fresh_dimension_registry() {
    let mut state = play_state();
    state.ready = true;
    state.position = Some([0.0, 64.0, 0.0]);
    state.dimensions.push(Dimension::new(-64, 384).unwrap());
    state.world.seed_replay_cell([0, 0, 0], 1);
    let replies = state
        .receive(ids::play_clientbound::START_CONFIGURATION, &[], 64)
        .unwrap();
    assert_eq!(state.phase, Phase::Configuration);
    assert!(!state.ready);
    assert!(state.position.is_none());
    assert!(state.dimensions.is_empty());
    assert_eq!(state.world.block([0, 0, 0]), None);
    assert_eq!(
        replies[0].0,
        ids::play_serverbound::CONFIGURATION_ACKNOWLEDGED
    );
    assert!(
        state
            .receive(
                ids::configuration_clientbound::FINISH_CONFIGURATION,
                &[],
                64
            )
            .is_err()
    );
}

#[test]
fn configuration_requires_full_dimension_entries_and_replies_to_keepalive() {
    let mut state = State::default();
    let mut payload = Vec::new();
    put_string(&mut payload, "minecraft:dimension_type");
    put_varint(&mut payload, 1);
    put_string(&mut payload, "minecraft:overworld");
    payload.extend([1, 10]); // present, anonymous compound
    for (key, value) in [("min_y", -64i32), ("height", 384)] {
        payload.push(3);
        payload.extend((key.len() as u16).to_be_bytes());
        payload.extend(key.as_bytes());
        payload.extend(value.to_be_bytes());
    }
    payload.push(0);
    state
        .receive(ids::configuration_clientbound::REGISTRY_DATA, &payload, 64)
        .unwrap();
    assert_eq!(state.dimensions[0].min_y, -64);
    let replies = state
        .receive(ids::configuration_clientbound::KEEP_ALIVE, &[3; 8], 64)
        .unwrap();
    assert_eq!(
        replies,
        vec![(ids::configuration_serverbound::KEEP_ALIVE, vec![3; 8])]
    );
    state
        .receive(
            ids::configuration_clientbound::FINISH_CONFIGURATION,
            &[],
            64,
        )
        .unwrap();
    assert_eq!(state.phase, Phase::Play);
    assert!(!state.ready);
    let mut missing = Vec::new();
    put_string(&mut missing, "minecraft:dimension_type");
    put_varint(&mut missing, 1);
    put_string(&mut missing, "minecraft:overworld");
    missing.push(0);
    assert!(
        State::default()
            .receive(ids::configuration_clientbound::REGISTRY_DATA, &missing, 64)
            .is_err()
    );
}

#[test]
fn retained_native_packets_reproduce_the_stale_stair_without_js() {
    // This is a regression for a diagnosed limitation, NOT a conformance pass.
    let capture: serde_json::Value = serde_json::from_reader(flate2::read::GzDecoder::new(
        &include_bytes!("../../../../docs/evidence/stairs-packets-20260929.json.gz")[..],
    ))
    .unwrap();
    let definitions: serde_json::Value =
        serde_json::from_str(include_str!("../../../../data/java_1_21_11/blocks.json")).unwrap();
    let maximum = definitions.as_array().unwrap().last().unwrap()["maxStateId"]
        .as_i64()
        .unwrap() as i32;
    let registry: BTreeMap<String, i32> = (0..=maximum)
        .map(|id| {
            (
                serde_json::to_string(&super::super::native_state(id).unwrap()).unwrap(),
                id,
            )
        })
        .collect();
    let mut state = play_state();
    for block in capture["before"]["blocks"].as_array().unwrap() {
        let p: [i32; 3] = serde_json::from_value(block["position"].clone()).unwrap();
        let native: crate::NativeBlockState =
            serde_json::from_value(block["state"].clone()).unwrap();
        let id = registry[&serde_json::to_string(&native).unwrap()];
        state.world.seed_replay_cell(p, id);
    }
    state.sequence = capture["before"]["receive_sequence"].as_u64().unwrap();
    state.world.revision = capture["before"]["revision"].as_u64().unwrap();
    assert_eq!(capture["trace"]["complete"], true);
    let mut actions = 0;
    for record in capture["trace"]["records"].as_array().unwrap() {
        assert_eq!(record["phase"], "play");
        assert_eq!(record["sequence"].as_u64().unwrap(), state.sequence + 1);
        let id = record["packet_id"].as_i64().unwrap() as i32;
        // Only captured cells are seeded; no chunk reload may fill gaps from a guess.
        assert!(
            ![
                ids::play_clientbound::MAP_CHUNK,
                ids::play_clientbound::UNLOAD_CHUNK
            ]
            .contains(&id)
        );
        actions += usize::from(id == ids::play_clientbound::BLOCK_ACTION);
        let payload: Vec<u8> = serde_json::from_value(record["payload"].clone()).unwrap();
        state.receive(id, &payload, 64).unwrap();
        if record["sequence"] == capture["on"]["receive_sequence"] {
            assert_cells(&state, &capture["on"]);
        }
    }
    assert_eq!(actions, 2);
    assert_eq!(
        state.sequence,
        capture["trace"]["through_sequence"].as_u64().unwrap()
    );
    assert_cells(&state, &capture["after"]);
    let target = super::super::native_state(state.world.block([102, 180, 99]).unwrap()).unwrap();
    assert_eq!(target.properties["shape"], "inner_left");
    let fresh: serde_json::Value = serde_json::from_reader(flate2::read::GzDecoder::new(
        &include_bytes!("../../../../docs/evidence/stairs-reconnected-20260929.json.gz")[..],
    ))
    .unwrap();
    let fresh_target = fresh["blocks"]
        .as_array()
        .unwrap()
        .iter()
        .find(|b| b["position"] == serde_json::json!([102, 180, 99]))
        .unwrap();
    assert_eq!(fresh_target["state"]["properties"]["shape"], "straight");
    assert!(
        state.reconstruction.issue.is_none(),
        "{:?}",
        state.reconstruction.issue
    );
    for block in fresh["blocks"].as_array().unwrap() {
        let p = serde_json::from_value(block["position"].clone()).unwrap();
        assert_eq!(
            serde_json::to_value(state.reconstruction.cell(&state.world, p).state.unwrap())
                .unwrap(),
            block["state"],
            "reconstructed {p:?}"
        );
    }
}

fn assert_cells(state: &State, expected: &serde_json::Value) {
    assert_eq!(state.world.revision, expected["revision"].as_u64().unwrap());
    for block in expected["blocks"].as_array().unwrap() {
        let p: [i32; 3] = serde_json::from_value(block["position"].clone()).unwrap();
        let actual = super::super::native_state(state.world.block(p).unwrap()).unwrap();
        assert_eq!(
            serde_json::to_value(actual).unwrap(),
            block["state"],
            "at {p:?}"
        );
    }
}

#[test]
fn retained_client_frames_replay_motion_roles_and_final_region() {
    for bytes in [
        &include_bytes!("../../../../docs/evidence/client-motion-b-replay-20260929.json.gz")[..],
        &include_bytes!("../../../../docs/evidence/client-motion-c-replay-20260929.json.gz")[..],
        &include_bytes!("../../../../docs/evidence/client-motion-d-replay-20260929.json.gz")[..],
        &include_bytes!("../../../../docs/evidence/client-slime-branch-replay-20260929.json.gz")[..],
        &include_bytes!("../../../../docs/evidence/client-honey-branch-replay-20260929.json.gz")[..],
        &include_bytes!("../../../../docs/evidence/client-callback-replay-20260929.json.gz")[..],
        &include_bytes!(
            "../../../../docs/evidence/client-reference-door-b-replay-20260929.json.gz"
        )[..],
        &include_bytes!(
            "../../../../docs/evidence/client-reference-door-c-replay-20260929.json.gz"
        )[..],
        &include_bytes!(
            "../../../../docs/evidence/client-reference-door-d-replay-20260929.json.gz"
        )[..],
    ] {
        let capture: serde_json::Value =
            serde_json::from_reader(flate2::read::GzDecoder::new(bytes)).unwrap();
        let mut state = play_state();
        for b in capture["before"]["blocks"].as_array().unwrap() {
            let pos = serde_json::from_value(b["position"].clone()).unwrap();
            let native = serde_json::from_value(b["state"].clone()).unwrap();
            state
                .world
                .seed_replay_cell(pos, super::super::state_id(&native).unwrap());
        }
        state.world.revision = capture["before"]["revision"].as_u64().unwrap();
        state.sequence = capture["before"]["receive_sequence"].as_u64().unwrap();
        assert_eq!(capture["trace"]["complete"], true);
        let mut records = capture["trace"]["records"].as_array().unwrap().iter();
        for sample in capture["samples"].as_array().unwrap() {
            let sequence = sample["sequence"].as_u64().unwrap();
            while state.sequence < sequence {
                let record = records.next().unwrap();
                assert_eq!(record["sequence"].as_u64().unwrap(), state.sequence + 1);
                assert_eq!(record["phase"], "play");
                state
                    .reconstruction
                    .advance(&state.world, record["client_tick"].as_u64().unwrap());
                let bytes: Vec<u8> = serde_json::from_value(record["payload"].clone()).unwrap();
                state
                    .receive(record["packet_id"].as_i64().unwrap() as i32, &bytes, 1024)
                    .unwrap();
            }
            state
                .reconstruction
                .advance(&state.world, sample["client_tick"].as_u64().unwrap());
            assert_eq!(
                serde_json::to_value(&state.reconstruction.issue).unwrap(),
                sample["issue"]
            );
            for b in sample["blocks"].as_array().unwrap() {
                let pos = serde_json::from_value(b["position"].clone()).unwrap();
                assert_eq!(
                    serde_json::to_value(state.reconstruction.cell(&state.world, pos)).unwrap(),
                    *b,
                    "sample seq={sequence} at {pos:?}"
                );
            }
            // A missing or extra live carrier also fails, even outside the selected states.
            let count = sample["moving"].as_array().unwrap().len();
            assert_eq!(
                capture["before"]["blocks"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .filter(|b| {
                        let pos = serde_json::from_value(b["position"].clone()).unwrap();
                        state
                            .reconstruction
                            .cell(&state.world, pos)
                            .moving
                            .is_some()
                    })
                    .count(),
                count
            );
        }
        for b in capture["final"]["blocks"].as_array().unwrap() {
            let pos = serde_json::from_value(b["position"].clone()).unwrap();
            assert_eq!(
                serde_json::to_value(state.reconstruction.cell(&state.world, pos)).unwrap(),
                *b,
                "final at {pos:?}"
            );
        }
    }
}

#[test]
fn retained_mid_motion_connection_restores_native_carriers_and_replays_client_views() {
    let bytes =
        &include_bytes!("../../../../docs/evidence/client-recovery-corrected-02-20260929.json.gz")
            [..];
    let capture: serde_json::Value =
        serde_json::from_reader(flate2::read::GzDecoder::new(bytes)).unwrap();
    assert_eq!(capture["trace"]["after_sequence"], 0);
    assert_eq!(capture["trace"]["complete"], true);
    let mut state = State::default();
    let mut records = capture["trace"]["records"].as_array().unwrap().iter();
    let mut restored = false;
    for sample in capture["samples"].as_array().unwrap() {
        let sequence = sample["received"]["receive_sequence"].as_u64().unwrap();
        while state.sequence < sequence {
            let record = records.next().unwrap();
            assert_eq!(record["sequence"].as_u64().unwrap(), state.sequence + 1);
            state
                .reconstruction
                .advance(&state.world, record["client_tick"].as_u64().unwrap());
            let payload: Vec<u8> = serde_json::from_value(record["payload"].clone()).unwrap();
            state
                .receive(record["packet_id"].as_i64().unwrap() as i32, &payload, 1024)
                .unwrap();
        }
        state
            .reconstruction
            .advance(&state.world, sample["client_tick"].as_u64().unwrap());
        assert!(
            state.reconstruction.issue.is_none(),
            "{:?}",
            state.reconstruction.issue
        );
        for expected in sample["blocks"].as_array().unwrap() {
            let p = serde_json::from_value(expected["position"].clone()).unwrap();
            let cell = state.reconstruction.cell(&state.world, p);
            restored |= cell
                .moving
                .as_ref()
                .is_some_and(|m| m.chunk_sequence.is_some());
            assert_eq!(
                serde_json::to_value(cell).unwrap(),
                *expected,
                "sequence={sequence} position={p:?}"
            );
        }
    }
    assert!(restored);
}

#[tokio::test]
async fn modern_disconnect_and_reconnect_do_not_reuse_world_or_connection_identity() {
    use crate::protocol::{read_packet, write_packet};
    use std::time::Duration;
    use tokio::net::TcpListener;
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let port = listener.local_addr().unwrap().port();
    let server = tokio::spawn(async move {
        for block_id in [1, 0] {
            let (mut stream, _) = listener.accept().await.unwrap();
            assert_eq!(read_packet(&mut stream, None).await.unwrap().0, 0);
            assert_eq!(
                read_packet(&mut stream, None).await.unwrap().0,
                ids::login_serverbound::LOGIN_START
            );
            let mut success = vec![0; 16];
            put_string(&mut success, "MockProbe");
            success.push(0);
            write_packet(&mut stream, None, ids::login_clientbound::SUCCESS, &success)
                .await
                .unwrap();
            assert_eq!(
                read_packet(&mut stream, None).await.unwrap().0,
                ids::login_serverbound::LOGIN_ACKNOWLEDGED
            );
            assert_eq!(
                read_packet(&mut stream, None).await.unwrap().0,
                ids::configuration_serverbound::SETTINGS
            );
            let mut registry = Vec::new();
            put_string(&mut registry, "minecraft:dimension_type");
            registry.push(1);
            put_string(&mut registry, "minecraft:overworld");
            registry.extend([1, 10]);
            for (key, value) in [("min_y", -64i32), ("height", 384)] {
                registry.push(3);
                registry.extend((key.len() as u16).to_be_bytes());
                registry.extend(key.as_bytes());
                registry.extend(value.to_be_bytes());
            }
            registry.push(0);
            write_packet(
                &mut stream,
                None,
                ids::configuration_clientbound::REGISTRY_DATA,
                &registry,
            )
            .await
            .unwrap();
            write_packet(
                &mut stream,
                None,
                ids::configuration_clientbound::FINISH_CONFIGURATION,
                &[],
            )
            .await
            .unwrap();
            assert_eq!(
                read_packet(&mut stream, None).await.unwrap().0,
                ids::configuration_serverbound::FINISH_CONFIGURATION
            );
            let mut join = vec![0; 4]; // entity ID
            join.extend([0, 0, 1, 4, 4, 0, 1, 0, 0]);
            put_string(&mut join, "minecraft:overworld");
            join.extend([0; 8]);
            join.extend([1, 255, 0, 1, 0, 0, 63, 0]);
            write_packet(&mut stream, None, ids::play_clientbound::LOGIN, &join)
                .await
                .unwrap();
            write_packet(
                &mut stream,
                None,
                ids::play_clientbound::GAME_STATE_CHANGE,
                &[13, 0, 0, 0, 0],
            )
            .await
            .unwrap();
            let mut chunk = vec![0; 9]; // coordinates and empty heightmaps
            let mut sections = Vec::new();
            for _ in 0..24 {
                sections.extend((if block_id == 0 { 0u16 } else { 4096 }).to_be_bytes());
                sections.extend([0, block_id, 0, 0]);
            }
            put_varint(&mut chunk, sections.len() as i32);
            chunk.extend(sections);
            chunk.extend([0; 7]); // block entities, masks and light arrays
            write_packet(&mut stream, None, ids::play_clientbound::MAP_CHUNK, &chunk)
                .await
                .unwrap();
            let position = vec![0; 61]; // teleport varint, six doubles, two floats, flags
            write_packet(
                &mut stream,
                None,
                ids::play_clientbound::POSITION,
                &position,
            )
            .await
            .unwrap();
            while read_packet(&mut stream, None).await.is_ok() {}
        }
    });
    timeout(Duration::from_secs(5), async {
        let region = Region {
            min: [0, -64, 0],
            max: [0, -64, 0],
        };
        let connect = || {
            Bot::connect(ConnectionConfig::offline(
                crate::Server::new("127.0.0.1", port),
                "MockProbe",
                MinecraftVersion::Java1_21_11,
            ))
        };
        let first = connect().await.unwrap();
        first.wait_until_ready().await.unwrap();
        let old = first.observe_region(region).await.unwrap();
        assert_eq!(
            old.blocks[0].state.as_ref().unwrap().name,
            "minecraft:stone"
        );
        first.disconnect().await.unwrap();
        assert!(first.observe_region(region).await.is_err());
        let second = connect().await.unwrap();
        second.wait_until_ready().await.unwrap();
        let fresh = second.observe_region(region).await.unwrap();
        assert_eq!(
            fresh.blocks[0].state.as_ref().unwrap().name,
            "minecraft:air"
        );
        assert_ne!(old.connection_id, fresh.connection_id);
        second.disconnect().await.unwrap();
        server.await.unwrap();
    })
    .await
    .unwrap();
}

#[test]
fn own_correction_resolves_native_rotation_velocity_and_keeps_submission_separate() {
    let fixture: serde_json::Value = serde_json::from_str(include_str!(
        "../../../../data/java_1_21_11/position_corrections.json"
    ))
    .unwrap();
    let mut s = play_state();
    let before = &fixture[0]["before"];
    let mut initial = vec![1];
    for key in ["position", "velocity"] {
        for v in before[key].as_array().unwrap() {
            initial.extend(v.as_f64().unwrap().to_be_bytes());
        }
    }
    for v in before["rotation"].as_array().unwrap() {
        initial.extend((v.as_f64().unwrap() as f32).to_be_bytes());
    }
    initial.extend(0u32.to_be_bytes());
    s.receive(ids::play_clientbound::POSITION, &initial, 0)
        .unwrap();
    let case = &fixture[0]["cases"][511];
    let mut correction = hex::decode(case["hex"].as_str().unwrap()).unwrap();
    correction.pop(); // Own correction has teleport ID instead of entity ID and no ground bit.
    let responses = s
        .receive(ids::play_clientbound::POSITION, &correction, 0)
        .unwrap();
    assert!(
        responses
            .iter()
            .any(|(id, bytes)| *id == ids::play_serverbound::TELEPORT_CONFIRM && bytes == &[42])
    );
    assert_eq!(s.rotation, [210.0, 80.0]);
    let v = s.operations.local_player.velocity.unwrap();
    for axis in 0..3 {
        assert!(
            (v.value[axis] - case["expected"]["velocity"][axis].as_f64().unwrap()).abs() < 1e-10
        );
    }
    assert_eq!(s.motion.received_pose.as_ref().unwrap().receive_sequence, 2);
    s.motion
        .begin(
            s.loading.generation,
            s.sequence,
            s.position.unwrap(),
            s.rotation,
        )
        .unwrap();
    s.motion.dispatched();
    s.receive(ids::play_clientbound::POSITION, &correction, 0)
        .unwrap();
    assert!(s.operations.local_player.velocity.is_none());
    assert!(s.motion.received_pose.as_ref().unwrap().velocity.is_none());
    assert_eq!(
        s.motion.last_submission.as_ref().unwrap().superseded_at,
        Some(3)
    );
    let prior = serde_json::to_value(&s.motion).unwrap();
    let len = correction.len();
    correction[len - 4..].copy_from_slice(&512u32.to_be_bytes());
    assert!(
        s.receive(ids::play_clientbound::POSITION, &correction, 0)
            .is_err()
    );
    assert_eq!(serde_json::to_value(&s.motion).unwrap(), prior);
    assert!(s.failure.is_some());
}
