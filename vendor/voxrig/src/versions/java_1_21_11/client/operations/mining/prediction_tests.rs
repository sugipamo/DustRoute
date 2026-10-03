//! One-client motion contract; the loopback peer supplies no position echo.
use super::*;

async fn settle(f: &mut Fixture, controls: &[SurvivalControl]) -> SurvivalMotionRecord {
    let start = f.api.start_predicted_survival_path(controls).await.unwrap();
    assert_eq!(start.contract, SurvivalMotionContract::Predicted);
    for _ in controls {
        assert_eq!(
            read_packet(&mut f.peer, None).await.unwrap().0,
            ids::play_serverbound::PLAYER_INPUT
        );
        assert_eq!(
            read_packet(&mut f.peer, None).await.unwrap().0,
            ids::play_serverbound::POSITION_LOOK
        );
    }
    timeout(Duration::from_secs(1), async {
        loop {
            let r = f.api.survival_motion().await.unwrap();
            if r.status != SurvivalMotionStatus::Running {
                break r;
            }
            tokio::task::yield_now().await;
        }
    })
    .await
    .unwrap()
}

#[tokio::test]
async fn predicted_motion_retains_provenance_and_matches_explicit_hypothetical_contract() {
    let mut f = Fixture::new().await;
    f.session
        .state
        .lock()
        .await
        .world
        .seed_replay_cell(TARGET, 0);
    let region = crate::Region {
        min: [-2, -1, -2],
        max: [4, 6, 2],
    };
    let captured = f.api.capture_survival_scene(region).await.unwrap();
    let controls: Vec<_> = (0..16)
        .map(|t| SurvivalControl {
            yaw: 0.,
            input: SurvivalInput {
                forward: i8::from(t < 4),
                ..Default::default()
            },
        })
        .collect();
    let future = captured
        .scenario_with_motion_contract(SurvivalMotionContract::Predicted)
        .after_path(&controls)
        .unwrap();
    let independent_future = captured.scenario().after_path(&controls).unwrap();
    let r = settle(&mut f, &controls).await;
    assert_eq!(r.status, SurvivalMotionStatus::Predicted);
    assert!(r.observed.is_none() && r.initial_watch.is_none() && r.final_watch.is_none());
    assert!(r.observer_connection_id.is_none());
    let standing = f.api.standing_context().await.unwrap();
    assert!(matches!(
        standing.position_basis,
        StandingPositionBasis::Predicted { .. }
    ));
    future
        .aim_requirement()
        .validate_standing(&standing)
        .unwrap();
    assert!(
        independent_future
            .aim_requirement()
            .validate_standing(&standing)
            .is_err()
    );
    assert_eq!(
        f.session
            .state
            .lock()
            .await
            .motion
            .received_pose
            .as_ref()
            .unwrap()
            .position,
        captured.source().position
    );
    let next = vec![
        SurvivalControl {
            yaw: -90.,
            input: Default::default()
        };
        3
    ];
    let preview = f.api.preview_survival_path(&next).await.unwrap();
    assert_eq!(
        preview.initial_frame,
        future.preview_path(&next).unwrap().initial_frame
    );
    assert_eq!(preview.frames, future.preview_path(&next).unwrap().frames);
    let fresh = f.api.capture_survival_scene(region).await.unwrap();
    assert!(matches!(
        fresh.scenario().aim_requirement(),
        HypotheticalAimRequirement::PredictedEndpoint { .. }
    ));
    let json = serde_json::to_value(&standing.position_basis).unwrap();
    assert_eq!(json["kind"], "predicted");
    assert!(json.get("observed").is_none());
    assert_eq!(
        json["planning_reserve"],
        serde_json::json!([0.0625, 0., 0.0625])
    );
    f.api.look([0., 0.]).await.unwrap();
    assert_eq!(
        read_packet(&mut f.peer, None).await.unwrap().0,
        ids::play_serverbound::LOOK
    );
    f.stop().await;
}

#[tokio::test]
async fn predicted_motion_refuses_correction_impulse_and_changed_generation_before_more_sends() {
    let mut f = Fixture::new().await;
    let controls = [SurvivalControl {
        yaw: 0.,
        input: Default::default(),
    }; 3];
    assert_eq!(
        settle(&mut f, &controls).await.status,
        SurvivalMotionStatus::Predicted
    );
    let (motion, player, generation) = {
        let s = f.session.state.lock().await;
        (
            s.motion.clone(),
            s.operations.local_player.clone(),
            s.loading.generation,
        )
    };
    for problem in ["correction", "impulse", "generation"] {
        {
            let mut s = f.session.state.lock().await;
            s.motion = motion.clone();
            s.operations.local_player = player.clone();
            s.loading.generation = generation;
            match problem {
                "correction" => {
                    let mut pose = s.motion.received_pose.clone().unwrap();
                    pose.receive_sequence += 1;
                    s.motion.receive(pose);
                }
                "impulse" => s.operations.local_player.velocity.as_mut().unwrap().value[0] = 0.1,
                _ => s.loading.generation += 1,
            }
        }
        assert!(f.api.standing_context().await.is_err(), "{problem}");
        assert!(
            f.api
                .start_predicted_survival_path(&controls)
                .await
                .is_err(),
            "{problem}"
        );
        assert!(f.api.select_hotbar(1).await.is_err(), "{problem}");
    }
    assert!(
        timeout(Duration::from_millis(20), read_packet(&mut f.peer, None))
            .await
            .is_err()
    );
    f.stop().await;
}

#[tokio::test]
async fn predicted_motion_checks_current_support_and_terminal_geometry_without_replaying() {
    let mut f = Fixture::new().await;
    let controls = [SurvivalControl {
        yaw: 0.,
        input: Default::default(),
    }; 3];
    assert_eq!(
        settle(&mut f, &controls).await.status,
        SurvivalMotionStatus::Predicted
    );
    f.session
        .state
        .lock()
        .await
        .world
        .seed_replay_cell([0, 0, 0], 0);
    assert!(f.api.preview_survival_path(&controls).await.is_err());
    assert!(
        f.api
            .place_survival_cube([0, 0, 0], crate::BlockFace::Up)
            .await
            .is_err()
    );
    f.session
        .state
        .lock()
        .await
        .world
        .seed_replay_cell([0, 1, 0], 1);
    assert!(f.api.standing_context().await.is_err());
    assert!(
        timeout(Duration::from_millis(20), read_packet(&mut f.peer, None))
            .await
            .is_err()
    );
    f.stop().await;
}

#[tokio::test]
async fn interrupted_predicted_writer_never_yields_a_continuation_candidate() {
    let f = Fixture::new().await;
    let writer = f.session.writer.lock().await;
    let controls = [SurvivalControl {
        yaw: 0.,
        input: Default::default(),
    }; 3];
    f.api
        .start_predicted_survival_path(&controls)
        .await
        .unwrap();
    tokio::time::sleep(Duration::from_millis(20)).await;
    f.session.stop();
    drop(writer);
    let record = timeout(Duration::from_secs(1), async {
        loop {
            let r = f.api.survival_motion().await.unwrap();
            if r.status == SurvivalMotionStatus::RequiresInspection {
                break r;
            }
            tokio::task::yield_now().await;
        }
    })
    .await
    .unwrap();
    assert_eq!(record.attempted_tick, 1);
    assert_eq!(record.dispatched_ticks, 0);
    assert!(!record.status.is_continuation_candidate());
    assert!(f.api.standing_context().await.is_err());
    f.stop().await;
}
