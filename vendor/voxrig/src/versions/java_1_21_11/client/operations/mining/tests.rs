use super::*;
use crate::versions::java_1_21_11::{state_id, world::Dimension};
use tokio::{
    net::{TcpListener, TcpStream},
    task::JoinHandle,
};

const TARGET: [i32; 3] = [2, 2, 0];
fn native(name: &str) -> crate::NativeBlockState {
    crate::NativeBlockState {
        name: format!("minecraft:{name}"),
        properties: Default::default(),
    }
}
fn state() -> State {
    let mut state = State {
        phase: Phase::Play,
        ready: true,
        position: Some([0.5, 1.0, 0.5]),
        rotation: [-90.0, 3.0],
        sequence: 10,
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
    state.operations.position_from_server = true;
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
        .seed_replay_cell(TARGET, state_id(&native("stone")).unwrap());
    let mut slots = vec![0, 1, 46];
    for slot in 0..46 {
        if slot == 9 {
            slots.push(5);
            put_varint(&mut slots, default_item("dirt", 5).unwrap().item_id);
            slots.extend([0, 0]);
        } else {
            slots.push(0);
        }
    }
    slots.push(0);
    super::super::receive(&mut state, ids::play_clientbound::WINDOW_ITEMS, &slots).unwrap();
    state
}
struct Fixture {
    api: Operations,
    session: Arc<Session>,
    peer: TcpStream,
    receiver: JoinHandle<()>,
}
impl Fixture {
    async fn new() -> Self {
        Self::new_id(42).await
    }
    async fn new_id(id: u64) -> Self {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let stream = TcpStream::connect(listener.local_addr().unwrap())
            .await
            .unwrap();
        let (peer, _) = listener.accept().await.unwrap();
        let (reader, writer) = stream.into_split();
        let session = Arc::new(Session {
            id,
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
        let api = Operations {
            bot: Bot {
                session: session.clone(),
                _lease: Arc::new(Lease(Arc::downgrade(&session))),
            },
        };
        let running = session.clone();
        let receiver = tokio::spawn(async move {
            running.run_receiver(reader).await;
        });
        let mut fixture = Self {
            api,
            session,
            peer,
            receiver,
        };
        fixture.session.state.lock().await.identity = Some(LoginIdentity {
            uuid: [id as u8; 16],
            name: format!("Miner{id}"),
            server: crate::Server::new("127.0.0.1", 25572),
        });
        fixture.api.select_hotbar(0).await.unwrap();
        assert_eq!(
            read_packet(&mut fixture.peer, None).await.unwrap(),
            (ids::play_serverbound::HELD_ITEM_SLOT, vec![0, 0])
        );
        fixture
    }
    async fn start(&mut self) -> MiningIntent {
        let intent = self
            .api
            .start_survival_mining(TARGET, crate::BlockFace::West)
            .await
            .unwrap();
        assert_eq!(
            read_packet(&mut self.peer, None).await.unwrap(),
            (
                ids::play_serverbound::BLOCK_DIG,
                packet(&intent, 0, intent.start_sequence)
            )
        );
        intent
    }
    async fn change(&mut self, p: [i32; 3], block: &crate::NativeBlockState) {
        let mut payload = pack_position(p).to_be_bytes().to_vec();
        put_varint(&mut payload, state_id(block).unwrap());
        let before = self.session.state.lock().await.sequence;
        write_packet(
            &mut self.peer,
            None,
            ids::play_clientbound::BLOCK_CHANGE,
            &payload,
        )
        .await
        .unwrap();
        timeout(Duration::from_secs(1), async {
            loop {
                if self.session.state.lock().await.sequence > before {
                    break;
                }
                tokio::task::yield_now().await;
            }
        })
        .await
        .unwrap();
    }
    async fn stop(self) {
        self.api.bot.disconnect().await.unwrap();
        timeout(Duration::from_secs(1), self.receiver)
            .await
            .unwrap()
            .unwrap();
    }
    async fn receive(&mut self, id: i32, payload: &[u8]) {
        let before = self.session.state.lock().await.sequence;
        write_packet(&mut self.peer, None, id, payload)
            .await
            .unwrap();
        timeout(Duration::from_secs(1), async {
            loop {
                if self.session.state.lock().await.sequence > before {
                    break;
                }
                tokio::task::yield_now().await;
            }
        })
        .await
        .unwrap();
    }
    async fn profile(&mut self, owner: u8) {
        let mut payload = vec![1, 1];
        payload.extend([owner; 16]);
        put_string(&mut payload, &format!("Miner{owner}"));
        payload.push(0);
        self.receive(ids::play_clientbound::PLAYER_INFO, &payload)
            .await;
    }
    async fn remove_profile(&mut self, owner: u8) {
        let mut payload = vec![1];
        payload.extend([owner; 16]);
        self.receive(ids::play_clientbound::PLAYER_REMOVE, &payload)
            .await;
    }
}

#[tokio::test]
async fn retirement_requires_exact_post_watch_receipt_and_local_closure() {
    let mut miner = Fixture::new().await;
    let intent = miner.start().await;
    let mut observer = Fixture::new_id(43).await;
    assert!(
        miner
            .api
            .prepare_survival_mining_retirement(&intent, &miner.api)
            .await
            .is_err()
    );
    assert!(
        miner
            .api
            .prepare_survival_mining_retirement(&intent, &observer.api)
            .await
            .is_err()
    );
    // Old receipt plus subsequent profile baseline cannot substitute for a new removal.
    observer.remove_profile(42).await;
    observer.profile(42).await;
    let watch = miner
        .api
        .prepare_survival_mining_retirement(&intent, &observer.api)
        .await
        .unwrap();
    observer.remove_profile(99).await;
    observer
        .receive(ids::play_clientbound::ENTITY_DESTROY, &[1, 42])
        .await;
    assert!(matches!(
        miner
            .api
            .wait_survival_mining_retirement(&watch, &observer.api, Duration::from_millis(10))
            .await
            .unwrap(),
        MiningRetirementStatus::Pending {
            source_closed: false,
            ..
        }
    ));
    observer.remove_profile(42).await;
    assert!(matches!(
        miner
            .api
            .observe_survival_mining_retirement(&watch, &observer.api)
            .await
            .unwrap(),
        MiningRetirementStatus::Pending {
            source_closed: false,
            ..
        }
    ));
    assert!(miner.api.select_hotbar(1).await.is_err());
    miner.api.bot.disconnect().await.unwrap();
    let retired = miner
        .api
        .wait_survival_mining_retirement(&watch, &observer.api, Duration::from_millis(10))
        .await
        .unwrap();
    assert!(matches!(retired, MiningRetirementStatus::Retired { .. }));
    assert!(miner.api.select_hotbar(1).await.is_err());
    assert!(
        miner
            .api
            .reconnect_survival_mining(
                &watch,
                &observer.api,
                ConnectionConfig::offline(
                    crate::Server::new("127.0.0.1", 1),
                    "Miner42",
                    MinecraftVersion::Java1_21_11
                ),
                native("stone")
            )
            .await
            .is_err()
    );
    // Observer is also tied to its live context; a rejoin invalidates old authority.
    observer.profile(42).await;
    assert!(matches!(
        miner
            .api
            .observe_survival_mining_retirement(&watch, &observer.api)
            .await
            .unwrap(),
        MiningRetirementStatus::RequiresInspection { .. }
    ));
    observer.remove_profile(42).await;
    assert!(matches!(
        miner
            .api
            .observe_survival_mining_retirement(&watch, &observer.api)
            .await
            .unwrap(),
        MiningRetirementStatus::RequiresInspection { .. }
    ));
    miner.stop().await;
    observer.stop().await;
}

#[tokio::test]
async fn retirement_closure_alone_and_changed_observer_context_never_authorize_recovery() {
    let mut miner = Fixture::new().await;
    let intent = miner.start().await;
    let mut observer = Fixture::new_id(43).await;
    observer.profile(42).await;
    let watch = miner
        .api
        .prepare_survival_mining_retirement(&intent, &observer.api)
        .await
        .unwrap();
    assert!(
        miner
            .api
            .prepare_survival_mining_retirement(&intent, &observer.api)
            .await
            .is_err()
    );
    miner.api.bot.disconnect().await.unwrap();
    assert!(matches!(
        miner
            .api
            .wait_survival_mining_retirement(&watch, &observer.api, Duration::from_millis(10))
            .await
            .unwrap(),
        MiningRetirementStatus::Pending {
            source_closed: true,
            ..
        }
    ));
    {
        let mut state = observer.session.state.lock().await;
        super::mining_world_changed(&mut state, "observer changed world");
    }
    observer.remove_profile(42).await;
    assert!(matches!(
        miner
            .api
            .observe_survival_mining_retirement(&watch, &observer.api)
            .await
            .unwrap(),
        MiningRetirementStatus::RequiresInspection { .. }
    ));
    observer.api.bot.disconnect().await.unwrap();
    assert!(
        miner
            .api
            .observe_survival_mining_retirement(&watch, &observer.api)
            .await
            .is_err()
    );
    let history = observer.api.operation_history().await;
    assert!(
        history
            .mining_retirement
            .unwrap()
            .requires_inspection
            .is_some()
    );
    miner.stop().await;
    observer.stop().await;
}

#[tokio::test]
async fn cancelled_recovery_login_retains_attempt_and_refuses_another_connection() {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let endpoint = crate::Server::new("127.0.0.1", listener.local_addr().unwrap().port());
    let mut miner = Fixture::new().await;
    let mut observer = Fixture::new_id(43).await;
    for f in [&miner, &observer] {
        f.session
            .state
            .lock()
            .await
            .identity
            .as_mut()
            .unwrap()
            .server = endpoint.clone();
    }
    let intent = miner.start().await;
    observer.profile(42).await;
    let watch = miner
        .api
        .prepare_survival_mining_retirement(&intent, &observer.api)
        .await
        .unwrap();
    miner.api.bot.disconnect().await.unwrap();
    observer.remove_profile(42).await;
    let config = ConnectionConfig::offline(endpoint, "Miner42", MinecraftVersion::Java1_21_11);
    let mut recovery = Box::pin(miner.api.reconnect_survival_mining(
        &watch,
        &observer.api,
        config.clone(),
        native("stone"),
    ));
    let (mut login, _) = timeout(Duration::from_secs(1), async {
        tokio::select! {
            accepted = listener.accept() => accepted.unwrap(),
            _ = &mut recovery => panic!("recovery returned before login fixture accepted"),
        }
    })
    .await
    .unwrap();
    let handshake = timeout(Duration::from_secs(1), async {
        tokio::select! {
            packet = read_packet(&mut login, None) => packet.unwrap(),
            _ = &mut recovery => panic!("recovery returned before login handshake"),
        }
    })
    .await
    .unwrap();
    assert_eq!(handshake.0, 0);
    // Cancel an actual in-progress TCP login, not a pre-I/O argument check.
    drop(recovery);
    assert!(
        observer
            .api
            .operation_history()
            .await
            .mining_retirement
            .unwrap()
            .recovery_started
    );
    assert_eq!(
        miner
            .api
            .reconnect_survival_mining(&watch, &observer.api, config, native("stone"))
            .await
            .err()
            .unwrap()
            .kind(),
        ErrorKind::State
    );
    assert!(
        timeout(Duration::from_millis(10), listener.accept())
            .await
            .is_err()
    );
    assert!(miner.api.operation_history().await.connection_closed);
    miner.stop().await;
    observer.stop().await;
}

#[tokio::test]
async fn fresh_recovery_observations_do_not_authorize_actions_before_native_loading() {
    let mut fixture = Fixture::new().await;
    fixture.session.state.lock().await.recovery_loading_pending = true;
    assert!(fixture.api.player_state().await.is_ok());
    assert!(fixture.api.standing_context().await.is_ok());
    assert!(
        fixture
            .api
            .operation_history()
            .await
            .recovery_loading_pending
    );
    assert!(fixture.api.select_hotbar(1).await.is_err());
    assert!(fixture.api.look([-90.0, 3.0]).await.is_err());
    assert!(fixture.api.swap_player_hotbar(9, 1).await.is_err());
    assert!(
        fixture
            .api
            .start_survival_mining(TARGET, crate::BlockFace::West)
            .await
            .is_err()
    );
    assert!(
        fixture
            .api
            .use_on_block(TARGET, crate::BlockFace::West, [0.5; 3])
            .await
            .is_err()
    );
    assert!(
        fixture
            .api
            .send_command("say should_not_send")
            .await
            .is_err()
    );
    assert!(
        timeout(
            Duration::from_millis(10),
            read_packet(&mut fixture.peer, None)
        )
        .await
        .is_err()
    );
    fixture.stop().await;
}

#[tokio::test]
async fn pending_abort_ack_and_timeout_refuse_every_following_mutation_until_fresh_removal() {
    let mut f = Fixture::new().await;
    let intent = f.start().await;
    assert!(intent.estimated_wait_ms == 8500 && intent.held_receive_sequence == 10);
    assert_eq!(
        f.api.select_hotbar(1).await.unwrap_err().kind(),
        ErrorKind::State
    );
    assert_eq!(
        f.api.swap_player_hotbar(9, 1).await.unwrap_err().kind(),
        ErrorKind::State
    );
    assert_eq!(
        f.api
            .set_creative_hotbar(1, Some(("dirt", 1)))
            .await
            .unwrap_err()
            .kind(),
        ErrorKind::State
    );
    assert_eq!(
        f.api
            .use_on_block(TARGET, crate::BlockFace::West, [0.5; 3])
            .await
            .unwrap_err()
            .kind(),
        ErrorKind::State
    );
    assert_eq!(
        f.api.look([0.0; 2]).await.unwrap_err().kind(),
        ErrorKind::State
    );
    assert_eq!(
        f.api.send_command("say blocked").await.unwrap_err().kind(),
        ErrorKind::State
    );
    assert_eq!(
        f.api.set_flying(false).await.unwrap_err().kind(),
        ErrorKind::State
    );
    assert_eq!(
        f.api
            .move_flying([0.5, 1.0, 0.5], [0.0; 2])
            .await
            .unwrap_err()
            .kind(),
        ErrorKind::State
    );
    assert_eq!(
        f.api
            .dig_creative(TARGET, crate::BlockFace::West)
            .await
            .unwrap_err()
            .kind(),
        ErrorKind::State
    );
    assert_eq!(
        f.api
            .start_survival_mining(TARGET, crate::BlockFace::West)
            .await
            .unwrap_err()
            .kind(),
        ErrorKind::State
    );
    let finish = f.api.finish_survival_mining(&intent).await.unwrap();
    assert!(finish > intent.start_sequence);
    assert_eq!(
        read_packet(&mut f.peer, None).await.unwrap(),
        (ids::play_serverbound::BLOCK_DIG, packet(&intent, 2, finish))
    );
    let abort = f.api.abort_survival_mining(&intent).await.unwrap();
    assert!(abort > finish);
    assert_eq!(
        read_packet(&mut f.peer, None).await.unwrap(),
        (ids::play_serverbound::BLOCK_DIG, packet(&intent, 1, abort))
    );
    assert!(f.api.finish_survival_mining(&intent).await.is_err());
    assert!(f.api.abort_survival_mining(&intent).await.is_err());
    let mut ack = vec![];
    put_varint(&mut ack, abort);
    write_packet(
        &mut f.peer,
        None,
        ids::play_clientbound::ACKNOWLEDGE_PLAYER_DIGGING,
        &ack,
    )
    .await
    .unwrap();
    assert!(matches!(
        f.api
            .wait_survival_mining(&intent, Duration::from_millis(5))
            .await
            .unwrap(),
        MiningStatus::PendingAfterFinish { .. }
    ));
    assert!(
        timeout(Duration::from_millis(5), read_packet(&mut f.peer, None))
            .await
            .is_err()
    );
    assert!(
        f.api
            .operation_history()
            .await
            .mining
            .unwrap()
            .removal
            .is_none()
    );
    let mut foreign = intent.clone();
    foreign.connection_id = 99;
    assert!(f.api.observe_survival_mining(&foreign).await.is_err());
    f.change(TARGET, &native("air")).await;
    let MiningStatus::ObservedRemoved { observation } = f
        .api
        .wait_survival_mining(&intent, Duration::from_secs(1))
        .await
        .unwrap()
    else {
        panic!("fresh air must resolve result");
    };
    assert_eq!(observation.intent, intent);
    assert!(observation.target_receipt.receive_sequence > intent.after_sequence);
    assert!(!observation.continuation_validated);
    assert_eq!(
        f.api.select_hotbar(1).await.unwrap_err().kind(),
        ErrorKind::State
    );
    assert!(
        f.api
            .start_survival_mining(TARGET, crate::BlockFace::West)
            .await
            .is_err()
    );
    f.stop().await;
}

#[tokio::test]
async fn unrelated_packet_or_cached_air_does_not_establish_target_freshness() {
    let mut f = Fixture::new().await;
    let intent = f.start().await;
    f.change([3, 2, 0], &native("air")).await;
    f.session
        .state
        .lock()
        .await
        .world
        .seed_replay_cell(TARGET, 0);
    assert!(matches!(
        f.api
            .wait_survival_mining(&intent, Duration::from_millis(5))
            .await
            .unwrap(),
        MiningStatus::Mining { .. }
    ));
    f.change(TARGET, &native("air")).await;
    assert!(matches!(
        f.api.observe_survival_mining(&intent).await.unwrap(),
        MiningStatus::ObservedRemoved { .. }
    ));
    f.stop().await;
}

#[tokio::test]
async fn conflict_then_air_never_clears_requires_inspection_and_history_survives_disconnect() {
    let mut f = Fixture::new().await;
    let intent = f.start().await;
    f.change(TARGET, &native("dirt")).await;
    f.change(TARGET, &native("air")).await;
    assert!(matches!(
        f.api.observe_survival_mining(&intent).await.unwrap(),
        MiningStatus::RequiresInspection { .. }
    ));
    f.api.bot.disconnect().await.unwrap();
    assert!(f.api.player_state().await.is_err());
    let history = f.api.operation_history().await;
    assert!(history.connection_closed);
    let record = history.mining.unwrap();
    assert_eq!(record.intent, intent);
    assert!(record.requires_inspection.is_some() && record.removal.is_none());
    assert!(
        f.api
            .start_survival_mining(TARGET, crate::BlockFace::West)
            .await
            .is_err()
    );
    timeout(Duration::from_secs(1), f.receiver)
        .await
        .unwrap()
        .unwrap();
}

#[tokio::test]
async fn cancelled_start_and_finish_attempts_are_retained_without_replay() {
    let f = Fixture::new().await;
    let locked = f.session.writer.lock().await;
    let mut start = Box::pin(f.api.start_survival_mining(TARGET, crate::BlockFace::West));
    std::future::poll_fn(|cx| {
        assert!(start.as_mut().poll(cx).is_pending());
        std::task::Poll::Ready(())
    })
    .await;
    drop(start);
    drop(locked);
    let record = f.api.operation_history().await.mining.unwrap();
    assert!(!record.start_dispatched);
    assert!(f.api.finish_survival_mining(&record.intent).await.is_err());
    assert!(
        f.api
            .start_survival_mining(TARGET, crate::BlockFace::West)
            .await
            .is_err()
    );
    f.stop().await;
    let mut f = Fixture::new().await;
    let intent = f.start().await;
    let locked = f.session.writer.lock().await;
    let mut finish = Box::pin(f.api.finish_survival_mining(&intent));
    std::future::poll_fn(|cx| {
        assert!(finish.as_mut().poll(cx).is_pending());
        std::task::Poll::Ready(())
    })
    .await;
    drop(finish);
    drop(locked);
    let record = f.api.operation_history().await.mining.unwrap();
    assert!(record.finish.is_some_and(|s| !s.dispatched));
    assert!(matches!(
        f.api.observe_survival_mining(&intent).await.unwrap(),
        MiningStatus::PendingAfterFinish { .. }
    ));
    assert!(f.api.finish_survival_mining(&intent).await.is_err());
    assert!(
        timeout(Duration::from_millis(5), read_packet(&mut f.peer, None))
            .await
            .is_err()
    );
    f.stop().await;
}

#[test]
fn admitted_target_and_held_selection_are_explicit_and_native_packet_updates_are_atomic() {
    let mut state = state();
    assert!(prepared(&mut state, 42, 0, TARGET, 4).is_err());
    super::super::receive(&mut state, ids::play_clientbound::HELD_ITEM_SLOT, &[0]).unwrap();
    let selected = state.operations.selected_hotbar.clone();
    for invalid in [vec![], vec![9], vec![0, 0], vec![255, 255, 255, 255, 15]] {
        assert!(
            super::super::receive(&mut state, ids::play_clientbound::HELD_ITEM_SLOT, &invalid)
                .is_err()
        );
        assert_eq!(state.operations.selected_hotbar, selected);
    }
    assert!(prepared(&mut state, 42, 0, TARGET, 4).is_ok());
    assert!(prepared(&mut state, 42, 0, TARGET, 5).is_err());
    state
        .world
        .seed_replay_cell([1, 2, 0], state_id(&native("stone")).unwrap());
    assert!(prepared(&mut state, 42, 0, TARGET, 4).is_err()); // obstruction
    state.world.seed_replay_cell([1, 2, 0], 0);
    state
        .world
        .seed_replay_cell(TARGET, state_id(&native("glass")).unwrap());
    assert_eq!(
        prepared(&mut state, 42, 0, TARGET, 4).unwrap_err().kind(),
        ErrorKind::Unsupported
    );
}

#[tokio::test]
async fn chunk_replacement_and_reconfiguration_preserve_pending_intent() {
    let mut f = Fixture::new().await;
    let intent = f.start().await;
    {
        let mut state = f.session.state.lock().await;
        let mut unload = 0i32.to_be_bytes().to_vec();
        unload.extend(0i32.to_be_bytes());
        state
            .receive(ids::play_clientbound::UNLOAD_CHUNK, &unload, 64)
            .unwrap();
        state.world.seed_replay_cell(TARGET, 0);
        state
            .receive(ids::play_clientbound::START_CONFIGURATION, &[], 64)
            .unwrap();
    }
    let history = f.api.operation_history().await;
    assert_eq!(history.mining.as_ref().unwrap().intent, intent);
    assert!(
        history
            .mining
            .as_ref()
            .unwrap()
            .requires_inspection
            .is_some()
    );
    assert!(matches!(
        f.api.observe_survival_mining(&intent).await.unwrap(),
        MiningStatus::RequiresInspection { .. }
    ));
    f.stop().await;
}
