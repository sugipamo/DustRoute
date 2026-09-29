use super::*;
use crate::protocol::{put_string, put_varint};
use ids::play_clientbound as p;

const UUID: [u8; 16] = [7; 16];
fn profile() -> Vec<u8> {
    let mut bytes = vec![255, 1];
    bytes.extend(UUID);
    put_string(&mut bytes, "Builder");
    bytes.push(0); // Properties.
    bytes.push(0); // No chat session.
    bytes.extend([1, 1, 0, 0]); // Game mode, listed, latency, absent display name.
    put_varint(&mut bytes, 456); // List order precedes show-hat.
    bytes.push(1);
    bytes
}
fn spawn() -> Vec<u8> {
    let mut bytes = vec![42];
    bytes.extend(UUID);
    put_varint(&mut bytes, ids::PLAYER_ENTITY_TYPE);
    for coordinate in [-0.5 / 4096.0f64, 80.123, 10.5] {
        bytes.extend(coordinate.to_be_bytes());
    }
    bytes.extend([0, 32, 0, 64, 0]); // Zero velocity, pitch, body yaw, head yaw, data.
    bytes
}
fn tracker() -> PlayerTracker {
    let mut tracker = PlayerTracker::default();
    tracker.receive(p::PLAYER_INFO, &profile(), 1).unwrap();
    tracker.receive(p::SPAWN_ENTITY, &spawn(), 2).unwrap();
    tracker
}
#[test]
fn observes_native_rounding_head_rotation_pose_and_scale_without_interpolation() {
    let mut tracker = tracker();
    let first = tracker.observations().remove(0);
    assert_eq!(first.rotation, [90.0, 45.0]);
    assert_eq!(first.name, "Builder");
    let movement = [42, 0, 1, 0, 0, 0, 0, 1];
    tracker.receive(p::REL_ENTITY_MOVE, &movement, 3).unwrap();
    let moved = tracker.observations().remove(0);
    assert_eq!(moved.position[0], 1.0 / 4096.0); // Java rounds negative half to zero.
    assert_eq!(moved.position[1], 80.123); // Zero delta preserves non-quantized coordinate.
    tracker
        .receive(p::ENTITY_HEAD_ROTATION, &[42, 128], 4)
        .unwrap();
    tracker
        .receive(
            p::ENTITY_METADATA,
            &[42, ids::PLAYER_POSE_METADATA, 20, 5, 255],
            5,
        )
        .unwrap();
    let mut scale = vec![42, 1, ids::SCALE_ATTRIBUTE as u8];
    scale.extend(1.0f64.to_be_bytes());
    scale.push(3);
    for (name, amount, operation) in [("add", 1.0f64, 0), ("base", 0.5, 1), ("total", 1.0, 2)] {
        put_string(&mut scale, name);
        scale.extend(amount.to_be_bytes());
        scale.push(operation);
    }
    tracker
        .receive(p::ENTITY_UPDATE_ATTRIBUTES, &scale, 6)
        .unwrap();
    let crouched = tracker.observations().remove(0);
    assert_eq!(crouched.rotation, [-180.0, 45.0]);
    assert_eq!(crouched.scale, 6.0);
    assert_eq!(
        crouched.eye_position.unwrap()[1],
        80.123 + f64::from(1.27f32 * 6.0)
    );
    assert_eq!(crouched.receive_sequence, 6);
}
#[test]
fn truncated_packets_are_atomic_and_unknown_metadata_invalidates_viewpoint() {
    let mut tracker = tracker();
    let before = serde_json::to_value(tracker.observations()).unwrap();
    for length in 1..8 {
        assert!(
            tracker
                .receive(p::REL_ENTITY_MOVE, &[42, 0, 1, 0, 1, 0, 1, 0][..length], 3)
                .is_err()
        );
        assert_eq!(
            serde_json::to_value(tracker.observations()).unwrap(),
            before
        );
    }
    tracker
        .receive(p::ENTITY_METADATA, &[42, 42, 127], 4)
        .unwrap();
    assert!(tracker.observations()[0].eye_position.is_none());
    // A health-only update cannot recover an unknown pose.
    let mut health = vec![42, 9, 3];
    health.extend(20f32.to_be_bytes());
    health.push(255);
    tracker.receive(p::ENTITY_METADATA, &health, 5).unwrap();
    assert!(tracker.observations()[0].eye_position.is_none());
    tracker
        .receive(p::ENTITY_METADATA, &[42, 6, 20, 0, 255], 6)
        .unwrap();
    assert!(tracker.observations()[0].eye_position.is_some());
    tracker
        .receive(p::ENTITY_METADATA, &[42, 6, 20, 2, 255], 7)
        .unwrap();
    assert!(tracker.observations()[0].eye_position.is_none());
}
#[test]
fn lifecycle_never_revives_old_entity_after_removal_or_dimension_change() {
    let mut tracker = tracker();
    tracker.reset_world();
    assert!(tracker.observations().is_empty());
    tracker.receive(p::SPAWN_ENTITY, &spawn(), 3).unwrap();
    assert_eq!(tracker.observations().len(), 1);
    let mut remove = vec![1];
    remove.extend(UUID);
    tracker.receive(p::PLAYER_REMOVE, &remove, 4).unwrap();
    tracker.receive(p::PLAYER_INFO, &profile(), 5).unwrap();
    assert!(tracker.observations().is_empty());
    tracker.receive(p::SPAWN_ENTITY, &spawn(), 6).unwrap();
    tracker.receive(p::ENTITY_DESTROY, &[1, 42], 7).unwrap();
    assert!(tracker.observations().is_empty());
    tracker.receive(p::SPAWN_ENTITY, &spawn(), 8).unwrap();
    let mut non_player = vec![42];
    non_player.extend([8; 16]);
    non_player.push(1);
    tracker.receive(p::SPAWN_ENTITY, &non_player, 9).unwrap();
    assert!(tracker.observations().is_empty());
}
#[test]
fn common_potion_metadata_keeps_pose_and_native_velocity_framing_is_bounded() {
    let mut tracker = tracker();
    let metadata = [
        42,
        10,
        17,
        1,
        ids::ENTITY_EFFECT_PARTICLE as u8,
        255,
        0,
        255,
        0,
        6,
        20,
        3,
        255,
    ];
    tracker.receive(p::ENTITY_METADATA, &metadata, 3).unwrap();
    assert_eq!(tracker.observations()[0].pose, Some(PlayerPose::Swimming));
    let mut moving = spawn();
    let velocity_index = moving.len() - 5;
    moving.splice(velocity_index..=velocity_index, [5, 0, 0, 0, 0, 0, 128, 1]);
    tracker.receive(p::SPAWN_ENTITY, &moving, 4).unwrap();
    assert_eq!(tracker.observations()[0].rotation, [90.0, 45.0]);
    moving.pop();
    assert!(tracker.receive(p::SPAWN_ENTITY, &moving, 5).is_err());
}

#[test]
fn native_attribute_registry_does_not_confuse_movement_speed_with_scale() {
    let mut tracker = tracker();
    // Native trial: movement_speed ID 22 and scale ID 25 (the protocol mapper
    // in the pinned data package incorrectly calls ID 22 scale).
    let mut speed = vec![42, 1, 22];
    speed.extend(0.1f64.to_be_bytes());
    speed.push(0);
    tracker
        .receive(p::ENTITY_UPDATE_ATTRIBUTES, &speed, 3)
        .unwrap();
    assert_eq!(tracker.observations()[0].scale, 1.0);
    let mut scale = vec![42, 1, 25];
    scale.extend(2.0f64.to_be_bytes());
    scale.push(0);
    tracker
        .receive(p::ENTITY_UPDATE_ATTRIBUTES, &scale, 4)
        .unwrap();
    assert_eq!(tracker.observations()[0].scale, 2.0);
}

#[test]
fn retained_native_two_client_trace_replays_every_reported_observation() {
    let bytes = include_bytes!("../../../../../docs/evidence/client-players-b-20260929.json.gz");
    let record: serde_json::Value =
        serde_json::from_reader(flate2::read::GzDecoder::new(&bytes[..])).unwrap();
    let expected: BTreeMap<u64, &serde_json::Value> =
        ["initial", "moved", "scaled", "departed", "joined"]
            .into_iter()
            .map(|key| {
                (
                    record[key]["receive_sequence"].as_u64().unwrap(),
                    &record[key]["players"],
                )
            })
            .collect();
    let mut tracker = PlayerTracker::default();
    let mut compared = 0;
    for packet in record["trace"]["records"].as_array().unwrap() {
        let sequence = packet["sequence"].as_u64().unwrap();
        let bytes: Vec<u8> = serde_json::from_value(packet["payload"].clone()).unwrap();
        tracker
            .receive(
                packet["packet_id"].as_i64().unwrap() as i32,
                &bytes,
                sequence,
            )
            .unwrap();
        if let Some(players) = expected.get(&sequence) {
            // Compare through the same JSON decoding boundary as the saved
            // evidence (serde_json's default float parser is not round-trip mode).
            let encoded = serde_json::to_vec(&tracker.observations()).unwrap();
            let decoded: serde_json::Value = serde_json::from_slice(&encoded).unwrap();
            assert_eq!(&decoded, *players);
            compared += 1;
        }
    }
    assert_eq!(compared, 5);
}
