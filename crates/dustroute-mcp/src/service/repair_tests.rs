//! Repair intent, apply/undo and saved-lifecycle recovery contracts.
use super::test_support::broken_wire_snapshot;
use super::*;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::net::TcpListener;

#[tokio::test]
async fn removal_candidate_keeps_intent_uncertain_and_saved_preview_separate() {
    let root = test_support::temporary();
    let (fake, address, bridge) = test_support::start_construction_bridge(
        test_support::DurableRegistry::Edits(root.join("unused-edit-record")),
    )
    .await;
    {
        let mut state = fake.lock().unwrap();
        state.snapshot = Some(broken_wire_snapshot(false));
        state.gaze_target = Some(json!({"x":2,"y":1,"z":0}));
    }
    let mut service =
        DustRouteMcp::with_test_transport_and_player(address, McpPolicy::default(), "Tester");
    service.state_store = PlanStateStore::new(root.clone(), 3600);
    let bounds =
        dustroute_translate::world_reverse::RegionBounds::new(Pos::new(0, 0, 0), Pos::new(2, 2, 0));
    service.selections.lock().await.insert(
        "Tester".into(),
        LocatedSelection::with_bounds("Tester", bounds, "minecraft:overworld".into()),
    );
    let proposed = test_support::decode_reply(
        &service
            .new_component_removal_plan(Parameters(PlayerParams { player: None }))
            .await,
    )
    .unwrap();
    assert_eq!(proposed["ok"], true, "{proposed}");
    assert_eq!(proposed["schema_version"], crate::api::REPAIR_SCHEMA_V1);
    assert_eq!(
        proposed["proposal"]["patch"]["reason"],
        "remove_unexpected_connection"
    );
    assert_eq!(proposed["proposal"]["patch"]["confidence_percent"], 40);
    assert!(
        proposed["warning"]
            .as_str()
            .unwrap()
            .contains("cannot be inferred")
    );
    assert!(proposed.get("execution_progress").is_none());
    let id = uuid::Uuid::parse_str(proposed["operation_id"].as_str().unwrap()).unwrap();
    let draft = service.repair_plan(id).await.unwrap().unwrap();
    assert_eq!(draft.lifecycle, RepairLifecycle::Draft);
    assert!(draft.baseline_truth_table.is_none());
    assert_eq!(draft.patch.changes[0].pos, Pos::new(2, 1, 0));
    assert_eq!(
        draft.patch.changes[0].after.kind,
        dustroute_physical::BlockKind::Air
    );
    // This owner historically stores a draft, without adding execution history.
    assert!(service.operations.get(id).await.is_none());
    let shown = test_support::decode_reply(
        &service
            .show_repair_plan(Parameters(PreviewRepairParams {
                player: None,
                operation_id: id.to_string(),
            }))
            .await,
    )
    .unwrap();
    assert_eq!(shown["ok"], true, "{shown}");
    assert_eq!(shown["patch"], proposed["proposal"]["patch"]);
    assert_eq!(shown["preview"]["particle_corners"], 8);
    assert_eq!(
        service.repair_plan(id).await.unwrap().unwrap().lifecycle,
        RepairLifecycle::Previewed
    );
    assert!(service.operations.activity(id).await.is_none());
    assert_eq!(fake.lock().unwrap().writes, 0);
    bridge.abort();
    std::fs::remove_dir_all(root).unwrap();
}

#[tokio::test]
async fn repairs_and_undoes_a_broken_wire_through_the_mcp_workflow() {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap().to_string();
    let server = tokio::spawn(async move {
        let mut repaired = false;
        for _ in 0..10 {
            let (mut stream, _) = listener.accept().await.unwrap();
            let mut request = String::new();
            BufReader::new(&mut stream)
                .read_line(&mut request)
                .await
                .unwrap();
            let request: Value = serde_json::from_str(&request).unwrap();
            let result = match request["method"].as_str().unwrap() {
                "scan_region" => broken_wire_snapshot(repaired),
                "preview_region" => json!({ "particle_corners": 8 }),
                "submit_physical_batch" => {
                    repaired = request["params"]["changes"][0]["action"] == "place";
                    json!({
                        "protocol": crate::bridge_protocol::MUTATION_PROTOCOL, "placed_changes": 1,
                        "placement_mode": "test_player",
                        "retreat": { "x": 2.5, "y": 18.0, "z": 0.5 }
                    })
                }
                method => panic!("unexpected fake bridge method {method}"),
            };
            let response = json!({ "id": request["id"], "result": crate::bridge::test_readback_response(&request,crate::bridge::test_scan_world(&request,result)) });
            stream
                .write_all(format!("{response}\n").as_bytes())
                .await
                .unwrap();
        }
    });

    let policy = McpPolicy {
        read_only: false,
        ..McpPolicy::default()
    };
    let root = std::env::temp_dir().join(format!(
        "dustroute-repair-workflow-{}",
        uuid::Uuid::new_v4()
    ));
    let make_service = || {
        let mut service = DustRouteMcp::with_test_transport_and_player(
            address.clone(),
            policy.clone(),
            "builder",
        );
        service.state_store = PlanStateStore::new(root.clone(), 3600);
        service
    };
    let service = make_service();
    let bounds =
        dustroute_translate::world_reverse::RegionBounds::new(Pos::new(0, 0, 0), Pos::new(2, 2, 0));
    service.selections.lock().await.insert(
        "builder".into(),
        LocatedSelection::with_bounds("builder", bounds, "minecraft:overworld".into()),
    );

    let shown: Value = test_support::decode_reply(
        &service
            .show_region(Parameters(PlayerParams { player: None }))
            .await,
    )
    .unwrap();
    let circuit_id = shown["circuit_id"].as_str().unwrap().to_owned();
    let proposed: Value = test_support::decode_reply(
        &service
            .new_repair(Parameters(ProposeRepairsParams {
                player: None,
                max_gap: Some(2),
                circuit_id: circuit_id.clone(),
            }))
            .await,
    )
    .unwrap();
    assert_eq!(proposed["proposal_count"], 3);
    let shared: dustroute_translate::diagnostic::report::Diagnosis =
        serde_json::from_value(proposed["diagnostic"].clone()).unwrap();
    assert_eq!(
        shared.method,
        dustroute_translate::diagnostic::report::DiagnosticMethod::Connectivity
    );
    assert_eq!(
        shared.repair.status,
        dustroute_translate::diagnostic::report::RepairStatus::PlanAvailable
    );
    assert_eq!(
        shared.repair.strategy,
        dustroute_translate::diagnostic::report::RepairStrategy::PartialPatch
    );
    assert!(!shared.repair.permission_granted);
    let context: Value = test_support::decode_reply(
        &service
            .get_repair_context(Parameters(GetRepairContextParams {
                player: None,
                circuit_id,
                operation_id: None,
                max_gap: Some(2),
            }))
            .await,
    )
    .unwrap();
    let hypothesis = context["hypotheses"]
        .as_array()
        .unwrap()
        .iter()
        .find(|h| h["kind"] == proposed["proposals"][0]["patch"]["reason"])
        .unwrap();
    assert_eq!(
        hypothesis["physical_evidence"],
        proposed["proposals"][0]["evidence"]
    );
    assert_eq!(
        hypothesis["counterfactual_impact"],
        proposed["proposals"][0]["impact"]
    );
    assert_eq!(
        hypothesis["diagnostic_finding_ids"],
        proposed["proposals"][0]["diagnostic_finding_ids"]
    );
    assert!(hypothesis.as_object().unwrap().contains_key("operation_id"));
    assert!(hypothesis["operation_id"].is_null());
    assert_eq!(context["facts"]["gap_candidates_truncated"], false);
    assert!(!context["related_components"].as_array().unwrap().is_empty());
    let context_diagnosis: dustroute_translate::diagnostic::report::Diagnosis =
        serde_json::from_value(context["facts"]["diagnostic"].clone()).unwrap();
    assert_eq!(context_diagnosis, shared);
    let finding_ids: std::collections::BTreeSet<_> = shared
        .findings
        .iter()
        .map(|f| f.finding_id.as_str())
        .collect();
    for candidate in proposed["proposals"].as_array().unwrap() {
        for id in candidate["diagnostic_finding_ids"].as_array().unwrap() {
            assert!(finding_ids.contains(id.as_str().unwrap()));
        }
    }

    let operation_id = proposed["proposals"][0]["operation_id"]
        .as_str()
        .unwrap()
        .to_owned();

    for candidate in proposed["proposals"].as_array().unwrap() {
        let id = uuid::Uuid::parse_str(candidate["operation_id"].as_str().unwrap()).unwrap();
        let record = service.operations.get(id).await.unwrap();
        let result = record.result.unwrap();
        assert!(matches!(
            &result,
            crate::operations::OperationResult::RepairCandidate(_)
        ));
        assert!(!result.failed());
        assert!(result.progress().is_none());
        assert!(!result.consumed());
        let wire = serde_json::to_value(result).unwrap();
        assert_eq!(wire["patch"], candidate["patch"]);
        assert_eq!(wire["evidence"], candidate["evidence"]);
        assert_eq!(wire["impact"], candidate["impact"]);
        assert!(wire.get("ok").is_none());
        assert!(wire.get("operation_id").is_none());
        assert!(wire.get("diagnostic_finding_ids").is_none());
        assert!(service.operations.activity(id).await.is_none());
    }
    let selected_context = test_support::decode_reply(
        &service
            .get_repair_context(Parameters(GetRepairContextParams {
                player: None,
                circuit_id: proposed["circuit_id"].as_str().unwrap().into(),
                operation_id: Some(operation_id.clone()),
                max_gap: Some(2),
            }))
            .await,
    )
    .unwrap();
    assert_eq!(selected_context["operation_id"], operation_id);
    let selected = selected_context["hypotheses"]
        .as_array()
        .unwrap()
        .iter()
        .find(|h| h.get("operation_id").is_some())
        .unwrap();
    assert_eq!(selected["operation_id"], operation_id);
    assert_eq!(
        selected["physical_evidence"],
        proposed["proposals"][0]["evidence"]
    );
    let preview = service
        .show_operation(Parameters(ShowOperationParams {
            operation_id: operation_id.clone(),
            player: None,
        }))
        .await;
    assert!(test_support::reply_text(&preview).contains("\"ok\": true"));
    // Preview and applied status must survive independent service instances.
    let restarted = make_service();
    let applied = restarted
        .invoke_operation(Parameters(InvokeOperationParams {
            blueprint_decision: None,
            operation_id: operation_id.clone(),
            confirm: true,
            contracts: None,
        }))
        .await;
    assert!(
        test_support::reply_text(&applied).contains("\"verified\": true"),
        "{applied:?}"
    );
    let applied_value: Value = test_support::decode_reply(&applied).unwrap();
    assert!(applied_value["resulting_logic"].is_object());
    assert_eq!(applied_value["semantic_verification"]["available"], false);
    let id = uuid::Uuid::parse_str(&operation_id).unwrap();
    assert_eq!(
        service.repair_plan(id).await.unwrap().unwrap().lifecycle,
        RepairLifecycle::Applied
    );
    let restarted = make_service();
    let undone = restarted
        .undo_operation(Parameters(ConfirmedOperationParams {
            operation_id,
            confirm: true,
        }))
        .await;
    assert!(
        test_support::reply_text(&undone).contains("\"verified\": true"),
        "{undone:?}"
    );
    assert_eq!(
        service.repair_plan(id).await.unwrap().unwrap().lifecycle,
        RepairLifecycle::Undone
    );
    server.await.unwrap();
    std::fs::remove_dir_all(root).unwrap();
}

#[tokio::test]
async fn repair_operations_reject_unavailable_saved_state_before_and_after_restart() {
    for invalidation in ["expired", "deleted", "corrupt"] {
        for applied in [false, true] {
            let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
            let address = listener.local_addr().unwrap().to_string();
            let root = std::env::temp_dir().join(format!(
                "dustroute-repair-retention-{}",
                uuid::Uuid::new_v4()
            ));
            let make_service = || {
                let mut service = DustRouteMcp::with_test_transport_and_player(
                    address.clone(),
                    McpPolicy {
                        read_only: false,
                        ..McpPolicy::default()
                    },
                    "builder",
                );
                service.state_store = PlanStateStore::new(root.clone(), 3600);
                service
            };
            let service = make_service();
            let id = uuid::Uuid::new_v4();
            let pos = Pos::new(1, 1, 0);
            service
                .store_repair_plan(
                    id,
                    StoredRepairPlan {
                        patch: PhysicalPatch {
                            reason: dustroute_physical::PhysicalPatchReason::ConnectMissingWire,
                            affected_fragments: vec![],
                            confidence_percent: 100,
                            explanation: "restore missing wire".into(),
                            changes: vec![PhysicalBlockChange {
                                pos,
                                before: dustroute_physical::Block::new(BlockKind::Air),
                                after: dustroute_physical::Block::new(BlockKind::RedstoneWire),
                            }],
                        },
                        dimension: "minecraft:overworld".into(),
                        analysis_bounds: dustroute_translate::world_reverse::RegionBounds::new(
                            Pos::new(0, 0, 0),
                            Pos::new(2, 2, 0),
                        ),
                        fragments_before: 2,
                        baseline_truth_table: None,
                        lifecycle: if applied {
                            RepairLifecycle::Applied
                        } else {
                            RepairLifecycle::Previewed
                        },
                        contract_satisfied: true,
                        preserved_boundary: vec![],
                    },
                )
                .await
                .unwrap();
            // Exercise the formerly cached read before invalidating its disk record.
            assert!(service.repair_plan(id).await.unwrap().is_some());
            let path = root.join("repairs").join(format!("{id}.store"));
            match invalidation {
                "expired" => {
                    let mut envelope: crate::state::PlanEnvelope<StoredRepairPlan> =
                        dustroute_codec::storage::decode(
                            PlanRecordKind::Repairs.schema(),
                            &std::fs::read(&path).unwrap(),
                            16 * 1024 * 1024,
                        )
                        .unwrap();
                    envelope.saved_at_unix_seconds = 1;
                    std::fs::write(
                        &path,
                        dustroute_codec::storage::encode(
                            PlanRecordKind::Repairs.schema(),
                            &envelope,
                            16 * 1024 * 1024,
                        )
                        .unwrap(),
                    )
                    .unwrap();
                }
                "deleted" => std::fs::remove_file(&path).unwrap(),
                "corrupt" => std::fs::write(&path, b"not json").unwrap(),
                _ => unreachable!(),
            }
            let restarted = make_service();
            let expected_code = if invalidation == "corrupt" {
                "internal"
            } else {
                "not_found"
            };
            let check = async {
                for instance in [&service, &restarted] {
                    for action in ["show", "invoke", "undo"] {
                        let response = match action {
                            "show" => {
                                instance
                                    .show_operation(Parameters(ShowOperationParams {
                                        operation_id: id.to_string(),
                                        player: None,
                                    }))
                                    .await
                            }
                            "invoke" => {
                                instance
                                    .invoke_operation(Parameters(InvokeOperationParams {
                                        operation_id: id.to_string(),
                                        confirm: true,
                                        blueprint_decision: None,
                                        contracts: None,
                                    }))
                                    .await
                            }
                            "undo" => {
                                instance
                                    .undo_operation(Parameters(ConfirmedOperationParams {
                                        operation_id: id.to_string(),
                                        confirm: true,
                                    }))
                                    .await
                            }
                            _ => unreachable!(),
                        };
                        let value: Value = test_support::decode_reply(&response).unwrap();
                        assert_eq!(value["ok"], false, "{invalidation}/{action}: {value}");
                        assert_eq!(
                            value["error_code"], expected_code,
                            "{invalidation}/{action}: {value}"
                        );
                    }
                }
            };
            tokio::select! {
                () = check => {},
                connection = listener.accept() => panic!(
                    "unavailable repair state must reject before any bridge call: {connection:?}"
                ),
            }
            // Also reject a connection queued while a handler returned without awaiting it.
            assert!(
                tokio::time::timeout(Duration::from_millis(10), listener.accept())
                    .await
                    .is_err()
            );
            std::fs::remove_dir_all(root).unwrap();
        }
    }
}
