//! Piston planning and fixed-door lifecycle contracts over offline transport.
use super::*;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::net::TcpListener;

#[tokio::test]
async fn piston_candidates_keep_admission_denial_before_transport_or_retention() {
    let service = DustRouteMcp::with_test_transport_and_player(
        "127.0.0.1:1",
        McpPolicy {
            allowed_players: BTreeSet::from(["someone_else".into()]),
            ..Default::default()
        },
        "builder",
    );
    let id = uuid::Uuid::new_v4();
    let replies = [
        service
            .plan_piston_placement(PreviewPlacementParams {
                circuit: "piston-door-1x2".into(),
                ..Default::default()
            })
            .await,
        service.show_piston_placement(id, None).await,
        service
            .new_piston_door_operation(Parameters(NewPistonDoorParams {
                circuit_id: id.to_string(),
                target: crate::piston_door::DoorState::Closed,
            }))
            .await,
        service.show_piston_door(id, None).await,
    ];
    for reply in replies {
        assert_eq!(reply.is_error, Some(true));
        let result = test_support::decode_reply(&reply).unwrap();
        assert_eq!(result["error_code"], "permission_denied");
        assert_eq!(result["failure"]["primary"]["phase"], "admission");
        assert!(result["failure"]["progress"].is_null());
        assert!(!result["error"].as_str().unwrap().starts_with('{'));
    }
    assert!(service.operations.list().await.is_empty());
    assert!(
        service
            .plans
            .table::<StoredPistonPlacement>()
            .lock()
            .await
            .is_empty()
    );
    assert!(
        service
            .plans
            .table::<StoredDoorPlan>()
            .lock()
            .await
            .is_empty()
    );
}

#[tokio::test]
async fn incomplete_door_candidate_returns_observation_without_retaining_a_plan() {
    let root = test_support::temporary();
    let (fake, address, bridge) = test_support::start_construction_bridge(
        test_support::DurableRegistry::Edits(root.join("unused-record")),
    )
    .await;
    let mut service =
        DustRouteMcp::with_test_transport_and_player(address, McpPolicy::default(), "Tester");
    service.state_store = PlanStateStore::new(root.clone(), 3600);
    let snapshot = crate::piston_door::sample(crate::piston_door::DoorState::Open);
    let bounds = dustroute_translate::world_reverse::RegionBounds::new(snapshot.min, snapshot.max);
    let id = service
        .store_circuit(StoredCircuit {
            player: "Tester".into(),
            dimension: "minecraft:overworld".into(),
            bounds,
            target: None,
            snapshot: service.bridge.share_snapshot(snapshot).unwrap(),
            expansion: ExpansionEvidence::Unspecified {},
            complete: false,
            expires_at: Instant::now() + Duration::from_secs(300),
        })
        .await;
    let reply = service
        .new_piston_door_operation(Parameters(NewPistonDoorParams {
            circuit_id: id.to_string(),
            target: crate::piston_door::DoorState::Closed,
        }))
        .await;
    assert_eq!(reply.is_error, Some(true));
    let result = test_support::decode_reply(&reply).unwrap();
    assert_eq!(result["observation"]["state"], "observation_incomplete");
    assert!(result.get("operation_id").is_none());
    assert_eq!(result["schema_version"], crate::api::ERROR_SCHEMA_V1);
    assert_eq!(result["failure"]["primary"]["phase"], "unknown");
    assert!(result["failure"]["progress"].is_null());
    assert!(
        service
            .plans
            .table::<StoredDoorPlan>()
            .lock()
            .await
            .is_empty()
    );
    assert!(service.operations.list().await.is_empty());
    assert_eq!(fake.lock().unwrap().writes, 0);
    assert!(!root.exists());
    bridge.abort();
}

#[tokio::test]
async fn piston_mutations_keep_player_denial_typed_before_any_transport() {
    let service = DustRouteMcp::with_test_transport_and_player(
        "127.0.0.1:1",
        McpPolicy {
            read_only: false,
            allowed_players: BTreeSet::from(["someone_else".into()]),
            ..Default::default()
        },
        "builder",
    );
    for door in [false, true] {
        let id = uuid::Uuid::new_v4();
        let reply = if door {
            service.invoke_piston_door(id, true).await
        } else {
            service.mutate_piston_placement(id, true, false).await
        };
        assert_eq!(reply.is_error, Some(true));
        let result = test_support::decode_reply(&reply).unwrap();
        assert_eq!(result["failure"]["primary"]["kind"], "permission_denied");
        assert_eq!(result["failure"]["primary"]["phase"], "admission");
        assert_eq!(result["failure"]["progress"]["operation_consumed"], false);
        assert_eq!(result["failure"]["progress"]["world"], "not_attempted");
        assert!(!result["error"].as_str().unwrap().starts_with('{'));
        assert!(service.operations.get(id).await.is_none());
    }
}

#[tokio::test]
async fn piston_placement_uses_common_tools_and_fails_closed() {
    use crate::piston_door::{DoorState, sample};
    use std::sync::atomic::{AtomicUsize, Ordering};
    for mode in [
        "ok",
        "stale",
        "uncertain",
        "post_mismatch",
        "unpreviewed",
        "read_only",
        "expired",
        "unconfirmed",
    ] {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap().to_string();
        let writes = Arc::new(AtomicUsize::new(0));
        let count = writes.clone();
        let server = tokio::spawn(async move {
            let mut scans = 0;
            loop {
                let (mut stream, _) = listener.accept().await.unwrap();
                let mut line = String::new();
                BufReader::new(&mut stream)
                    .read_line(&mut line)
                    .await
                    .unwrap();
                let req: Value = serde_json::from_str(&line).unwrap();
                let mut error = None;
                let result = match req["method"].as_str().unwrap() {
                    "status" => {
                        json!({"connected":true,"username":"DustRouteBot","host":"localhost","port":25565,"version":"1.21.11","dimension":"minecraft:overworld"})
                    }
                    "observe_player" => {
                        json!({"player":"builder","eye_position":{"x":0.0,"y":0.0,"z":0.0},"yaw":0.0,"pitch":0.0,"targeted_block":{"x":0,"y":-3,"z":0},"targeted_face":"up","distance":3.0,"dimension":"minecraft:overworld"})
                    }
                    "preview_region" | "wait_ticks" => json!({}),
                    "scan_region" => {
                        let mut snapshot = sample(DoorState::Open);
                        if count.load(Ordering::SeqCst) != 1 || mode == "post_mismatch" {
                            snapshot.blocks.clear();
                        }
                        if mode == "stale" && scans > 0 {
                            snapshot
                                .blocks
                                .push(sample(DoorState::Open).blocks[0].clone());
                        }
                        scans += 1;
                        serde_json::to_value(snapshot).unwrap()
                    }
                    "submit_command_batch" => {
                        count.fetch_add(1, Ordering::SeqCst);
                        if mode == "uncertain" {
                            error = Some("write response lost");
                        }
                        json!({"protocol":crate::bridge_protocol::MUTATION_PROTOCOL,"submitted_changes":req["params"]["changes"].as_array().unwrap().len()})
                    }
                    other => panic!("unexpected request {other}"),
                };
                let reply = if let Some(error) = error {
                    json!({"id":req["id"],"error":error})
                } else {
                    json!({"id":req["id"],"result":crate::bridge::test_readback_response(&req,result)})
                };
                stream
                    .write_all(format!("{reply}\n").as_bytes())
                    .await
                    .unwrap();
            }
        });
        let service = DustRouteMcp::with_test_transport_and_player(
            address,
            McpPolicy {
                read_only: mode == "read_only",
                ..McpPolicy::default()
            },
            "builder",
        );
        let proposal: Value = test_support::decode_reply(
            &service
                .new_placement(Parameters(PreviewPlacementParams {
                    player: None,
                    circuit: "piston-door-1x2".into(),
                    revision_id: None,
                    assembly_revision_id: None,
                    assembly_target: None,
                    edit_scope: None,
                    work_regions: None,
                    max_blocks: None,
                    optimize: None,
                }))
                .await,
        )
        .unwrap();
        assert_eq!(proposal["ok"], true, "{mode}: {proposal}");
        let candidate_id =
            uuid::Uuid::parse_str(proposal["operation_id"].as_str().unwrap()).unwrap();
        let candidate = service
            .operations
            .get(candidate_id)
            .await
            .unwrap()
            .result
            .unwrap();
        assert!(matches!(
            &candidate,
            crate::operations::OperationResult::PistonPlacementPreview(_)
        ));
        assert!(candidate.progress().is_none());
        assert!(!candidate.consumed());
        assert_eq!(
            test_support::decode_reply(&typed_reply(candidate)).unwrap(),
            proposal
        );
        assert!(service.operations.activity(candidate_id).await.is_none());
        let id = proposal["operation_id"].as_str().unwrap().to_owned();
        let detail: Value = test_support::decode_reply(
            &service
                .get_circuit_placement(Parameters(OperationParams {
                    operation_id: id.clone(),
                }))
                .await,
        )
        .unwrap();
        assert_eq!(detail["ok"], true);
        assert_eq!(detail["plan"]["changes"], proposal["changes"]);
        assert_eq!(detail["plan"]["undo_changes"], proposal["undo_changes"]);
        assert_eq!(detail["plan"]["state"], "Planned");
        assert_eq!(detail["plan"]["previewed"], false);
        assert!(detail.get("execution_progress").is_none());
        if mode != "unpreviewed" {
            let shown: Value = test_support::decode_reply(
                &service
                    .show_operation(Parameters(ShowOperationParams {
                        operation_id: id.clone(),
                        player: None,
                    }))
                    .await,
            )
            .unwrap();
            assert_eq!(shown["ok"], true);
        }
        if mode == "expired" {
            service
                .plans
                .table::<StoredPistonPlacement>()
                .lock()
                .await
                .get_mut(&uuid::Uuid::parse_str(&id).unwrap())
                .unwrap()
                .expires_at = Instant::now();
        }
        let result: Value = test_support::decode_reply(
            &service
                .invoke_operation(Parameters(InvokeOperationParams {
                    blueprint_decision: None,
                    operation_id: id.clone(),
                    confirm: mode != "unconfirmed",
                    contracts: None,
                }))
                .await,
        )
        .unwrap();
        assert_eq!(result["ok"], mode == "ok", "{mode}: {result:?}");
        let expected = usize::from(matches!(mode, "ok" | "uncertain" | "post_mismatch"));
        assert_eq!(writes.load(Ordering::SeqCst), expected, "{mode}");
        if expected == 1 {
            let retry: Value = test_support::decode_reply(
                &service
                    .invoke_operation(Parameters(InvokeOperationParams {
                        blueprint_decision: None,
                        operation_id: id.clone(),
                        confirm: true,
                        contracts: None,
                    }))
                    .await,
            )
            .unwrap();
            assert_eq!(retry["ok"], false);
            assert_eq!(writes.load(Ordering::SeqCst), 1);
        }
        let undo: Value = test_support::decode_reply(
            &service
                .undo_operation(Parameters(ConfirmedOperationParams {
                    operation_id: id,
                    confirm: true,
                }))
                .await,
        )
        .unwrap();
        assert_eq!(undo["ok"], mode == "ok", "{mode}: {undo}");
        assert_eq!(
            writes.load(Ordering::SeqCst),
            if mode == "ok" { 2 } else { expected }
        );
        server.abort();
    }
}

#[tokio::test]
async fn piston_door_operations_revalidate_consume_and_verify() {
    use crate::piston_door::{DoorState, sample};
    use std::sync::atomic::{AtomicUsize, Ordering};
    for mode in [
        "ok",
        "changed",
        "post_mismatch",
        "uncertain",
        "noop",
        "expired",
        "unpreviewed",
        "unconfirmed",
        "read_only",
    ] {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap().to_string();
        let activations = Arc::new(AtomicUsize::new(0));
        let count = activations.clone();
        let server = tokio::spawn(async move {
            let mut scans = 0;
            loop {
                let (mut stream, _) = listener.accept().await.unwrap();
                let mut line = String::new();
                BufReader::new(&mut stream)
                    .read_line(&mut line)
                    .await
                    .unwrap();
                let req: Value = serde_json::from_str(&line).unwrap();
                let mut error = None;
                let result = match req["method"].as_str().unwrap() {
                    "status" => {
                        json!({"connected":true,"username":"DustRouteBot","host":"localhost","port":25565,"version":"1.21.11","dimension":"minecraft:overworld"})
                    }
                    "preview_region" => json!({}),
                    "approach_lever" => {
                        json!({"pos":{"x":2,"y":0,"z":4},"moved":false,"distance":2.0})
                    }
                    "scan_region" => {
                        let closed = scans > 0 && mode != "post_mismatch";
                        let mut s = sample(if closed {
                            DoorState::Closed
                        } else {
                            DoorState::Open
                        });
                        if mode == "changed" {
                            s.blocks.remove(0);
                        }
                        scans += 1;
                        serde_json::to_value(s).unwrap()
                    }
                    "activate_lever" => {
                        count.fetch_add(1, Ordering::SeqCst);
                        if mode == "uncertain" {
                            error = Some("activation reply lost");
                        }
                        json!({"pos":{"x":2,"y":0,"z":4},"before_powered":false,"after_powered":true})
                    }
                    "wait_ticks" => json!({}),
                    other => panic!("unexpected write or request: {other}"),
                };
                let reply = if let Some(error) = error {
                    json!({"id":req["id"],"error":error})
                } else {
                    json!({"id":req["id"],"result":crate::bridge::test_readback_response(&req,result)})
                };
                stream
                    .write_all(format!("{reply}\n").as_bytes())
                    .await
                    .unwrap();
            }
        });
        let service = DustRouteMcp::with_test_transport_and_player(
            address,
            McpPolicy {
                read_only: mode == "read_only",
                ..McpPolicy::default()
            },
            "builder",
        );
        let snapshot = sample(DoorState::Open);
        let bounds =
            dustroute_translate::world_reverse::RegionBounds::new(snapshot.min, snapshot.max);
        let circuit = service
            .store_circuit(StoredCircuit {
                player: "builder".into(),
                dimension: "minecraft:overworld".into(),
                bounds,
                target: None,
                snapshot: service.bridge.share_snapshot(snapshot).unwrap(),
                expansion: ExpansionEvidence::Unspecified {},
                complete: true,
                expires_at: Instant::now() + std::time::Duration::from_secs(300),
            })
            .await;
        let proposed: Value = test_support::decode_reply(
            &service
                .new_piston_door_operation(Parameters(NewPistonDoorParams {
                    circuit_id: circuit.to_string(),
                    target: if mode == "noop" {
                        DoorState::Open
                    } else {
                        DoorState::Closed
                    },
                }))
                .await,
        )
        .unwrap();
        assert_eq!(proposed["ok"], true, "{mode}: {proposed}");
        let candidate_id =
            uuid::Uuid::parse_str(proposed["operation_id"].as_str().unwrap()).unwrap();
        let candidate = service
            .operations
            .get(candidate_id)
            .await
            .unwrap()
            .result
            .unwrap();
        assert!(matches!(
            &candidate,
            crate::operations::OperationResult::DoorProposal(_)
        ));
        assert!(candidate.progress().is_none());
        assert!(!candidate.consumed());
        assert_eq!(
            test_support::decode_reply(&typed_reply(candidate)).unwrap(),
            proposed
        );
        assert!(service.operations.activity(candidate_id).await.is_none());
        let id = proposed["operation_id"].as_str().unwrap().to_owned();
        if mode != "unpreviewed" {
            let preview: Value = test_support::decode_reply(
                &service
                    .show_operation(Parameters(ShowOperationParams {
                        operation_id: id.clone(),
                        player: None,
                    }))
                    .await,
            )
            .unwrap();
            assert_eq!(preview["ok"], true);
        }
        if mode == "expired" {
            service
                .plans
                .table::<StoredDoorPlan>()
                .lock()
                .await
                .get_mut(&uuid::Uuid::parse_str(&id).unwrap())
                .unwrap()
                .expires_at = Instant::now();
        }
        let result: Value = test_support::decode_reply(
            &service
                .invoke_operation(Parameters(InvokeOperationParams {
                    blueprint_decision: None,
                    operation_id: id.clone(),
                    confirm: mode != "unconfirmed",
                    contracts: None,
                }))
                .await,
        )
        .unwrap();
        assert_eq!(
            result["ok"],
            mode == "ok" || mode == "noop",
            "{mode}: {result:?}"
        );
        let expected = usize::from(matches!(mode, "ok" | "post_mismatch" | "uncertain"));
        assert_eq!(activations.load(Ordering::SeqCst), expected, "{mode}");
        if matches!(mode, "ok" | "post_mismatch" | "uncertain" | "noop") {
            let retry: Value = test_support::decode_reply(
                &service
                    .invoke_operation(Parameters(InvokeOperationParams {
                        blueprint_decision: None,
                        operation_id: id,
                        confirm: true,
                        contracts: None,
                    }))
                    .await,
            )
            .unwrap();
            assert_eq!(retry["ok"], false);
            assert_eq!(activations.load(Ordering::SeqCst), expected);
        }
        if matches!(mode, "post_mismatch" | "uncertain") {
            assert_eq!(result["status"], "needs_inspection");
        }
        if matches!(mode, "ok" | "post_mismatch" | "uncertain") {
            assert_eq!(
                result["observation"]["state"],
                if mode == "post_mismatch" {
                    "open"
                } else {
                    "closed"
                }
            );
        }
        service.selections.lock().await.insert(
            "builder".into(),
            LocatedSelection::with_bounds("builder", bounds, "minecraft:overworld".into()),
        );
        let fresh: Value = test_support::decode_reply(
            &service
                .show_region(Parameters(PlayerParams { player: None }))
                .await,
        )
        .unwrap();
        assert_eq!(fresh["ok"], true, "{mode}: {fresh}");
        assert_eq!(fresh["source"], "fresh_scan");
        assert_ne!(fresh["circuit_id"], circuit.to_string());
        if mode == "ok" || mode == "uncertain" {
            assert_eq!(fresh["mechanisms"][0]["kind"], "piston_door");
            assert_eq!(fresh["mechanisms"][0]["state"], "closed");
        }
        if mode == "ok" {
            let converted: Value = test_support::decode_reply(&service.convert_from_circuit(Parameters(
                serde_json::from_value(json!({"circuit_id": fresh["circuit_id"], "include_truth_table": false})).unwrap()
            )).await).unwrap();
            assert_eq!(converted["ok"], true, "{converted}");
            assert_eq!(converted["mechanisms"], fresh["mechanisms"]);
            let original: Value = test_support::decode_reply(&service.convert_from_circuit(Parameters(
                serde_json::from_value(json!({"circuit_id": circuit.to_string(), "include_truth_table": false})).unwrap()
            )).await).unwrap();
            assert_eq!(original["mechanisms"][0]["state"], "open");
        }
        if mode == "changed" {
            assert_eq!(
                fresh["mechanisms"][0]["kind"],
                "unidentified_piston_mechanism"
            );
            assert!(fresh["mechanisms"][0]["state"].is_null());
        }
        assert_eq!(activations.load(Ordering::SeqCst), expected);
        server.abort();
    }
}
