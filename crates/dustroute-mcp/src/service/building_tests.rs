//! Public building lifecycle. The bridge is a transport stub, not live proof.
use super::test_support::*;
use serde_json::{Value, json};

async fn generate_and_propose(client: &Client) -> (Value, Value) {
    let before = call(
        client,
        "get_circuit_revision",
        json!({"blueprint":{"kind":"archive"}}),
    )
    .await;
    let generated = call(
        client,
        "test_circuit_change",
        json!({"blueprint":{"action":"generate_building","request":{
            "namespace":"public.enclosure","width":5,"depth":5,"height":4,"wall_material":"glass"
        }}}),
    )
    .await;
    assert_eq!(generated["ok"], true, "{generated}");
    assert_eq!(generated["writes_minecraft"], false);
    assert_eq!(generated["catalog_changed"], false);
    assert_eq!(generated["adoption_authorized"], false);
    assert_eq!(generated["result"]["verification"]["status"], "passed");
    assert_eq!(
        generated["result"]["verification"]["live_world_verified"],
        false
    );
    let after = call(
        client,
        "get_circuit_revision",
        json!({"blueprint":{"kind":"archive"}}),
    )
    .await;
    assert_eq!(before, after, "generation must not publish definitions");
    let result = generated["result"].clone();
    let imported = call(
        client,
        "test_circuit_change",
        json!({"blueprint":{"action":"import","records":result["records"]}}),
    )
    .await;
    assert_eq!(imported["ok"], true, "{imported}");
    let mut request = result["request"].clone();
    request.as_object_mut().unwrap().remove("id");
    let proposed = call(
        client,
        "test_circuit_change",
        json!({"blueprint":{"action":"propose_update","request":request}}),
    )
    .await;
    assert_eq!(proposed["ok"], true, "{proposed}");
    (result, proposed["operation_id"].clone())
}

fn target(result: &Value) -> Value {
    json!({"assembly_revision_id":result["request"]["candidate_state"]["id"],"assembly_target":{
        "source_anchor":{"x":0,"y":0,"z":0},"target_anchor":{"x":-96,"y":180,"z":1000},"rotation":"r90"
    }})
}
async fn adopt(client: &Client, id: &Value) {
    let shown = call(client, "show_operation", json!({"operation_id":id})).await;
    assert_eq!(shown["can_adopt"], true, "{shown}");
    let adopted = call(
        client,
        "invoke_operation",
        json!({"operation_id":id,"confirm":true,"blueprint_decision":{"action":"adopt"}}),
    )
    .await;
    assert_eq!(adopted["ok"], true, "{adopted}");
    assert_eq!(adopted["writes_minecraft"], false);
}
async fn preview(client: &Client, plan: &Value) {
    assert_eq!(
        call(
            client,
            "show_operation",
            json!({"operation_id":plan["operation_id"]})
        )
        .await["ok"],
        true
    );
}
async fn apply(client: &Client, plan: &Value) -> Value {
    call(
        client,
        "invoke_operation",
        json!({"operation_id":plan["operation_id"],"confirm":true}),
    )
    .await
}

#[tokio::test]
async fn building_generation_adoption_batched_placement_diagnosis_repair_and_removal() {
    let root = temporary();
    let (fake, address, bridge) = start_construction_bridge(root.join("assembly-instances")).await;
    let (client, server) = start(&root).await;
    let (result, proposal) = generate_and_propose(&client).await;
    stop(client, server).await;
    let (client, server) = connected(&root, &address).await;
    assert_eq!(
        call(&client, "new_placement", target(&result)).await["ok"],
        false
    );
    adopt(&client, &proposal).await;
    let plan = call(&client, "new_placement", target(&result)).await;
    assert_eq!(plan["ok"], true, "{plan}");
    assert_eq!(plan["construction_steps"].as_array().unwrap().len(), 80);
    assert_eq!(plan["execution_batches"].as_array().unwrap().len(), 3);
    assert_eq!(
        apply(&client, &plan).await["ok"],
        false,
        "preview gate is required"
    );
    preview(&client, &plan).await;
    {
        let mut state = fake.lock().unwrap();
        state.snapshot = Some(
            json!({"min":plan["bounds"]["min"],"max":plan["bounds"]["max"],"blocks":[{"pos":plan["bounds"]["min"],"name":"minecraft:stone"}]}),
        );
    }
    assert_eq!(
        apply(&client, &plan).await["ok"],
        false,
        "occupied guard must block writes"
    );
    assert_eq!(fake.lock().unwrap().writes, 0);
    {
        let mut state = fake.lock().unwrap();
        state.snapshot = None;
        state.steps = plan["construction_steps"]
            .as_array()
            .unwrap()
            .iter()
            .cloned()
            .collect();
    }
    let applied = apply(&client, &plan).await;
    assert_eq!(applied["ok"], true, "{applied}");
    assert_eq!(applied["verified_steps"], 80);
    assert_eq!(fake.lock().unwrap().write_batches, 3);
    let settled = fake.lock().unwrap().snapshot.clone().unwrap();
    stop(client, server).await;
    let (client, server) = connected(&root, &address).await;
    let id = plan["operation_id"].clone();
    let matched = call(
        &client,
        "manage_assembly",
        json!({"action":"diagnose","instance_id":id}),
    )
    .await;
    assert_eq!(
        matched["diagnosis"]["status"], "matches_reference",
        "{matched}"
    );
    let missing = fake.lock().unwrap().snapshot.as_mut().unwrap()["blocks"]
        .as_array_mut()
        .unwrap()
        .remove(0)["pos"]
        .clone();
    let damaged = call(
        &client,
        "manage_assembly",
        json!({"action":"diagnose","instance_id":id}),
    )
    .await;
    assert_eq!(
        damaged["diagnosis"]["summary"]["by_kind"]["missing"], 1,
        "{damaged}"
    );
    assert!(
        damaged["diagnosis"]["findings"]
            .as_array()
            .unwrap()
            .iter()
            .any(|f| f["position"] == missing)
    );
    assert_eq!(fake.lock().unwrap().writes, 80, "diagnosis is read-only");
    let repair = call(
        &client,
        "manage_assembly",
        json!({"action":"plan_reconstruction","instance_id":id}),
    )
    .await;
    assert_eq!(repair["ok"], true, "{repair}");
    preview(&client, &repair).await;
    fake.lock().unwrap().steps = repair["reconstruction"]["steps"]
        .as_array()
        .unwrap()
        .iter()
        .cloned()
        .collect();
    let repaired = apply(&client, &repair).await;
    assert_eq!(repaired["ok"], true, "{repaired}");
    assert_eq!(fake.lock().unwrap().snapshot.as_ref(), Some(&settled));
    let removal = call(
        &client,
        "manage_assembly",
        json!({"action":"plan_removal","instance_id":id}),
    )
    .await;
    assert_eq!(removal["ok"], true, "{removal}");
    preview(&client, &removal).await;
    fake.lock().unwrap().steps = removal["removal_steps"]
        .as_array()
        .unwrap()
        .iter()
        .cloned()
        .collect();
    let removed = apply(&client, &removal).await;
    assert_eq!(removed["ok"], true, "{removed}");
    assert!(
        fake.lock().unwrap().snapshot.as_ref().unwrap()["blocks"]
            .as_array()
            .unwrap()
            .is_empty()
    );
    stop(client, server).await;
    bridge.abort();
    std::fs::remove_dir_all(root).unwrap();
}

#[tokio::test]
async fn building_partial_batch_is_diagnosed_and_replanned_after_restart() {
    let root = temporary();
    let (fake, address, bridge) = start_construction_bridge(root.join("assembly-instances")).await;
    let (client, server) = connected(&root, &address).await;
    let (result, proposal) = generate_and_propose(&client).await;
    adopt(&client, &proposal).await;
    let plan = call(&client, "new_placement", target(&result)).await;
    assert_eq!(plan["ok"], true, "{plan}");
    preview(&client, &plan).await;
    {
        let mut state = fake.lock().unwrap();
        state.steps = plan["construction_steps"]
            .as_array()
            .unwrap()
            .iter()
            .cloned()
            .collect();
        state.lose_write_reply_at = Some(1);
    }
    let interrupted = apply(&client, &plan).await;
    assert_eq!(interrupted["status"], "needs_inspection", "{interrupted}");
    assert_eq!(interrupted["verified_steps"], 0);
    assert_eq!(fake.lock().unwrap().writes, 1);
    stop(client, server).await;
    let (client, server) = connected(&root, &address).await;
    fake.lock().unwrap().lose_write_reply_at = None;
    let id = plan["operation_id"].clone();
    let history = call(
        &client,
        "manage_assembly",
        json!({"action":"get","instance_id":id}),
    )
    .await;
    assert_eq!(history["instance"]["state"], "needs_inspection");
    assert_eq!(history["instance"]["attempts"][0]["verified_steps"], 0);
    assert_eq!(apply(&client, &plan).await["ok"], false);
    assert_eq!(
        call(
            &client,
            "undo_operation",
            json!({"operation_id":id,"confirm":true})
        )
        .await["ok"],
        false
    );
    let diagnosis = call(
        &client,
        "manage_assembly",
        json!({"action":"diagnose","instance_id":id}),
    )
    .await;
    assert_eq!(
        diagnosis["diagnosis"]["status"], "differences_found",
        "{diagnosis}"
    );
    assert_eq!(diagnosis["diagnosis"]["summary"]["by_kind"]["missing"], 79);
    let repair = call(
        &client,
        "manage_assembly",
        json!({"action":"plan_reconstruction","instance_id":id}),
    )
    .await;
    assert_eq!(repair["ok"], true, "{repair}");
    preview(&client, &repair).await;
    fake.lock().unwrap().steps = repair["reconstruction"]["steps"]
        .as_array()
        .unwrap()
        .iter()
        .cloned()
        .collect();
    let repaired = apply(&client, &repair).await;
    assert_eq!(repaired["ok"], true, "{repaired}");
    stop(client, server).await;
    let (client, server) = connected(&root, &address).await;
    let observed = call(
        &client,
        "manage_assembly",
        json!({"action":"observe","instance_id":id}),
    )
    .await;
    assert_eq!(observed["instance"]["state"], "applied");
    assert_eq!(observed["observation"]["status"], "matches");
    let attempts = observed["instance"]["attempts"].as_array().unwrap();
    assert_eq!(attempts.len(), 2);
    assert_eq!(attempts[0]["verified_steps"], 0);
    assert_eq!(attempts[1]["verified_steps"], 81);
    stop(client, server).await;
    bridge.abort();
    std::fs::remove_dir_all(root).unwrap();
}

#[tokio::test]
async fn building_invalid_request_does_not_publish_or_contact_the_world() {
    use rmcp::model::{CallToolRequestParams, ContentBlock};
    let root = temporary();
    let (client, server) = start(&root).await;
    let before = call(
        &client,
        "get_circuit_revision",
        json!({"blueprint":{"kind":"archive"}}),
    )
    .await;
    let result = call(
        &client,
        "test_circuit_change",
        json!({"blueprint":{"action":"generate_building","request":{
            "namespace":"bad","width":16,"depth":16,"height":16
        }}}),
    )
    .await;
    assert_eq!(result["ok"], false, "{result}");
    // Unknown typed materials are rejected by the MCP parameter decoder, before
    // the handler can return its ordinary JSON diagnostics or author records.
    let invalid = json!({"blueprint":{"action":"generate_building","request":{
        "namespace":"bad","width":5,"depth":5,"height":4,"wall_material":"oak_planks"
    }}});
    let rejected = client
        .call_tool(
            CallToolRequestParams::new("test_circuit_change")
                .with_arguments(invalid.as_object().unwrap().clone()),
        )
        .await
        .unwrap();
    assert_eq!(rejected.is_error, Some(true));
    let ContentBlock::Text(reason) = &rejected.content[0] else {
        panic!("expected parameter diagnostics")
    };
    assert!(reason.text.contains("unknown variant `oak_planks`"));
    let after = call(
        &client,
        "get_circuit_revision",
        json!({"blueprint":{"kind":"archive"}}),
    )
    .await;
    assert_eq!(before, after);
    let catalog = call(
        &client,
        "get_circuit_revision",
        json!({"blueprint":{"kind":"catalog"}}),
    )
    .await;
    assert_eq!(catalog["ok"], true);
    assert_eq!(catalog["result"]["assemblies"], json!([]));
    stop(client, server).await;
    std::fs::remove_dir_all(root).unwrap();
}
