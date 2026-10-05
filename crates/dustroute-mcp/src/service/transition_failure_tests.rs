//! Offline transport failures exercise cleanup and restoration, not Minecraft physics.
use super::*;
use dustroute_translate::world_reverse::RegionBounds;
use std::sync::{
    Arc,
    atomic::{AtomicUsize, Ordering},
};
use tokio::{
    io::{AsyncBufReadExt, AsyncWriteExt, BufReader},
    net::TcpListener,
};

#[tokio::test]
async fn transition_retains_original_failure_and_cleanup_errors_without_replay() {
    for activation_fails in [false, true] {
        let lever = Pos::new(0, 81, 0);
        let snapshot:dustroute_translate::snapshot::MinecraftSnapshot=serde_json::from_value(json!({
            "min":{"x":-1,"y":80,"z":-1},"max":{"x":1,"y":82,"z":1},
            "blocks":[{"pos":{"x":0,"y":80,"z":0},"name":"minecraft:stone","properties":{}},
                {"pos":lever,"name":"minecraft:lever","properties":{"face":"floor","facing":"north","powered":"false"}}]})).unwrap();
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap().to_string();
        let activations = Arc::new(AtomicUsize::new(0));
        let count = activations.clone();
        let initial = snapshot.clone();
        let transport = tokio::spawn(async move {
            let mut reads = 0;
            let mut scans = 0;
            let mut waits = 0;
            loop {
                let (mut stream, _) = listener.accept().await.unwrap();
                let mut line = String::new();
                BufReader::new(&mut stream)
                    .read_line(&mut line)
                    .await
                    .unwrap();
                let req: Value = serde_json::from_str(&line).unwrap();
                let result: Result<Value, &str> = match req["method"].as_str().unwrap() {
                    "get_block" => {
                        reads += 1;
                        if reads == 1 {
                            Ok(crate::bridge::test_readback_response(
                                &req,
                                json!({"pos":lever,"name":"minecraft:lever","properties":{"face":"floor","facing":"north","powered":"false"}}),
                            ))
                        } else {
                            Err("restored lever read unavailable")
                        }
                    }
                    "scan_region" => {
                        scans += 1;
                        if scans == 1 {
                            Ok(crate::bridge::test_readback_response(
                                &req,
                                crate::bridge::test_scan_world(&req, json!(initial)),
                            ))
                        } else {
                            Err("restored region read unavailable")
                        }
                    }
                    "approach_lever" => Ok(json!({"pos":lever,"moved":false,"distance":1.0})),
                    "start_update_recording" => {
                        Ok(json!({"recording_id":"test","started_game_tick":42}))
                    }
                    "activate_lever" => {
                        count.fetch_add(1, Ordering::SeqCst);
                        if activation_fails {
                            Err("activation reply unavailable")
                        } else {
                            Ok(json!({"pos":lever,"before_powered":false,"after_powered":true}))
                        }
                    }
                    "wait_ticks" => {
                        waits += 1;
                        if waits == 1 {
                            Ok(json!({"waited_ticks":1}))
                        } else {
                            Err("restore wait unavailable")
                        }
                    }
                    "stop_update_recording" => Err("recording unavailable"),
                    method => panic!("unexpected method {method}"),
                };
                let response = match result {
                    Ok(result) => json!({"id":req["id"],"result":result}),
                    Err(error) => json!({"id":req["id"],"error":error}),
                };
                stream
                    .write_all(format!("{response}\n").as_bytes())
                    .await
                    .unwrap();
            }
        });
        let service = DustRouteMcp::with_test_transport_and_player(
            address,
            McpPolicy {
                read_only: false,
                ..Default::default()
            },
            "Tester",
        );
        let id = uuid::Uuid::new_v4();
        service
            .plans
            .table::<StoredTransitionPlan>()
            .lock()
            .await
            .insert(
                id,
                StoredTransitionPlan {
                    player: "Tester".into(),
                    dimension: "minecraft:overworld".into(),
                    bounds: RegionBounds::new(snapshot.min, snapshot.max),
                    lever,
                    original_powered: false,
                    initial_snapshot: snapshot,
                    observation_ticks: 1,
                    max_events: 16,
                    safety: TransitionSafetyAssessment {
                        safety: TransitionSafety::Ready,
                        reasons: vec![],
                    },
                    lifecycle: InvocationState::Previewed,
                },
            );
        let activity = service
            .operations
            .begin_activity(id, crate::operations::ActivityAction::InvokeOperation)
            .await
            .unwrap();
        let result: Value = super::test_support::decode_reply(
            &crate::performance::with_activity(
                Some(activity.0.clone()),
                service.invoke_transition_test(Parameters(RunTransitionParams {
                    operation_id: id.to_string(),
                    confirm: true,
                    contracts: None,
                })),
            )
            .await,
        )
        .unwrap();
        drop(activity);
        let live = serde_json::to_value(service.operations.activity(id).await.unwrap()).unwrap();
        assert_eq!(live["active"], false);
        assert_eq!(live["execution_progress"], result["failure"]["progress"]);
        assert_eq!(result["ok"], false, "{result}");
        assert_eq!(result["failure"]["progress"]["world"], "unknown");
        let secondary = result["failure"]["secondary"].as_array().unwrap();
        if activation_fails {
            assert_eq!(result["failure"]["primary"]["phase"], "submission");
            assert_eq!(secondary.len(), 1);
            assert!(
                secondary[0]["message"]
                    .as_str()
                    .unwrap()
                    .contains("recording unavailable")
            );
        } else {
            assert_eq!(result["failure"]["primary"]["phase"], "after_readback");
            assert!(
                result["error"]
                    .as_str()
                    .unwrap()
                    .contains("recording unavailable")
            );
            assert_eq!(secondary.len(), 3, "{result}");
            for (error, message) in secondary.iter().zip([
                "restore wait unavailable",
                "restored lever read unavailable",
                "restored region read unavailable",
            ]) {
                assert_eq!(error["phase"], "restore");
                assert!(error["message"].as_str().unwrap().contains(message));
            }
        }
        let before = activations.load(Ordering::SeqCst);
        let activity = service
            .operations
            .begin_activity(id, crate::operations::ActivityAction::InvokeOperation)
            .await
            .unwrap();
        let replay: Value = super::test_support::decode_reply(
            &crate::performance::with_activity(
                Some(activity.0.clone()),
                service.invoke_transition_test(Parameters(RunTransitionParams {
                    operation_id: id.to_string(),
                    confirm: true,
                    contracts: None,
                })),
            )
            .await,
        )
        .unwrap();
        drop(activity);
        let live = serde_json::to_value(service.operations.activity(id).await.unwrap()).unwrap();
        assert!(
            live["execution_progress"].is_null(),
            "replay refusal must not import the previous attempt's progress"
        );
        assert_eq!(replay["ok"], false);
        assert_eq!(activations.load(Ordering::SeqCst), before);
        assert_eq!(
            service.operations.get(id).await.unwrap().status,
            OperationStatus::Failed
        );
        transport.abort();
    }
}

#[tokio::test]
async fn transition_results_keep_simulation_recording_and_restoration_separate() {
    for mode in [
        "normal",
        "truncated",
        "restore_wait_error",
        "forced_restore",
        "forced_submission_error",
    ] {
        let lever = Pos::new(0, 81, 0);
        let snapshot:dustroute_translate::snapshot::MinecraftSnapshot=serde_json::from_value(json!({
            "min":{"x":-1,"y":80,"z":-1},"max":{"x":1,"y":82,"z":1},
            "blocks":[{"pos":{"x":0,"y":80,"z":0},"name":"minecraft:stone","properties":{}},
                {"pos":lever,"name":"minecraft:lever","properties":{"face":"floor","facing":"north","powered":"false"}}]})).unwrap();
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap().to_string();
        let activations = Arc::new(AtomicUsize::new(0));
        let restoring = Arc::new(std::sync::atomic::AtomicBool::new(false));
        let writes = Arc::new(AtomicUsize::new(0));
        let write_count = writes.clone();
        let count = activations.clone();
        let restore_phase = restoring.clone();
        let initial = snapshot.clone();
        let transport = tokio::spawn(async move {
            let mut powered = false;
            let mut written = false;
            loop {
                let (mut stream, _) = listener.accept().await.unwrap();
                let mut line = String::new();
                BufReader::new(&mut stream)
                    .read_line(&mut line)
                    .await
                    .unwrap();
                let req: Value = serde_json::from_str(&line).unwrap();
                let result: Result<Value, &str> = match req["method"].as_str().unwrap() {
                    "get_block" => Ok(crate::bridge::test_readback_response(
                        &req,
                        json!({"pos":lever,"name":"minecraft:lever","properties":{"face":"floor","facing":"north","powered":powered.to_string()}}),
                    )),
                    "scan_region" => {
                        let mut current = initial.clone();
                        current
                            .blocks
                            .iter_mut()
                            .find(|block| block.pos == lever)
                            .unwrap()
                            .properties
                            .insert("powered".into(), powered.to_string());
                        if mode.starts_with("forced_")
                            && restore_phase.load(Ordering::SeqCst)
                            && !written
                        {
                            current
                                .blocks
                                .iter_mut()
                                .find(|block| block.pos != lever)
                                .unwrap()
                                .name = "minecraft:dirt".into();
                        }
                        Ok(crate::bridge::test_readback_response(
                            &req,
                            crate::bridge::test_scan_world(&req, json!(current)),
                        ))
                    }
                    "approach_lever" => Ok(json!({"pos":lever,"moved":false,"distance":1.0})),
                    "start_update_recording" => {
                        Ok(json!({"recording_id":"test","started_game_tick":42}))
                    }
                    "activate_lever" => {
                        count.fetch_add(1, Ordering::SeqCst);
                        let before = powered;
                        powered = !powered;
                        Ok(json!({"pos":lever,"before_powered":before,"after_powered":powered}))
                    }
                    "wait_ticks"
                        if mode == "restore_wait_error" && restore_phase.load(Ordering::SeqCst) =>
                    {
                        Err("restore wait unavailable")
                    }
                    "wait_ticks" => Ok(json!({"waited_ticks":1})),
                    "stop_update_recording" => Ok(
                        json!({"recording_id":"test","started_game_tick":42,"stopped_game_tick":44,"seen_events":0,"truncated":mode=="truncated","events":[]}),
                    ),
                    "submit_command_batch" => {
                        write_count.fetch_add(1, Ordering::SeqCst);
                        if mode == "forced_submission_error" {
                            Err("restoration write reply unavailable")
                        } else {
                            written = true;
                            Ok(
                                json!({"protocol":"dustroute.bridge-mutation.v1","submitted_changes":req["params"]["changes"].as_array().unwrap().len()}),
                            )
                        }
                    }
                    method => panic!("unexpected method {method}"),
                };
                let response = match result {
                    Ok(result) => json!({"id":req["id"],"result":result}),
                    Err(error) => json!({"id":req["id"],"error":error}),
                };
                stream
                    .write_all(format!("{response}\n").as_bytes())
                    .await
                    .unwrap();
            }
        });
        let service = DustRouteMcp::with_test_transport_and_player(
            address,
            McpPolicy {
                read_only: false,
                ..Default::default()
            },
            "Tester",
        );
        let id = uuid::Uuid::new_v4();
        service
            .plans
            .table::<StoredTransitionPlan>()
            .lock()
            .await
            .insert(
                id,
                StoredTransitionPlan {
                    player: "Tester".into(),
                    dimension: "minecraft:overworld".into(),
                    bounds: RegionBounds::new(snapshot.min, snapshot.max),
                    lever,
                    original_powered: false,
                    initial_snapshot: snapshot,
                    observation_ticks: 1,
                    max_events: 16,
                    safety: TransitionSafetyAssessment {
                        safety: TransitionSafety::Ready,
                        reasons: vec![],
                    },
                    lifecycle: InvocationState::Previewed,
                },
            );
        let activity = service
            .operations
            .begin_activity(id, crate::operations::ActivityAction::InvokeOperation)
            .await
            .unwrap();
        let reply = crate::performance::with_activity(
            Some(activity.0.clone()),
            service.invoke_transition_test(Parameters(RunTransitionParams {
                operation_id: id.to_string(),
                confirm: true,
                contracts: None,
            })),
        )
        .await;
        drop(activity);
        let result = test_support::decode_reply(&reply).unwrap();
        assert_eq!(result["ok"], mode != "truncated", "{mode}: {result}");
        assert_eq!(reply.is_error, Some(mode == "truncated"));
        assert_eq!(activations.load(Ordering::SeqCst), 2);
        assert_eq!(result["restoration"]["verified"], true);
        assert_eq!(result["recording"]["truncated"], mode == "truncated");
        assert!(result["scenario_verification"]["live_trace"]["final_strengths"].is_array());
        assert!(
            result["scenario_verification"]["simulated"]["run"]["trace"]["final_powered"]
                .is_array()
        );
        assert_eq!(
            result["scenario_verification"]["scenario"]["expectation"],
            json!({"final_strengths":{},"final_powered":{},"pulses":[]})
        );
        let live = serde_json::to_value(service.operations.activity(id).await.unwrap()).unwrap();
        let expected = if mode == "truncated" {
            &result["failure"]["progress"]
        } else {
            &result["execution_progress"]
        };
        assert_eq!(&live["execution_progress"], expected);
        assert_eq!(expected["world"], "verified");
        assert_eq!(expected["verified_steps"], 2);
        let recorded = service.operations.get(id).await.unwrap();
        assert!(matches!(
            recorded.result,
            Some(crate::operations::OperationResult::TransitionRun(_))
        ));
        let replay = test_support::decode_reply(
            &service
                .invoke_transition_test(Parameters(RunTransitionParams {
                    operation_id: id.to_string(),
                    confirm: true,
                    contracts: None,
                }))
                .await,
        )
        .unwrap();
        assert_eq!(replay["ok"], false);
        assert_eq!(activations.load(Ordering::SeqCst), 2);
        restoring.store(true, Ordering::SeqCst);
        let restored = test_support::decode_reply(
            &service
                .restore_transition_test(Parameters(RunTransitionParams {
                    operation_id: id.to_string(),
                    confirm: true,
                    contracts: None,
                }))
                .await,
        )
        .unwrap();
        assert_eq!(
            restored["ok"],
            !matches!(mode, "restore_wait_error" | "forced_submission_error"),
            "{mode}: {restored}"
        );
        assert_eq!(
            restored["restoration_verified"],
            mode != "forced_submission_error"
        );
        assert_eq!(
            restored["natural_restore_verified"],
            !mode.starts_with("forced_")
        );
        assert_eq!(
            restored["snapshot_restore_attempted"],
            mode.starts_with("forced_")
        );
        if mode == "forced_submission_error" {
            assert!(
                restored["snapshot_restore_error"]
                    .as_str()
                    .unwrap()
                    .contains("restoration write reply unavailable")
            );
            assert_eq!(restored["failure"]["progress"]["world"], "unknown");
            assert_eq!(
                restored["failure"]["progress"]["submitted_changes"],
                json!(null)
            );
        } else {
            assert_eq!(restored["snapshot_restore_error"], json!(null));
        }
        assert_eq!(
            writes.load(Ordering::SeqCst),
            usize::from(mode.starts_with("forced_"))
        );
        assert_eq!(activations.load(Ordering::SeqCst), 2);
        let recorded = service.operations.get(id).await.unwrap();
        assert!(matches!(
            recorded.result,
            Some(crate::operations::OperationResult::TransitionRestore(_))
        ));
        if mode == "restore_wait_error" {
            assert_eq!(restored["failure"]["progress"]["world"], "verified");
            assert_eq!(restored["failure"]["primary"]["phase"], "restore");
            assert_eq!(recorded.status, OperationStatus::Failed);
        }
        transport.abort();
    }
}

#[tokio::test]
async fn transition_preflight_refusals_preserve_wire_details_without_creating_progress() {
    for (lifecycle, safety, require_preview, code) in [
        (
            InvocationState::Draft,
            TransitionSafety::Ready,
            true,
            "invalid_state",
        ),
        (
            InvocationState::NeedsInspection,
            TransitionSafety::Ready,
            false,
            "internal",
        ),
        (
            InvocationState::Previewed,
            TransitionSafety::PreviewOnly,
            true,
            "invalid_state",
        ),
    ] {
        let service = DustRouteMcp::with_test_transport_and_player(
            "127.0.0.1:1",
            McpPolicy {
                read_only: false,
                preview_required: require_preview,
                ..Default::default()
            },
            "Tester",
        );
        let id = uuid::Uuid::new_v4();
        let lever = Pos::new(0, 81, 0);
        service
            .plans
            .table::<StoredTransitionPlan>()
            .lock()
            .await
            .insert(
                id,
                StoredTransitionPlan {
                    player: "Tester".into(),
                    dimension: "minecraft:overworld".into(),
                    bounds: RegionBounds::new(lever, lever),
                    lever,
                    original_powered: false,
                    initial_snapshot: dustroute_translate::snapshot::MinecraftSnapshot {
                        min: lever,
                        max: lever,
                        blocks: vec![],
                    },
                    observation_ticks: 1,
                    max_events: 16,
                    safety: TransitionSafetyAssessment {
                        safety,
                        reasons: vec![],
                    },
                    lifecycle,
                },
            );
        let activity = service
            .operations
            .begin_activity(id, crate::operations::ActivityAction::InvokeOperation)
            .await
            .unwrap();
        let reply = crate::performance::with_activity(
            Some(activity.0.clone()),
            service.invoke_transition_test(Parameters(RunTransitionParams {
                operation_id: id.to_string(),
                confirm: true,
                contracts: None,
            })),
        )
        .await;
        drop(activity);
        assert_eq!(reply.is_error, Some(true));
        let result = test_support::decode_reply(&reply).unwrap();
        assert_eq!(result["schema_version"], crate::api::ERROR_SCHEMA_V1);
        assert_eq!(result["error_code"], code);
        assert!(result["failure"]["progress"].is_null());
        if safety == TransitionSafety::PreviewOnly {
            assert_eq!(
                result["safety"],
                json!({"safety":"preview_only","reasons":[]})
            );
        }
        let live = serde_json::to_value(service.operations.activity(id).await.unwrap()).unwrap();
        assert!(live["execution_progress"].is_null());
        assert!(service.operations.get(id).await.is_none());
        assert_eq!(
            service
                .plans
                .table::<StoredTransitionPlan>()
                .lock()
                .await
                .get(&id)
                .unwrap()
                .lifecycle,
            lifecycle
        );
    }
}

#[tokio::test]
async fn transition_candidates_preserve_safety_and_have_no_execution_progress() {
    use dustroute_translate::snapshot::{MinecraftSnapshot, MinecraftSnapshotBlock};
    for (mode, expected_safety, candidate_count) in [
        ("off", "ready", 1),
        ("on", "ready", 1),
        ("temporal", "preview_only", 1),
        ("dangerous", "rejected", 0),
        ("no_lever", "rejected", 0),
        ("invalid_powered", "ready", 0),
    ] {
        let service = DustRouteMcp::with_test_transport_and_player(
            "127.0.0.1:1",
            McpPolicy::default(),
            "Tester",
        );
        let lever = Pos::new(0, 81, 0);
        let mut blocks = vec![MinecraftSnapshotBlock {
            pos: Pos::new(0, 80, 0),
            name: "minecraft:stone".into(),
            properties: Default::default(),
        }];
        if mode != "no_lever" {
            blocks.push(MinecraftSnapshotBlock {
                pos: lever,
                name: "minecraft:lever".into(),
                properties: std::collections::BTreeMap::from([
                    ("face".into(), "floor".into()),
                    ("facing".into(), "north".into()),
                    (
                        "powered".into(),
                        match mode {
                            "on" => "true",
                            "invalid_powered" => "invalid",
                            _ => "false",
                        }
                        .into(),
                    ),
                ]),
            });
        }
        if matches!(mode, "temporal" | "dangerous") {
            blocks.push(MinecraftSnapshotBlock {
                pos: Pos::new(1, 80, 0),
                name: if mode == "temporal" {
                    "minecraft:piston"
                } else {
                    "minecraft:tnt"
                }
                .into(),
                properties: Default::default(),
            });
        }
        let bounds = RegionBounds::new(Pos::new(-1, 80, -1), Pos::new(1, 82, 1));
        let initial = MinecraftSnapshot {
            min: bounds.min,
            max: bounds.max,
            blocks,
        };
        let circuit_id = service
            .store_circuit(StoredCircuit {
                player: "Tester".into(),
                dimension: "minecraft:overworld".into(),
                bounds,
                target: None,
                snapshot: service.bridge.share_snapshot(initial.clone()).unwrap(),
                expansion: ExpansionEvidence::Unspecified {},
                complete: true,
                expires_at: Instant::now() + Duration::from_secs(300),
            })
            .await;
        let reply = service
            .new_transition_test(Parameters(ProposeTransitionParams {
                player: None,
                observation_ticks: Some(40),
                max_events: Some(512),
                circuit_id: circuit_id.to_string(),
            }))
            .await;
        let result = test_support::decode_reply(&reply).unwrap();
        let rejected = expected_safety == "rejected";
        assert_eq!(reply.is_error, Some(rejected));
        assert_eq!(result["ok"], !rejected);
        assert_eq!(
            service
                .plans
                .table::<StoredTransitionPlan>()
                .lock()
                .await
                .len(),
            candidate_count
        );
        if rejected {
            assert_eq!(result["safety"]["safety"], expected_safety);
            assert!(result["failure"]["progress"].is_null());
            continue;
        }
        assert_eq!(result["schema_version"], TRANSITION_SCHEMA_V1);
        assert_eq!(result["bounds"], json!({"min":bounds.min,"max":bounds.max}));
        assert_eq!(result["circuit_id"], json!(circuit_id));
        let proposals = result["proposals"].as_array().unwrap();
        assert_eq!(proposals.len(), candidate_count);
        for proposal in proposals {
            assert!(proposal.get("ok").is_none());
            assert!(proposal.get("execution_progress").is_none());
            assert_eq!(proposal["lever"], json!(lever));
            assert_eq!(proposal["observation_ticks"], 40);
            assert_eq!(proposal["max_events"], 512);
            assert_eq!(proposal["safety"]["safety"], expected_safety);
            assert_eq!(
                proposal["transition"],
                if mode == "on" {
                    "on_to_off"
                } else {
                    "off_to_on"
                }
            );
            let id = uuid::Uuid::parse_str(proposal["operation_id"].as_str().unwrap()).unwrap();
            let stored = service.operations.get(id).await.unwrap();
            assert_eq!(stored.kind, OperationKind::TransitionProposal);
            assert_eq!(stored.status, OperationStatus::Completed);
            assert_eq!(stored.progress_percent, 100);
            let result = stored.result.unwrap();
            assert!(matches!(
                result,
                crate::operations::OperationResult::TransitionProposal(_)
            ));
            assert!(!result.failed());
            assert!(result.progress().is_none());
            assert_eq!(serde_json::to_value(result).unwrap(), *proposal);
            let plans = service.plans.table::<StoredTransitionPlan>().lock().await;
            let plan = plans.get(&id).unwrap();
            assert_eq!(plan.lifecycle, InvocationState::Draft);
            assert_eq!(plan.original_powered, mode == "on");
            assert_eq!(plan.initial_snapshot, initial);
        }
    }
    // Authorization happens before parsing/loading a circuit or creating plans.
    let service = DustRouteMcp::with_test_transport_and_player(
        "127.0.0.1:1",
        McpPolicy {
            allowed_players: ["AnotherPlayer".to_owned()].into(),
            ..Default::default()
        },
        "Tester",
    );
    let reply = service
        .new_transition_test(Parameters(ProposeTransitionParams {
            player: None,
            observation_ticks: None,
            max_events: None,
            circuit_id: "not a circuit id".into(),
        }))
        .await;
    assert_eq!(reply.is_error, Some(true));
    let result = test_support::decode_reply(&reply).unwrap();
    assert_eq!(result["error_code"], "permission_denied");
    assert_eq!(result["failure"]["primary"]["phase"], "admission");
    assert!(result["failure"]["progress"].is_null());
    assert!(
        service
            .plans
            .table::<StoredTransitionPlan>()
            .lock()
            .await
            .is_empty()
    );
}

#[tokio::test]
async fn transition_preview_submission_does_not_replace_candidate_or_execution_evidence() {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap().to_string();
    let previews = Arc::new(AtomicUsize::new(0));
    let count = previews.clone();
    let transport = tokio::spawn(async move {
        loop {
            let (mut stream, _) = listener.accept().await.unwrap();
            let mut line = String::new();
            BufReader::new(&mut stream)
                .read_line(&mut line)
                .await
                .unwrap();
            let request: Value = serde_json::from_str(&line).unwrap();
            assert_eq!(request["method"], "preview_region");
            assert_eq!(request["params"]["player"], "Tester");
            count.fetch_add(1, Ordering::SeqCst);
            stream
                .write_all(
                    format!(
                        "{}\n",
                        json!({"id":request["id"],"result":{"particle_corners":8}})
                    )
                    .as_bytes(),
                )
                .await
                .unwrap();
        }
    });
    let service =
        DustRouteMcp::with_test_transport_and_player(address, McpPolicy::default(), "Tester");
    let lever = Pos::new(0, 81, 0);
    let snapshot = dustroute_translate::snapshot::MinecraftSnapshot {
        min: lever,
        max: lever,
        blocks: vec![dustroute_translate::snapshot::MinecraftSnapshotBlock {
            pos: lever,
            name: "minecraft:lever".into(),
            properties: [("powered".into(), "false".into())].into(),
        }],
    };
    let (_, candidates) = service
        .create_transition_proposals(
            "Tester",
            "minecraft:overworld",
            RegionBounds::new(lever, lever),
            &snapshot,
            20,
            512,
        )
        .await;
    let candidate = serde_json::to_value(&candidates[0]).unwrap();
    let id = uuid::Uuid::parse_str(candidate["operation_id"].as_str().unwrap()).unwrap();
    let request = || {
        Parameters(PreviewTransitionParams {
            operation_id: id.to_string(),
            player: None,
        })
    };
    let reply = service.show_transition_test(request()).await;
    assert_eq!(reply.is_error, Some(false));
    let result = test_support::decode_reply(&reply).unwrap();
    assert_eq!(
        result["preview"],
        json!({"min":lever,"max":lever,"particle_corners":8,"submission_only":true})
    );
    assert_eq!(result["original_powered"], false);
    assert!(result.get("execution_progress").is_none());
    assert_eq!(
        service
            .plans
            .table::<StoredTransitionPlan>()
            .lock()
            .await
            .get(&id)
            .unwrap()
            .lifecycle,
        InvocationState::Previewed
    );
    // Keep the existing submission-before-lifecycle-check order on re-preview.
    service
        .plans
        .table::<StoredTransitionPlan>()
        .lock()
        .await
        .get_mut(&id)
        .unwrap()
        .lifecycle = InvocationState::NeedsInspection;
    let refused = service.show_transition_test(request()).await;
    assert_eq!(refused.is_error, Some(true));
    let refused = test_support::decode_reply(&refused).unwrap();
    assert!(refused["failure"]["progress"].is_null());
    assert_eq!(previews.load(Ordering::SeqCst), 2);
    let stored = service.operations.get(id).await.unwrap().result.unwrap();
    assert_eq!(serde_json::to_value(&stored).unwrap(), candidate);
    assert!(stored.progress().is_none());
    assert_eq!(
        service
            .plans
            .table::<StoredTransitionPlan>()
            .lock()
            .await
            .get(&id)
            .unwrap()
            .lifecycle,
        InvocationState::NeedsInspection
    );
    transport.abort();
}

/// Conflicting invalid inputs must still be rejected in the established order,
/// before transport access or attempt publication, for both run and restore.
#[tokio::test]
async fn transition_admission_preserves_refusal_priority_without_starting_work() {
    let missing_id = uuid::Uuid::nil().to_string();
    for (read_only, confirm, id, code, message) in [
        (
            true,
            false,
            "invalid-id",
            "invalid_argument",
            "confirm=true is required",
        ),
        (
            true,
            true,
            "invalid-id",
            "permission_denied",
            "read-only policy",
        ),
        (false, true, "invalid-id", "invalid_argument", "invalid"),
        (
            false,
            true,
            missing_id.as_str(),
            "not_found",
            "transition scenario not found",
        ),
    ] {
        let service = DustRouteMcp::with_test_transport_and_player(
            "127.0.0.1:1",
            McpPolicy {
                read_only,
                ..Default::default()
            },
            "Tester",
        );
        for restore in [false, true] {
            let params = Parameters(RunTransitionParams {
                operation_id: id.into(),
                confirm,
                contracts: None,
            });
            let reply = if restore {
                service.restore_transition_test(params).await
            } else {
                service.invoke_transition_test(params).await
            };
            let response = test_support::decode_reply(&reply).unwrap();
            assert_eq!(reply.is_error, Some(true));
            assert_eq!(response["error_code"], code);
            assert!(response["error"].as_str().unwrap().contains(message));
            assert!(response["failure"]["progress"].is_null());
            assert!(service.operations.list().await.is_empty());
        }
    }
}
