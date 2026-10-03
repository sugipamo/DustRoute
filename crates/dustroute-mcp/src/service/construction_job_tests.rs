//! End-to-end public job lifecycle; the fake bridge is not a physics oracle.
use super::test_support::*;
use serde_json::{Value, json};

async fn fixture() -> (
    std::path::PathBuf,
    std::sync::Arc<std::sync::Mutex<Fake>>,
    String,
    tokio::task::JoinHandle<()>,
) {
    let root = temporary();
    let (fake, address, bridge) = start_construction_bridge(root.join("world-edits")).await;
    {
        let mut state = fake.lock().unwrap();
        state.snapshot = Some(
            json!({"min":{"x":99,"y":99,"z":102},"max":{"x":110,"y":104,"z":113},"blocks":[
            {"pos":{"x":100,"y":102,"z":103},"name":"minecraft:glass"}]}),
        );
        state.resize_scan = true;
        state.include_air = true;
        state.gaze_target = None;
    }
    (root, fake, address, bridge)
}
async fn propose(client: &Client) -> Value {
    let captured = call(
        client,
        "get_world",
        json!({"region":{"min":{"x":99,"y":99,"z":102},"max":{"x":110,"y":104,"z":113}}}),
    )
    .await;
    assert_eq!(captured["ok"], true, "{captured}");
    assert!(captured["target"].is_null());
    assert_eq!(
        captured["observation_capabilities"]["backend"],
        "test_transport"
    );
    let changes = (100..110)
        .flat_map(|x| {
            (104..112)
                .map(move |z| json!({"position":{"x":x,"y":101,"z":z},"block":"minecraft:stone"}))
        })
        .collect::<Vec<_>>();
    let revision = call(
        client,
        "test_circuit_change",
        json!({"circuit_id":captured["circuit_id"],"changes":changes,"simulation_ticks":1}),
    )
    .await;
    assert_eq!(revision["ok"], true, "{revision}");
    let regions = json!([
        {"min":{"x":100,"y":101,"z":104},"max":{"x":104,"y":101,"z":111}},
        {"min":{"x":105,"y":101,"z":104},"max":{"x":109,"y":101,"z":111}}]);
    let plan = call(
        client,
        "new_placement",
        json!({"revision_id":revision["revision_id"],"work_regions":regions}),
    )
    .await;
    assert_eq!(plan["ok"], true, "{plan}");
    assert_eq!(
        plan["schema_version"],
        "dustroute.electrical-edit-preview.v2"
    );
    assert_eq!(plan["job"]["regions"].as_array().unwrap().len(), 2);
    assert_eq!(plan["future_regions_verified"], false);
    assert_eq!(plan["steps"].as_array().unwrap().len(), 40);
    plan
}
async fn apply(
    client: &Client,
    fake: &std::sync::Arc<std::sync::Mutex<Fake>>,
    plan: &Value,
) -> Value {
    let operation = json!({"operation_id":plan["operation_id"],"confirm":true});
    assert_eq!(
        call(client, "show_operation", operation.clone()).await["ok"],
        true
    );
    fake.lock().unwrap().steps = plan["steps"].as_array().unwrap().iter().cloned().collect();
    call(client, "invoke_operation", operation).await
}

#[tokio::test]
async fn legacy_projected_history_is_retained_and_cannot_authorize_writes() {
    let (root, fake, address, bridge) = fixture().await;
    let (client, server) = connected(&root, &address).await;
    let first = propose(&client).await;
    let path = root
        .join("construction-jobs")
        .join(format!("{}.json", first["job_id"].as_str().unwrap()));
    let mut legacy: Value = serde_json::from_slice(&std::fs::read(&path).unwrap()).unwrap();
    legacy["schema"] = json!("dustroute.construction-job.v1");
    legacy.as_object_mut().unwrap().remove("boundaries");
    for stage in legacy["regions"].as_array_mut().unwrap() {
        stage.as_object_mut().unwrap().remove("parts");
    }
    let bytes = serde_json::to_vec(&legacy).unwrap();
    std::fs::write(&path, &bytes).unwrap();
    let history = call(
        &client,
        "manage_construction_job",
        json!({"job_id":first["job_id"],"action":"get"}),
    )
    .await;
    assert_eq!(history["ok"], false);
    assert!(
        history["error"]
            .as_str()
            .unwrap()
            .contains("recapture a v2 job")
    );
    let old = call(
        &client,
        "invoke_operation",
        json!({"operation_id":first["operation_id"],"confirm":true}),
    )
    .await;
    assert_eq!(old["ok"], false);
    assert_eq!(fake.lock().unwrap().writes, 0);
    assert_eq!(std::fs::read(&path).unwrap(), bytes);
    stop(client, server).await;
    bridge.abort();
    let _ = std::fs::remove_dir_all(root);
}

#[tokio::test]
async fn job_resumes_with_fresh_operations_after_restart_and_undo_runs_in_reverse_order() {
    let (root, fake, address, bridge) = fixture().await;
    let (client, server) = connected(&root, &address).await;
    let first = propose(&client).await;
    let job_id = first["job_id"].clone();
    assert_eq!(apply(&client, &fake, &first).await["ok"], true);
    let history = call(
        &client,
        "manage_construction_job",
        json!({"job_id":job_id,"action":"get"}),
    )
    .await;
    assert_eq!(
        history["schema_version"],
        "dustroute.construction-job-response.v2"
    );
    assert_eq!(history["job"]["completed_regions"], 1);
    assert_eq!(history["job"]["state"], "ready");
    stop(client, server).await;
    let (client, server) = connected(&root, &address).await;
    let old = call(
        &client,
        "invoke_operation",
        json!({"operation_id":first["operation_id"],"confirm":true}),
    )
    .await;
    assert_eq!(old["ok"], false);
    let next = call(
        &client,
        "manage_construction_job",
        json!({"job_id":job_id,"action":"plan_next"}),
    )
    .await;
    assert_eq!(next["ok"], true, "{next}");
    assert_ne!(next["operation_id"], first["operation_id"]);
    assert_eq!(next["job_stage"]["region_index"], 1);
    assert_eq!(apply(&client, &fake, &next).await["ok"], true);
    let history = call(
        &client,
        "manage_construction_job",
        json!({"job_id":job_id,"action":"get"}),
    )
    .await;
    assert_eq!(history["job"]["state"], "completed");
    let observation = call(
        &client,
        "manage_construction_job",
        json!({"job_id":job_id,"action":"observe"}),
    )
    .await;
    assert_eq!(observation["matches_verified_prefix"], true);
    stop(client, server).await;
    let (client, server) = connected(&root, &address).await;
    for index in [1, 0] {
        let undo = call(
            &client,
            "manage_construction_job",
            json!({"job_id":job_id,"action":"plan_undo"}),
        )
        .await;
        assert_eq!(undo["ok"], true, "{undo}");
        assert_eq!(undo["job_stage"]["region_index"], index);
        assert_eq!(undo["job_stage"]["undo"], true);
        assert_eq!(apply(&client, &fake, &undo).await["ok"], true);
    }
    let history = call(
        &client,
        "manage_construction_job",
        json!({"job_id":job_id,"action":"get"}),
    )
    .await;
    assert_eq!(history["job"]["completed_regions"], 0);
    assert_eq!(history["job"]["attempts"].as_array().unwrap().len(), 4);
    assert!(history["job"]["active_operation_id"].is_null());
    assert_eq!(fake.lock().unwrap().writes, 160);
    stop(client, server).await;
    bridge.abort();
    std::fs::remove_dir_all(root).unwrap();
}

#[tokio::test]
async fn superseded_previews_cancelled_jobs_and_protected_drift_cannot_write() {
    let (root, fake, address, bridge) = fixture().await;
    let (client, server) = connected(&root, &address).await;
    let first = propose(&client).await;
    assert_eq!(
        call(
            &client,
            "show_operation",
            json!({"operation_id":first["operation_id"]})
        )
        .await["ok"],
        true
    );
    let next = call(
        &client,
        "manage_construction_job",
        json!({"job_id":first["job_id"],"action":"plan_next"}),
    )
    .await;
    assert_eq!(next["ok"], true);
    assert_eq!(
        call(
            &client,
            "invoke_operation",
            json!({"operation_id":first["operation_id"],"confirm":true})
        )
        .await["ok"],
        false
    );
    fake.lock().unwrap().snapshot.as_mut().unwrap()["blocks"][0]["name"] = json!("minecraft:stone");
    assert_eq!(
        call(
            &client,
            "show_operation",
            json!({"operation_id":next["operation_id"]})
        )
        .await["ok"],
        false
    );
    let diagnostic = call(
        &client,
        "manage_construction_job",
        json!({"job_id":first["job_id"],"action":"observe"}),
    )
    .await;
    assert_eq!(diagnostic["matches_verified_prefix"], false);
    assert_eq!(diagnostic["observation"]["differing_positions"], 1);
    assert_eq!(
        diagnostic["observation"]["differences"][0]["position"],
        json!({"x":100,"y":102,"z":103})
    );
    assert!(diagnostic["observation"].get("snapshot").is_none());
    let failed = call(
        &client,
        "manage_construction_job",
        json!({"job_id":first["job_id"],"action":"plan_next"}),
    )
    .await;
    assert_eq!(failed["ok"], false);
    assert_eq!(
        call(
            &client,
            "manage_construction_job",
            json!({"job_id":first["job_id"],"action":"cancel"})
        )
        .await["job"]["state"],
        "cancelled"
    );
    assert_eq!(
        call(
            &client,
            "invoke_operation",
            json!({"operation_id":next["operation_id"],"confirm":true})
        )
        .await["ok"],
        false
    );
    assert_eq!(
        call(
            &client,
            "manage_construction_job",
            json!({"job_id":first["job_id"],"action":"plan_next"})
        )
        .await["ok"],
        false
    );
    assert_eq!(fake.lock().unwrap().writes, 0);
    stop(client, server).await;
    bridge.abort();
    std::fs::remove_dir_all(root).unwrap();
}

#[tokio::test]
async fn ambiguous_partial_write_retains_uncertain_job_across_restart_and_refuses_retry() {
    let (root, fake, address, bridge) = fixture().await;
    let (client, server) = connected(&root, &address).await;
    let first = propose(&client).await;
    fake.lock().unwrap().lose_write_reply_at = Some(1);
    let result = apply(&client, &fake, &first).await;
    assert_eq!(result["ok"], false, "{result}");
    assert_eq!(result["status"], "needs_inspection");
    let writes = fake.lock().unwrap().writes;
    stop(client, server).await;
    let (client, server) = connected(&root, &address).await;
    let history = call(
        &client,
        "manage_construction_job",
        json!({"job_id":first["job_id"],"action":"get"}),
    )
    .await;
    assert_eq!(history["job"]["state"], "needs_inspection");
    assert_eq!(history["job"]["completed_regions"], 0);
    for action in ["plan_next", "plan_undo", "plan_recovery", "cancel"] {
        assert_eq!(
            call(
                &client,
                "manage_construction_job",
                json!({"job_id":first["job_id"],"action":action})
            )
            .await["ok"],
            false,
            "{action}"
        );
    }
    assert_eq!(fake.lock().unwrap().writes, writes);
    stop(client, server).await;
    bridge.abort();
    std::fs::remove_dir_all(root).unwrap();
}

#[tokio::test]
async fn explicit_recovery_reproves_an_unchanged_baseline_and_discards_the_old_stage() {
    use super::PlanStateStore;
    use crate::construction_jobs::{JobAttempt, JobRegistry, JobState};
    let (root, fake, address, bridge) = fixture().await;
    let (client, server) = connected(&root, &address).await;
    let first = propose(&client).await;
    // A journal can survive interruption after durable intent but before any
    // command. No host/process fault is injected to create this saved state.
    let job_id = uuid::Uuid::parse_str(first["job_id"].as_str().unwrap()).unwrap();
    let old_id = uuid::Uuid::parse_str(first["operation_id"].as_str().unwrap()).unwrap();
    {
        let registry = JobRegistry::acquire(&PlanStateStore::new(root.clone(), 3600)).unwrap();
        let mut record = registry.load(job_id, "Tester").unwrap();
        let mut changed = record.clone();
        changed.after.blocks.clear();
        assert!(registry.save(&changed).unwrap_err().contains("immutable"));
        record.state = JobState::NeedsInspection;
        record.attempts.push(JobAttempt {
            operation_id: old_id,
            region_index: 0,
            undo: false,
            verified: false,
            error: None,
            failure: None,
        });
        registry.save(&record).unwrap();
    }
    assert_eq!(
        call(
            &client,
            "manage_construction_job",
            json!({"job_id":job_id,"action":"plan_next"})
        )
        .await["ok"],
        false
    );
    let fresh = call(
        &client,
        "manage_construction_job",
        json!({"job_id":job_id,"action":"plan_recovery"}),
    )
    .await;
    assert_eq!(fresh["ok"], true, "{fresh}");
    assert_ne!(fresh["operation_id"], first["operation_id"]);
    assert_eq!(
        call(
            &client,
            "invoke_operation",
            json!({"operation_id":old_id,"confirm":true})
        )
        .await["ok"],
        false
    );
    assert_eq!(fake.lock().unwrap().writes, 0);
    assert_eq!(apply(&client, &fake, &fresh).await["ok"], true);
    stop(client, server).await;
    bridge.abort();
    std::fs::remove_dir_all(root).unwrap();
}

#[tokio::test]
async fn job_history_is_owned_and_read_only_policy_does_not_consume_write_intent() {
    use super::{DustRouteMcp, McpPolicy, PlanStateStore};
    let (root, fake, address, bridge) = fixture().await;
    let (client, server) = connected(&root, &address).await;
    let first = propose(&client).await;
    stop(client, server).await;
    let mut other =
        DustRouteMcp::with_test_transport_and_player(&address, McpPolicy::default(), "Other");
    other.state_store = PlanStateStore::new(root.clone(), 3600);
    let (client, server) = serve(other).await;
    let refused = call(
        &client,
        "manage_construction_job",
        json!({"job_id":first["job_id"],"action":"get"}),
    )
    .await;
    assert_eq!(refused["ok"], false);
    assert!(refused["error"].as_str().unwrap().contains("owner"));
    stop(client, server).await;
    let mut readonly =
        DustRouteMcp::with_test_transport_and_player(&address, McpPolicy::default(), "Tester");
    readonly.state_store = PlanStateStore::new(root.clone(), 3600);
    let (client, server) = serve(readonly).await;
    let fresh = call(
        &client,
        "manage_construction_job",
        json!({"job_id":first["job_id"],"action":"plan_next"}),
    )
    .await;
    assert_eq!(fresh["ok"], true, "{fresh}");
    assert_eq!(
        call(
            &client,
            "show_operation",
            json!({"operation_id":fresh["operation_id"]})
        )
        .await["ok"],
        true
    );
    assert_eq!(
        call(
            &client,
            "invoke_operation",
            json!({"operation_id":fresh["operation_id"],"confirm":true})
        )
        .await["ok"],
        false
    );
    let history = call(
        &client,
        "manage_construction_job",
        json!({"job_id":first["job_id"],"action":"get"}),
    )
    .await;
    assert_eq!(history["job"]["state"], "ready");
    assert_eq!(history["job"]["attempts"].as_array().unwrap().len(), 0);
    assert_eq!(fake.lock().unwrap().writes, 0);
    stop(client, server).await;
    bridge.abort();
    std::fs::remove_dir_all(root).unwrap();
}

#[tokio::test]
async fn cancelled_partial_job_can_be_undone_after_restart_without_reenabling_forward_work() {
    let (root, fake, address, bridge) = fixture().await;
    let (client, server) = connected(&root, &address).await;
    let first = propose(&client).await;
    assert_eq!(apply(&client, &fake, &first).await["ok"], true);
    let cancelled = call(
        &client,
        "manage_construction_job",
        json!({"job_id":first["job_id"],"action":"cancel"}),
    )
    .await;
    assert_eq!(cancelled["job"]["state"], "cancelled");
    {
        let registry = crate::construction_jobs::JobRegistry::acquire(
            &crate::state::PlanStateStore::new(root.clone(), 3600),
        )
        .unwrap();
        let mut record = registry
            .load(
                uuid::Uuid::parse_str(first["job_id"].as_str().unwrap()).unwrap(),
                "Tester",
            )
            .unwrap();
        record.forward_cancelled = false;
        assert!(registry.save(&record).unwrap_err().contains("permanent"));
    }
    stop(client, server).await;
    let (client, server) = connected(&root, &address).await;
    assert_eq!(
        call(
            &client,
            "manage_construction_job",
            json!({"job_id":first["job_id"],"action":"plan_next"})
        )
        .await["ok"],
        false
    );
    let undo = call(
        &client,
        "manage_construction_job",
        json!({"job_id":first["job_id"],"action":"plan_undo"}),
    )
    .await;
    assert_eq!(undo["ok"], true, "{undo}");
    assert_eq!(apply(&client, &fake, &undo).await["ok"], true);
    let history = call(
        &client,
        "manage_construction_job",
        json!({"job_id":first["job_id"],"action":"get"}),
    )
    .await;
    assert_eq!(history["job"]["state"], "cancelled");
    assert_eq!(history["job"]["completed_regions"], 0);
    assert_eq!(
        call(
            &client,
            "manage_construction_job",
            json!({"job_id":first["job_id"],"action":"plan_next"})
        )
        .await["ok"],
        false
    );
    assert_eq!(fake.lock().unwrap().writes, 80);
    stop(client, server).await;
    bridge.abort();
    std::fs::remove_dir_all(root).unwrap();
}

async fn coupled_proposal(client: &Client, existing_lamp: bool) -> Value {
    let capture = call(
        client,
        "get_world",
        json!({"region":{"min":{"x":99,"y":99,"z":102},"max":{"x":110,"y":104,"z":113}}}),
    )
    .await;
    assert_eq!(capture["ok"], true, "{capture}");
    let (source, changes, regions) = if existing_lamp {
        (
            json!({"x":101,"y":101,"z":104}),
            json!([{"position":{"x":101,"y":101,"z":104},"block":"minecraft:redstone_block"},
             {"position":{"x":100,"y":101,"z":104},"block":"minecraft:redstone_lamp","properties":{"lit":"true"}}]),
            json!([{ "min":{"x":101,"y":101,"z":104},"max":{"x":101,"y":101,"z":104}},
             {"min":{"x":100,"y":101,"z":104},"max":{"x":100,"y":101,"z":104}}]),
        )
    } else {
        (
            json!({"x":100,"y":102,"z":104}),
            json!([{"position":{"x":100,"y":101,"z":104},"block":"minecraft:redstone_lamp","properties":{"lit":"true"}},
             {"position":{"x":100,"y":102,"z":104},"block":"minecraft:lever","properties":{"face":"floor","facing":"north","powered":"true"}}]),
            json!([{ "min":{"x":100,"y":102,"z":104},"max":{"x":100,"y":102,"z":104}},
             {"min":{"x":100,"y":101,"z":104},"max":{"x":100,"y":101,"z":104}}]),
        )
    };
    let revised = call(
        client,
        "test_circuit_change",
        json!({"circuit_id":capture["circuit_id"],"changes":changes,"simulation_ticks":1}),
    )
    .await;
    assert_eq!(revised["ok"], true, "{revised}");
    let planned = call(
        client,
        "new_placement",
        json!({"revision_id":revised["revision_id"],"work_regions":regions}),
    )
    .await;
    assert_eq!(planned["ok"], true, "{planned}");
    assert!(
        planned["job"]["regions"]
            .as_array()
            .unwrap()
            .iter()
            .any(|r| r["changed_positions"].as_array().unwrap().contains(&source))
    );
    planned
}

#[tokio::test]
async fn temporary_output_and_natural_cross_region_state_survive_restart_and_reverse_undo() {
    let (root, fake, address, bridge) = fixture().await;
    let (client, server) = connected(&root, &address).await;
    let first = coupled_proposal(&client, false).await;
    assert_eq!(first["requested_changes"][0]["properties"]["lit"], "false");
    let job_id = first["job_id"].clone();
    assert_eq!(apply(&client, &fake, &first).await["ok"], true);
    stop(client, server).await;
    let (client, server) = connected(&root, &address).await;
    let next = call(
        &client,
        "manage_construction_job",
        json!({"job_id":job_id,"action":"plan_next"}),
    )
    .await;
    assert_eq!(next["ok"], true, "{next}");
    assert_eq!(next["requested_changes"].as_array().unwrap().len(), 1);
    assert_eq!(
        next["requested_changes"][0]["properties"]["powered"],
        "true"
    );
    assert_eq!(apply(&client, &fake, &next).await["ok"], true);
    let history = call(
        &client,
        "manage_construction_job",
        json!({"job_id":job_id,"action":"get"}),
    )
    .await;
    assert_eq!(history["job"]["state"], "completed");
    assert_eq!(
        history["job"]["boundaries"][1]["changes"]
            .as_array()
            .unwrap()
            .len(),
        2
    );
    stop(client, server).await;
    let (client, server) = connected(&root, &address).await;
    for index in [1, 0] {
        let undo = call(
            &client,
            "manage_construction_job",
            json!({"job_id":job_id,"action":"plan_undo"}),
        )
        .await;
        assert_eq!(undo["ok"], true, "{undo}");
        if index == 1 {
            assert!(
                undo["steps"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .all(|s| s["position"]["y"] == 102)
            );
        }
        assert_eq!(apply(&client, &fake, &undo).await["ok"], true);
    }
    let history = call(
        &client,
        "manage_construction_job",
        json!({"job_id":job_id,"action":"get"}),
    )
    .await;
    assert_eq!(history["job"]["boundaries"], json!([]));
    assert_eq!(history["job"]["completed_regions"], 0);
    stop(client, server).await;
    bridge.abort();
    std::fs::remove_dir_all(root).unwrap();
}

#[tokio::test]
async fn naturally_satisfied_region_requires_fresh_confirmation_and_writes_no_blocks() {
    let (root, fake, address, bridge) = fixture().await;
    fake.lock().unwrap().snapshot.as_mut().unwrap()["blocks"].as_array_mut().unwrap().push(json!({"pos":{"x":100,"y":101,"z":104},"name":"minecraft:redstone_lamp","properties":{"lit":"false"}}));
    let (client, server) = connected(&root, &address).await;
    let first = coupled_proposal(&client, true).await;
    let job_id = first["job_id"].clone();
    assert_eq!(apply(&client, &fake, &first).await["ok"], true);
    let writes = fake.lock().unwrap().writes;
    let next = call(
        &client,
        "manage_construction_job",
        json!({"job_id":job_id,"action":"plan_next"}),
    )
    .await;
    assert_eq!(next["ok"], true, "{next}");
    assert_eq!(next["no_write_checkpoint"], true);
    assert_eq!(
        call(
            &client,
            "invoke_operation",
            json!({"operation_id":next["operation_id"],"confirm":true})
        )
        .await["ok"],
        false
    );
    assert_eq!(apply(&client, &fake, &next).await["ok"], true);
    assert_eq!(fake.lock().unwrap().writes, writes);
    let history = call(
        &client,
        "manage_construction_job",
        json!({"job_id":job_id,"action":"get"}),
    )
    .await;
    assert_eq!(history["job"]["completed_regions"], 2);
    assert_eq!(history["job"]["boundaries"][1]["changes"], json!([]));
    for _ in 0..2 {
        let undo = call(
            &client,
            "manage_construction_job",
            json!({"job_id":job_id,"action":"plan_undo"}),
        )
        .await;
        assert_eq!(undo["ok"], true, "{undo}");
        assert_eq!(apply(&client, &fake, &undo).await["ok"], true);
    }
    assert_eq!(fake.lock().unwrap().writes, writes + 1);
    stop(client, server).await;
    bridge.abort();
    std::fs::remove_dir_all(root).unwrap();
}
