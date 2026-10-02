use super::*;
use crate::versions::java_1_21_11::{state_id, world::Dimension};
use tokio::{
    net::{TcpListener, TcpStream},
    task::JoinHandle,
};

// Regression from a detached roof preflight; no network or world edits.
#[test]
fn roof_diagonal_view_allows_off_ray_foot_support_but_refuses_occlusion() {
    use super::super::geometry::GeometryView;
    struct RoofView {
        foot_support: bool,
        obstacle: bool,
    }
    impl GeometryView for RoofView {
        fn block(&self, p: [i32; 3]) -> Result<crate::NativeBlockState> {
            if (0..3).any(|i| p[i] < [-5, -62, -5][i] || p[i] > [9, -50, 10][i]) {
                return Err(invalid("outside declared characterization scene"));
            }
            Ok(if p[1] <= -61 || (self.obstacle && p == [1, -59, 5]) {
                native("stone")
            } else if self.foot_support && p == [2, -60, 6] {
                native("dirt")
            } else {
                native("air")
            })
        }
    }
    let view = RoofView {
        foot_support: true,
        obstacle: false,
    };
    // Exact detached endpoint from DustRoute's first roof preflight refusal.
    let position = [2.5, -59.0, 6.544924947876652];
    let eye = [position[0], position[1] + f64::from(1.62f32), position[2]];
    let point = [0.5, -60.0, 4.5];
    let d: [f64; 3] = std::array::from_fn(|i| point[i] - eye[i]);
    let rotation = [
        (-d[0]).atan2(d[2]).to_degrees() as f32,
        (-d[1]).atan2(d[0].hypot(d[2])).to_degrees() as f32,
    ];
    let ray = |v: &RoofView, eye| {
        super::super::super::raycast::outline_hit_in(eye, rotation, 4.5, |p| {
            v.block(p).map_err(anyhow::Error::from)
        })
        .unwrap()
        .unwrap()
    };
    let hit = ray(&view, eye);
    assert_eq!(hit.position, [0, -61, 4]);
    assert_eq!(hit.face.map(|f| f as u8), Some(crate::BlockFace::Up as u8));
    let margin = 2.0 / 4096.0 + 1e-9;
    // Samples diagnose the refusal; they do NOT prove a continuous uncertainty
    // volume and must not replace the conservative guard in production.
    for dx in [-margin, -margin / 2.0, 0.0, margin / 2.0, margin] {
        for dz in [-margin, -margin / 2.0, 0.0, margin / 2.0, margin] {
            let h = ray(&view, [eye[0] + dx, eye[1], eye[2] + dz]);
            assert_eq!(h.position, hit.position);
            assert_eq!(h.face, hit.face);
        }
    }
    let bounds = super::super::survival::standing_geometry(&view, position, [0.0625, 0.0, 0.0625])
        .unwrap()
        .bounds;
    let exact = super::super::placement::placement_geometry(
        &view,
        position,
        bounds,
        [0.0; 3],
        rotation,
        [0, -61, 4],
        crate::BlockFace::Up as u8,
    )
    .unwrap();
    assert_eq!(exact.target, [0, -60, 4]);
    let uncertain = super::super::placement::placement_geometry(
        &view,
        position,
        bounds,
        [margin, 0.0, margin],
        rotation,
        [0, -61, 4],
        crate::BlockFace::Up as u8,
    )
    .unwrap();
    assert_eq!(uncertain.target, exact.target);
    assert_eq!(uncertain.cursor, exact.cursor);
    // Counterfactual visibility only: removing a foot support is not a safe
    // construction action, nor a replacement standing context.
    let without_support = RoofView {
        foot_support: false,
        obstacle: false,
    };
    assert_eq!(ray(&without_support, eye).position, hit.position);
    super::super::survival::uncertain_target_in(
        &without_support,
        eye,
        [margin, 0.0, margin],
        rotation,
        &hit,
    )
    .unwrap();
    let obstructed = RoofView {
        foot_support: true,
        obstacle: true,
    };
    assert_eq!(ray(&obstructed, eye).position, [1, -59, 5]);
    assert!(
        super::super::survival::uncertain_target_in(
            &obstructed,
            eye,
            [margin, 0.0, margin],
            rotation,
            &hit,
        )
        .unwrap_err()
        .to_string()
        .contains("corridor is not clear")
    );
}

#[tokio::test]
async fn hypothetical_scene_shares_native_prediction_and_never_changes_live_state() {
    let mut f = Fixture::new().await;
    f.session
        .state
        .lock()
        .await
        .world
        .seed_replay_cell(TARGET, 0);
    let scene = f
        .api
        .capture_survival_scene(crate::Region {
            min: [-2, -1, -2],
            max: [6, 7, 4],
        })
        .await
        .unwrap();
    let controls: Vec<_> = (0..16)
        .map(|tick| SurvivalControl {
            yaw: 0.0,
            input: SurvivalInput {
                forward: i8::from(tick < 4),
                ..Default::default()
            },
        })
        .collect();
    let live = f.api.preview_survival_path(&controls).await.unwrap();
    let scenario = scene.scenario();
    let predicted = scenario.preview_path(&controls).unwrap();
    assert_eq!(live.frames, predicted.frames);
    assert_eq!(
        serde_json::to_value(&live.terminal_clearance).unwrap(),
        serde_json::to_value(&predicted.terminal_clearance).unwrap()
    );
    let edit = HypotheticalBlockEdit {
        position: [1, 1, 0],
        before: native("air"),
        after: native("dirt"),
    };
    let next = scenario.after_edits(std::slice::from_ref(&edit)).unwrap();
    assert_eq!(next.block(edit.position).unwrap(), native("dirt"));
    assert_eq!(scenario.block(edit.position).unwrap(), native("air"));
    {
        let state = f.session.state.lock().await;
        assert_eq!(
            state.reconstruction.cell(&state.world, edit.position).state,
            Some(native("air"))
        );
    }
    f.api.validate_survival_scene(&scene).await.unwrap();
    f.session.state.lock().await.operations.game_mode = Some(GameMode::Creative);
    assert!(f.api.validate_survival_scene(&scene).await.is_err());
    f.session.state.lock().await.operations.game_mode = Some(GameMode::Survival);
    assert!(scenario.matches_preview(&predicted));
    assert!(!next.matches_preview(&predicted));
    assert!(
        scenario
            .preview_path(&vec![
                SurvivalControl {
                    yaw: 0.0,
                    input: Default::default()
                };
                121
            ])
            .is_err()
    );
    assert!(
        f.api
            .capture_survival_scene(crate::Region {
                min: [0, 0, 0],
                max: [65, 1, 1]
            })
            .await
            .is_err()
    );
    assert!(
        scenario
            .after_edits(&[HypotheticalBlockEdit {
                position: [0, 0, 0],
                before: native("stone"),
                after: native("air")
            }])
            .is_err()
    );
    assert!(next.after_edits(std::slice::from_ref(&edit)).is_err());
    assert!(scenario.after_edits(&[edit.clone(), edit.clone()]).is_err());
    assert!(
        scenario
            .after_edits(&[HypotheticalBlockEdit {
                position: [30, 1, 0],
                ..edit.clone()
            }])
            .is_err()
    );
    assert!(
        scenario
            .after_edits(&[HypotheticalBlockEdit {
                after: native("water"),
                ..edit
            }])
            .is_err()
    );
    f.session
        .state
        .lock()
        .await
        .world
        .seed_replay_cell([1, 1, 0], 1);
    assert!(f.api.validate_survival_scene(&scene).await.is_err());
    assert_eq!(scenario.block([1, 1, 0]).unwrap(), native("air"));
    assert!(
        tokio::time::timeout(Duration::from_millis(30), read_packet(&mut f.peer, None))
            .await
            .is_err()
    );
    f.stop().await;
}

#[tokio::test]
async fn hypothetical_native_place_step_and_removal_require_safe_standing() {
    let f = Fixture::new().await;
    f.session
        .state
        .lock()
        .await
        .world
        .seed_replay_cell(TARGET, 0);
    let scene = f
        .api
        .capture_survival_scene(crate::Region {
            min: [-2, -1, -2],
            max: [6, 7, 4],
        })
        .await
        .unwrap();
    let scenario = scene.scenario();
    let delta = [1.0f64, -f64::from(1.62f32), 0.0];
    let rotation = [-90.0, (-delta[1]).atan2(delta[0]).to_degrees() as f32];
    let placement = scenario
        .preview_cube_placement([1, 0, 0], crate::BlockFace::Up, rotation, "dirt")
        .unwrap();
    assert_eq!(placement.edit.position, [1, 1, 0]);
    assert!(
        scenario
            .preview_cube_placement([1, 0, 0], crate::BlockFace::Down, rotation, "dirt")
            .is_err()
    );
    let mut edits = vec![placement.edit];
    for x in 2..=3 {
        edits.push(HypotheticalBlockEdit {
            position: [x, 1, 0],
            before: native("air"),
            after: native("dirt"),
        });
    }
    let platform = scenario.after_edits(&edits).unwrap();
    let controls: Vec<_> = (0..28)
        .map(|tick| SurvivalControl {
            yaw: -90.0,
            input: SurvivalInput {
                forward: i8::from(tick < 8),
                jump: tick == 0,
                strafe: 0,
            },
        })
        .collect();
    let arrived = platform.after_path(&controls).unwrap();
    assert_eq!(arrived.position()[1], 2.0);
    let foot = [arrived.position()[0].floor() as i32, 1, 0];
    assert!(
        arrived
            .after_edits(&[HypotheticalBlockEdit {
                position: foot,
                before: native("dirt"),
                after: native("air")
            }])
            .is_err()
    );
    let return_controls: Vec<_> = (0..28)
        .map(|tick| SurvivalControl {
            yaw: 90.0,
            input: SurvivalInput {
                forward: i8::from(tick < 8),
                ..Default::default()
            },
        })
        .collect();
    let returned = arrived.after_path(&return_controls).unwrap();
    assert_eq!(returned.position()[1], 1.0);
    let eye = [
        returned.position()[0],
        returned.position()[1] + f64::from(1.62f32),
        returned.position()[2],
    ];
    let delta = [1.0 - eye[0], 1.5 - eye[1], 0.5 - eye[2]];
    let rotation = [
        (-delta[0]).atan2(delta[2]).to_degrees() as f32,
        (-delta[1]).atan2(delta[0].hypot(delta[2])).to_degrees() as f32,
    ];
    let first_removal = returned
        .preview_cube_removal([1, 1, 0], crate::BlockFace::West, rotation)
        .unwrap();
    assert_eq!(first_removal.before, native("dirt"));
    assert!(
        returned
            .preview_cube_removal([1, 1, 0], crate::BlockFace::East, rotation)
            .is_err()
    );
    let removals: Vec<_> = edits
        .iter()
        .map(|e| HypotheticalBlockEdit {
            position: e.position,
            before: e.after.clone(),
            after: e.before.clone(),
        })
        .collect();
    let cleared = returned.after_edits(&removals).unwrap();
    assert_eq!(cleared.block([1, 1, 0]).unwrap(), native("air"));
    f.stop().await;
}

const TARGET: [i32; 3] = [2, 2, 0];

// Shared native geometry regression, not live bridging acceptance.
// Conservative standing clearance remains independent of aiming uncertainty.
#[tokio::test]
async fn hypothetical_edge_placement_separates_aim_from_clearance() {
    use super::super::geometry::GeometryView;
    let mut f = Fixture::new().await;
    let position = [1.2, 1.0, 0.5];
    let rotation = [
        90.0,
        (f64::from(1.62f32) + 0.5).atan2(0.2).to_degrees() as f32,
    ];
    {
        let mut s = f.session.state.lock().await;
        s.position = Some(position);
        s.rotation = rotation;
        let generation = s.loading.generation;
        let receive_sequence = s.sequence;
        s.motion.receive(ReceivedPose {
            generation,
            receive_sequence,
            position,
            rotation,
            velocity: Some([0.0; 3]),
        });
        s.world.seed_replay_cell([1, 0, 0], 0);
    }
    let scene = f
        .api
        .capture_survival_scene(crate::Region {
            min: [-2, -1, -2],
            max: [4, 5, 2],
        })
        .await
        .unwrap();
    let origin = scene.scenario();
    let placement = origin
        .preview_cube_placement([0, 0, 0], crate::BlockFace::East, rotation, "dirt")
        .unwrap();
    assert_eq!(placement.edit.position, [1, 0, 0]);
    let idle = [SurvivalControl {
        yaw: 90.0,
        input: Default::default(),
    }; 3];
    let after = origin.after_path(&idle).unwrap();
    assert_eq!(after.position(), origin.position());
    let future = after
        .preview_cube_placement([0, 0, 0], crate::BlockFace::East, rotation, "dirt")
        .unwrap();
    assert_eq!(future.edit.position, placement.edit.position);
    assert_eq!(future.cursor, placement.cursor);
    assert!(matches!(
        placement.aim_requirement,
        HypotheticalAimRequirement::CapturedPosition { .. }
    ));
    assert!(matches!(
        future.aim_requirement,
        HypotheticalAimRequirement::IndependentlyObservedEndpoint { .. }
    ));
    assert_eq!(future.aim_requirement, after.aim_requirement());
    let preview = after.preview_path(&idle).unwrap();
    assert_eq!(preview.initial_aim_requirement, after.aim_requirement());
    let changed = after
        .after_edits(std::slice::from_ref(&future.edit))
        .unwrap();
    assert_eq!(changed.aim_requirement(), after.aim_requirement());
    {
        let s = f.session.state.lock().await;
        let eye = [position[0], position[1] + f64::from(1.62f32), position[2]];
        let hit = super::super::super::raycast::outline_hit_in(eye, rotation, 4.5, |p| {
            s.block(p).map_err(anyhow::Error::from)
        })
        .unwrap()
        .unwrap();
        // Endpoint admission bounds each packet error by 1/4096 and each
        // model/observer discrepancy by that error plus 1e-9. This is a bound,
        // not fabricated observation provenance or permission to move.
        let admitted_bound = 2.0 / 4096.0 + 1e-9;
        super::super::survival::uncertain_target_in(
            &*s,
            eye,
            [admitted_bound, 0.0, admitted_bound],
            rotation,
            &hit,
        )
        .unwrap();
        let ambiguous = super::super::survival::uncertain_target_in(
            &*s,
            eye,
            [0.0625, 0.0, 0.0625],
            rotation,
            &hit,
        )
        .unwrap_err();
        assert!(ambiguous.to_string().contains("target face/reach differs"));
    }
    f.api.validate_survival_scene(&scene).await.unwrap();
    assert!(
        tokio::time::timeout(Duration::from_millis(30), read_packet(&mut f.peer, None))
            .await
            .is_err()
    );
    // A more extreme overhang still cannot become a future stopping position.
    {
        let mut s = f.session.state.lock().await;
        let position = [1.27, 1.0, 0.5];
        s.position = Some(position);
        let generation = s.loading.generation;
        let receive_sequence = s.sequence;
        s.motion.receive(ReceivedPose {
            generation,
            receive_sequence,
            position,
            rotation,
            velocity: Some([0.0; 3]),
        });
    }
    let unsafe_scene = f.api.capture_survival_scene(scene.region()).await.unwrap();
    assert!(unsafe_scene.scenario().after_path(&idle).is_err());
    f.stop().await;
}

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
        loading: loading::InteractionLoading::completed_fixture(),
        ..State::default()
    };
    state.operations.reset_world(0).unwrap();
    state.operations.local_player = LocalPlayerState::spawned(42);
    state.operations.local_player.velocity = Some(VelocitySample {
        value: [0.0; 3],
        receive_sequence: 10,
    });
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
    let source = crate::Client::from_java_1_21_11(miner.api.bot.clone())
        .survival()
        .unwrap();
    let independent = crate::Client::from_java_1_21_11(observer.api.bot.clone())
        .survival()
        .unwrap();
    assert!(
        source
            .prepare_mining_retirement(&intent, &source)
            .await
            .is_err()
    );
    assert!(
        source
            .prepare_mining_retirement(&intent, &independent)
            .await
            .is_err()
    );
    // Old receipt plus subsequent profile baseline cannot substitute for a new removal.
    observer.remove_profile(42).await;
    observer.profile(42).await;
    let retirement = source
        .prepare_mining_retirement(&intent, &independent)
        .await
        .unwrap();
    observer.remove_profile(99).await;
    observer
        .receive(ids::play_clientbound::ENTITY_DESTROY, &[1, 42])
        .await;
    assert!(matches!(
        retirement.wait(Duration::from_millis(10)).await.unwrap(),
        MiningRetirementStatus::Pending {
            source_closed: false,
            ..
        }
    ));
    observer.remove_profile(42).await;
    assert!(matches!(
        retirement.observe().await.unwrap(),
        MiningRetirementStatus::Pending {
            source_closed: false,
            ..
        }
    ));
    assert!(source.select_hotbar(1).await.is_err());
    retirement.close_source().await.unwrap();
    assert!(retirement.source_history().await.connection_closed);
    let retired = retirement.wait(Duration::from_millis(10)).await.unwrap();
    assert!(matches!(retired, MiningRetirementStatus::Retired { .. }));
    assert!(source.select_hotbar(1).await.is_err());
    assert!(
        retirement
            .reconnect(
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
        retirement.observe().await.unwrap(),
        MiningRetirementStatus::RequiresInspection { .. }
    ));
    observer.remove_profile(42).await;
    assert!(matches!(
        retirement.observe().await.unwrap(),
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
    let source = crate::Client::from_java_1_21_11(miner.api.bot.clone())
        .survival()
        .unwrap();
    let independent = crate::Client::from_java_1_21_11(observer.api.bot.clone())
        .survival()
        .unwrap();
    let retirement = source
        .prepare_mining_retirement(&intent, &independent)
        .await
        .unwrap();
    retirement.close_source().await.unwrap();
    observer.remove_profile(42).await;
    let config = ConnectionConfig::offline(endpoint, "Miner42", MinecraftVersion::Java1_21_11);
    let mut recovery = Box::pin(retirement.reconnect(config.clone(), native("stone")));
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
        retirement
            .clone()
            .reconnect(config, native("stone"))
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
    {
        let mut state = fixture.session.state.lock().await;
        state.loading.reset(11);
        let pose = ReceivedPose {
            generation: state.loading.generation,
            receive_sequence: 12,
            position: state.position.unwrap(),
            rotation: state.rotation,
            velocity: Some([0.0; 3]),
        };
        state.motion.receive(pose); // Fresh position, still no PLAYER_LOADED dispatch.
    }
    assert!(fixture.api.player_state().await.is_ok());
    assert!(fixture.api.standing_context().await.is_ok());
    assert!(
        !fixture
            .api
            .operation_history()
            .await
            .interaction_loading
            .notification_dispatched()
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

#[tokio::test]
async fn dry_motion_preview_preserves_received_state_and_sends_no_player_actions() {
    let mut f = Fixture::new().await;
    let before = f.api.player_state().await.unwrap();
    let mut inputs = vec![SurvivalInput::default(); 30];
    inputs[0].jump = true;
    let preview = f.api.preview_survival_motion(0.0, &inputs).await.unwrap();
    assert_eq!(preview.initial.position, [0.5, 1.0, 0.5]);
    assert!(preview.frames.iter().any(|f| f.position[1] > 2.2));
    assert!(preview.frames.last().unwrap().resting);
    assert_eq!(preview.frames.last().unwrap().position, [0.5, 1.0, 0.5]);
    let after = f.api.player_state().await.unwrap();
    assert_eq!(
        serde_json::to_value(after).unwrap(),
        serde_json::to_value(before).unwrap()
    );
    assert!(
        timeout(Duration::from_millis(10), read_packet(&mut f.peer, None))
            .await
            .is_err()
    );
    let unsupported = SurvivalInput {
        forward: 2,
        ..Default::default()
    };
    assert!(
        f.api
            .preview_survival_motion(0.0, &[unsupported])
            .await
            .is_err()
    );
    f.session
        .state
        .lock()
        .await
        .world
        .seed_replay_cell([0, 0, 0], state_id(&native("slime_block")).unwrap());
    assert!(f.api.preview_survival_motion(0.0, &inputs).await.is_err());
    f.stop().await;
}

impl Fixture {
    async fn observe_mover(&mut self) {
        self.profile(42).await;
        let mut bytes = vec![42];
        bytes.extend([42; 16]);
        put_varint(&mut bytes, ids::PLAYER_ENTITY_TYPE);
        for v in [0.5f64, 1.0, 0.5] {
            bytes.extend(v.to_be_bytes());
        }
        bytes.extend([0, 0, 0, 0, 0]);
        self.receive(ids::play_clientbound::SPAWN_ENTITY, &bytes)
            .await;
    }
    async fn mover_position(&mut self, p: [f64; 3]) {
        let mut bytes = vec![42];
        for v in p.into_iter().chain([0.0; 3]) {
            bytes.extend(v.to_be_bytes());
        }
        for v in [0f32; 2] {
            bytes.extend(v.to_be_bytes());
        }
        bytes.push(1);
        self.receive(ids::play_clientbound::SYNC_ENTITY_POSITION, &bytes)
            .await;
    }
}
#[tokio::test]
async fn finite_motion_preserves_intent_and_needs_fresh_observer_before_shared_standing() {
    let mut mover = Fixture::new().await;
    let mut observer = Fixture::new_id(43).await;
    observer.observe_mover().await;
    let inputs = vec![SurvivalInput::default(); 3];
    assert!(
        mover
            .api
            .start_survival_motion(0.0, &inputs, &mover.api)
            .await
            .is_err()
    );
    let run = mover
        .api
        .start_survival_motion(0.0, &inputs, &observer.api)
        .await
        .unwrap();
    assert_eq!(run.dispatched_ticks, 0);
    assert!(mover.api.select_hotbar(1).await.is_err());
    assert!(mover.api.standing_context().await.is_err());
    assert!(
        mover
            .api
            .start_survival_motion(0.0, &inputs, &observer.api)
            .await
            .is_err()
    );
    // No caller future drives the run after start; all six frames still arrive.
    for _ in 0..3 {
        assert_eq!(
            read_packet(&mut mover.peer, None).await.unwrap(),
            (ids::play_serverbound::PLAYER_INPUT, vec![0])
        );
        assert_eq!(
            read_packet(&mut mover.peer, None).await.unwrap().0,
            ids::play_serverbound::POSITION_LOOK
        );
    }
    timeout(Duration::from_secs(1), async {
        while mover.api.survival_motion().await.unwrap().status
            != SurvivalMotionStatus::AwaitingObservation
        {
            tokio::task::yield_now().await;
        }
    })
    .await
    .unwrap();
    // Cached matching spawn is not fresh. Rotation alone cannot finish the run.
    observer
        .receive(ids::play_clientbound::ENTITY_HEAD_ROTATION, &[42, 10])
        .await;
    assert!(mover.api.standing_context().await.is_err());
    observer.mover_position([0.5, 1.0, 0.5]).await;
    timeout(Duration::from_secs(1), async {
        while mover.api.survival_motion().await.unwrap().status != SurvivalMotionStatus::Observed {
            tokio::task::yield_now().await;
        }
    })
    .await
    .unwrap();
    let standing = mover.api.standing_context().await.unwrap();
    assert!(matches!(
        standing.position_basis,
        StandingPositionBasis::PredictedAndObserved { .. }
    ));
    assert!(!mover.api.player_state().await.unwrap().position_from_server);
    assert_eq!(standing.player.velocity.unwrap().value, [0.0; 3]);
    assert_eq!(
        mover
            .api
            .survival_motion()
            .await
            .unwrap()
            .preview
            .frames
            .last()
            .unwrap()
            .velocity[1],
        -0.08 * f64::from(0.98f32)
    );
    mover.api.select_hotbar(1).await.unwrap();
    observer.mover_position([0.6, 1.0, 0.5]).await;
    assert!(mover.api.standing_context().await.is_err());
    observer.mover_position([0.5, 1.0, 0.5]).await;
    assert!(mover.api.standing_context().await.is_ok());
    // Exact new receipt restores agreement; observer closure never does.
    observer.session.stop();
    assert!(mover.api.standing_context().await.is_err());
    // Fresh unsupported impulse blocks shared standing even after observation.
    mover
        .session
        .state
        .lock()
        .await
        .operations
        .local_player
        .velocity = Some(VelocitySample {
        value: [0.1, 0.0, 0.0],
        receive_sequence: 999,
    });
    assert!(mover.api.standing_context().await.is_err());
    observer.stop().await;
    mover.stop().await;
}
#[tokio::test]
async fn motion_waiting_on_writer_retains_attempt_and_close_never_releases_construction() {
    let mover = Fixture::new().await;
    let mut observer = Fixture::new_id(43).await;
    observer.observe_mover().await;
    let writer = mover.session.writer.lock().await;
    let run = mover
        .api
        .start_survival_motion(0.0, &[SurvivalInput::default(); 3], &observer.api)
        .await
        .unwrap();
    // The actor now owns a state lock while waiting on the writer. Releasing a
    // stopped session cannot turn the retained attempt into a successful send.
    tokio::time::sleep(Duration::from_millis(20)).await;
    mover.session.stop();
    drop(writer);
    timeout(Duration::from_secs(1), async {
        while mover.api.survival_motion().await.unwrap().status
            != SurvivalMotionStatus::RequiresInspection
        {
            tokio::task::yield_now().await;
        }
    })
    .await
    .unwrap();
    let history = mover.api.operation_history().await;
    let record = history.survival_motion.unwrap();
    assert_eq!(record.run_id, run.run_id);
    assert_eq!(record.attempted_tick, 1);
    assert_eq!(record.dispatched_ticks, 0);
    assert_eq!(
        history.motion.position_basis,
        PositionBasis::PendingSubmission
    );
    assert!(mover.api.standing_context().await.is_err());
    observer.stop().await;
    mover.stop().await;
}

#[tokio::test]
async fn terminal_wall_clearance_refuses_before_io_and_retreat_plan_is_admitted() {
    let mut mover = Fixture::new().await;
    let mut observer = Fixture::new_id(43).await;
    observer.observe_mover().await;
    mover
        .session
        .state
        .lock()
        .await
        .world
        .seed_replay_cell([2, 1, 0], 1);
    let mut inputs = vec![SurvivalInput::default(); 60];
    for input in inputs.iter_mut().take(20) {
        input.forward = 1;
    }
    let preview = mover
        .api
        .preview_survival_motion(-90.0, &inputs)
        .await
        .unwrap();
    assert!(preview.frames.iter().any(|f| f.horizontal_collision));
    assert!(matches!(
        preview.terminal_clearance,
        TerminalClearance::RequiresReplan { .. }
    ));
    assert!(
        mover
            .api
            .start_survival_motion(-90.0, &inputs, &observer.api)
            .await
            .is_err()
    );
    assert!(mover.api.survival_motion().await.is_none());
    assert!(
        timeout(
            Duration::from_millis(20),
            read_packet(&mut mover.peer, None)
        )
        .await
        .is_err()
    );
    for input in inputs.iter_mut().skip(30).take(4) {
        input.forward = -1;
    }
    let retreat = mover
        .api
        .preview_survival_motion(-90.0, &inputs)
        .await
        .unwrap();
    assert!(matches!(
        retreat.terminal_clearance,
        TerminalClearance::Admitted { .. }
    ));
    assert!(retreat.frames.last().unwrap().position[0] < 1.7 - 1.0 / 16.0);
    observer.stop().await;
    mover.stop().await;
}
#[tokio::test]
async fn terminal_recheck_requires_new_receipt_current_geometry_and_exact_run_without_sends() {
    let mut mover = Fixture::new().await;
    let mut observer = Fixture::new_id(43).await;
    observer.observe_mover().await;
    let run = mover
        .api
        .start_survival_motion(0.0, &[SurvivalInput::default(); 3], &observer.api)
        .await
        .unwrap();
    for _ in 0..6 {
        read_packet(&mut mover.peer, None).await.unwrap();
    }
    timeout(Duration::from_secs(1), async {
        while mover.api.survival_motion().await.unwrap().status
            != SurvivalMotionStatus::AwaitingObservation
        {
            tokio::task::yield_now().await;
        }
    })
    .await
    .unwrap();
    // A world edit after dispatch makes terminal admission fail; the old intent
    // remains complete but cannot authorize a placement or a replay.
    mover
        .session
        .state
        .lock()
        .await
        .world
        .seed_replay_cell([0, 1, 0], 1);
    observer.mover_position([0.5, 1.0, 0.5]).await;
    timeout(Duration::from_secs(1), async {
        while mover.api.survival_motion().await.unwrap().status
            != SurvivalMotionStatus::RequiresInspection
        {
            tokio::task::yield_now().await;
        }
    })
    .await
    .unwrap();
    let original = mover.api.survival_motion().await.unwrap().problem;
    {
        let mut state = mover.session.state.lock().await;
        state.survival_motion.as_mut().unwrap().dispatched_ticks = 2;
    }
    assert!(
        mover
            .api
            .prepare_survival_motion_recheck(run.run_id, &observer.api)
            .await
            .is_err()
    );
    mover
        .session
        .state
        .lock()
        .await
        .survival_motion
        .as_mut()
        .unwrap()
        .dispatched_ticks = 3;
    let old_motion = {
        let mut state = mover.session.state.lock().await;
        let old = state.motion.clone();
        let mut correction = old.received_pose.clone().unwrap();
        correction.receive_sequence += 100;
        state.motion.receive(correction);
        old
    };
    assert!(
        mover
            .api
            .prepare_survival_motion_recheck(run.run_id, &observer.api)
            .await
            .is_err()
    );
    mover.session.state.lock().await.motion = old_motion;

    assert!(
        mover
            .api
            .prepare_survival_motion_recheck(run.run_id + 1, &observer.api)
            .await
            .is_err()
    );
    let token = mover
        .api
        .prepare_survival_motion_recheck(run.run_id, &observer.api)
        .await
        .unwrap();
    assert!(
        mover
            .api
            .observe_survival_motion_recheck(&token)
            .await
            .is_err()
    ); // stale position
    observer.mover_position([0.5, 1.0, 0.5]).await;
    assert!(
        mover
            .api
            .observe_survival_motion_recheck(&token)
            .await
            .is_err()
    ); // still obstructed
    assert!(mover.api.select_hotbar(1).await.is_err());
    mover
        .session
        .state
        .lock()
        .await
        .world
        .seed_replay_cell([0, 1, 0], 0);
    let superseding = mover
        .api
        .prepare_survival_motion_recheck(run.run_id, &observer.api)
        .await
        .unwrap();
    assert!(
        mover
            .api
            .observe_survival_motion_recheck(&token)
            .await
            .is_err()
    );
    assert!(
        mover
            .api
            .observe_survival_motion_recheck(&superseding)
            .await
            .is_err()
    );
    observer.mover_position([0.5, 1.0, 0.5]).await;
    assert!(
        mover
            .api
            .observe_survival_motion_recheck(&superseding)
            .await
            .unwrap()
            .on_ground
    );
    assert_eq!(mover.api.survival_motion().await.unwrap().problem, original);
    assert!(!mover.api.player_state().await.unwrap().position_from_server);
    assert!(
        timeout(
            Duration::from_millis(20),
            read_packet(&mut mover.peer, None)
        )
        .await
        .is_err()
    );
    assert!(
        mover
            .api
            .observe_survival_motion_recheck(&superseding)
            .await
            .is_err()
    ); // no reuse
    observer.stop().await;
    mover.stop().await;
}

#[tokio::test]
async fn planned_multi_heading_path_preserves_turns_and_refuses_stale_preview_before_io() {
    let mut mover = Fixture::new().await;
    let mut observer = Fixture::new_id(43).await;
    observer.observe_mover().await;
    let mut controls = Vec::new();
    for yaw in [0.0, -90.0] {
        for tick in 0..20 {
            controls.push(SurvivalControl {
                yaw,
                input: SurvivalInput {
                    forward: if tick < 8 { 1 } else { 0 },
                    ..Default::default()
                },
            });
        }
    }
    let preview = mover.api.preview_survival_path(&controls).await.unwrap();
    let end = preview.frames.last().unwrap().position;
    assert!(end[0] > 2.0 && end[2] > 2.0);
    assert!(matches!(
        preview.terminal_clearance,
        TerminalClearance::Admitted { .. }
    ));
    let mut stale = preview.clone();
    stale.generation += 1;
    assert!(
        mover
            .api
            .start_previewed_survival_motion(&stale, &observer.api)
            .await
            .is_err()
    );
    let mut stale = preview.clone();
    stale.initial.world_revision += 1;
    assert!(
        mover
            .api
            .start_previewed_survival_motion(&stale, &observer.api)
            .await
            .is_err()
    );
    assert!(mover.api.survival_motion().await.is_none());
    assert!(
        timeout(
            Duration::from_millis(20),
            read_packet(&mut mover.peer, None)
        )
        .await
        .is_err()
    );
    mover
        .api
        .start_previewed_survival_motion(&preview, &observer.api)
        .await
        .unwrap();
    for c in controls {
        assert_eq!(
            read_packet(&mut mover.peer, None).await.unwrap().0,
            ids::play_serverbound::PLAYER_INPUT
        );
        let (id, payload) = read_packet(&mut mover.peer, None).await.unwrap();
        assert_eq!(id, ids::play_serverbound::POSITION_LOOK);
        assert_eq!(
            f32::from_be_bytes(payload[24..28].try_into().unwrap()),
            c.yaw
        );
    }
    timeout(Duration::from_secs(1), async {
        while mover.api.survival_motion().await.unwrap().status
            != SurvivalMotionStatus::AwaitingObservation
        {
            tokio::task::yield_now().await;
        }
    })
    .await
    .unwrap();
    observer.mover_position(end).await;
    timeout(Duration::from_secs(1), async {
        while mover.api.survival_motion().await.unwrap().status != SurvivalMotionStatus::Observed {
            tokio::task::yield_now().await;
        }
    })
    .await
    .unwrap();
    assert_eq!(mover.api.standing_context().await.unwrap().position, end);
    observer.stop().await;
    mover.stop().await;
}

#[path = "inventory_tests.rs"]
mod inventory_tests;
