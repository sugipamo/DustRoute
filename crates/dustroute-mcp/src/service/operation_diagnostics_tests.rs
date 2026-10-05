//! Offline MCP tests: controllable transport delays, not live Minecraft trials.
use super::test_support::{call, serve, stop, temporary};
use super::*;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::net::TcpListener;
use tokio::sync::oneshot;
use tokio::time::{Duration, timeout};

#[tokio::test]
async fn diagnostics_preserve_input_constraints_identity_and_connection_causes() {
    let root = temporary();
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap().to_string();
    drop(listener); // A deterministic disconnected transport, no Minecraft process.
    let mut service =
        DustRouteMcp::with_test_transport_and_player(address, McpPolicy::default(), "Tester");
    service.state_store = PlanStateStore::new(root.clone(), 3600);
    let (client, server) = serve(service).await;
    let limit = call(&client, "get_world", json!({"max_components":0})).await;
    assert_eq!(limit["error_code"], "invalid_argument");
    assert_eq!(
        limit["failure"]["primary"]["details"]["parameter"],
        "max_components"
    );
    assert_eq!(
        limit["failure"]["primary"]["details"]["allowed_range"],
        json!([1.0, 32768.0])
    );
    assert_eq!(limit["failure"]["progress"], Value::Null);
    let denied = call(&client, "get_world", json!({"player":"Other"})).await;
    assert_eq!(denied["failure"]["primary"]["kind"], "permission_denied");
    let disconnected = call(&client, "get_world", json!({})).await;
    assert_eq!(disconnected["failure"]["primary"]["kind"], "connection");
    assert!(disconnected["error"].is_string());
    stop(client, server).await;
    if root.exists() {
        std::fs::remove_dir_all(root).unwrap();
    }
}

#[tokio::test]
async fn polling_observes_inflight_scan_and_keeps_typed_failure_or_cancellation() {
    for cancel in [false, true] {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap().to_string();
        let (entered_tx, entered_rx) = oneshot::channel();
        let (release_tx, release_rx) = oneshot::channel();
        let transport = tokio::spawn(async move {
            let (mut stream, _) = listener.accept().await.unwrap();
            let mut line = String::new();
            BufReader::new(&mut stream)
                .read_line(&mut line)
                .await
                .unwrap();
            let request: Value = serde_json::from_str(&line).unwrap();
            assert_eq!(request["method"], "scan_region");
            entered_tx.send(()).unwrap();
            release_rx.await.unwrap();
            let snapshot = crate::bridge::test_readback_response(
                &request,
                json!({
                    "min":{"x":0,"y":80,"z":0},"max":{"x":0,"y":80,"z":0},
                    "blocks":[{"pos":{"x":0,"y":80,"z":0},"name":"minecraft:repeater","properties":{"facing":"invalid"}}]
                }),
            );
            let response = json!({"id":request["id"],"result":snapshot});
            stream
                .write_all(format!("{response}\n").as_bytes())
                .await
                .unwrap();
        });
        let root = temporary();
        let mut service =
            DustRouteMcp::with_test_transport_and_player(address, McpPolicy::default(), "Tester");
        service.state_store = PlanStateStore::new(root.clone(), 3600);
        service.tool_router = DustRouteMcp::tool_router();
        service.selections.lock().await.insert(
            "Tester".into(),
            LocatedSelection::with_bounds(
                "Tester",
                dustroute_translate::world_reverse::RegionBounds::new(
                    Pos::new(0, 80, 0),
                    Pos::new(0, 80, 0),
                ),
                "minecraft:overworld".into(),
            ),
        );
        let registry = service.operations.clone();
        let (client, server) = serve(service).await;
        let started = call(&client, "start_selected_region_conversion", json!({})).await;
        let id = started["operation_id"].clone();
        timeout(Duration::from_secs(3), entered_rx)
            .await
            .unwrap()
            .unwrap();
        let running = call(&client, "get_operation", json!({"operation_id":id})).await;
        assert_eq!(running["operation"]["status"], "running");
        assert_eq!(running["ok"], true);
        assert_eq!(running["activity"]["active"], true);
        assert_eq!(running["activity"]["phase"], "scan");
        assert_eq!(running["activity"]["phase_active"], true);
        assert_eq!(running["activity"]["execution_progress"], Value::Null);
        assert!(running["activity"]["elapsed_ms"].is_number());
        if cancel {
            assert_eq!(
                call(&client, "stop_operation", json!({"operation_id":id})).await["ok"],
                true
            );
            // Cancellation is a request; the in-flight transport is still observable.
            assert_eq!(
                call(&client, "get_operation", json!({"operation_id":id})).await["activity"]["active"],
                true
            );
        }
        release_tx.send(()).unwrap();
        transport.await.unwrap();
        let uuid = uuid::Uuid::parse_str(id.as_str().unwrap()).unwrap();
        timeout(Duration::from_secs(3), async {
            while registry.activity(uuid).await.unwrap().active {
                tokio::task::yield_now().await;
            }
        })
        .await
        .unwrap();
        let finished = call(&client, "get_operation", json!({"operation_id":id})).await;
        assert_eq!(finished["activity"]["active"], false);
        assert_eq!(finished["activity"]["phase_active"], false);
        if cancel {
            assert_eq!(finished["operation"]["status"], "cancelled");
            assert_eq!(finished["operation"]["result"], Value::Null);
        } else {
            assert_eq!(finished["operation"]["status"], "failed");
            assert_eq!(
                finished["operation"]["result"]["failure"]["primary"]["kind"],
                "observation_incomplete"
            );
            assert_eq!(
                finished["operation"]["result"]["failure"]["primary"]["phase"],
                "normalization"
            );
            assert_eq!(
                finished["operation"]["result"]["failure"]["primary"]["details"]["position"],
                json!({"x":0,"y":80,"z":0})
            );
        }
        stop(client, server).await;
        if root.exists() {
            std::fs::remove_dir_all(root).unwrap();
        }
    }
}

#[tokio::test]
async fn live_activity_does_not_replace_consumed_history_or_claim_mutation_cancellation() {
    let root = temporary();
    let mut service =
        DustRouteMcp::with_test_transport_and_player("127.0.0.1:1", McpPolicy::default(), "Tester");
    service.state_store = PlanStateStore::new(root.clone(), 3600);
    service.tool_router = DustRouteMcp::tool_router();
    let registry = service.operations.clone();
    let id = uuid::Uuid::new_v4();
    let original = crate::operations::mutation::PlacementAttempt {
        operation_id: id.to_string(),
        outcome: crate::operations::mutation::PlacementOutcome::failed(
            ExecutionProgress {
                operation_consumed: true,
                world: crate::failure::WorldOutcome::Unknown,
                ..Default::default()
            }
            .cause(FailureCause::new(CauseKind::Timeout, "reply lost")),
        ),
    };
    registry
        .record_completed(id, OperationKind::PlacementApply, original.clone().into())
        .await;
    let expected = serde_json::to_value(&original).unwrap();
    let guard = registry
        .begin_activity(id, crate::operations::ActivityAction::InvokeOperation)
        .await
        .unwrap();
    assert!(
        registry
            .begin_activity(id, crate::operations::ActivityAction::InvokeOperation)
            .await
            .is_none()
    );
    let (client, server) = serve(service).await;
    crate::performance::with_activity(Some(guard.0.clone()), async {
        let _phase = crate::performance::span(crate::performance::Phase::Wait);
        let progress = ExecutionProgress {
            total_changes: Some(12),
            submitted_changes: Some(8),
            verified_steps: 4,
            durable_verified_steps: Some(3),
            ..Default::default()
        };
        crate::performance::execution_progress(&progress);
        let read = call(&client, "get_operation", json!({"operation_id":id})).await;
        assert_eq!(read["operation"]["result"], expected);
        assert_eq!(read["activity"]["phase"], "wait");
        assert_eq!(read["activity"]["execution_progress"]["verified_steps"], 4);
        assert_eq!(
            read["activity"]["execution_progress"]["durable_verified_steps"],
            3
        );
        assert_eq!(read["activity"]["cancellable"], false);
        assert_eq!(
            call(&client, "stop_operation", json!({"operation_id":id})).await["ok"],
            false
        );
    })
    .await;
    registry
        .record_completed(
            id,
            OperationKind::PlacementApply,
            crate::operations::mutation::PlacementAttempt {
                operation_id: id.to_string(),
                outcome: crate::operations::mutation::PlacementOutcome::refused("refused"),
            }
            .into(),
        )
        .await;
    drop(guard);
    let saved = call(&client, "get_operation", json!({"operation_id":id})).await;
    assert_eq!(saved["operation"]["result"], expected);
    assert_eq!(saved["activity"]["active"], false);
    stop(client, server).await;
    if root.exists() {
        std::fs::remove_dir_all(root).unwrap();
    }
}

#[tokio::test]
async fn invoke_reports_submitted_but_unverified_changes_while_readback_is_pending() {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap().to_string();
    let (entered_tx, entered_rx) = oneshot::channel();
    let (release_tx, release_rx) = oneshot::channel();
    let transport = tokio::spawn(async move {
        let mut submitted = false;
        let mut gate = Some((entered_tx, release_rx));
        loop {
            let (mut stream, _) = listener.accept().await.unwrap();
            let mut line = String::new();
            BufReader::new(&mut stream)
                .read_line(&mut line)
                .await
                .unwrap();
            let request: Value = serde_json::from_str(&line).unwrap();
            let result = match request["method"].as_str().unwrap() {
                "scan_region" => {
                    if submitted && let Some((entered, release)) = gate.take() {
                        entered.send(()).unwrap();
                        release.await.unwrap();
                    }
                    let blocks = if submitted {
                        json!([{"pos":{"x":0,"y":1,"z":0},"name":"minecraft:stone","properties":{}}])
                    } else {
                        json!([])
                    };
                    crate::bridge::test_readback_response(
                        &request,
                        json!({
                            "min":request["params"]["min"],"max":request["params"]["max"],"blocks":blocks
                        }),
                    )
                }
                "submit_command_batch" => {
                    submitted = true;
                    json!({"protocol":crate::bridge_protocol::MUTATION_PROTOCOL,"submitted_changes":1})
                }
                method => panic!("unexpected transport call {method}"),
            };
            stream
                .write_all(format!("{}\n", json!({"id":request["id"],"result":result})).as_bytes())
                .await
                .unwrap();
        }
    });
    let root = temporary();
    let mut service = DustRouteMcp::with_test_transport_and_profile(
        address,
        McpPolicy {
            read_only: false,
            preview_required: false,
            ..Default::default()
        },
        ToolProfile::Debug,
    );
    service.state_store = PlanStateStore::new(root.clone(), 3600);
    let mut proposed = dustroute_translate::world::World::new();
    proposed.place(BlockKind::Solid, Pos::new(0, 1, 0));
    let plan = plan_world_overlay(
        &dustroute_translate::world::World::new(),
        &dustroute_translate::world::ValidatedWorld::try_from(proposed).unwrap(),
        Pos::new(0, 0, 0),
        10,
    )
    .unwrap();
    let id = plan.operation_id;
    service
        .plans
        .placements()
        .lock()
        .await
        .insert(plan, "minecraft:overworld".into(), None);
    let (client, server) = serve(service).await;
    let invoke = call(
        &client,
        "invoke_operation",
        json!({"operation_id":id,"confirm":true}),
    );
    let observe = async {
        timeout(Duration::from_secs(3), entered_rx)
            .await
            .unwrap()
            .unwrap();
        let running = call(&client, "get_operation", json!({"operation_id":id})).await;
        assert_eq!(running["ok"], true);
        assert_eq!(running["activity"]["active"], true);
        assert_eq!(running["activity"]["phase"], "scan");
        assert_eq!(
            running["activity"]["execution_progress"]["phase"],
            "after_readback"
        );
        assert_eq!(
            running["activity"]["execution_progress"]["submitted_changes"],
            1
        );
        assert_eq!(
            running["activity"]["execution_progress"]["verified_steps"],
            0
        );
        assert_eq!(
            call(&client, "stop_operation", json!({"operation_id":id})).await["ok"],
            false
        );
        release_tx.send(()).unwrap();
    };
    let (applied, ()) = timeout(Duration::from_secs(5), async {
        tokio::join!(invoke, observe)
    })
    .await
    .unwrap();
    assert_eq!(applied["ok"], true, "{applied}");
    let finished = call(&client, "get_operation", json!({"operation_id":id})).await;
    assert_eq!(finished["activity"]["active"], false);
    assert_eq!(
        finished["activity"]["execution_progress"]["verified_steps"],
        1
    );
    assert_eq!(finished["operation"]["status"], "completed");
    stop(client, server).await;
    transport.abort();
    if root.exists() {
        std::fs::remove_dir_all(root).unwrap();
    }
}

#[tokio::test]
async fn visible_player_reacquisition_and_refresh_failures_keep_their_cause() {
    for refresh_failure in [false, true] {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap().to_string();
        let transport = tokio::spawn(async move {
            for step in 0..if refresh_failure { 3 } else { 2 } {
                let (mut stream, _) = listener.accept().await.unwrap();
                let mut line = String::new();
                BufReader::new(&mut stream)
                    .read_line(&mut line)
                    .await
                    .unwrap();
                let request: Value = serde_json::from_str(&line).unwrap();
                let response = match step {
                    0 => {
                        assert_eq!(request["method"], "visible_players");
                        json!({"id":request["id"],"result":[]})
                    }
                    1 if refresh_failure => {
                        assert_eq!(request["method"], "observe_player");
                        json!({"id":request["id"],"result":{"player":"Tester","eye_position":{"x":0.0,"y":80.0,"z":0.0},"yaw":0.0,"pitch":0.0,"dimension":"minecraft:overworld","targeted_block":null}})
                    }
                    _ => json!({"id":request["id"],"error":"player observation transport failed"}),
                };
                stream
                    .write_all(format!("{response}\n").as_bytes())
                    .await
                    .unwrap();
            }
        });
        let root = temporary();
        let mut service = DustRouteMcp::with_test_transport_and_profile(
            address,
            McpPolicy::default(),
            ToolProfile::Debug,
        );
        service.assist_player = Some("Tester".into());
        service.state_store = PlanStateStore::new(root.clone(), 3600);
        let (client, server) = serve(service).await;
        let failed = call(&client, "get_visible_player", json!({})).await;
        assert_eq!(
            failed["ok"], false,
            "refresh failure must not become success"
        );
        assert_eq!(failed["failure"]["primary"]["kind"], "protocol");
        assert_eq!(failed["failure"]["progress"], Value::Null);
        assert!(
            failed["reacquire_error"]
                .as_str()
                .unwrap()
                .contains("transport failed")
        );
        assert_eq!(failed["error"], failed["reacquire_error"]);
        transport.await.unwrap();
        stop(client, server).await;
        if root.exists() {
            std::fs::remove_dir_all(root).unwrap();
        }
    }
}

#[tokio::test]
async fn selected_region_analysis_completes_native_flat_and_hierarchical_reports() {
    for count in [1, 513] {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap().to_string();
        let transport = tokio::spawn(async move {
            let (mut stream, _) = listener.accept().await.unwrap();
            let mut line = String::new();
            BufReader::new(&mut stream)
                .read_line(&mut line)
                .await
                .unwrap();
            let request: Value = serde_json::from_str(&line).unwrap();
            assert_eq!(request["method"], "scan_region");
            let snapshot = crate::bridge::test_readback_response(
                &request,
                json!({
                    "min":{"x":0,"y":80,"z":0},"max":{"x":count-1,"y":80,"z":0},
                    "blocks":(0..count).map(|x|json!({"pos":{"x":x,"y":80,"z":0},"name":"minecraft:redstone_block","properties":{}})).collect::<Vec<_>>(),
                }),
            );
            let response = json!({"id":request["id"],"result":snapshot});
            stream
                .write_all(format!("{response}\n").as_bytes())
                .await
                .unwrap();
        });
        let mut service =
            DustRouteMcp::with_test_transport_and_player(address, McpPolicy::default(), "Tester");
        service.tool_router = DustRouteMcp::tool_router();
        service.selections.lock().await.insert(
            "Tester".into(),
            LocatedSelection::with_bounds(
                "Tester",
                dustroute_translate::world_reverse::RegionBounds::new(
                    Pos::new(0, 80, 0),
                    Pos::new(count - 1, 80, 0),
                ),
                "minecraft:overworld".into(),
            ),
        );
        let registry = service.operations.clone();
        let (client, server) = serve(service).await;
        let started = call(
            &client,
            "start_selected_region_conversion",
            json!({"include_truth_table":count==1}),
        )
        .await;
        assert_eq!(started["ok"], true, "{started}");
        assert_eq!(started["status"], "queued");
        let id = uuid::Uuid::parse_str(started["operation_id"].as_str().unwrap()).unwrap();
        timeout(Duration::from_secs(20), async {
            loop {
                let record = registry.get(id).await.unwrap();
                if record.completed_at_unix_ms.is_some()
                    && registry.activity(id).await.is_some_and(|a| !a.active)
                {
                    break;
                }
                tokio::task::yield_now().await;
            }
        })
        .await
        .unwrap();
        transport.await.unwrap();
        let record = registry.get(id).await.unwrap();
        assert_eq!(record.status, OperationStatus::Completed, "{record:?}");
        let result = record.result.unwrap();
        assert!(matches!(
            result,
            crate::operations::OperationResult::Analysis(_)
        ));
        assert!(result.progress().is_none());
        assert!(!result.consumed());
        let read = call(&client, "get_operation", json!({"operation_id":id})).await;
        assert_eq!(read["ok"], true);
        assert_eq!(read["operation"]["progress_percent"], 100);
        assert!(read["activity"]["execution_progress"].is_null());
        let public = &read["operation"]["result"];
        assert_eq!(public["ok"], true);
        assert!(public["truth_table"].is_null());
        if count == 1 {
            assert_eq!(public["truth_table_status"], "unavailable");
            assert!(public["truth_table_error_details"].is_object());
        } else {
            assert_eq!(public["analysis_mode"], "hierarchical_local_first");
            assert_eq!(public["truth_table_status"], "skipped_large_circuit");
            assert_eq!(public["expansion"]["components_loaded"], 513);
            assert!(public["expansion"]["component_limit"].is_null());
            assert!(public["focused_component"].is_null());
        }
        stop(client, server).await;
    }
}

#[tokio::test]
async fn analysis_admission_refuses_before_creating_work() {
    // Check the public refusal contract without rebuilding the retired adapter
    // or calling the same validation helpers to manufacture an expected reply.
    for (case, code, kind, phase) in [
        (
            "player_override",
            "permission_denied",
            "permission_denied",
            None,
        ),
        (
            "player_denied",
            "permission_denied",
            "permission_denied",
            Some("admission"),
        ),
        ("missing_selection", "invalid_state", "invalid_state", None),
        ("scan_limit", "resource_limit", "resource_limit", None),
        (
            "invalid_budget",
            "invalid_argument",
            "invalid_input",
            Some("unknown"),
        ),
    ] {
        let root = temporary();
        let mut service = DustRouteMcp::with_test_transport_and_player(
            "127.0.0.1:1",
            McpPolicy::default(),
            "Tester",
        );
        service.state_store = PlanStateStore::new(root.clone(), 3600);
        let mut args = json!({});
        match case {
            "player_override" => args["player"] = json!("Other"),
            "player_denied" => {
                service.policy.allowed_players = ["AnotherPlayer".into()].into();
            }
            "missing_selection" => {}
            "scan_limit" | "invalid_budget" => {
                let max = if case == "scan_limit" { 4096 } else { 0 };
                let bounds = dustroute_translate::world_reverse::RegionBounds::new(
                    Pos::new(0, 80, 0),
                    Pos::new(max, 80, max),
                );
                service.selections.lock().await.insert(
                    "Tester".into(),
                    LocatedSelection::with_bounds("Tester", bounds, "minecraft:overworld".into()),
                );
                if case == "invalid_budget" {
                    args = json!({"include_truth_table":true,"truth_table_max_inputs":0});
                }
            }
            _ => unreachable!(),
        }
        let reply = service
            .start_selected_region_conversion(Parameters(serde_json::from_value(args).unwrap()))
            .await;
        assert_eq!(reply.is_error, Some(true), "{case}");
        let response = test_support::decode_reply(&reply).unwrap();
        assert_eq!(response["error_code"], code, "{case}");
        assert_eq!(response["failure"]["primary"]["kind"], kind, "{case}");
        assert_eq!(
            response["failure"]["primary"]["phase"],
            json!(phase),
            "{case}"
        );
        assert_eq!(response["failure"]["progress"], Value::Null, "{case}");
        assert!(service.operations.list().await.is_empty(), "{case}");
        assert!(!root.exists(), "{case}");
    }
}
