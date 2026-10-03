use super::*;
use crate::service::test_support::{Client, call, serve, start, stop, temporary};
use crate::survival_construction::native_roof_trial as fixture;
use std::io::Write;

async fn comparison_observer() -> voxrig::Client {
    let client = voxrig::Client::connect(voxrig::ConnectionConfig::offline(
        voxrig::Server::new("127.0.0.1", 25572),
        "NatMineView",
        voxrig::MinecraftVersion::Java1_21_11,
    ))
    .await
    .unwrap();
    client.wait_until_ready().await.unwrap();
    client
}

// Test-only comparison after completion; this observer is never configured on
// the production service and cannot affect its admission or continuation gates.
async fn compare_completed_site(builder: &voxrig::Client, observer: &voxrig::Client) -> Value {
    let scene = builder
        .survival()
        .unwrap()
        .capture_survival_scene(region(fixture::scope().observed))
        .await
        .unwrap();
    let observed = observer.observe_region(scene.region()).await.unwrap();
    assert_ne!(observed.connection_id, scene.source().connection_id);
    assert_eq!(observed.blocks.len(), 3120);
    let scenario = scene.scenario();
    for cell in &observed.blocks {
        assert_eq!(
            cell.state.as_ref(),
            Some(&scenario.block(cell.position).unwrap()),
            "{:?}",
            cell.position
        );
    }
    let design = fixture::design().unwrap();
    crate::survival_execution::check_completed_snapshot_for_test(&scene, &design.expected).unwrap();
    let position = tokio::time::timeout(Duration::from_secs(5), async {
        loop {
            let seen = observer
                .survival()
                .unwrap()
                .visible_players()
                .await
                .unwrap();
            if let Some(p) = seen.players.into_iter().find(|p| p.name == "NatMineBot") {
                if (0..3).all(|i| {
                    (p.position[i] - scene.source().position[i]).abs()
                        <= p.motion.position_error[i] + 1e-9
                }) {
                    break p;
                }
            }
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
    })
    .await
    .expect("independent final position comparison");
    json!({"independently_checked_cells":observed.blocks.len(),"observation":observed,
        "position":position,"standing":scene.source(),"used_for_execution_admission":false})
}

async fn generate(client: &Client, spec: Value) -> Value {
    let r = call(
        client,
        "test_circuit_change",
        json!({"blueprint":{"action":"generate_grounded_building_design","request":spec}}),
    )
    .await;
    assert_eq!(r["ok"], true, "{r}");
    assert_eq!(r["writes_minecraft"], false);
    r["result"].clone()
}
async fn adopt(client: &Client, generated: &Value) {
    let imported = call(
        client,
        "test_circuit_change",
        json!({"blueprint":{"action":"import","records":generated["records"]}}),
    )
    .await;
    assert_eq!(imported["ok"], true, "{imported}");
    let mut request = generated["request"].clone();
    request.as_object_mut().unwrap().remove("id");
    let proposed = call(
        client,
        "test_circuit_change",
        json!({"blueprint":{"action":"propose_update","request":request}}),
    )
    .await;
    assert_eq!(proposed["ok"], true, "{proposed}");
    let id = proposed["operation_id"].clone();
    let shown = call(client, "show_operation", json!({"operation_id":id})).await;
    assert_eq!(shown["ok"], true, "{shown}");
    let result = call(
        client,
        "invoke_operation",
        json!({"operation_id":id,"confirm":true,"blueprint_decision":{"action":"adopt"}}),
    )
    .await;
    assert_eq!(result["ok"], true, "{result}");
}
fn plan_request(spec: Value, id: Value) -> Value {
    json!({"action":"plan","assembly_revision_id":id,"specification":spec,"scope":fixture::scope(),
        "supplied":{"minecraft:cobblestone":49,"minecraft:dirt":32},"temporary_material":"minecraft:dirt",
        "limits":{"candidate_checks":12000,"expanded":512,"frontier":16,"actions":256}})
}

#[tokio::test]
async fn public_grounded_adoption_is_required_and_prediction_contract_is_explicit() {
    let root = temporary();
    let (client, server) = start(&root).await;
    let spec = serde_json::to_value(fixture::design().unwrap().specification).unwrap();
    let generated = generate(&client, spec.clone()).await;
    let request = plan_request(spec, generated["request"]["candidate_state"]["id"].clone());
    let refused = call(&client, "survival_construction", request.clone()).await;
    assert_eq!(
        refused["error"]["code"], "source_not_adopted_or_mismatched",
        "{refused}"
    );
    adopt(&client, &generated).await;
    let missing = call(&client, "survival_construction", request.clone()).await;
    assert_eq!(missing["error"]["code"], "backend_unavailable", "{missing}");
    assert_eq!(
        missing["execution_contract"]["independent_observer_required"],
        false
    );
    assert_eq!(
        missing["execution_contract"]["motion"],
        "predicted_dry_cube_v1"
    );
    assert_eq!(
        missing["execution_contract"]["server_stop_acknowledged"],
        false
    );
    assert!(missing["execution_contract"]["server_position_error_bound"].is_null());
    let mut different = request;
    different["specification"]["design"]["name"] = json!("Different design");
    let mismatch = call(&client, "survival_construction", different).await;
    assert_eq!(
        mismatch["error"]["code"], "source_not_adopted_or_mismatched",
        "{mismatch}"
    );
    let denied = call(
        &client,
        "survival_construction",
        json!({"action":"start","job_id":uuid::Uuid::new_v4(),"confirmed":true}),
    )
    .await;
    assert_eq!(denied["error"]["code"], "permission_denied");
    stop(client, server).await;
    let (client, server) = start(&root).await;
    let persisted = crate::blueprint_mcp::construction_source(
        &PlanStateStore::new(root.clone(), 3600),
        "Tester",
        &serde_json::from_value(generated["request"]["candidate_state"]["id"].clone()).unwrap(),
    )
    .unwrap();
    assert_eq!(
        json!(persisted.record),
        generated["request"]["candidate_state"]
    );
    stop(client, server).await;
    std::fs::remove_dir_all(root).unwrap();
}

#[tokio::test]
async fn public_saved_jobs_are_diagnostic_and_corrupt_identity_is_refused() {
    let root = temporary();
    let mut service = DustRouteMcp::with_test_transport_and_player(
        "127.0.0.1:1",
        McpPolicy {
            read_only: false,
            allowed_players: std::collections::BTreeSet::from(["Tester".into()]),
            ..Default::default()
        },
        "Tester",
    );
    service.state_store = PlanStateStore::new(root.clone(), 3600);
    let id = uuid::Uuid::new_v4();
    let path = service.state_store.survival_job_root().join(id.to_string());
    std::fs::create_dir_all(&path).unwrap();
    let mut manifest = json!({"schema":"dustroute.survival-job.v1","job_id":id,
        "owner":"Tester","execution_authority_restorable":false});
    let preview = crate::survival_execution::diagnostic::RecordedConstructionPreview {
        plan: crate::survival_execution::diagnostic::RecordedConstructionPlan {
            scope: crate::survival_construction::tests::scope(),
            initial_temporary: vec![],
            steps: vec![],
            motion_contract: None,
            source: None,
            baseline: None,
            expected: None,
            materials: None,
            final_position: None,
        },
        search: crate::survival_construction::generation::ConstructionSearch {
            candidate_checks: 12,
            best_remaining_targets: vec![[1, 6, 1]],
            progress: vec![crate::survival_construction::generation::SearchProgress {
                candidate_checks: 12,
                actions: 2,
                position: [0.5, 1.0, 0.5],
                remaining_permanent: 1,
                remaining_temporary: 0,
            }],
            refusal_examples: vec![
                crate::survival_construction::ConstructionPlanningError {
                    code: SurvivalErrorCode::InsufficientSuppliedMaterials,
                    action: Some(2),
                    position: Some([1, 6, 1]),
                    detail: "missing reserved material".into(),
                    missing_materials: BTreeMap::from([("minecraft:cobblestone".into(), 1)]),
                },
                crate::survival_construction::ConstructionPlanningError {
                    code: SurvivalErrorCode::NativeGeometryRefused,
                    action: None,
                    position: None,
                    detail: "target outside reach".into(),
                    missing_materials: BTreeMap::new(),
                },
            ],
            ..Default::default()
        },
    };
    manifest["preview"] = json!(preview);
    save(&path.join("manifest.json"), &manifest).unwrap();
    save(
        &path.join("status.json"),
        &JobStatus::AdmissionRefused {
            failure: JobFailure::boundary(JobRefusalCode::SourceChanged, "source changed"),
            construction_dispatched: false,
        },
    )
    .unwrap();
    let (client, server) = serve(service).await;
    let get = json!({"action":"get","job_id":id});
    let history = call(&client, "survival_construction", get.clone()).await;
    assert_eq!(history["historical_only"], true);
    assert_eq!(history["execution_authority_restored"], false);
    assert_eq!(history["status"]["state"], "admission_refused");
    assert_eq!(
        history["status"]["failure"]["error"]["code"],
        "source_changed"
    );
    let detailed = call(
        &client,
        "survival_construction",
        json!({"action":"get","job_id":id,"include_record":true}),
    )
    .await;
    assert_eq!(detailed["manifest"]["preview"], json!(preview));
    let refused = call(
        &client,
        "survival_construction",
        json!({"action":"start","job_id":id,"confirmed":true}),
    )
    .await;
    assert_eq!(refused["error"]["code"], "plan_not_live");
    let refused = call(
        &client,
        "survival_construction",
        json!({"action":"cancel","job_id":id}),
    )
    .await;
    assert_eq!(refused["error"]["code"], "job_not_live");
    let execution = path.join("execution");
    std::fs::create_dir_all(&execution).unwrap();
    std::fs::write(execution.join("executor.lock"), []).unwrap();
    let intent = json!({"step":0,"phase":"mining_start_send","outcome":"uncertain","continuation":"needs_inspection","evidence":crate::survival_execution::evidence::mining_start_fixture()});
    let record = json!({"schema":"dustroute.survival-execution.v2","id":uuid::Uuid::new_v4(),"plan":{},"completed_steps":0,"outcome":"uncertain","continuation":"needs_inspection","reconnects":0,"events":[intent]});
    save(&execution.join("record.json"), &record).unwrap();
    let refusal = call(
        &client,
        "survival_construction",
        json!({"action":"continue","job_id":id}),
    )
    .await;
    assert_eq!(refusal["error"]["code"], "safe_checkpoint_missing");
    assert_eq!(
        refusal["historical_diagnosis"]["last_event"]["phase"],
        "mining_start_send"
    );
    assert_eq!(
        load::<Value>(&execution.join("record.json")).unwrap(),
        record
    );
    manifest["owner"] = json!("AnotherPlayer");
    save(&path.join("manifest.json"), &manifest).unwrap();
    let denied = call(&client, "survival_construction", get.clone()).await;
    assert_eq!(denied["error"]["code"], "permission_denied");
    manifest["job_id"] = json!(uuid::Uuid::new_v4());
    save(&path.join("manifest.json"), &manifest).unwrap();
    let invalid = call(&client, "survival_construction", get).await;
    assert_eq!(invalid["error"]["code"], "invalid_record");
    stop(client, server).await;
    std::fs::remove_dir_all(root).unwrap();
}

#[path = "tests/continuation.rs"]
mod continuation;

#[tokio::test]
#[ignore = "explicit isolated non-OP server and supplied inventory; public MCP roof acceptance"]
async fn native_public_roof() {
    assert_eq!(
        std::env::var("DUSTROUTE_SURVIVAL_ROOF_PORT").unwrap(),
        "25572"
    );
    assert_eq!(
        std::env::var("DUSTROUTE_SURVIVAL_ROOF_EXECUTE").unwrap(),
        "1"
    );
    let output = std::env::var("DUSTROUTE_SURVIVAL_ROOF_OUTPUT").unwrap();
    assert!(!Path::new(&output).exists());
    let root = std::path::PathBuf::from(format!("{output}.state"));
    assert!(!root.exists());
    let mut service = DustRouteMcp::connect_voxrig(
        McpConfig::new("127.0.0.1:25572", "Tester").unwrap(),
        McpPolicy {
            read_only: false,
            ..Default::default()
        },
        "NatMineBot",
    )
    .await
    .unwrap();
    assert!(service.survival_observer.is_none());
    service.state_store = PlanStateStore::new(root.clone(), 3600);
    let native = service.bridge.survival_bridge().unwrap();
    let initial = native.lease_survival().unwrap();
    let bot = initial.source();
    initial.release(bot.clone());
    let observer = comparison_observer().await;
    println!(
        "FIXTURE roof: flat stone y=-61; air above; bot 2.5 -60 7.5; viewer 7.5 -60 2.5; clear bot; inventory.0 cobblestone 49, inventory.1 dirt 32; enter"
    );
    std::io::stdout().flush().unwrap();
    tokio::task::spawn_blocking(|| {
        let mut s = String::new();
        std::io::stdin().read_line(&mut s).unwrap();
    })
    .await
    .unwrap();
    let started = std::time::Instant::now();
    let (client, server) = serve(service.clone()).await;
    let spec = serde_json::to_value(fixture::design().unwrap().specification).unwrap();
    let generated = generate(&client, spec.clone()).await;
    adopt(&client, &generated).await;
    let request = plan_request(spec, generated["request"]["candidate_state"]["id"].clone());
    let plan = call(&client, "survival_construction", request).await;
    std::fs::write(
        &output,
        serde_json::to_vec_pretty(
            &json!({"error":"not finished","events":[{"phase":"public_plan","result":plan}]}),
        )
        .unwrap(),
    )
    .unwrap();
    assert_eq!(plan["ok"], true, "{plan}");
    let id = plan["job_id"].clone();
    // Fork the already checked process-local preview as a fixture, never from
    // serialized plan data. Exercise lifecycle gates without another search or
    // granting either fork permission to dispatch native actions.
    let mut lifecycle = Vec::new();
    for expired in [false, true] {
        let fork_id = uuid::Uuid::new_v4();
        let original_id: uuid::Uuid = serde_json::from_value(id.clone()).unwrap();
        let mut jobs = service.survival.entries.lock().await;
        let original = jobs.get(&original_id).unwrap();
        let fork = original.fork_preview(expired.then(|| Instant::now() - Duration::from_secs(1)));
        jobs.insert(fork_id, fork);
        drop(jobs);
        let directory = service
            .state_store
            .survival_job_root()
            .join(fork_id.to_string());
        std::fs::create_dir_all(&directory).unwrap();
        if !expired {
            let cancelled = call(
                &client,
                "survival_construction",
                json!({"action":"cancel","job_id":fork_id}),
            )
            .await;
            assert_eq!(cancelled["ok"], true);
            assert_eq!(
                load::<Value>(&directory.join("status.json")).unwrap()["state"],
                "cancelled_before_start"
            );
        }
        let refused = call(
            &client,
            "survival_construction",
            json!({"action":"start","job_id":fork_id,"confirmed":true}),
        )
        .await;
        assert_eq!(refused["error"]["code"], "plan_expired_or_cancelled");
        let status = call(
            &client,
            "survival_construction",
            json!({"action":"get","job_id":fork_id}),
        )
        .await;
        assert_eq!(status["status"]["construction_dispatched"], false);
        assert_eq!(
            service
                .survival
                .entries
                .lock()
                .await
                .get(&fork_id)
                .unwrap()
                .start_readiness(),
            StartReadiness::ExpiredOrCancelled,
        );
        lifecycle.push(json!({"case":if expired {"expired_preview"} else {"cancelled_preview"},"result":refused,"status":status}));
    }
    // The controller changes only the isolated fixture between plan and start.
    // All tested admission/dispatch decisions still use the real public tool.
    let refusal = std::env::var("DUSTROUTE_SURVIVAL_PUBLIC_REFUSAL").ok();
    let before_start = if let Some(kind) = &refusal {
        assert!(matches!(kind.as_str(), "site" | "materials"));
        println!("INJECT public {kind}:");
        std::io::stdout().flush().unwrap();
        tokio::task::spawn_blocking(|| {
            let mut line = String::new();
            std::io::stdin().read_line(&mut line).unwrap();
        })
        .await
        .unwrap();
        let ops = bot.survival().unwrap();
        let deadline = Instant::now() + Duration::from_secs(10);
        loop {
            let ready = if kind == "site" {
                let observed = bot
                    .observe_region(voxrig::Region {
                        min: [4, -54, 4],
                        max: [4, -54, 4],
                    })
                    .await
                    .unwrap();
                observed.blocks[0]
                    .state
                    .as_ref()
                    .is_some_and(|b| b.name == "minecraft:dirt")
            } else {
                let state = ops.player_state().await.unwrap();
                matches!(&state.inventory.slots[9], voxrig::checked_survival::InventorySlot::Item { item } if item.name == "minecraft:cobblestone" && item.count == 48)
            };
            if ready {
                break;
            }
            assert!(Instant::now() < deadline, "fixture update not received");
            tokio::time::sleep(Duration::from_millis(50)).await;
        }
        Some(
            observer
                .observe_region(region(fixture::scope().observed))
                .await
                .unwrap(),
        )
    } else {
        None
    };
    let denied = call(
        &client,
        "survival_construction",
        json!({"action":"start","job_id":id,"confirmed":false}),
    )
    .await;
    assert_eq!(denied["error"]["code"], "confirmation_required");
    let started_response = call(
        &client,
        "survival_construction",
        json!({"action":"start","job_id":id,"confirmed":true}),
    )
    .await;
    assert_eq!(started_response["ok"], true, "{started_response}");
    assert!(
        native.lease_survival().is_err(),
        "executor must own the source"
    );
    let mut last = Value::Null;
    let final_result = loop {
        let status = call(
            &client,
            "survival_construction",
            json!({"action":"get","job_id":id}),
        )
        .await;
        if status["completed_steps"] != last {
            last = status["completed_steps"].clone();
            println!("PROGRESS {} public job", last);
        }
        let state = status["status"]["state"].as_str().unwrap_or("");
        if matches!(
            state,
            "completed" | "admission_refused" | "needs_inspection" | "cancelled_needs_inspection"
        ) {
            break call(
                &client,
                "survival_construction",
                json!({"action":"get","job_id":id,"include_record":true}),
            )
            .await;
        }
        assert!(started.elapsed().as_secs() < 1000, "public job timeout");
        tokio::time::sleep(Duration::from_millis(200)).await;
    };
    let error = if final_result["status"]["state"] == "completed" {
        Value::Null
    } else {
        final_result["status"].clone()
    };
    std::fs::write(&output,serde_json::to_vec_pretty(&json!({"execute":true,"error":error,"lifecycle_checks":lifecycle,"events":[{"phase":"public_plan","result":plan},{"phase":"public_start","result":started_response},{"phase":"public_final","result":final_result}]})).unwrap()).unwrap();
    if let Some(kind) = refusal {
        assert_eq!(
            final_result["status"]["state"], "admission_refused",
            "{final_result}"
        );
        assert_eq!(final_result["status"]["construction_dispatched"], false);
        assert_eq!(
            final_result["status"]["failure"]["error"]["code"],
            if kind == "site" {
                "snapshot_mismatch"
            } else {
                "supplied_materials_missing"
            },
            "{final_result}"
        );
        let after = observer
            .observe_region(region(fixture::scope().observed))
            .await
            .unwrap();
        let before = before_start.unwrap();
        assert_eq!(
            json!(before.blocks),
            json!(after.blocks),
            "admission refusal must leave fixture unchanged"
        );
        let restored = native.lease_survival().unwrap();
        restored.release(bot.clone());
        let replay = call(
            &client,
            "survival_construction",
            json!({"action":"start","job_id":id,"confirmed":true}),
        )
        .await;
        assert_eq!(replay["error"]["code"], "job_already_started");
        let mut evidence: Value = serde_json::from_slice(&std::fs::read(&output).unwrap()).unwrap();
        evidence["expected_refusal"] = json!(kind);
        evidence["refusal_verified"] = json!(true);
        evidence["independently_unchanged_cells"] = json!(after.blocks.len());
        evidence["error"] = Value::Null;
        std::fs::write(&output, serde_json::to_vec_pretty(&evidence).unwrap()).unwrap();
        bot.disconnect().await.unwrap();
        observer.disconnect().await.unwrap();
        stop(client, server).await;
        return;
    }
    assert!(error.is_null(), "{error}");
    assert_eq!(
        final_result["completed_steps"],
        json!(plan["preview"]["plan"]["steps"].as_array().unwrap().len())
    );
    assert_eq!(
        final_result["diagnosis"]["record"]["events"]
            .as_array()
            .unwrap()
            .last()
            .unwrap()["evidence"]["data"]["builder_checked_cells"],
        3120
    );
    let restored = native.lease_survival().unwrap();
    let current = restored.source();
    let comparison = compare_completed_site(&current, &observer).await;
    let mut evidence: Value = serde_json::from_slice(&std::fs::read(&output).unwrap()).unwrap();
    evidence["independent_final_comparison"] = comparison;
    evidence["production_observer_configured"] = json!(false);
    std::fs::write(&output, serde_json::to_vec_pretty(&evidence).unwrap()).unwrap();
    assert_ne!(
        bot.survival()
            .unwrap()
            .operation_history()
            .await
            .connection_id,
        current
            .survival()
            .unwrap()
            .operation_history()
            .await
            .connection_id
    );
    restored.release(current.clone());
    current.disconnect().await.unwrap();
    observer.disconnect().await.unwrap();
    stop(client, server).await;
    let (reopened, server) = start(&root).await;
    let history = call(
        &reopened,
        "survival_construction",
        json!({"action":"get","job_id":id,"include_record":true}),
    )
    .await;
    assert_eq!(history["historical_only"], true);
    assert_eq!(history["execution_authority_restored"], false);
    assert_eq!(history["diagnosis"]["continuation"], "needs_inspection");
    let mut evidence: Value = serde_json::from_slice(&std::fs::read(&output).unwrap()).unwrap();
    evidence["events"]
        .as_array_mut()
        .unwrap()
        .push(json!({"phase":"public_restart_diagnosis","result":history}));
    std::fs::write(&output, serde_json::to_vec_pretty(&evidence).unwrap()).unwrap();
    stop(reopened, server).await;
}
