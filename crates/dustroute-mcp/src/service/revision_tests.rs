//! Hypothetical revisions, adoption and cumulative live-placement contracts.
use super::*;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::net::TcpListener;

#[tokio::test]
async fn adopted_grounded_assembly_enters_existing_revision_placement_path() {
    let (records, proposal) = super::blueprint_tests::placement_fixture();
    let base_record: dustroute_library::assembly::AssemblyRevision =
        serde_json::from_value(records["blueprint"]["records"]["assemblies"][0].clone()).unwrap();
    let candidate_id: dustroute_library::blueprint::AssemblyRevisionId =
        serde_json::from_value(proposal["blueprint"]["request"]["candidate_state"]["id"].clone())
            .unwrap();
    let known = base_record.assembly.known_regions[0];
    let base = snapshot_from_grounded_assembly(&base_record, known.min, known.max).unwrap();
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap().to_string();
    let bridge_base = base.clone();
    let server = tokio::spawn(async move {
        loop {
            let (mut stream, _) = listener.accept().await.unwrap();
            let mut line = String::new();
            BufReader::new(&mut stream)
                .read_line(&mut line)
                .await
                .unwrap();
            let request: Value = serde_json::from_str(&line).unwrap();
            let result = match request["method"].as_str().unwrap() {
                "status" => {
                    json!({"connected":true,"username":"DustRouteBot","host":"localhost","port":25565,"version":"1.21.11","dimension":"minecraft:overworld"})
                }
                "scan_region" => {
                    let min: Pos =
                        serde_json::from_value(request["params"]["min"].clone()).unwrap();
                    let max: Pos =
                        serde_json::from_value(request["params"]["max"].clone()).unwrap();
                    let blocks = bridge_base
                        .blocks
                        .iter()
                        .filter(|block| {
                            let p = block.pos;
                            p.x >= min.x
                                && p.y >= min.y
                                && p.z >= min.z
                                && p.x <= max.x
                                && p.y <= max.y
                                && p.z <= max.z
                        })
                        .collect::<Vec<_>>();
                    json!({"min":min,"max":max,"blocks":blocks})
                }
                other => panic!("unexpected bridge request {other}"),
            };
            stream
                .write_all(
                    format!("{}\n", json!({"id":request["id"],"result":crate::bridge::test_readback_response(&request,result)})).as_bytes(),
                )
                .await
                .unwrap();
        }
    });
    let root = std::env::temp_dir().join(format!(
        "dustroute-adopted-placement-{}",
        uuid::Uuid::new_v4()
    ));
    let mut service =
        DustRouteMcp::with_test_transport_and_player(address, McpPolicy::default(), "builder");
    service.state_store = PlanStateStore::new(root.clone(), 3600);
    let imported: Value = test_support::decode_reply(
        &service
            .test_circuit_change(Parameters(serde_json::from_value(records).unwrap()))
            .await,
    )
    .unwrap();
    assert_eq!(imported["ok"], true, "{imported}");
    let proposed: Value = test_support::decode_reply(
        &service
            .test_circuit_change(Parameters(serde_json::from_value(proposal).unwrap()))
            .await,
    )
    .unwrap();
    assert_eq!(proposed["ok"], true, "{proposed}");
    let unadopted =
        crate::blueprint_mcp::grounded_source(&service.state_store, "builder", &candidate_id)
            .unwrap_err();
    assert!(
        unadopted.contains("exactly one adopted update"),
        "{unadopted:?}"
    );
    let adopted: Value = test_support::decode_reply(
        &service
            .invoke_operation(Parameters(InvokeOperationParams {
                blueprint_decision: Some(crate::blueprint_mcp::BlueprintDecision::Adopt),
                operation_id: proposed["operation_id"].as_str().unwrap().into(),
                confirm: true,
                contracts: None,
            }))
            .await,
    )
    .unwrap();
    assert_eq!(adopted["ok"], true, "{adopted}");
    let ungrounded =
        crate::blueprint_mcp::grounded_source(&service.state_store, "builder", &candidate_id)
            .unwrap_err();
    assert!(
        ungrounded.contains("captured Circuit Revision ancestor"),
        "{ungrounded:?}"
    );
    let grounding_revision = uuid::Uuid::new_v4();
    let grounding = crate::blueprint_mcp::AssemblyGrounding {
        assembly_revision_id: base_record.id.clone(),
        circuit_revision_id: grounding_revision,
        base_observation_id: uuid::Uuid::new_v4(),
        dimension: "minecraft:overworld".into(),
        complete: true,
        base_snapshot: base,
    };
    let mut incomplete = grounding.clone();
    incomplete.complete = false;
    let rejected = crate::blueprint_mcp::execute(
        &service.state_store,
        "builder",
        crate::blueprint_mcp::Command::Capture {
            record: Box::new(base_record.clone()),
            grounding: incomplete,
        },
    )
    .unwrap_err();
    assert!(
        rejected.contains("complete Assembly Revision"),
        "{rejected:?}"
    );
    crate::blueprint_mcp::execute(
        &service.state_store,
        "builder",
        crate::blueprint_mcp::Command::Capture {
            record: Box::new(base_record.clone()),
            grounding,
        },
    )
    .unwrap();
    let plan: Value = test_support::decode_reply(
        &service
            .new_placement(Parameters(
                serde_json::from_value(json!({"assembly_revision_id":candidate_id})).unwrap(),
            ))
            .await,
    )
    .unwrap();
    assert_eq!(plan["ok"], true, "{plan}");
    assert_eq!(plan["source"]["kind"], "adopted_assembly_revision");
    assert_eq!(plan["source"]["fresh_review"]["status"], "passed");
    assert_eq!(
        plan["source"]["literal_observation"],
        "grounding.base_snapshot"
    );
    assert!(!plan["plan"]["changes"].as_array().unwrap().is_empty());
    server.abort();
    std::fs::remove_dir_all(root).unwrap();
}

#[tokio::test]
async fn revision_placement_revalidates_cumulative_diff_context_and_undo() {
    use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
    for mode in [
        "ok",
        "stale",
        "uncertain",
        "post_mismatch",
        "unpreviewed",
        "read_only",
    ] {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap().to_string();
        let writes = Arc::new(AtomicUsize::new(0));
        let count = writes.clone();
        let drift = Arc::new(AtomicBool::new(false));
        let changed = drift.clone();
        let server = tokio::spawn(async move {
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
                    "scan_region" => {
                        let min: Pos =
                            serde_json::from_value(req["params"]["min"].clone()).unwrap();
                        let max: Pos =
                            serde_json::from_value(req["params"]["max"].clone()).unwrap();
                        let x = if count.load(Ordering::SeqCst) == 1 && mode != "post_mismatch" {
                            1
                        } else {
                            0
                        };
                        let mut positions = vec![Pos::new(x, 0, 0)];
                        if changed.load(Ordering::SeqCst) {
                            positions.push(Pos::new(2, 0, 0));
                        }
                        let blocks = positions
                            .into_iter()
                            .filter(|p| {
                                p.x >= min.x
                                    && p.y >= min.y
                                    && p.z >= min.z
                                    && p.x <= max.x
                                    && p.y <= max.y
                                    && p.z <= max.z
                            })
                            .map(|p| json!({"pos":p,"name":"minecraft:stone","properties":{}}))
                            .collect::<Vec<_>>();
                        json!({"min":min,"max":max,"blocks":blocks})
                    }
                    "submit_command_batch" => {
                        count.fetch_add(1, Ordering::SeqCst);
                        if mode == "uncertain" {
                            error = Some("write reply lost");
                        }
                        json!({"protocol":crate::bridge_protocol::MUTATION_PROTOCOL,"submitted_changes":req["params"]["changes"].as_array().unwrap().len()})
                    }
                    other => panic!("unexpected bridge request {other}"),
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
        let root = std::env::temp_dir().join(format!(
            "dustroute-revision-placement-{}",
            uuid::Uuid::new_v4()
        ));
        let mut service = DustRouteMcp::with_test_transport_and_player(
            address,
            McpPolicy {
                read_only: mode == "read_only",
                ..McpPolicy::default()
            },
            "builder",
        );
        service.state_store = PlanStateStore::new(root.clone(), 3600);
        let snapshot:dustroute_translate::snapshot::MinecraftSnapshot=serde_json::from_value(json!({"min":{"x":0,"y":0,"z":0},"max":{"x":1,"y":1,"z":1},"blocks":[{"pos":{"x":0,"y":0,"z":0},"name":"minecraft:stone","properties":{}}]})).unwrap();
        let id = service
            .store_circuit(StoredCircuit {
                player: "builder".into(),
                dimension: "minecraft:overworld".into(),
                bounds: dustroute_translate::world_reverse::RegionBounds::new(
                    snapshot.min,
                    snapshot.max,
                ),
                target: None,
                snapshot: service.bridge.share_snapshot(snapshot).unwrap(),
                expansion: ExpansionEvidence::Unspecified {},
                complete: true,
                expires_at: Instant::now() + Duration::from_secs(300),
            })
            .await;
        let first:Value=test_support::decode_reply(&service.test_circuit_change(Parameters(serde_json::from_value(json!({"circuit_id":id,"changes":[{"position":{"x":1,"y":0,"z":0},"block":"minecraft:stone"}]})).unwrap())).await).unwrap();
        let revision:Value=test_support::decode_reply(&service.test_circuit_change(Parameters(serde_json::from_value(json!({"revision_id":first["revision_id"],"changes":[{"position":{"x":0,"y":0,"z":0},"block":"minecraft:air"}]})).unwrap())).await).unwrap();
        if mode == "ok" {
            let mut legacy = service
                .load_revision(revision["revision_id"].as_str().unwrap(), "builder")
                .unwrap();
            legacy.revision_id = uuid::Uuid::new_v4();
            legacy.base_snapshot = None;
            legacy.assembly = None;
            service
                .state_store
                .save(
                    PlanRecordKind::CircuitRevisions,
                    legacy.revision_id,
                    &legacy,
                )
                .unwrap();
            let denied: Value = test_support::decode_reply(
                &service
                    .new_placement(Parameters(
                        serde_json::from_value(json!({"revision_id":legacy.revision_id})).unwrap(),
                    ))
                    .await,
            )
            .unwrap();
            assert_eq!(denied["ok"], false);
            assert_eq!(writes.load(Ordering::SeqCst), 0);
        }
        service.circuits.lock().await.clear(); // persisted base survives original observation expiry.
        let proposal: Value = test_support::decode_reply(
            &service
                .new_placement(Parameters(
                    serde_json::from_value(json!({"revision_id":revision["revision_id"]})).unwrap(),
                ))
                .await,
        )
        .unwrap();
        assert_eq!(proposal["ok"], true, "{mode}: {proposal}");
        assert_eq!(proposal["plan"]["changes"].as_array().unwrap().len(), 2);
        let op = proposal["operation_id"].as_str().unwrap().to_owned();
        if mode != "unpreviewed" {
            let shown: Value = test_support::decode_reply(
                &service
                    .show_operation(Parameters(ShowOperationParams {
                        operation_id: op.clone(),
                        player: None,
                    }))
                    .await,
            )
            .unwrap();
            assert_eq!(shown["ok"], true);
        }
        if mode == "stale" {
            drift.store(true, Ordering::SeqCst);
        }
        let result: Value = test_support::decode_reply(
            &service
                .invoke_operation(Parameters(InvokeOperationParams {
                    blueprint_decision: None,
                    operation_id: op.clone(),
                    confirm: true,
                    contracts: None,
                }))
                .await,
        )
        .unwrap();
        assert_eq!(result["ok"], mode == "ok", "{mode}: {result:?}");
        let expected = usize::from(matches!(mode, "ok" | "uncertain" | "post_mismatch"));
        assert_eq!(writes.load(Ordering::SeqCst), expected);
        if mode == "uncertain" || mode == "post_mismatch" {
            let retry: Value = test_support::decode_reply(
                &service
                    .invoke_operation(Parameters(InvokeOperationParams {
                        blueprint_decision: None,
                        operation_id: op.clone(),
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
                    operation_id: op,
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
        std::fs::remove_dir_all(root).unwrap();
    }
}

#[tokio::test]
async fn revisions_branch_persist_validate_and_never_become_live_circuits() {
    let root = std::env::temp_dir().join(format!("dustroute-revisions-{}", uuid::Uuid::new_v4()));
    let mut service = DustRouteMcp::with_test_transport_and_player(
        "127.0.0.1:1",
        McpPolicy::default(),
        "builder",
    );
    service.state_store = PlanStateStore::new(root.clone(), 3600);
    let snapshot:dustroute_translate::snapshot::MinecraftSnapshot=serde_json::from_value(json!({
        "min":{"x":-1,"y":-1,"z":-1},"max":{"x":3,"y":3,"z":3},
        "blocks":[{"pos":{"x":0,"y":0,"z":0},"name":"minecraft:stone","properties":{}},
        {"pos":{"x":0,"y":1,"z":0},"name":"minecraft:repeater","properties":{"facing":"north","delay":"1","powered":"false","locked":"false"}}]
    })).unwrap();
    let id = service
        .store_circuit(StoredCircuit {
            player: "builder".into(),
            dimension: "minecraft:overworld".into(),
            bounds: dustroute_translate::world_reverse::RegionBounds::new(
                snapshot.min,
                snapshot.max,
            ),
            target: None,
            snapshot: service.bridge.share_snapshot(snapshot.clone()).unwrap(),
            expansion: ExpansionEvidence::Unspecified {},
            complete: true,
            expires_at: Instant::now() + Duration::from_secs(300),
        })
        .await;
    async fn edit(service: &DustRouteMcp, args: Value) -> Value {
        test_support::decode_reply(
            &service
                .test_circuit_change(Parameters(serde_json::from_value(args).unwrap()))
                .await,
        )
        .unwrap()
    }
    let base = edit(&service, json!({"circuit_id":id,"changes":[]})).await;
    assert_eq!(base["ok"], true, "{base}");
    assert!(base.get("snapshot").is_none());
    assert!(base.get("assembly_revision").is_none());
    for field in [
        "mutation_performed",
        "live_world_evidence",
        "placement_authorized",
    ] {
        assert_eq!(base[field], false);
    }

    assert_eq!(base["parent_revision_ids"], json!([]));
    assert_eq!(base["base_observation_id"], id.to_string());
    assert_eq!(base["validation"]["after"]["status"], "structurally_valid");
    assert_eq!(base["assembly_state"]["status"], "available");
    assert_eq!(base["assembly_state"]["source_instances"], 0);
    let removed=edit(&service,json!({"revision_id":base["revision_id"],"changes":[{"position":{"x":0,"y":0,"z":0},"block":"minecraft:air"}]})).await;
    assert_eq!(removed["ok"], true, "{removed}");
    assert_eq!(
        removed["validation"]["after"]["status"],
        "invalid_or_unsupported"
    );
    assert_eq!(
        removed["validation"]["after"]["simulation"]["status"],
        "not_run"
    );
    let branch=edit(&service,json!({"revision_id":base["revision_id"],"changes":[{"position":{"x":0,"y":1,"z":0},"block":"minecraft:repeater","properties":{"facing":"east","delay":"2","powered":"false","locked":"false"}}]})).await;
    assert_eq!(branch["ok"], true, "{branch}");
    assert_eq!(branch["parent_revision_ids"], json!([base["revision_id"]]));
    assert_eq!(
        removed["parent_revision_ids"],
        branch["parent_revision_ids"]
    );
    assert_ne!(removed["revision_id"], branch["revision_id"]);
    assert_ne!(
        removed["assembly_state"]["assembly_revision_id"],
        branch["assembly_state"]["assembly_revision_id"]
    );
    assert_eq!(
        branch["assembly_state"]["parent_assembly_revision_ids"],
        json!([base["assembly_state"]["assembly_revision_id"]])
    );
    let repaired=edit(&service,json!({"revision_id":removed["revision_id"],"changes":[{"position":{"x":0,"y":0,"z":0},"block":"minecraft:stone"}]})).await;
    assert_eq!(
        repaired["validation"]["after"]["status"],
        "structurally_valid"
    );
    assert_eq!(
        repaired["parent_revision_ids"],
        json!([removed["revision_id"]])
    );
    assert_eq!(repaired["base_observation_id"], id.to_string());
    assert!(
        service
            .load_circuit(branch["revision_id"].as_str().unwrap(), "builder")
            .await
            .is_err()
    );
    assert_eq!(
        service
            .load_circuit(&id.to_string(), "builder")
            .await
            .unwrap()
            .1
            .snapshot
            .to_owned_snapshot(),
        snapshot
    );
    assert!(service.plans.placements().lock().await.is_empty());
    assert!(
        service
            .plans
            .table::<StoredDoorPlan>()
            .lock()
            .await
            .is_empty()
    );
    assert!(
        service
            .plans
            .table::<StoredPistonPlacement>()
            .lock()
            .await
            .is_empty()
    );
    // A fresh service can read/edit revisions without the original observation or bridge.
    let mut restarted = DustRouteMcp::with_test_transport_and_player(
        "127.0.0.1:1",
        McpPolicy::default(),
        "builder",
    );
    restarted.state_store = PlanStateStore::new(root.clone(), 3600);
    let persisted: Value = test_support::decode_reply(
        &restarted
            .get_circuit_revision(Parameters(GetCircuitRevisionParams {
                blueprint: None,
                revision_id: base["revision_id"].as_str().unwrap().into(),
                include_snapshot: Some(true),
            }))
            .await,
    )
    .unwrap();
    assert_eq!(persisted["ok"], true);
    assert_eq!(persisted["snapshot"], json!(snapshot));
    assert_eq!(
        persisted["assembly_revision"]["id"],
        base["assembly_state"]["assembly_revision_id"]
    );
    assert!(
        persisted["assembly_revision"]["assembly"]["instances"]
            .as_array()
            .unwrap()
            .is_empty()
    );
    assert_eq!(persisted["validation"], base["validation"]);
    let child = edit(
        &restarted,
        json!({"revision_id":branch["revision_id"],"changes":[]}),
    )
    .await;
    assert_eq!(child["ok"], true);
    assert_eq!(child["base_observation_id"], id.to_string());
    assert!(
        restarted
            .load_revision(base["revision_id"].as_str().unwrap(), "someone_else")
            .is_err()
    );
    for args in [
        json!({"changes":[]}),
        json!({"circuit_id":id,"revision_id":base["revision_id"],"changes":[]}),
        json!({"revision_id":id,"changes":[]}),
        json!({"revision_id":base["revision_id"],"changes":[],"simulation_ticks":257}),
        json!({"revision_id":base["revision_id"],"changes":[{"position":{"x":99,"y":0,"z":0},"block":"minecraft:stone"}]}),
    ] {
        assert_eq!(edit(&service, args).await["ok"], false);
    }
    // A revision ID is not an operation capability either.
    let invoked: Value = test_support::decode_reply(
        &service
            .invoke_operation(Parameters(InvokeOperationParams {
                blueprint_decision: None,
                operation_id: base["revision_id"].as_str().unwrap().into(),
                confirm: true,
                contracts: None,
            }))
            .await,
    )
    .unwrap();
    assert_eq!(invoked["ok"], false);
    let invalid_state=edit(&service,json!({"revision_id":base["revision_id"],"changes":[{"position":{"x":0,"y":1,"z":0},"block":"minecraft:repeater","properties":{"facing":"sideways","delay":"2"}}]})).await;
    assert_eq!(invalid_state["ok"], true);
    assert_eq!(
        invalid_state["assembly_state"]["status"],
        "unavailable_or_legacy"
    );
    assert_eq!(
        invalid_state["validation"]["after"]["status"],
        "unavailable"
    );
    for include_snapshot in [false, true] {
        let display = test_support::decode_reply(
            &restarted
                .get_circuit_revision(Parameters(GetCircuitRevisionParams {
                    blueprint: None,
                    revision_id: invalid_state["revision_id"].as_str().unwrap().into(),
                    include_snapshot: Some(include_snapshot),
                }))
                .await,
        )
        .unwrap();
        assert_eq!(display["ok"], true);
        assert_eq!(
            display["assembly_state"],
            json!({"status":"unavailable_or_legacy"})
        );
        assert_eq!(display.get("snapshot").is_some(), include_snapshot);
        assert_eq!(display.get("assembly_revision").is_some(), include_snapshot);
        if include_snapshot {
            assert!(display["assembly_revision"].is_null());
            assert!(display["snapshot"].is_object());
        }
        assert_eq!(display["validation"], invalid_state["validation"]);
        assert_eq!(display["placement_authorized"], false);
        assert!(display.get("player").is_none());
        assert!(display.get("base_snapshot").is_none());
    }
    // Expiry of an ancestor does not invalidate a self-contained descendant.
    let path = root
        .join("circuit_revisions")
        .join(format!("{}.store", base["revision_id"].as_str().unwrap()));
    let mut envelope: crate::state::PlanEnvelope<crate::revision::CircuitRevision> =
        dustroute_codec::storage::decode(
            PlanRecordKind::CircuitRevisions.schema(),
            &std::fs::read(&path).unwrap(),
            4 * 1024 * 1024 + 4096,
        )
        .unwrap();
    envelope.saved_at_unix_seconds = 0;
    std::fs::write(
        &path,
        dustroute_codec::storage::encode(
            PlanRecordKind::CircuitRevisions.schema(),
            &envelope,
            4 * 1024 * 1024 + 4096,
        )
        .unwrap(),
    )
    .unwrap();
    assert!(
        service
            .load_revision(base["revision_id"].as_str().unwrap(), "builder")
            .is_err()
    );
    assert!(
        service
            .load_revision(branch["revision_id"].as_str().unwrap(), "builder")
            .is_ok()
    );
    std::fs::remove_dir_all(root).unwrap();
}
