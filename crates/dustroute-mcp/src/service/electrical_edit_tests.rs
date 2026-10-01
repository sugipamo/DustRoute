//! Public workflow coverage; the transport fixture is not a physics oracle.
use super::test_support::*;
use serde_json::{Value, json};

pub(super) fn machine() -> Value {
    json!({"min":{"x":99,"y":99,"z":102},"max":{"x":105,"y":104,"z":109},"blocks":[
        {"pos":{"x":101,"y":101,"z":104},"name":"minecraft:sticky_piston","properties":{"facing":"east","extended":"false"}},
        {"pos":{"x":101,"y":101,"z":105},"name":"minecraft:slime_block"},
        {"pos":{"x":101,"y":102,"z":105},"name":"minecraft:observer","properties":{"facing":"south","powered":"false"}},
        {"pos":{"x":102,"y":101,"z":104},"name":"minecraft:slime_block"},
        {"pos":{"x":102,"y":101,"z":105},"name":"minecraft:sticky_piston","properties":{"facing":"west","extended":"false"}},
        {"pos":{"x":102,"y":102,"z":104},"name":"minecraft:observer","properties":{"facing":"north","powered":"false"}}
    ]})
}
async fn propose(client: &Client) -> Value {
    let snapshot = machine();
    let capture = call(
        client,
        "get_world",
        json!({"region":{"min":snapshot["min"],"max":snapshot["max"]},"include_block_list":true}),
    )
    .await;
    assert_eq!(capture["ok"], true, "{capture}");
    assert_eq!(capture["counts"]["non_air"], 6);
    let revision = call(
        client,
        "test_circuit_change",
        json!({"circuit_id":capture["circuit_id"],"changes":[
            {"position":{"x":101,"y":101,"z":106},"block":"minecraft:slime_block"},
            {"position":{"x":101,"y":101,"z":107},"block":"minecraft:glass"}
        ]}),
    )
    .await;
    assert_eq!(revision["ok"], true, "{revision}");
    assert_eq!(
        revision["validation"]["electrical_modification"]["status"],
        "passed"
    );
    let plan = call(
        client,
        "new_placement",
        json!({"revision_id":revision["revision_id"]}),
    )
    .await;
    assert_eq!(plan["ok"], true, "{plan}");
    assert_eq!(plan["kind"], "electrical_revision_modification");
    assert_eq!(plan["steps"].as_array().unwrap().len(), 2);
    assert_eq!(plan["execution_batches"].as_array().unwrap().len(), 1);
    assert_eq!(plan["execution_batches"][0]["changed_blocks"], 2);
    plan
}
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
        state.snapshot = Some(machine());
        state.gaze_target = Some(json!({"x":101,"y":101,"z":105}));
        state.resize_scan = true;
    }
    (root, fake, address, bridge)
}

#[tokio::test]
async fn scoped_edit_checks_permissions_and_retains_scope_in_undo_and_restart_history() {
    let (root, fake, address, bridge) = fixture().await;
    let (client, server) = connected(&root, &address).await;
    let initial = propose(&client).await;
    let scope = json!({"editable":[{"min":{"x":101,"y":101,"z":106},"max":{"x":101,"y":101,"z":107}}],
        "protected":[{"min":{"x":101,"y":101,"z":104},"max":{"x":102,"y":102,"z":105}}]});
    let invalid = json!({"revision_id":initial["revision_id"],"edit_scop":scope});
    let rejected = client
        .call_tool(
            rmcp::model::CallToolRequestParams::new("new_placement")
                .with_arguments(invalid.as_object().unwrap().clone()),
        )
        .await
        .unwrap();
    assert_eq!(rejected.is_error, Some(true));
    let rmcp::model::ContentBlock::Text(reason) = &rejected.content[0] else {
        panic!("expected parameter diagnostics")
    };
    assert!(reason.text.contains("unknown field `edit_scop`"));
    let incompatible = call(
        &client,
        "new_placement",
        json!({"circuit":"half-adder","edit_scope":scope}),
    )
    .await;
    assert_eq!(incompatible["ok"], false);
    let mut denied = scope.clone();
    denied["editable"][0]["max"]["z"] = json!(106);
    let refusal = call(
        &client,
        "new_placement",
        json!({"revision_id":initial["revision_id"],"edit_scope":denied}),
    )
    .await;
    assert_eq!(refusal["ok"], false);
    assert!(
        refusal["error"]
            .as_str()
            .unwrap()
            .contains("declared write")
    );
    assert_eq!(fake.lock().unwrap().writes, 0);
    let plan = call(
        &client,
        "new_placement",
        json!({"revision_id":initial["revision_id"],"edit_scope":scope}),
    )
    .await;
    assert_eq!(plan["ok"], true, "{plan}");
    assert_eq!(plan["edit_scope"], scope);
    let operation = json!({"operation_id":plan["operation_id"],"confirm":true});
    assert_eq!(
        call(&client, "show_operation", operation.clone()).await["ok"],
        true
    );
    fake.lock().unwrap().steps = plan["steps"].as_array().unwrap().iter().cloned().collect();
    assert_eq!(
        call(&client, "invoke_operation", operation.clone()).await["ok"],
        true
    );
    fake.lock().unwrap().steps = plan["undo_steps"]
        .as_array()
        .unwrap()
        .iter()
        .cloned()
        .collect();
    assert_eq!(
        call(&client, "undo_operation", operation.clone()).await["ok"],
        true
    );
    stop(client, server).await;
    let (client, server) = connected(&root, &address).await;
    let history = call(
        &client,
        "get_operation",
        json!({"operation_id":plan["operation_id"]}),
    )
    .await;
    assert_eq!(history["record"]["edit_scope"], scope);
    assert_eq!(history["record"]["state"], "undone");
    assert_eq!(history["executable_plan_restored"], false);
    assert_eq!(
        call(&client, "undo_operation", operation).await["ok"],
        false
    );
    stop(client, server).await;
    bridge.abort();
    std::fs::remove_dir_all(root).unwrap();
}

#[tokio::test]
async fn scoped_inert_building_edits_use_common_physics_and_check_protected_readback() {
    let (root, fake, address, bridge) = fixture().await;
    let mut snapshot = machine();
    snapshot["blocks"] = json!([
        {"pos":{"x":101,"y":101,"z":104},"name":"minecraft:stone"},
        {"pos":{"x":102,"y":101,"z":104},"name":"minecraft:glass"}]);
    fake.lock().unwrap().snapshot = Some(snapshot.clone());
    let (client, server) = connected(&root, &address).await;
    let captured = call(
        &client,
        "get_world",
        json!({"region":{"min":snapshot["min"],"max":snapshot["max"]},"include_block_list":true}),
    )
    .await;
    assert_eq!(captured["ok"], true, "{captured}");
    let revision = call(
        &client,
        "test_circuit_change",
        json!({"circuit_id":captured["circuit_id"],"changes":[
        {"position":{"x":101,"y":101,"z":104},"block":"minecraft:smooth_quartz"}]}),
    )
    .await;
    assert_eq!(revision["ok"], true, "{revision}");
    let scope = json!({"editable":[{"min":{"x":101,"y":101,"z":104},"max":{"x":101,"y":101,"z":104}}],
        "protected":[{"min":{"x":102,"y":101,"z":104},"max":{"x":102,"y":101,"z":104}}]});
    let plan = call(
        &client,
        "new_placement",
        json!({"revision_id":revision["revision_id"],"edit_scope":scope}),
    )
    .await;
    assert_eq!(plan["ok"], true, "{plan}");
    assert_eq!(plan["kind"], "electrical_revision_modification");
    let operation = json!({"operation_id":plan["operation_id"],"confirm":true});
    fake.lock().unwrap().snapshot.as_mut().unwrap()["blocks"][1]["name"] = json!("minecraft:stone");
    assert_eq!(
        call(&client, "show_operation", operation.clone()).await["ok"],
        false
    );
    assert_eq!(fake.lock().unwrap().writes, 0);
    fake.lock().unwrap().snapshot.as_mut().unwrap()["blocks"][1]["name"] = json!("minecraft:glass");
    assert_eq!(
        call(&client, "show_operation", operation.clone()).await["ok"],
        true
    );
    fake.lock().unwrap().steps = plan["steps"].as_array().unwrap().iter().cloned().collect();
    assert_eq!(
        call(&client, "invoke_operation", operation).await["ok"],
        true
    );
    assert!(fake.lock().unwrap().snapshot.as_ref().unwrap()["blocks"].as_array().unwrap().iter().any(|b|
        b["name"]=="minecraft:glass" && b["pos"]==json!({"x":102,"y":101,"z":104})));
    stop(client, server).await;
    bridge.abort();
    std::fs::remove_dir_all(root).unwrap();
}

#[tokio::test]
async fn electrical_edit_public_capture_preview_apply_undo_and_restart_history() {
    let (root, fake, address, bridge) = fixture().await;
    let (client, server) = connected(&root, &address).await;
    let plan = propose(&client).await;
    let operation = json!({"operation_id":plan["operation_id"],"confirm":true});
    assert_eq!(
        call(&client, "invoke_operation", operation.clone()).await["ok"],
        false
    );
    assert_eq!(fake.lock().unwrap().writes, 0);
    assert_eq!(
        call(
            &client,
            "show_operation",
            json!({"operation_id":plan["operation_id"]})
        )
        .await["ok"],
        true
    );
    fake.lock().unwrap().steps = plan["steps"].as_array().unwrap().iter().cloned().collect();
    let result = call(&client, "invoke_operation", operation.clone()).await;
    assert_eq!(result["ok"], true, "{result}");
    assert_eq!(result["verified_steps"], 2);
    assert_eq!(fake.lock().unwrap().write_batches, 1);

    assert_eq!(
        call(&client, "invoke_operation", operation.clone()).await["ok"],
        false
    );
    // Undo also guards unchanged cells outside the original captured region.
    fake.lock().unwrap().snapshot.as_mut().unwrap()["blocks"]
        .as_array_mut()
        .unwrap()
        .push(json!({"pos":{"x":98,"y":99,"z":102},"name":"minecraft:stone"}));
    let drifted = call(&client, "undo_operation", operation.clone()).await;
    assert_eq!(drifted["ok"], false, "{drifted}");
    assert_eq!(fake.lock().unwrap().writes, 2);

    fake.lock().unwrap().snapshot.as_mut().unwrap()["blocks"]
        .as_array_mut()
        .unwrap()
        .pop();
    fake.lock().unwrap().steps = plan["undo_steps"]
        .as_array()
        .unwrap()
        .iter()
        .cloned()
        .collect();
    let result = call(&client, "undo_operation", operation.clone()).await;
    assert_eq!(result["ok"], true, "{result}");
    assert_eq!(result["verified_steps"], 2);
    assert_eq!(fake.lock().unwrap().writes, 4);
    assert_eq!(fake.lock().unwrap().write_batches, 2);
    stop(client, server).await;
    let (client, server) = connected(&root, &address).await;
    let history = call(
        &client,
        "get_operation",
        json!({"operation_id":plan["operation_id"]}),
    )
    .await;
    assert_eq!(history["ok"], true, "{history}");
    assert_eq!(history["record"]["state"], "undone");
    assert_eq!(history["record"]["attempts"].as_array().unwrap().len(), 2);
    assert_eq!(history["executable_plan_restored"], false);
    assert_eq!(
        call(&client, "invoke_operation", operation).await["ok"],
        false
    );
    stop(client, server).await;
    bridge.abort();
    std::fs::remove_dir_all(root).unwrap();
}

#[tokio::test]
async fn electrical_edit_stops_after_unverified_write_and_retains_inspection_history() {
    let (root, fake, address, bridge) = fixture().await;
    let (client, server) = connected(&root, &address).await;
    let plan = propose(&client).await;
    assert_eq!(
        call(
            &client,
            "show_operation",
            json!({"operation_id":plan["operation_id"]})
        )
        .await["ok"],
        true
    );
    {
        let mut state = fake.lock().unwrap();
        state.steps = plan["steps"].as_array().unwrap().iter().cloned().collect();
        state.fail_after_write = Some(1);
    }
    let operation = json!({"operation_id":plan["operation_id"],"confirm":true});
    let result = call(&client, "invoke_operation", operation.clone()).await;
    assert_eq!(result["ok"], false);
    assert_eq!(result["status"], "needs_inspection");
    assert_eq!(result["verified_steps"], 0);
    assert_eq!(fake.lock().unwrap().writes, 2);
    assert_eq!(result["failure"]["primary"]["phase"], "after_readback");
    assert_eq!(result["failure"]["progress"]["world"], "unknown");
    assert_eq!(result["failure"]["progress"]["submitted_changes"], 2);
    assert_eq!(result["recovery"]["reobserve_required"], true);
    assert_eq!(result["recovery"]["replan_required"], true);
    assert!(
        result["error"]
            .as_str()
            .unwrap()
            .contains("batch steps 1..2")
    );
    assert_eq!(
        call(&client, "invoke_operation", operation.clone()).await["ok"],
        false
    );
    assert_eq!(
        call(&client, "undo_operation", operation).await["ok"],
        false
    );
    stop(client, server).await;
    let (client, server) = connected(&root, &address).await;
    let history = call(
        &client,
        "get_operation",
        json!({"operation_id":plan["operation_id"]}),
    )
    .await;
    assert_eq!(history["record"]["state"], "needs_inspection");
    assert_eq!(history["record"]["attempts"][0]["verified_steps"], 0);
    assert_eq!(
        history["record"]["attempts"][0]["failure"]["progress"]["world"],
        "unknown"
    );

    stop(client, server).await;
    bridge.abort();
    std::fs::remove_dir_all(root).unwrap();
}

#[tokio::test]
async fn electrical_edit_partial_batch_submission_never_verifies_a_prefix_or_retries() {
    let (root, fake, address, bridge) = fixture().await;
    let (client, server) = connected(&root, &address).await;
    let plan = propose(&client).await;
    assert_eq!(
        call(
            &client,
            "show_operation",
            json!({"operation_id":plan["operation_id"]})
        )
        .await["ok"],
        true
    );
    {
        let mut state = fake.lock().unwrap();
        state.steps = plan["steps"].as_array().unwrap().iter().cloned().collect();
        state.lose_write_reply_at = Some(1);
    }
    let operation = json!({"operation_id":plan["operation_id"],"confirm":true});
    let result = call(&client, "invoke_operation", operation.clone()).await;
    assert_eq!(result["status"], "needs_inspection", "{result}");
    assert_eq!(result["verified_steps"], 0);
    assert_eq!(fake.lock().unwrap().writes, 1);
    assert_eq!(fake.lock().unwrap().write_batches, 1);
    assert_eq!(result["failure"]["primary"]["phase"], "submission");
    assert!(result["failure"]["progress"]["submitted_changes"].is_null());
    assert_eq!(result["failure"]["progress"]["verified_steps"], 0);
    assert_eq!(
        call(&client, "invoke_operation", operation.clone()).await["ok"],
        false
    );
    assert_eq!(
        call(&client, "undo_operation", operation).await["ok"],
        false
    );
    stop(client, server).await;
    let (client, server) = connected(&root, &address).await;
    let history = call(
        &client,
        "get_operation",
        json!({"operation_id":plan["operation_id"]}),
    )
    .await;
    assert_eq!(history["record"]["state"], "needs_inspection");
    assert_eq!(history["record"]["attempts"][0]["verified_steps"], 0);
    assert_eq!(
        history["record"]["attempts"][0]["failure"]["progress"]["world"],
        "unknown"
    );

    assert!(
        history["record"]["attempts"][0]["error"]
            .as_str()
            .unwrap()
            .contains("batch steps 1..2")
    );
    stop(client, server).await;
    bridge.abort();
    std::fs::remove_dir_all(root).unwrap();
}

#[tokio::test]
async fn electrical_edit_rejects_guard_drift_and_invalid_workspace_before_writes() {
    let (root, fake, address, bridge) = fixture().await;
    let (client, server) = connected(&root, &address).await;
    let invalid = call(
        &client,
        "get_world",
        json!({"region":{"min":{"x":1,"y":0,"z":0},"max":{"x":0,"y":0,"z":0}}}),
    )
    .await;
    assert_eq!(invalid["ok"], false);
    let plan = propose(&client).await;
    assert_eq!(
        call(
            &client,
            "show_operation",
            json!({"operation_id":plan["operation_id"]})
        )
        .await["ok"],
        true
    );
    fake.lock().unwrap().snapshot.as_mut().unwrap()["blocks"]
        .as_array_mut()
        .unwrap()
        .push(json!({"pos":{"x":98,"y":99,"z":102},"name":"minecraft:stone"}));
    let result = call(
        &client,
        "invoke_operation",
        json!({"operation_id":plan["operation_id"],"confirm":true}),
    )
    .await;
    assert_eq!(result["ok"], false);
    assert_eq!(fake.lock().unwrap().writes, 0);
    stop(client, server).await;
    bridge.abort();
    std::fs::remove_dir_all(root).unwrap();
}

#[tokio::test]
async fn electrical_edit_refuses_changed_target_features_and_unverified_observation() {
    for reason in ["target", "features", "readback"] {
        let (root, fake, address, bridge) = fixture().await;
        let (client, server) = connected(&root, &address).await;
        let plan = propose(&client).await;
        assert_eq!(
            call(
                &client,
                "show_operation",
                json!({"operation_id":plan["operation_id"]})
            )
            .await["ok"],
            true
        );
        {
            let mut state = fake.lock().unwrap();
            match reason {
                "target" => state.wrong_target = true,
                "features" => state.unknown_features = true,
                "readback" => state.unverified_readback = true,
                _ => unreachable!(),
            }
        }
        let result = call(
            &client,
            "invoke_operation",
            json!({"operation_id":plan["operation_id"],"confirm":true}),
        )
        .await;
        assert_eq!(result["ok"], false, "{reason}: {result}");
        assert_eq!(fake.lock().unwrap().writes, 0);
        stop(client, server).await;
        bridge.abort();
        std::fs::remove_dir_all(root).unwrap();
    }
}

#[tokio::test]
async fn electrical_edit_keeps_read_only_policy_and_requires_explicit_confirmation() {
    let (root, fake, address, bridge) = fixture().await;
    let mut service = super::DustRouteMcp::with_policy_and_player(
        &address,
        super::McpPolicy::default(),
        "Tester",
    );
    service.state_store = super::PlanStateStore::new(root.clone(), 3600);
    let (client, server) = serve(service).await;
    let plan = propose(&client).await;
    assert_eq!(
        call(
            &client,
            "show_operation",
            json!({"operation_id":plan["operation_id"]})
        )
        .await["ok"],
        true
    );
    for confirm in [false, true] {
        let result = call(
            &client,
            "invoke_operation",
            json!({"operation_id":plan["operation_id"],"confirm":confirm}),
        )
        .await;
        assert_eq!(result["ok"], false);
    }
    assert_eq!(fake.lock().unwrap().writes, 0);
    stop(client, server).await;
    bridge.abort();
    std::fs::remove_dir_all(root).unwrap();
}

#[tokio::test]
async fn electrical_edit_harvests_and_restores_a_declared_column_with_fixed_water() {
    let (root, fake, address, bridge) = fixture().await;
    let snapshot: Value = serde_json::from_str(include_str!(
        "../../../dustroute-translate/tests/fixtures/sugar-cane-static-root.json"
    ))
    .unwrap();
    {
        let mut state = fake.lock().unwrap();
        state.snapshot = Some(snapshot.clone());
        state.gaze_target = Some(json!({"x":0,"y":1,"z":0}));
    }
    let (client, server) = connected(&root, &address).await;
    let captured = call(
        &client,
        "get_world",
        json!({"region":{"min":snapshot["min"],"max":snapshot["max"]}}),
    )
    .await;
    assert_eq!(captured["ok"], true, "{captured}");
    let revision = call(
        &client,
        "test_circuit_change",
        json!({"circuit_id":captured["circuit_id"],"changes":[
            {"position":{"x":0,"y":2,"z":0},"block":"minecraft:air"},
            {"position":{"x":0,"y":3,"z":0},"block":"minecraft:air"}
        ]}),
    )
    .await;
    assert_eq!(
        revision["validation"]["electrical_modification"]["status"], "passed",
        "{revision}"
    );
    assert_eq!(
        revision["validation"]["before"]["summary"]["functional_behavior_verified"],
        false
    );
    assert_eq!(
        revision["validation"]["before"]["status"],
        "invalid_or_unsupported"
    );
    let plan = call(
        &client,
        "new_placement",
        json!({"revision_id":revision["revision_id"]}),
    )
    .await;
    assert_eq!(plan["ok"], true, "{plan}");
    assert_eq!(plan["steps"].as_array().unwrap().len(), 1);
    assert!(plan["steps"][0]["wait_ticks"].as_u64().unwrap() >= 1);
    let op = json!({"operation_id":plan["operation_id"],"confirm":true});
    assert_eq!(
        call(&client, "show_operation", op.clone()).await["ok"],
        true
    );
    fake.lock().unwrap().steps = plan["steps"].as_array().unwrap().iter().cloned().collect();
    let applied = call(&client, "invoke_operation", op.clone()).await;
    assert_eq!(applied["ok"], true, "{applied}");
    fake.lock().unwrap().steps = plan["undo_steps"]
        .as_array()
        .unwrap()
        .iter()
        .cloned()
        .collect();
    let undone = call(&client, "undo_operation", op).await;
    assert_eq!(undone["ok"], true, "{undone}");
    assert_eq!(fake.lock().unwrap().writes, 3);
    stop(client, server).await;
    bridge.abort();
    std::fs::remove_dir_all(root).unwrap();
}
