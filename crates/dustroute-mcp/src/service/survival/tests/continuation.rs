//! Two genuinely separate test processes; normal checked shutdown, no crash injection.
use super::*;
use std::path::PathBuf;

// This test aggregates two processes' previews, diagnoses and final records.
// It is not a persisted production job and never restores native authority.
const MAX_TRIAL_EVIDENCE_BYTES: usize = 64 * 1024 * 1024;

fn save_trial_evidence(path: &Path, value: &Value) -> Result<(), String> {
    let bytes = serde_json::to_vec_pretty(value).map_err(|e| e.to_string())?;
    if bytes.len() > MAX_TRIAL_EVIDENCE_BYTES {
        return Err("continuation trial evidence exceeds bound".into());
    }
    crate::storage::replace(path, &bytes, crate::storage::Durability::FileAndDirectory)
        .map_err(|e| e.to_string())
}

fn load_trial_evidence(path: &Path) -> Result<Value, String> {
    use std::io::Read;
    let mut bytes = Vec::new();
    std::fs::File::open(path)
        .map_err(|e| e.to_string())?
        .take((MAX_TRIAL_EVIDENCE_BYTES + 1) as u64)
        .read_to_end(&mut bytes)
        .map_err(|e| e.to_string())?;
    if bytes.len() > MAX_TRIAL_EVIDENCE_BYTES {
        return Err("continuation trial evidence exceeds bound".into());
    }
    serde_json::from_slice(&bytes).map_err(|e| e.to_string())
}

#[test]
fn survival_continuation_evidence_exceeds_job_bound_without_changing_job_storage() {
    let root = temporary();
    std::fs::create_dir_all(&root).unwrap();
    let output = root.join("trial.json");
    let value = json!({"continuation_final":{"status":"needs_inspection",
        "diagnostic":"x".repeat(16 * 1024 * 1024)}});
    assert_eq!(
        save(&output, &value).unwrap_err(),
        "survival job record exceeds bound"
    );
    save_trial_evidence(&output, &value).unwrap();
    assert_eq!(load_trial_evidence(&output).unwrap(), value);
    assert_eq!(
        load::<Value>(&output).unwrap_err(),
        "survival job record exceeds bound"
    );
    std::fs::remove_dir_all(root).unwrap();
}

#[test]
fn survival_continuation_evidence_still_bounds_input() {
    let root = temporary();
    std::fs::create_dir_all(&root).unwrap();
    let output = root.join("trial.json");
    let file = std::fs::File::create(&output).unwrap();
    file.set_len((MAX_TRIAL_EVIDENCE_BYTES + 1) as u64).unwrap();
    assert_eq!(
        load_trial_evidence(&output).unwrap_err(),
        "continuation trial evidence exceeds bound"
    );
    drop(file);
    std::fs::remove_dir_all(root).unwrap();
}

async fn console(marker: &str) {
    println!("{marker}");
    std::io::stdout().flush().unwrap();
    tokio::task::spawn_blocking(|| {
        let mut line = String::new();
        std::io::stdin().read_line(&mut line).unwrap();
    })
    .await
    .unwrap();
}
fn limits() -> Value {
    json!({"candidate_checks":12000,"expanded":512,"frontier":16,"actions":256})
}
async fn get(client: &Client, id: &Value, full: bool) -> Value {
    call(
        client,
        "survival_construction",
        json!({"action":"get","job_id":id,"include_record":full}),
    )
    .await
}

#[tokio::test]
#[ignore = "explicit isolated non-OP server; controller runs checkpoint and continuation in separate processes"]
async fn native_public_idle_continuation() {
    assert_eq!(
        std::env::var("DUSTROUTE_SURVIVAL_ROOF_PORT").unwrap(),
        "25572"
    );
    let output = PathBuf::from(std::env::var("DUSTROUTE_SURVIVAL_ROOF_OUTPUT").unwrap());
    let phase = std::env::var("DUSTROUTE_CONTINUATION_PHASE").unwrap();
    let stop_at = std::env::var("DUSTROUTE_CONTINUATION_BOUNDARY").unwrap();
    assert!(matches!(phase.as_str(), "checkpoint" | "continue"));
    assert!(matches!(stop_at.as_str(), "placement" | "mining"));
    let root = output.with_extension("state");
    let mut service = DustRouteMcp::connect_voxrig(
        McpConfig::new("127.0.0.1:25572", "Tester", "127.0.0.1:1").unwrap(),
        McpPolicy {
            read_only: false,
            allowed_players: std::collections::BTreeSet::from(["Tester".into()]),
            max_placement_blocks: 512,
            ..Default::default()
        },
        "NatMineBot",
    )
    .await
    .unwrap();
    assert!(service.survival_observer.is_none());
    service.state_store = PlanStateStore::new(root.clone(), 3600);
    let native = service.bridge.survival_bridge().unwrap();
    let observer = comparison_observer().await;
    if phase == "checkpoint" {
        console("FIXTURE roof: checkpoint trial; enter").await;
    }
    let (client, server) = serve(service.clone()).await;
    let started = Instant::now();
    let mut events = Vec::new();
    let mut evidence = if phase == "continue" {
        load_trial_evidence(&output).unwrap()
    } else {
        json!({"checkpoint_pid":std::process::id(),"boundary":stop_at})
    };
    let id;
    if phase == "checkpoint" {
        let spec = json!(fixture::design().unwrap().specification);
        let generated = generate(&client, spec.clone()).await;
        adopt(&client, &generated).await;
        let plan = call(
            &client,
            "survival_construction",
            plan_request(spec, generated["request"]["candidate_state"]["id"].clone()),
        )
        .await;
        assert_eq!(plan["ok"], true, "{plan}");
        id = plan["job_id"].clone();
        let first_temporary = plan["preview"]["plan"]["steps"]
            .as_array()
            .unwrap()
            .iter()
            .position(|s| s["purpose"] == "temporary")
            .unwrap()
            + 1;
        events.push(json!({"phase":"plan","result":plan}));
        let start = call(
            &client,
            "survival_construction",
            json!({"action":"start","job_id":id,"confirmed":true}),
        )
        .await;
        assert_eq!(start["ok"], true, "{start}");
        events.push(json!({"phase":"start","result":start}));
        let mut requested = false;
        loop {
            let status = get(&client, &id, false).await;
            let state = status["status"]["state"].as_str().unwrap_or("");
            if !requested
                && ((stop_at == "placement"
                    && status["completed_steps"].as_u64().unwrap_or(0) >= first_temporary as u64)
                    || (stop_at == "mining"
                        && status["status"]["progress"]["kind"] == "mining_started"))
            {
                let response = call(
                    &client,
                    "survival_construction",
                    json!({"action":"checkpoint","job_id":id}),
                )
                .await;
                assert_eq!(response["ok"], true, "{response}");
                assert_eq!(response["idle_confirmed"], false);
                events.push(json!({"phase":"checkpoint_request","at":status,"result":response}));
                requested = true;
            }
            if state == "checkpointed" {
                break;
            }
            assert!(
                !matches!(
                    state,
                    "completed" | "admission_refused" | "needs_inspection"
                ),
                "{status}"
            );
            assert!(started.elapsed().as_secs() < 1000, "checkpoint timeout");
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
        let final_result = get(&client, &id, true).await;
        let checkpoint = &final_result["diagnosis"]["record"]["events"]
            .as_array()
            .unwrap()
            .last()
            .unwrap()["evidence"];
        assert_eq!(final_result["status"]["safe_idle"], true);
        assert!(
            !checkpoint["temporary"].as_array().unwrap().is_empty(),
            "checkpoint must retain cleanup obligations"
        );
        if stop_at == "mining" {
            assert!(
                final_result["diagnosis"]["record"]["reconnects"]
                    .as_u64()
                    .unwrap()
                    > 0
            );
        }
        events.push(json!({"phase":"checkpoint_final","result":final_result}));
        evidence["parent_job_id"] = id.clone();
        evidence["checkpoint_events"] = json!(events);
    } else {
        assert_ne!(evidence["checkpoint_pid"], json!(std::process::id()));
        evidence["continuation_pid"] = json!(std::process::id());
        let parent = evidence["parent_job_id"].clone();
        let historical = get(&client, &parent, true).await;
        assert_eq!(historical["historical_only"], true);
        assert_eq!(historical["execution_authority_restored"], false);
        assert_eq!(historical["recorded_continuation"], "checkpoint");
        events.push(json!({"phase":"fresh_process_diagnosis","result":historical}));
        let request = json!({"action":"continue","job_id":parent,"limits":limits()});
        // External fixture changes test refusal; restoration is explicit fixture
        // setup by the controller, never an automatic production repair.
        console("INJECT continuation foreign:").await;
        let before = observer
            .observe_region(region(fixture::scope().observed))
            .await
            .unwrap();
        let refused = call(&client, "survival_construction", request.clone()).await;
        assert_eq!(
            refused["error"]["code"], "checkpoint_site_changed",
            "{refused}"
        );
        assert_eq!(
            refused["diagnosis"]["conflicts"][0]["position"],
            json!([8, -60, 8])
        );
        let after = observer
            .observe_region(region(fixture::scope().observed))
            .await
            .unwrap();
        assert_eq!(json!(before.blocks), json!(after.blocks));
        events.push(json!({"phase":"foreign_change_refused","result":refused,"independently_unchanged_cells":after.blocks.len()}));
        console("RESTORE continuation foreign:").await;
        let lease = native.lease_survival().unwrap();
        let bot = lease.source();
        lease.release(bot.clone());
        let counts = crate::survival_execution::received_materials(
            &bot.survival().unwrap().player_state().await.unwrap(),
        )
        .unwrap();
        let restore = counts.get("minecraft:cobblestone").copied().unwrap_or(0);
        assert!(restore > 0);
        console("INJECT continuation materials:").await;
        let refused = call(&client, "survival_construction", request.clone()).await;
        assert_eq!(refused["error"]["code"], "generation_refused", "{refused}");
        assert_eq!(
            refused["error"]["cause"]["kind"], "insufficient_materials",
            "{refused}"
        );
        events.push(json!({"phase":"materials_shortage_refused","result":refused}));
        console(&format!("RESTORE continuation materials {restore}:")).await;
        let plan = call(&client, "survival_construction", request.clone()).await;
        // Save failures before assertion to preserve search diagnostics.
        evidence["continuation_preview"] = plan.clone();
        save_trial_evidence(&output, &evidence).unwrap();
        assert_eq!(plan["ok"], true, "{plan}");
        assert!(
            !plan["diagnosis"]["completed_targets"]
                .as_array()
                .unwrap()
                .is_empty()
        );
        assert!(
            !plan["preview"]["plan"]["initial_temporary"]
                .as_array()
                .unwrap()
                .is_empty()
        );
        let unbuilt = plan["diagnosis"]["unbuilt_targets"]
            .as_array()
            .unwrap()
            .len();
        assert_eq!(
            plan["preview"]["plan"]["materials"]["permanent"]["minecraft:cobblestone"],
            json!(unbuilt)
        );
        id = plan["job_id"].clone();
        events.push(json!({"phase":"continuation_plan","result":plan}));
        let before = observer
            .observe_region(region(fixture::scope().observed))
            .await
            .unwrap();
        let denied = call(
            &client,
            "survival_construction",
            json!({"action":"start","job_id":id,"confirmed":false}),
        )
        .await;
        assert_eq!(denied["error"]["code"], "confirmation_required");
        let after = observer
            .observe_region(region(fixture::scope().observed))
            .await
            .unwrap();
        assert_eq!(json!(before.blocks), json!(after.blocks));
        let start = call(
            &client,
            "survival_construction",
            json!({"action":"start","job_id":id,"confirmed":true}),
        )
        .await;
        assert_eq!(start["ok"], true, "{start}");
        let final_result = loop {
            let status = get(&client, &id, false).await;
            let state = status["status"]["state"].as_str().unwrap_or("");
            if matches!(
                state,
                "completed" | "admission_refused" | "needs_inspection"
            ) {
                break get(&client, &id, true).await;
            }
            assert!(started.elapsed().as_secs() < 1000, "continuation timeout");
            tokio::time::sleep(Duration::from_millis(200)).await;
        };
        events.push(json!({"phase":"continuation_final","result":final_result}));
        evidence["continuation_events"] = json!(events);
        save_trial_evidence(&output, &evidence).unwrap();
        assert_eq!(
            final_result["status"]["state"], "completed",
            "{}",
            final_result["status"]
        );
        assert_eq!(
            final_result["status"]["final_evidence"]["evidence"]["remaining_owned_temporary"],
            json!([])
        );
        assert_eq!(
            final_result["status"]["final_evidence"]["evidence"]["builder_checked_cells"],
            3120
        );
        let lease = native.lease_survival().unwrap();
        let current = lease.source();
        evidence["independent_final_comparison"] =
            compare_completed_site(&current, &observer).await;
        evidence["production_observer_configured"] = json!(false);
        lease.release(current);
        let consumed = call(&client, "survival_construction", request).await;
        assert_eq!(consumed["error"]["code"], "checkpoint_consumed");
        assert_eq!(consumed["continuation_job_id"], id);
        evidence["duplicate_continuation_refused"] = consumed;
    }
    evidence[format!("{phase}_elapsed_seconds")] = json!(started.elapsed().as_secs_f64());
    evidence["error"] = Value::Null;
    save_trial_evidence(&output, &evidence).unwrap();
    let lease = native.lease_survival().unwrap();
    let current = lease.source();
    lease.release(current.clone());
    current.disconnect().await.unwrap();
    observer.disconnect().await.unwrap();
    stop(client, server).await;
    println!("ACCEPTED {phase}: {id}");
}
