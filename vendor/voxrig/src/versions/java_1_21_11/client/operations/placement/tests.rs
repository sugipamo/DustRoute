use super::*;
use crate::versions::java_1_21_11::{state_id, world::Dimension};
use tokio::{
    net::{TcpListener, TcpStream},
    task::JoinHandle,
};
const SUPPORT: [i32; 3] = [2, 2, 0];
const TARGET: [i32; 3] = [1, 2, 0];
fn native(name: &str) -> crate::NativeBlockState {
    crate::NativeBlockState {
        name: format!("minecraft:{name}"),
        properties: Default::default(),
    }
}
fn fixture_state() -> State {
    let mut state = State {
        phase: Phase::Play,
        ready: true,
        position: Some([0.5, 1.0, 0.5]),
        rotation: [-90.0, 3.0],
        sequence: 10,
        loading: loading::InteractionLoading::completed_fixture(),
        ..State::default()
    };
    state.operations.reset_world(0).unwrap();
    state.operations.local_player = LocalPlayerState::spawned(42);
    state
        .operations
        .local_player
        .correct_velocity([0.0; 3], 0, 10)
        .unwrap();
    state.operations.local_player.health = Some(PlayerHealth {
        health: 20.0,
        food: 20,
        saturation: 5.0,
        receive_sequence: 10,
    });
    state.motion.receive(ReceivedPose {
        generation: state.loading.generation,
        receive_sequence: state.sequence,
        position: state.position.unwrap(),
        rotation: state.rotation,
        velocity: Some([0.0; 3]),
    });
    state.world.select_dimension(
        "minecraft:overworld".into(),
        Dimension::new(-64, 384).unwrap(),
    );
    for x in -2..=4 {
        for z in -2..=2 {
            state.world.seed_replay_cell([x, 0, z], 1);
        }
    }
    state
        .world
        .seed_replay_cell(SUPPORT, state_id(&native("stone")).unwrap());
    let mut slots = vec![0, 1, 46];
    for slot in 0..46 {
        if slot == 36 {
            slots.push(5);
            put_varint(&mut slots, default_item("dirt", 5).unwrap().item_id);
            slots.extend([0, 0]);
        } else {
            slots.push(0);
        }
    }
    slots.push(0);
    super::super::receive(&mut state, ids::play_clientbound::WINDOW_ITEMS, &slots).unwrap();
    state.operations.selected_hotbar = Some(HotbarSelection {
        slot: 0,
        sequence: 10,
        dispatched: true,
        from_server: true,
    });
    state
}
struct Fixture {
    api: Operations,
    peer: TcpStream,
    receiver: JoinHandle<()>,
}
impl Fixture {
    async fn new() -> Self {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let stream = TcpStream::connect(listener.local_addr().unwrap())
            .await
            .unwrap();
        let (peer, _) = listener.accept().await.unwrap();
        let (reader, writer) = stream.into_split();
        let session = Arc::new(Session {
            id: 72,
            started: Instant::now(),
            writer: Mutex::new(Writer {
                stream: writer,
                compression: None,
            }),
            state: Mutex::new(fixture_state()),
            changed: Notify::new(),
            cancel: Notify::new(),
            stopped: AtomicBool::new(false),
            interrupted_packet: AtomicI32::new(-1),
            limits: crate::ConnectionOptions::default(),
            interaction_sequence: AtomicI32::new(0),
        });
        let api = Operations {
            bot: Bot {
                session: session.clone(),
                _lease: Arc::new(Lease(Arc::downgrade(&session))),
            },
        };
        let receiver = tokio::spawn(async move {
            session.run_receiver(reader).await;
        });
        Self {
            api,
            peer,
            receiver,
        }
    }
    async fn receive(&mut self, id: i32, payload: &[u8]) {
        let before = self.api.bot.session.state.lock().await.sequence;
        write_packet(&mut self.peer, None, id, payload)
            .await
            .unwrap();
        timeout(Duration::from_secs(1), async {
            while self.api.bot.session.state.lock().await.sequence == before {
                tokio::task::yield_now().await;
            }
        })
        .await
        .unwrap();
    }
    async fn start(&mut self) -> PlacementIntent {
        let intent = self
            .api
            .place_survival_cube(SUPPORT, crate::BlockFace::West)
            .await
            .unwrap();
        assert_eq!(intent.target, TARGET);
        let (id, p) = read_packet(&mut self.peer, None).await.unwrap();
        assert_eq!(id, ids::play_serverbound::BLOCK_PLACE);
        let mut r = Reader::new(&p);
        assert_eq!(r.varint().unwrap(), 0);
        assert_eq!(
            super::super::super::super::wire::unpack_position(r.u64().unwrap()),
            SUPPORT
        );
        assert_eq!(r.varint().unwrap(), 4);
        assert_eq!(r.f32().unwrap(), 0.0);
        assert!((0.0..1.0).contains(&r.f32().unwrap()));
        assert_eq!(r.f32().unwrap(), 0.5);
        assert!(!r.bool().unwrap());
        assert!(!r.bool().unwrap());
        assert_eq!(r.varint().unwrap(), intent.sequence);
        r.end().unwrap();
        intent
    }
    async fn block(&mut self, p: [i32; 3], name: &str) {
        let mut payload = pack_position(p).to_be_bytes().to_vec();
        put_varint(&mut payload, state_id(&native(name)).unwrap());
        self.receive(ids::play_clientbound::BLOCK_CHANGE, &payload)
            .await;
    }
    async fn stack(&mut self, count: i32) {
        let mut p = vec![0, 2];
        p.extend(36i16.to_be_bytes());
        put_varint(&mut p, count);
        if count > 0 {
            put_varint(&mut p, default_item("dirt", 1).unwrap().item_id);
            p.extend([0, 0]);
        }
        self.receive(ids::play_clientbound::SET_SLOT, &p).await;
    }
    async fn ack(&mut self, sequence: i32) {
        let mut p = Vec::new();
        put_varint(&mut p, sequence);
        self.receive(ids::play_clientbound::ACKNOWLEDGE_PLAYER_DIGGING, &p)
            .await;
    }
    async fn stop(self) {
        self.api.bot.disconnect().await.unwrap();
        timeout(Duration::from_secs(1), self.receiver)
            .await
            .unwrap()
            .unwrap();
    }
}
#[tokio::test]
async fn placement_requires_both_receipts_and_processed_sequence_in_either_order() {
    for inventory_first in [false, true] {
        let mut f = Fixture::new().await;
        let i = f.start().await;
        assert!(f.api.select_hotbar(1).await.is_err());
        assert!(
            f.api
                .use_on_block(SUPPORT, crate::BlockFace::West, [0.5; 3])
                .await
                .is_err()
        );
        assert!(
            f.api
                .place_survival_cube(SUPPORT, crate::BlockFace::West)
                .await
                .is_err()
        );
        f.ack(i.sequence).await;
        assert!(matches!(
            f.api
                .wait_survival_placement(&i, Duration::from_millis(5))
                .await
                .unwrap(),
            PlacementStatus::Pending { .. }
        ));
        if inventory_first {
            f.stack(4).await;
        } else {
            f.block(TARGET, "dirt").await;
        }
        assert!(matches!(
            f.api.observe_survival_placement(&i).await.unwrap(),
            PlacementStatus::Pending { .. }
        ));
        if inventory_first {
            f.block(TARGET, "dirt").await;
        } else {
            f.stack(4).await;
        }
        let PlacementStatus::ObservedPlaced { observation } = f
            .api
            .wait_survival_placement(&i, Duration::from_secs(1))
            .await
            .unwrap()
        else {
            panic!("missing both receipts");
        };
        assert_eq!(
            observation.held_after,
            InventorySlot::Item {
                item: default_item("dirt", 4).unwrap()
            }
        );
        f.api.select_hotbar(1).await.unwrap();
        assert_eq!(
            read_packet(&mut f.peer, None).await.unwrap().0,
            ids::play_serverbound::HELD_ITEM_SLOT
        );
        f.stop().await;
    }
}
#[tokio::test]
async fn missing_ack_last_item_and_conflicting_receipts_do_not_silently_release() {
    let mut f = Fixture::new().await;
    f.stack(1).await;
    let i = f.start().await;
    f.block(TARGET, "dirt").await;
    f.stack(0).await;
    assert!(matches!(
        f.api.observe_survival_placement(&i).await.unwrap(),
        PlacementStatus::Pending { .. }
    ));
    f.ack(i.sequence).await;
    assert!(matches!(
        f.api.observe_survival_placement(&i).await.unwrap(),
        PlacementStatus::ObservedPlaced { .. }
    ));
    f.stop().await;
    for inventory_conflict in [false, true] {
        let mut f = Fixture::new().await;
        let i = f.start().await;
        if inventory_conflict {
            f.stack(3).await;
            f.stack(4).await;
        } else {
            f.block(TARGET, "dirt").await;
            f.block(TARGET, "air").await;
            f.block(TARGET, "dirt").await;
        }
        f.ack(i.sequence).await;
        assert!(matches!(
            f.api.observe_survival_placement(&i).await.unwrap(),
            PlacementStatus::RequiresInspection { .. }
        ));
        assert!(f.api.select_hotbar(1).await.is_err());
        f.stop().await;
    }
}
#[tokio::test]
async fn cancelled_send_retains_intent_without_replay_and_closed_history_survives() {
    let f = Fixture::new().await;
    let writer = f.api.bot.session.writer.lock().await;
    assert!(
        timeout(
            Duration::from_millis(10),
            f.api.place_survival_cube(SUPPORT, crate::BlockFace::West)
        )
        .await
        .is_err()
    );
    drop(writer);
    let h = f.api.operation_history().await;
    assert!(!h.placement.unwrap().dispatched);
    assert!(
        f.api
            .place_survival_cube(SUPPORT, crate::BlockFace::West)
            .await
            .is_err()
    );
    f.api.bot.disconnect().await.unwrap();
    assert!(f.api.operation_history().await.placement.is_some());
    f.stop().await;
}
#[test]
fn admission_rejects_body_unknown_target_wrong_hit_empty_or_unsupported_material() {
    let mut s = fixture_state();
    assert!(prepare(&mut s, 1, 0, SUPPORT, 4).is_ok());
    assert!(prepare(&mut s, 1, 0, SUPPORT, 1).is_err());
    s.world.seed_replay_cell(TARGET, 1);
    assert!(prepare(&mut s, 1, 0, SUPPORT, 4).is_err());
    let mut s = fixture_state();
    s.operations.inventory.slots[36] = InventorySlot::Empty;
    assert!(prepare(&mut s, 1, 0, SUPPORT, 4).is_err());
    s.operations.inventory.slots[36] = InventorySlot::Item {
        item: default_item("sand", 5).unwrap(),
    };
    assert!(prepare(&mut s, 1, 0, SUPPORT, 4).is_err());
    assert!(survival::standing_intersects([0.5, 1.0, 0.5], [0, 2, 0]));
    assert!(!survival::standing_intersects([0.5, 1.0, 0.5], TARGET));
}

#[test]
fn packet_and_admitted_materials_match_native_oracle() {
    let oracle: serde_json::Value = serde_json::from_str(include_str!(
        "../../../../../../data/java_1_21_11/survival_placement.json"
    ))
    .unwrap();
    let names: Vec<_> = oracle["materials"]
        .as_array()
        .unwrap()
        .iter()
        .map(|m| m["name"].as_str().unwrap())
        .collect();
    assert_eq!(
        names,
        survival::DRY_CUBES
            .iter()
            .copied()
            .filter(|n| *n != "minecraft:grass_block")
            .collect::<Vec<_>>()
    );
    let mut s = fixture_state();
    for m in oracle["materials"].as_array().unwrap() {
        let item = default_item(m["name"].as_str().unwrap(), 5).unwrap();
        assert_eq!(i64::from(item.item_id), m["item_id"].as_i64().unwrap());
        s.operations.inventory.slots[36] = InventorySlot::Item { item };
        let i = prepare(&mut s, 1, 0, SUPPORT, 4).unwrap();
        assert_eq!(
            i64::from(state_id(&i.expected).unwrap()),
            m["state_id"].as_i64().unwrap()
        );
    }
    let mut i = prepare(&mut s, 1, 0, SUPPORT, 4).unwrap();
    i.support = [-2, -61, 3];
    i.sequence = 17;
    for p in oracle["packets"].as_array().unwrap() {
        i.face_id = p["face"].as_u64().unwrap() as u8;
        i.cursor = offset(i.face_id).map(|d| 0.5 + d as f32 * 0.5);
        assert_eq!(hex::encode(packet(&i)), p["hex"].as_str().unwrap());
    }
}
#[tokio::test]
async fn chunk_world_and_restored_material_conflicts_remain_latched() {
    for kind in 0..3 {
        let mut f = Fixture::new().await;
        let i = f.start().await;
        if kind == 0 {
            f.stack(4).await;
            f.stack(5).await;
            f.stack(4).await;
        } else if kind == 1 {
            f.receive(ids::play_clientbound::UNLOAD_CHUNK, &[0; 8])
                .await;
        } else {
            f.receive(
                ids::play_clientbound::GAME_STATE_CHANGE,
                &[3, 0x3f, 0x80, 0, 0],
            )
            .await;
            f.receive(ids::play_clientbound::GAME_STATE_CHANGE, &[3, 0, 0, 0, 0])
                .await;
        }
        assert!(matches!(
            f.api.observe_survival_placement(&i).await.unwrap(),
            PlacementStatus::RequiresInspection { .. }
        ));
        assert!(f.api.select_hotbar(1).await.is_err());
        f.stop().await;
    }
}
