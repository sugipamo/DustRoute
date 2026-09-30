//! Public building lifecycle. The bridge is a transport stub, not live proof.
use super::blueprint_tests::door_fixture;
use super::blueprint_tests::runtime_fixture;
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
    let operation = propose_generated(client, &result).await;
    (result, operation)
}

async fn propose_generated(client: &Client, result: &Value) -> Value {
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
    proposed["operation_id"].clone()
}

fn virtual_design() -> Value {
    json!({"namespace":"public.design","name":"Room with window",
        "known_region":{"min":{"x":-1,"y":-1,"z":-1},"max":{"x":5,"y":4,"z":5}},
        "parts":[
            {"name":"shell","shapes":[{"kind":"shell","material":"stone",
                "region":{"min":{"x":0,"y":0,"z":0},"max":{"x":4,"y":3,"z":4}}}],
                "cutouts":[{"min":{"x":2,"y":1,"z":0},"max":{"x":2,"y":2,"z":0}},
                    {"min":{"x":4,"y":1,"z":2},"max":{"x":4,"y":2,"z":2}}]},
            {"name":"window","shapes":[{"kind":"blocks","material":"glass",
                "positions":[{"x":4,"y":1,"z":2},{"x":4,"y":2,"z":2}]}]}],
        "spaces":[{"name":"room","region":{"min":{"x":1,"y":1,"z":1},"max":{"x":3,"y":2,"z":3}}}]})
}

#[tokio::test]
async fn virtual_design_updates_require_adoption_retain_pins_and_leave_base_unchanged() {
    let root = temporary();
    let (client, server) = start(&root).await;
    let previous = virtual_design();
    let original = call(
        &client,
        "test_circuit_change",
        json!({"blueprint":{"action":"generate_building_design","request":previous}}),
    )
    .await;
    assert_eq!(original["ok"], true, "{original}");
    let op = propose_generated(&client, &original["result"]).await;
    let base = original["result"]["request"]["candidate_state"]["id"].clone();
    let mut design = previous.clone();
    design["namespace"] = json!("public.design.updated");
    design["parts"][1]["shapes"][0]["material"] = json!("tinted_glass");
    let request = json!({"base_assembly_revision_id":base,"previous":previous,"design":design});
    let unadopted = call(
        &client,
        "test_circuit_change",
        json!({"blueprint":{"action":"generate_building_design_update","request":request}}),
    )
    .await;
    assert_eq!(unadopted["ok"], false);
    adopt(&client, &op).await;
    let before = call(
        &client,
        "get_circuit_revision",
        json!({"blueprint":{"kind":"archive"}}),
    )
    .await;
    let generated = call(
        &client,
        "test_circuit_change",
        json!({"blueprint":{"action":"generate_building_design_update","request":request}}),
    )
    .await;
    assert_eq!(generated["ok"], true, "{generated}");
    assert_eq!(generated["result"]["request"]["base_state"], base);
    assert_eq!(
        generated["result"]["diff"]["blocks"]
            .as_array()
            .unwrap()
            .len(),
        2
    );
    assert_eq!(
        generated["result"]["retained_revisions"]["shell"],
        "public.design.shell.v1"
    );
    assert_eq!(generated["result"]["placed_instances_modified"], false);
    assert!(
        generated["result"]["records"]["assemblies"]
            .as_array()
            .unwrap()
            .is_empty()
    );
    assert_eq!(
        generated["result"]["records"]["types"]
            .as_array()
            .unwrap()
            .len(),
        2
    );
    assert_eq!(
        before,
        call(
            &client,
            "get_circuit_revision",
            json!({"blueprint":{"kind":"archive"}})
        )
        .await
    );
    let mut wrong = request.clone();
    wrong["previous"]["spaces"] = json!([]);
    let rejected = call(
        &client,
        "test_circuit_change",
        json!({"blueprint":{"action":"generate_building_design_update","request":wrong}}),
    )
    .await;
    assert_eq!(rejected["ok"], false);
    assert_eq!(rejected["errors"][0]["code"], "base_design_mismatch");
    let op = propose_generated(&client, &generated["result"]).await;
    stop(client, server).await;
    let (client, server) = start(&root).await;
    adopt(&client, &op).await;
    let old = call(
        &client,
        "get_circuit_revision",
        json!({"blueprint":{"kind":"assembly","id":base}}),
    )
    .await;
    assert_eq!(
        old["result"]["record"]["assembly"],
        original["result"]["request"]["candidate_state"]["assembly"]
    );
    stop(client, server).await;
    std::fs::remove_dir_all(root).unwrap();
}

#[tokio::test]
async fn virtual_design_returns_structured_combined_world_failures_without_publishing() {
    let root = temporary();
    let (client, server) = start(&root).await;
    let f = runtime_fixture::fixture(false, false);
    let source = json!({"records":{"types":f.catalog.type_revisions().collect::<Vec<_>>(),
        "revisions":f.catalog.revisions().collect::<Vec<_>>(),"assemblies":[f.base]},"request":f.request});
    let proposal = propose_generated(&client, &source).await;
    adopt(&client, &proposal).await;
    let before = call(
        &client,
        "get_circuit_revision",
        json!({"blueprint":{"kind":"archive"}}),
    )
    .await;
    let generated=call(&client,"test_circuit_change",json!({"blueprint":{"action":"generate_building_design","request":{
        "namespace":"public.bad-motion","name":"Insufficient motion allocation",
        "known_region":f.context.known_region,
        "parts":[{"name":"marker","shapes":[{"kind":"blocks","material":"stone","positions":[{"x":2,"y":1,"z":0}]}]}],
        "component":{"name":"engine","assembly_revision_id":f.request.candidate_state.id,
            "source_anchor":{"x":0,"y":0,"z":0},"target_anchor":{"x":0,"y":0,"z":0},"rotation":"r0",
            "reserved_space":{"min":{"x":-1,"y":0,"z":0},"max":{"x":1,"y":1,"z":0}}}
    }}})).await;
    assert_eq!(generated["ok"], false, "{generated}");
    assert!(generated.get("result").is_none());
    assert_eq!(
        generated["errors"][0]["code"],
        "verification_not_established"
    );
    let diagnostics = &generated["errors"][0]["diagnostics"];
    assert_eq!(diagnostics["status"], "failed");
    let marker = diagnostics["findings"]
        .as_array()
        .unwrap()
        .iter()
        .find(|f| {
            f["evidence"]["type_revision"] == "public.bad-motion.marker.pattern.v1"
                && f["status"] == "failed"
        })
        .unwrap();
    assert_eq!(marker["evidence"]["position"], json!({"x":2,"y":1,"z":0}));
    assert!(marker["instance"].is_array());
    assert_eq!(
        marker["evidence"]["observation"]["stage"],
        "committed_runtime_state"
    );
    assert!(
        marker["evidence"]["inputs"]
            .as_array()
            .unwrap()
            .iter()
            .any(|i| i["powered"] == true)
    );
    assert_eq!(
        before,
        call(
            &client,
            "get_circuit_revision",
            json!({"blueprint":{"kind":"archive"}})
        )
        .await
    );
    stop(client, server).await;
}

#[tokio::test]
async fn virtual_design_components_require_unique_adoption_and_keep_pinned_requirements() {
    let root = temporary();
    let (client, server) = start(&root).await;
    let (source, proposal) = generate_and_propose(&client).await;
    let generate = json!({"blueprint":{"action":"generate_building_design","request":{
        "namespace":"public.composition","name":"Two structures",
        "known_region":{"min":{"x":-1,"y":-1,"z":-1},"max":{"x":12,"y":4,"z":5}},
        "parts":[{"name":"marker","shapes":[{"kind":"blocks","material":"glass","positions":[{"x":0,"y":0,"z":0}]}]}],
        "component":{"name":"room","assembly_revision_id":source["request"]["candidate_state"]["id"],
            "source_anchor":{"x":0,"y":0,"z":0},"target_anchor":{"x":7,"y":0,"z":0},"rotation":"r0",
            "reserved_space":{"min":{"x":0,"y":0,"z":0},"max":{"x":4,"y":3,"z":4}}}
    }}});
    let rejected = call(&client, "test_circuit_change", generate.clone()).await;
    assert_eq!(rejected["ok"], false);
    assert_eq!(
        rejected["errors"][0]["code"],
        "source_not_adopted_or_reviewable"
    );
    adopt(&client, &proposal).await;
    let before = call(
        &client,
        "get_circuit_revision",
        json!({"blueprint":{"kind":"archive"}}),
    )
    .await;
    let generated = call(&client, "test_circuit_change", generate).await;
    assert_eq!(generated["ok"], true, "{generated}");
    assert_eq!(generated["result"]["unique_blocks"], 81);
    assert_eq!(
        generated["result"]["component"]["origin"],
        json!({"x":7,"y":0,"z":0})
    );
    assert_eq!(
        before,
        call(
            &client,
            "get_circuit_revision",
            json!({"blueprint":{"kind":"archive"}})
        )
        .await
    );
    let proposal = propose_generated(&client, &generated["result"]).await;
    stop(client, server).await;
    let (client, server) = start(&root).await;
    adopt(&client, &proposal).await;
    stop(client, server).await;
}

#[tokio::test]
async fn virtual_design_error_correction_adoption_restart_placement_and_removal() {
    let root = temporary();
    let (fake, address, bridge) = start_construction_bridge(root.join("assembly-instances")).await;
    let (client, server) = start(&root).await;
    let before = call(
        &client,
        "get_circuit_revision",
        json!({"blueprint":{"kind":"archive"}}),
    )
    .await;
    let mut broken = virtual_design();
    broken["parts"][0]["cutouts"].as_array_mut().unwrap().pop();
    let error = call(
        &client,
        "test_circuit_change",
        json!({"blueprint":{"action":"generate_building_design","request":broken}}),
    )
    .await;
    assert_eq!(error["ok"], false, "{error}");
    assert_eq!(error["errors"][0]["code"], "material_conflict");
    assert_eq!(error["errors"][0]["item"], "window");
    assert_eq!(error["errors"][0]["position"], json!({"x":4,"y":1,"z":2}));
    assert!(error.get("result").is_none());
    let generated = call(
        &client,
        "test_circuit_change",
        json!({"blueprint":{"action":"generate_building_design","request":virtual_design()}}),
    )
    .await;
    assert_eq!(generated["ok"], true, "{generated}");
    for flag in ["writes_minecraft", "catalog_changed", "adoption_authorized"] {
        assert_eq!(generated[flag], false);
    }
    assert_eq!(generated["result"]["unique_blocks"], 80);
    assert_eq!(
        generated["result"]["verification"]["live_world_verified"],
        false
    );
    assert_eq!(
        before,
        call(
            &client,
            "get_circuit_revision",
            json!({"blueprint":{"kind":"archive"}})
        )
        .await
    );
    assert_eq!(fake.lock().unwrap().writes, 0);
    let result = generated["result"].clone();
    let proposal = propose_generated(&client, &result).await;
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
    preview(&client, &plan).await;
    {
        let mut state = fake.lock().unwrap();
        state.snapshot = Some(
            json!({"min":plan["bounds"]["min"],"max":plan["bounds"]["max"],
            "blocks":[{"pos":plan["bounds"]["min"],"name":"minecraft:stone"}]}),
        );
    }
    assert_eq!(
        apply(&client, &plan).await["ok"],
        false,
        "occupied air guard must prevent writes"
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
    stop(client, server).await;
    let (client, server) = connected(&root, &address).await;
    let diagnosis = call(
        &client,
        "manage_assembly",
        json!({"action":"diagnose","instance_id":plan["operation_id"]}),
    )
    .await;
    assert_eq!(
        diagnosis["diagnosis"]["status"], "matches_reference",
        "{diagnosis}"
    );
    let removal = call(
        &client,
        "manage_assembly",
        json!({"action":"plan_removal","instance_id":plan["operation_id"]}),
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
    let _ = bridge.await;
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
async fn building_with_adopted_door_uses_common_public_placement_and_removal() {
    let root = temporary();
    let (fake, address, bridge) = start_construction_bridge(root.join("assembly-instances")).await;
    let (client, server) = start(&root).await;
    let f = door_fixture::ordinary_fixture();
    let generate = json!({"blueprint":{"action":"generate_building_with_door","request":{
        "building":{"namespace":"public.door-house","width":9,"depth":3,"height":8,
            "entrance":{"offset":3,"width":3,"height":3}},
        "door":{"assembly_revision_id":f.request.candidate_state.id,"instance":["root","mechanism"],
            "behavior_type":"dustroute.type.piston-door-3x3.v1","rotation":"r270",
            "reserved_space":{"min":{"x":0,"y":2,"z":-3},"max":{"x":0,"y":11,"z":3}}}
    }}});
    assert_eq!(
        call(&client, "test_circuit_change", generate.clone()).await["ok"],
        false,
        "a source without unique adoption is refused"
    );
    let imported = call(
        &client,
        "test_circuit_change",
        json!({"blueprint":{"action":"import","records":{
            "types":f.catalog.type_revisions().collect::<Vec<_>>(),
            "revisions":f.catalog.revisions().collect::<Vec<_>>(),"assemblies":[f.base]
        }}}),
    )
    .await;
    assert_eq!(imported["ok"], true, "{imported}");
    let mut source_request = serde_json::to_value(f.request).unwrap();
    source_request.as_object_mut().unwrap().remove("id");
    let source_proposal = call(
        &client,
        "test_circuit_change",
        json!({"blueprint":{"action":"propose_update","request":source_request}}),
    )
    .await;
    assert_eq!(source_proposal["ok"], true, "{source_proposal}");
    assert_eq!(
        call(&client, "test_circuit_change", generate.clone()).await["ok"],
        false,
        "a pending source proposal is not adoption"
    );
    adopt(&client, &source_proposal["operation_id"]).await;
    let before = call(
        &client,
        "get_circuit_revision",
        json!({"blueprint":{"kind":"archive"}}),
    )
    .await;
    let generated = call(&client, "test_circuit_change", generate).await;
    assert_eq!(generated["ok"], true, "{generated}");
    assert_eq!(generated["writes_minecraft"], false);
    assert_eq!(generated["catalog_changed"], false);
    assert_eq!(generated["adoption_authorized"], false);
    assert_eq!(
        generated["result"]["verification"]["live_world_verified"],
        false
    );
    assert_eq!(
        generated["result"]["expected"]["blocks"]
            .as_array()
            .unwrap()
            .len(),
        168
    );
    assert_eq!(
        generated["result"]["door"]["control"],
        json!({"x":4,"y":6,"z":0})
    );
    assert_eq!(
        before,
        call(
            &client,
            "get_circuit_revision",
            json!({"blueprint":{"kind":"archive"}})
        )
        .await
    );
    let result = generated["result"].clone();
    assert_eq!(
        call(
            &client,
            "test_circuit_change",
            json!({"blueprint":{"action":"import","records":result["records"]}})
        )
        .await["ok"],
        true
    );
    let mut request = result["request"].clone();
    request.as_object_mut().unwrap().remove("id");
    let proposal = call(
        &client,
        "test_circuit_change",
        json!({"blueprint":{"action":"propose_update","request":request}}),
    )
    .await;
    assert_eq!(proposal["ok"], true, "{proposal}");
    stop(client, server).await;
    let (client, server) = connected(&root, &address).await;
    adopt(&client, &proposal["operation_id"]).await;
    let plan = call(&client, "new_placement", target(&result)).await;
    assert_eq!(plan["ok"], true, "{plan}");
    assert_eq!(plan["construction_steps"].as_array().unwrap().len(), 168);
    preview(&client, &plan).await;
    fake.lock().unwrap().steps = plan["construction_steps"]
        .as_array()
        .unwrap()
        .iter()
        .cloned()
        .collect();
    let applied = apply(&client, &plan).await;
    assert_eq!(applied["ok"], true, "{applied}");
    assert_eq!(applied["verified_steps"], 168);
    let writes = fake.lock().unwrap().writes;
    stop(client, server).await;
    let (client, server) = connected(&root, &address).await;
    let instance = plan["operation_id"].clone();
    let diagnosis = call(
        &client,
        "manage_assembly",
        json!({"action":"diagnose","instance_id":instance}),
    )
    .await;
    assert_eq!(
        diagnosis["diagnosis"]["status"], "matches_reference",
        "{diagnosis}"
    );
    assert_eq!(
        fake.lock().unwrap().writes,
        writes,
        "diagnosis must not write"
    );
    let removal = call(
        &client,
        "manage_assembly",
        json!({"action":"plan_removal","instance_id":instance}),
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
    let _ = bridge.await;
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
