use super::*;
use crate::versions::java_1_21_11::{native_state, state_id, world::Dimension};

fn fixture() -> serde_json::Value {
    serde_json::from_str(include_str!(
        "../../../../../../data/java_1_21_11/survival_foundation.json"
    ))
    .unwrap()
}
fn state() -> State {
    let mut s = State {
        ready: true,
        loading: loading::InteractionLoading::completed_fixture(),
        position: Some([0.5, 1.0, 0.5]),
        sequence: 10,
        ..State::default()
    };
    s.operations.reset_world(0).unwrap();
    s.operations.local_player = LocalPlayerState::spawned(42);
    s.operations.local_player.velocity = Some(VelocitySample {
        value: [0.0; 3],
        receive_sequence: 10,
    });
    s.motion.receive(ReceivedPose {
        generation: s.loading.generation,
        receive_sequence: s.sequence,
        position: s.position.unwrap(),
        rotation: s.rotation,
        velocity: Some([0.0; 3]),
    });
    s.world.select_dimension(
        "minecraft:overworld".into(),
        Dimension::new(-64, 384).unwrap(),
    );
    for x in -1..=2 {
        for z in -1..=2 {
            s.world.seed_replay_cell([x, 0, z], 0);
        }
    }
    s.world.seed_replay_cell([0, 0, 0], 1); // Native stone.
    s
}
fn owned(id: i32, tail: &[u8]) -> Vec<u8> {
    let mut p = Vec::new();
    put_varint(&mut p, id);
    p.extend(tail);
    p
}
fn apply(s: &mut State, id: i32, payload: &[u8]) {
    s.sequence += 1;
    assert!(receive(s, id, payload).unwrap());
}

#[test]
fn native_attribute_ids_defaults_and_packed_velocities_match_game_oracle() {
    let oracle = fixture();
    assert_eq!(
        oracle["dry_cubes"],
        serde_json::to_value(DRY_CUBES).unwrap()
    );
    let local = LocalPlayerState::spawned(42);
    assert_eq!(local.scale.unwrap().basis, ValueBasis::NativeReset);
    assert!(local.health.is_none());
    assert!(local.velocity.is_none());
    assert!(!local.effects_complete);
    for case in oracle["velocities"].as_array().unwrap() {
        let bytes = hex::decode(case["hex"].as_str().unwrap()).unwrap();
        let mut r = Reader::new(&bytes);
        let value = velocity(&mut r).unwrap();
        r.end().unwrap();
        for (axis, component) in value.iter().enumerate() {
            assert!((component - case["decoded"][axis].as_f64().unwrap()).abs() < 1e-10);
        }
        for end in 0..bytes.len() {
            assert!(velocity(&mut Reader::new(&bytes[..end])).is_err());
        }
    }
}

#[test]
fn own_receive_is_atomic_and_cannot_take_remote_entity_state() {
    use ids::play_clientbound as p;
    let mut s = state();
    let health = [
        20f32.to_be_bytes().as_slice(),
        &[20],
        5f32.to_be_bytes().as_slice(),
    ]
    .concat();
    let metadata = owned(42, &[6, 20, 5, 255]);
    let velocity = owned(
        42,
        &hex::decode(fixture()["velocities"][1]["hex"].as_str().unwrap()).unwrap(),
    );
    let mut attribute = vec![42, 1, 5];
    attribute.extend(2f64.to_be_bytes());
    attribute.push(0);
    let effect = owned(42, &[3, 1, 100, 7]);
    for (id, bytes) in [
        (p::UPDATE_HEALTH, health),
        (p::ENTITY_METADATA, metadata),
        (p::ENTITY_VELOCITY, velocity),
        (p::ENTITY_UPDATE_ATTRIBUTES, attribute),
        (p::ENTITY_EFFECT, effect),
    ] {
        for end in 0..bytes.len() {
            let before = s.operations.local_player.clone();
            assert!(receive(&mut s, id, &bytes[..end]).is_err());
            assert_eq!(s.operations.local_player, before);
        }
        apply(&mut s, id, &bytes);
        if id != p::UPDATE_HEALTH {
            let mut remote = bytes.clone();
            remote[0] = 43;
            let before = s.operations.local_player.clone();
            assert!(!receive(&mut s, id, &remote).unwrap());
            assert_eq!(s.operations.local_player, before);
        }
    }
    assert_eq!(s.operations.local_player.pose, Some(PlayerPose::Crouching));
    assert_eq!(
        s.operations.local_player.block_break_speed.unwrap().value,
        2.0
    );
    assert_eq!(
        s.operations.local_player.effect_updates[&3].duration_at_receipt,
        100
    );
    apply(&mut s, p::REMOVE_ENTITY_EFFECT, &[42, 3]);
    assert!(s.operations.local_player.effect_updates.is_empty());
    assert!(!s.operations.local_player.effects_complete);
    apply(&mut s, p::ENTITY_METADATA, &[42, 6, 20, 0, 255]);
    assert_eq!(
        s.operations.local_player.pose_basis,
        Some(ValueBasis::Received {
            sequence: s.sequence
        })
    );
    apply(&mut s, p::ENTITY_METADATA, &[42, 7, 7]); // Unsupported slot serializer.
    assert!(s.operations.local_player.pose.is_none());
    assert!(s.operations.local_player.pose_basis.is_none());
    assert!(context(&mut s, 1, 0).is_err());
    s.operations.reset_world(0).unwrap();
    assert_eq!(s.operations.local_player.entity_id, Some(42));
    assert_eq!(
        s.operations.local_player.pose_basis,
        Some(ValueBasis::NativeReset)
    );
    assert!(s.operations.local_player.health.is_none());
    assert!(s.operations.local_player.velocity.is_none());
    s.operations.reset_configuration(20);
    assert_eq!(s.operations.local_player, LocalPlayerState::default());
}

#[test]
fn attributes_use_native_modifier_order_and_limits_without_confusing_scale_ids() {
    let mut s = state();
    let mut packet = vec![42, 2, 5];
    packet.extend(2f64.to_be_bytes());
    packet.push(3);
    for (name, amount, operation) in [
        ("minecraft:add", 3.0f64, 0),
        ("minecraft:base", 0.5, 1),
        ("minecraft:total", 1.0, 2),
    ] {
        put_string(&mut packet, name);
        packet.extend(amount.to_be_bytes());
        packet.push(operation);
    }
    packet.push(22); // Native movement speed, not scale.
    packet.extend(3f64.to_be_bytes());
    packet.push(0);
    apply(
        &mut s,
        ids::play_clientbound::ENTITY_UPDATE_ATTRIBUTES,
        &packet,
    );
    assert_eq!(
        s.operations.local_player.block_break_speed.unwrap().value,
        15.0
    );
    assert_eq!(s.operations.local_player.scale.unwrap().value, 1.0);
    let mut clipped = vec![42, 1, 25];
    clipped.extend(99f64.to_be_bytes());
    clipped.push(0);
    apply(
        &mut s,
        ids::play_clientbound::ENTITY_UPDATE_ATTRIBUTES,
        &clipped,
    );
    assert_eq!(s.operations.local_player.scale.unwrap().value, 16.0);
    assert!(context(&mut s, 1, 0).is_err());
}

#[test]
fn standing_body_contacts_match_native_and_support_is_rechecked_after_world_edit() {
    let f = fixture();
    for case in f["contacts"].as_array().unwrap() {
        let mut s = state();
        s.position = Some(std::array::from_fn(|axis| {
            case["position"][axis].as_f64().unwrap()
        }));
        s.motion.receive(ReceivedPose {
            generation: s.loading.generation,
            receive_sequence: s.sequence,
            position: s.position.unwrap(),
            rotation: s.rotation,
            velocity: Some([0.0; 3]),
        });
        let observed = context(&mut s, 77, 0);
        if case["clear"].as_bool().unwrap() {
            let observed = observed.unwrap();
            assert_eq!(observed.on_ground, case["ground"].as_bool().unwrap());
            assert_eq!(observed.connection_id, 77);
            assert!(!observed.submerged);
            assert_eq!(
                observed.bounds[4] - observed.position[1],
                f64::from(f["standing_dimensions"][1].as_f64().unwrap() as f32)
            );
        } else {
            assert!(observed.is_err());
        }
    }
    let mut s = state();
    assert_eq!(context(&mut s, 77, 0).unwrap().support, [[0, 0, 0]]);
    let mut change = pack_position([0, 0, 0]).to_be_bytes().to_vec();
    change.push(0);
    s.world.block_change(&change).unwrap();
    let after = context(&mut s, 77, 0).unwrap();
    assert!(!after.on_ground);
    assert!(after.support.is_empty());
    assert_eq!(after.world_revision, s.world.revision);
    // Negative positions and chunk boundaries must not read the wrong section.
    s.position = Some([-0.5, 1.0, -0.5]);
    s.motion.receive(ReceivedPose {
        generation: s.loading.generation,
        receive_sequence: s.sequence,
        position: s.position.unwrap(),
        rotation: s.rotation,
        velocity: Some([0.0; 3]),
    });
    s.world.seed_replay_cell([-1, 0, -1], 1);
    assert!(context(&mut s, 77, 0).unwrap().on_ground);
    s.position = Some([16.0, 1.0, 0.5]);
    s.motion.receive(ReceivedPose {
        generation: s.loading.generation,
        receive_sequence: s.sequence,
        position: s.position.unwrap(),
        rotation: s.rotation,
        velocity: Some([0.0; 3]),
    });
    assert!(context(&mut s, 77, 0).is_err()); // Adjacent chunk is not loaded.
}

#[test]
fn unsupported_posture_motion_fluid_and_reconstruction_never_become_ground_evidence() {
    let mut s = state();
    s.operations.local_player.pose = Some(PlayerPose::Swimming);
    assert!(context(&mut s, 1, 0).is_err());
    s.operations.local_player.pose = Some(PlayerPose::Standing);
    s.operations.local_player.velocity.as_mut().unwrap().value[1] = -0.08;
    assert!(context(&mut s, 1, 0).is_err());
    s.operations.local_player.velocity = None;
    assert!(context(&mut s, 1, 0).is_err());
    s.operations.local_player.velocity = Some(VelocitySample {
        value: [0.0; 3],
        receive_sequence: 11,
    });
    s.motion.invalidate(s.sequence, "test local movement");
    assert!(context(&mut s, 1, 0).is_err());
    s.motion.receive(ReceivedPose {
        generation: s.loading.generation,
        receive_sequence: s.sequence,
        position: s.position.unwrap(),
        rotation: s.rotation,
        velocity: Some([0.0; 3]),
    });
    s.operations.requested_flying = true;
    assert!(context(&mut s, 1, 0).is_err());
    s.operations.requested_flying = false;
    let water = (0..500)
        .map(|id| native_state(id).unwrap())
        .find(|b| b.name == "minecraft:water")
        .unwrap();
    s.world
        .seed_replay_cell([0, 2, 0], state_id(&water).unwrap());
    assert_eq!(
        context(&mut s, 1, 0).unwrap_err().kind(),
        ErrorKind::Unsupported
    );
    s.world.seed_replay_cell([0, 2, 0], 0);
    s.reconstruction.unsupported_ticking();
    assert!(context(&mut s, 1, 0).is_err());
}

#[tokio::test]
async fn survival_look_sends_native_ground_bit_and_refusal_sends_nothing() {
    use super::super::super::{Lease, Session, Writer};
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let connection = tokio::net::TcpStream::connect(listener.local_addr().unwrap())
        .await
        .unwrap();
    let (mut peer, _) = listener.accept().await.unwrap();
    let (_, writer) = connection.into_split();
    let session = Arc::new(Session {
        id: 42,
        started: Instant::now(),
        writer: Mutex::new(Writer {
            stream: writer,
            compression: None,
        }),
        state: Mutex::new(state()),
        changed: Notify::new(),
        cancel: Notify::new(),
        stopped: AtomicBool::new(false),
        interrupted_packet: AtomicI32::new(-1),
        limits: crate::ConnectionOptions::default(),
        interaction_sequence: AtomicI32::new(0),
    });
    let operations = Operations {
        bot: Bot {
            session: session.clone(),
            _lease: Arc::new(Lease(Arc::downgrade(&session))),
        },
    };
    operations.look([45.0, -20.0]).await.unwrap();
    let (id, payload) = read_packet(&mut peer, None).await.unwrap();
    assert_eq!(id, ids::play_serverbound::LOOK);
    assert_eq!(
        payload,
        hex::decode(fixture()["looks"][1]["hex"].as_str().unwrap()).unwrap()
    );
    assert!(operations.standing_context().await.unwrap().on_ground);
    {
        let mut s = session.state.lock().await;
        s.operations.local_player.velocity = None;
    }
    assert!(operations.look([60.0, 0.0]).await.is_err());
    assert!(
        timeout(
            std::time::Duration::from_millis(5),
            read_packet(&mut peer, None)
        )
        .await
        .is_err()
    );
    assert_eq!(
        operations.player_state().await.unwrap().rotation,
        [45.0, -20.0]
    );
    {
        let mut s = session.state.lock().await;
        s.operations.reset_world(1).unwrap(); // Creative flight stays on its existing path.
        s.ready = true;
    }
    operations.look([45.0, -20.0]).await.unwrap();
    let (_, payload) = read_packet(&mut peer, None).await.unwrap();
    assert_eq!(
        payload,
        hex::decode(fixture()["looks"][0]["hex"].as_str().unwrap()).unwrap()
    );
}

#[test]
fn unsupported_impulses_and_own_vehicle_cannot_leave_stale_stationary_authority() {
    use ids::play_clientbound as p;
    let mut s = state();
    let before = s.operations.local_player.clone();
    apply(&mut s, p::SET_PASSENGERS, &[10, 1, 43]); // Another player's vehicle.
    assert_eq!(s.operations.local_player, before);
    for bytes in [&[10, 1][..], &[10, 1, 42, 0][..]] {
        assert!(receive(&mut s, p::SET_PASSENGERS, bytes).is_err());
        assert_eq!(s.operations.local_player, before);
    }
    apply(&mut s, p::SET_PASSENGERS, &[10, 1, 42]);
    assert!(context(&mut s, 1, 0).is_err());
    assert!(s.operations.local_player.velocity.is_none());
    s.operations.local_player.velocity = Some(VelocitySample {
        value: [0.0; 3],
        receive_sequence: 13,
    });
    assert!(context(&mut s, 1, 0).is_err()); // Zero velocity alone cannot prove dismount.
    for id in [p::EXPLOSION, p::VEHICLE_MOVE] {
        s.operations.reset_world(0).unwrap();
        s.operations.local_player.velocity = Some(VelocitySample {
            value: [0.0; 3],
            receive_sequence: 14,
        });
        s.motion.receive(ReceivedPose {
            generation: s.loading.generation,
            receive_sequence: s.sequence,
            position: s.position.unwrap(),
            rotation: s.rotation,
            velocity: Some([0.0; 3]),
        });
        assert!(context(&mut s, 1, 0).unwrap().on_ground);
        apply(&mut s, id, &[]); // Even undecoded/malformed unsupported payload cannot grant permission.
        let issue = s
            .operations
            .local_player
            .motion_interruption
            .as_ref()
            .unwrap();
        assert_eq!(issue.packet_id, id);
        assert_eq!(issue.receive_sequence, s.sequence);
        assert!(context(&mut s, 1, 0).is_err());
    }
}

#[test]
fn recovery_waits_for_standing_halo_across_chunk_edges() {
    let mut s = state();
    assert!(standing_baselines_received(&s).unwrap());
    s.position = Some([15.5, 1.0, 0.5]);
    assert!(s.world.block([15, 1, 0]).is_some());
    assert!(!standing_baselines_received(&s).unwrap());
    s.world.seed_replay_cell([16, 0, 0], 1);
    assert!(!standing_baselines_received(&s).unwrap()); // Negative-z neighbor still missing.
    s.world.seed_replay_cell([16, 0, -1], 1);
    assert!(standing_baselines_received(&s).unwrap());
    s.position = Some([15.5, -64.0, 0.5]);
    assert!(standing_baselines_received(&s).is_err()); // Never wait for impossible terrain.
}

#[test]
fn movement_attributes_are_received_for_own_player_with_native_limits_and_reset_basis() {
    let mut s = state();
    let mut packet = vec![42, 8];
    for (id, base) in [
        (22, 0.25f64),
        (14, -2.0),
        (15, 99.0),
        (28, 0.75),
        (21, 0.4),
        (26, 0.6),
        (24, -5.0),
        (11, 2.0),
    ] {
        put_varint(&mut packet, id);
        packet.extend(base.to_be_bytes());
        packet.push(0);
    }
    apply(
        &mut s,
        ids::play_clientbound::ENTITY_UPDATE_ATTRIBUTES,
        &packet,
    );
    let p = &s.operations.local_player;
    for (v, expected) in [
        (p.movement_speed, 0.25),
        (p.gravity, -1.0),
        (p.jump_strength, 32.0),
        (p.step_height, 0.75),
        (p.movement_efficiency, 0.4),
        (p.sneaking_speed, 0.6),
        (p.safe_fall_distance, -5.0),
        (p.fall_damage_multiplier, 2.0),
    ] {
        let v = v.unwrap();
        assert_eq!(v.value, expected);
        assert_eq!(
            v.basis,
            ValueBasis::Received {
                sequence: s.sequence
            }
        );
    }
    assert_eq!(p.scale.unwrap().value, 1.0);
    let retained = p.clone();
    packet[0] = 43;
    assert!(
        !receive(
            &mut s,
            ids::play_clientbound::ENTITY_UPDATE_ATTRIBUTES,
            &packet
        )
        .unwrap()
    );
    assert_eq!(s.operations.local_player, retained);
    packet[0] = 42;
    for end in 0..packet.len() {
        assert!(
            receive(
                &mut s,
                ids::play_clientbound::ENTITY_UPDATE_ATTRIBUTES,
                &packet[..end]
            )
            .is_err()
        );
        assert_eq!(
            s.operations.local_player, retained,
            "truncated batch applied partial attributes"
        );
    }
    s.operations.reset_world(0).unwrap();
    let p = &s.operations.local_player;
    assert_eq!(p.movement_speed.unwrap().value, f64::from(0.1f32));
    assert_eq!(p.jump_strength.unwrap().value, f64::from(0.42f32));
    assert_eq!(p.gravity.unwrap().value, 0.08);
    assert_eq!(p.gravity.unwrap().basis, ValueBasis::NativeReset);
    s.operations.reset_configuration(200);
    assert_eq!(s.operations.local_player, LocalPlayerState::default());
}

#[test]
fn movement_attribute_modifiers_do_not_get_applied_twice_or_to_other_fields() {
    let mut s = state();
    let mut packet = vec![42, 1, 22];
    packet.extend(0.1f64.to_be_bytes());
    packet.push(3);
    for (name, amount, op) in [
        ("minecraft:add", 0.1f64, 0),
        ("minecraft:base", 0.5, 1),
        ("minecraft:total", 1.0, 2),
    ] {
        put_string(&mut packet, name);
        packet.extend(amount.to_be_bytes());
        packet.push(op);
    }
    for _ in 0..2 {
        apply(
            &mut s,
            ids::play_clientbound::ENTITY_UPDATE_ATTRIBUTES,
            &packet,
        );
        let p = &s.operations.local_player;
        assert!((p.movement_speed.unwrap().value - 0.6).abs() < 1e-15);
        assert_eq!(p.gravity.unwrap().value, 0.08);
        assert_eq!(p.step_height.unwrap().value, 0.6);
    }
}
