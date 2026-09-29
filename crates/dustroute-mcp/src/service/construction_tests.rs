//! Each scenario starts with independent persisted state and an offline transport.
//! The final case retains the complete interrupt/restart/reconstruct lifecycle.
use super::*;
use crate::service::test_support::{Client, Fake, start_construction_bridge};
use std::{
    path::PathBuf,
    sync::{Arc, Mutex},
};
use tokio::task::JoinHandle;
struct Case {
    root: PathBuf,
    fake: Arc<Mutex<Fake>>,
    address: String,
    bridge: JoinHandle<()>,
    client: Client,
    server: JoinHandle<()>,
    target: Value,
}
struct AppliedCase {
    case: Case,
    planned: Value,
    settled: Value,
}
async fn adopted_case() -> Case {
    let root = temporary();
    let (fake, address, bridge) = start_construction_bridge(root.join("assembly-instances")).await;
    let f = runtime_fixture::electrical_fixture(false);
    let (client, server) = connected(&root, &address).await;
    let imported = call(&client,"test_circuit_change",json!({"blueprint":{"action":"import","records":{
        "types":f.catalog.type_revisions().collect::<Vec<_>>(),"classifications":f.catalog.classifications().collect::<Vec<_>>(),
        "revisions":f.catalog.revisions().collect::<Vec<_>>(),"assemblies":[f.base]}}})).await;
    assert_eq!(imported["ok"], true, "{imported}");
    let target = json!({"assembly_revision_id":f.request.candidate_state.id,"assembly_target":{"source_anchor":{"x":0,"y":0,"z":0},"target_anchor":{"x":-96,"y":180,"z":1000},"rotation":"r90"}});
    assert_eq!(
        call(&client, "new_placement", target.clone()).await["ok"],
        false
    );
    let mut request = serde_json::to_value(f.request).unwrap();
    request.as_object_mut().unwrap().remove("id");
    request["behavior_context"] = json!({"piston": {
        "known_region":f.context.known_region, "input_levers":f.context.input_levers,
        "root_limits":f.context.root_limits
    }});
    let proposed = call(
        &client,
        "test_circuit_change",
        json!({"blueprint":{"action":"propose_update","request":request}}),
    )
    .await;
    assert_eq!(proposed["ok"], true, "{proposed}");
    let adopted = call(&client,"invoke_operation",json!({"operation_id":proposed["operation_id"],"confirm":true,"blueprint_decision":{"action":"adopt"}})).await;
    assert_eq!(adopted["ok"], true, "{adopted}");
    assert_eq!(
        adopted["operation"]["request"]["behavior_context"],
        serde_json::to_value(&f.context).unwrap()
    );
    stop(client, server).await;
    let (client, server) = connected(&root, &address).await;
    Case {
        root,
        fake,
        address,
        bridge,
        client,
        server,
        target,
    }
}
async fn applied_case() -> AppliedCase {
    let Case {
        root,
        fake,
        address,
        bridge,
        client,
        server,
        target,
    } = adopted_case().await;
    let planned = call(&client, "new_placement", target.clone()).await;
    assert_eq!(planned["ok"], true, "{planned}");
    let id = planned["operation_id"].clone();
    assert_eq!(
        call(&client, "show_operation", json!({"operation_id":id})).await["ok"],
        true
    );
    fake.lock().unwrap().steps = planned["construction_steps"]
        .as_array()
        .unwrap()
        .iter()
        .cloned()
        .collect();
    let result = call(
        &client,
        "invoke_operation",
        json!({"operation_id":id,"confirm":true}),
    )
    .await;
    assert_eq!(result["ok"], true, "{result}");
    let settled = fake.lock().unwrap().snapshot.clone().unwrap();
    AppliedCase {
        case: Case {
            root,
            fake,
            address,
            bridge,
            client,
            server,
            target,
        },
        planned,
        settled,
    }
}

#[tokio::test]
async fn construction_checks_adoption_preview_baseline_and_observation_before_writing() {
    let Case {
        root,
        fake,
        address: _,
        bridge,
        client,
        server,
        target,
    } = adopted_case().await;
    fake.lock().unwrap().unknown_features = true;
    assert_eq!(
        call(&client, "new_placement", target.clone()).await["ok"],
        false
    );
    fake.lock().unwrap().unknown_features = false;
    let planned = call(&client, "new_placement", target.clone()).await;
    assert_eq!(planned["ok"], true, "{planned}");
    assert_eq!(
        planned["execution_context"]["profile"],
        "dustroute.piston-electrical-root-exploration.v18"
    );
    let id = planned["operation_id"].clone();
    assert_eq!(
        call(
            &client,
            "invoke_operation",
            json!({"operation_id":id,"confirm":true})
        )
        .await["ok"],
        false
    );
    assert_eq!(
        call(&client, "show_operation", json!({"operation_id":id})).await["ok"],
        true
    );
    let empty = json!({"min":planned["bounds"]["min"],"max":planned["bounds"]["max"],"blocks":[]});
    let mut dirty = empty.clone();
    dirty["blocks"] =
        json!([{"pos":planned["bounds"]["min"],"name":"minecraft:stone","properties":{}}]);
    fake.lock().unwrap().snapshot = Some(dirty);
    assert_eq!(
        call(
            &client,
            "invoke_operation",
            json!({"operation_id":id,"confirm":true})
        )
        .await["ok"],
        false
    );
    fake.lock().unwrap().snapshot = Some(empty);
    fake.lock().unwrap().partial = true;
    assert_eq!(
        call(
            &client,
            "invoke_operation",
            json!({"operation_id":id,"confirm":true})
        )
        .await["ok"],
        false
    );
    fake.lock().unwrap().partial = false;
    fake.lock().unwrap().unverified_readback = true;
    let refused = call(
        &client,
        "invoke_operation",
        json!({"operation_id":id,"confirm":true}),
    )
    .await;
    assert_eq!(
        refused["ok"], false,
        "client-only observation must not authorize writes: {refused}"
    );
    fake.lock().unwrap().unverified_readback = false;
    assert_eq!(fake.lock().unwrap().writes, 0);
    fake.lock().unwrap().steps = planned["construction_steps"]
        .as_array()
        .unwrap()
        .iter()
        .cloned()
        .collect();
    let applied = call(
        &client,
        "invoke_operation",
        json!({"operation_id":id,"confirm":true}),
    )
    .await;
    assert_eq!(applied["ok"], true, "{applied}");
    assert_eq!(
        applied["verified_steps"],
        planned["construction_steps"].as_array().unwrap().len()
    );
    fake.lock().unwrap().partial = true;
    assert_eq!(
        call(
            &client,
            "undo_operation",
            json!({"operation_id":id,"confirm":true})
        )
        .await["ok"],
        false
    );
    fake.lock().unwrap().partial = false;
    stop(client, server).await;
    bridge.abort();
    fs::remove_dir_all(root).unwrap();
}

#[tokio::test]
async fn diagnosis_reports_human_damage_and_incomplete_evidence_without_writes() {
    let AppliedCase {
        case:
            Case {
                root,
                fake,
                address,
                bridge,
                client,
                server,
                target,
            },
        planned,
        settled,
    } = applied_case().await;
    let id = planned["operation_id"].clone();
    stop(client, server).await;
    let (client, server) = connected(&root, &address).await;
    let list = call(&client, "manage_assembly", json!({"action":"list"})).await;
    assert_eq!(list["instances"].as_array().unwrap().len(), 1);
    assert_eq!(list["instances"][0]["instance_id"], id);
    assert_eq!(list["instances"][0]["state"], "applied");
    let get = call(
        &client,
        "manage_assembly",
        json!({"action":"get","instance_id":id}),
    )
    .await;
    assert_eq!(get["expected_snapshot"], settled);
    assert_eq!(get["fresh_observation"], false);
    assert_eq!(
        get["instance"]["attempts"][0]["readbacks"]
            .as_array()
            .unwrap()
            .len(),
        1 + 2 * planned["construction_steps"].as_array().unwrap().len()
    );
    // An unrelated catalog append must not invalidate pinned definitions.
    let mut extra = get["pinned_source"]["record"].clone();
    extra["id"] = json!("runtime-test.unrelated-instance.v1");
    assert_eq!(
        call(
            &client,
            "test_circuit_change",
            json!({"blueprint":{"action":"import","records":{"assemblies":[extra]}}})
        )
        .await["ok"],
        true
    );
    let matched = call(
        &client,
        "manage_assembly",
        json!({"action":"observe","instance_id":id}),
    )
    .await;
    assert_eq!(matched["observation"]["status"], "matches", "{matched}");
    assert_eq!(matched["observation"]["revalidation"]["status"], "passed");
    assert_eq!(matched["observation"]["removal_eligible"], true);
    let writes_before_diagnosis = fake.lock().unwrap().writes;
    let diagnosis = call(
        &client,
        "manage_assembly",
        json!({"action":"diagnose","instance_id":id}),
    )
    .await;
    assert_eq!(
        diagnosis["diagnosis"]["status"], "matches_reference",
        "{diagnosis}"
    );
    assert_eq!(
        diagnosis["diagnosis"]["reference"]["mode"],
        "observed_inputs"
    );
    assert_eq!(
        diagnosis["diagnosis"]["repair"]["status"],
        "not_needed_for_reference_match"
    );
    // Human changes have no failed tool attempt. Unsupported added material must
    // not hide the precise missing/configuration/orientation findings.
    let mut human_damage = settled.clone();
    let blocks = human_damage["blocks"].as_array_mut().unwrap();
    let missing_index = blocks
        .iter()
        .position(|b| b["name"] == "minecraft:stone")
        .unwrap();
    let missing_position = blocks.remove(missing_index)["pos"].clone();
    let body = blocks
        .iter_mut()
        .find(|b| b["name"] == "minecraft:sticky_piston")
        .unwrap();
    let facing = body["properties"]["facing"].as_str().unwrap();
    body["properties"]["facing"] = json!(if facing == "north" { "south" } else { "north" });
    let repeater = blocks
        .iter_mut()
        .find(|b| b["name"] == "minecraft:repeater")
        .unwrap();
    let delay = repeater["properties"]["delay"].as_str().unwrap();
    repeater["properties"]["delay"] = json!(if delay == "1" { "2" } else { "1" });
    blocks.push(json!({"pos":planned["bounds"]["min"],"name":"minecraft:chest","properties":{"facing":"north","type":"single","waterlogged":"false"}}));
    fake.lock().unwrap().snapshot = Some(human_damage);
    let diagnosis = call(
        &client,
        "manage_assembly",
        json!({"action":"diagnose","instance_id":id}),
    )
    .await;
    let report = &diagnosis["diagnosis"];
    assert_eq!(report["status"], "differences_found", "{diagnosis}");
    assert_eq!(report["summary"]["by_kind"]["missing"], 1);
    assert_eq!(report["summary"]["by_kind"]["unexpected"], 1);
    assert_eq!(report["summary"]["by_kind"]["orientation"], 1);
    assert_eq!(report["summary"]["by_kind"]["configuration"], 1);
    assert!(
        report["findings"]
            .as_array()
            .unwrap()
            .iter()
            .any(|f| f["position"] == missing_position && f["observed"].is_null())
    );
    assert_eq!(report["repair"]["status"], "blocked");
    let shared: dustroute_translate::diagnostic::report::Diagnosis =
        serde_json::from_value(report.clone()).unwrap();
    assert_eq!(
        shared.method,
        dustroute_translate::diagnostic::report::DiagnosticMethod::DesignComparison
    );
    assert_eq!(
        shared.repair.strategy,
        dustroute_translate::diagnostic::report::RepairStrategy::TeardownAndRebuild
    );
    assert!(!shared.repair.permission_granted);
    assert_eq!(
        serde_json::to_value(&shared.design.as_ref().unwrap().assembly_revision_id).unwrap(),
        target["assembly_revision_id"]
    );
    assert!(shared.design.as_ref().unwrap().fresh_review_passed);
    let design = shared.design.as_ref().unwrap();
    let source = call(
        &client,
        "get_circuit_revision",
        json!({"blueprint":{
            "kind":"assembly", "id":design.assembly_revision_id
        }}),
    )
    .await;
    assert_eq!(source["ok"], true);
    assert_eq!(
        source["result"]["record"]["id"],
        target["assembly_revision_id"]
    );
    for occurrence in design.occurrences.iter().take(1) {
        let child = call(
            &client,
            "get_circuit_revision",
            json!({"blueprint":{
                "kind":"blueprint", "id":occurrence.revision
            }}),
        )
        .await;
        assert_eq!(child["ok"], true);
        assert_eq!(child["result"]["record"]["id"], json!(occurrence.revision));
    }
    assert_eq!(shared.repair.finding_ids.len(), shared.findings.len());
    assert!(shared.findings.iter().all(|finding| matches!(
        finding.evidence,
        dustroute_translate::diagnostic::report::FindingEvidence::DesignComparison(_)
    )));
    assert_eq!(report["cause"], "not_inferred");
    let mut missing_input = settled.clone();
    missing_input["blocks"]
        .as_array_mut()
        .unwrap()
        .retain(|b| b["name"] != "minecraft:lever");
    fake.lock().unwrap().snapshot = Some(missing_input);
    let diagnosis = call(
        &client,
        "manage_assembly",
        json!({"action":"diagnose","instance_id":id}),
    )
    .await;
    assert_eq!(
        diagnosis["diagnosis"]["reference"]["mode"],
        "declared_initial"
    );
    assert!(
        diagnosis["diagnosis"]["findings"]
            .as_array()
            .unwrap()
            .iter()
            .any(|f| f["target"]["name"] == "minecraft:lever")
    );
    assert_eq!(fake.lock().unwrap().writes, writes_before_diagnosis);
    fake.lock().unwrap().snapshot = Some(settled.clone());
    fake.lock().unwrap().partial = true;
    let incomplete = call(
        &client,
        "manage_assembly",
        json!({"action":"plan_removal","instance_id":id}),
    )
    .await;
    assert_eq!(
        incomplete["observation"]["status"],
        "observation_incomplete"
    );
    assert_eq!(incomplete["ok"], false);
    let diagnosis = call(
        &client,
        "manage_assembly",
        json!({"action":"diagnose","instance_id":id}),
    )
    .await;
    assert_eq!(diagnosis["diagnosis"]["status"], "observation_unavailable");
    assert_eq!(diagnosis["diagnosis"]["repair"]["status"], "not_assessed");
    assert!(
        diagnosis["diagnosis"]["findings"]
            .as_array()
            .unwrap()
            .is_empty()
    );
    fake.lock().unwrap().partial = false;
    fake.lock().unwrap().wrong_target = true;
    let other = call(
        &client,
        "manage_assembly",
        json!({"action":"plan_removal","instance_id":id}),
    )
    .await;
    assert_eq!(other["observation"]["status"], "target_mismatch");
    assert_eq!(other["ok"], false);
    fake.lock().unwrap().wrong_target = false;
    let mut changed = settled.clone();
    changed["blocks"]
        .as_array_mut()
        .unwrap()
        .push(json!({"pos":planned["bounds"]["min"],"name":"minecraft:stone","properties":{}}));
    fake.lock().unwrap().snapshot = Some(changed.clone());
    let dirty = call(
        &client,
        "manage_assembly",
        json!({"action":"plan_removal","instance_id":id}),
    )
    .await;
    assert_eq!(dirty["observation"]["status"], "changed");
    assert_eq!(dirty["ok"], false);
    let mut moving = changed.clone();
    moving["blocks"].as_array_mut().unwrap().last_mut().unwrap()["name"] =
        json!("minecraft:moving_piston");
    fake.lock().unwrap().snapshot = Some(moving);
    let unknown = call(
        &client,
        "manage_assembly",
        json!({"action":"plan_removal","instance_id":id}),
    )
    .await;
    assert_eq!(unknown["observation"]["status"], "history_unavailable");
    assert_eq!(unknown["ok"], false);
    let diagnosis = call(
        &client,
        "manage_assembly",
        json!({"action":"diagnose","instance_id":id}),
    )
    .await;
    assert_eq!(diagnosis["diagnosis"]["status"], "observation_unavailable");
    assert_eq!(fake.lock().unwrap().writes, writes_before_diagnosis);
    fake.lock().unwrap().snapshot = Some(settled.clone());
    stop(client, server).await;
    bridge.abort();
    fs::remove_dir_all(root).unwrap();
}

#[tokio::test]
async fn removal_revalidates_pins_expectations_revision_and_live_baseline_after_restart() {
    let AppliedCase {
        case:
            Case {
                root,
                fake,
                address,
                bridge,
                client,
                server,
                target: _,
            },
        planned,
        settled,
    } = applied_case().await;
    let id = planned["operation_id"].clone();
    stop(client, server).await;
    let (client, server) = connected(&root, &address).await;
    let list = call(&client, "manage_assembly", json!({"action":"list"})).await;
    assert_eq!(list["instances"].as_array().unwrap().len(), 1);
    assert_eq!(list["instances"][0]["instance_id"], id);
    assert_eq!(list["instances"][0]["state"], "applied");
    let get = call(
        &client,
        "manage_assembly",
        json!({"action":"get","instance_id":id}),
    )
    .await;
    assert_eq!(get["expected_snapshot"], settled);
    assert_eq!(get["fresh_observation"], false);
    assert_eq!(
        get["instance"]["attempts"][0]["readbacks"]
            .as_array()
            .unwrap()
            .len(),
        1 + 2 * planned["construction_steps"].as_array().unwrap().len()
    );
    let mut changed = settled.clone();
    changed["blocks"]
        .as_array_mut()
        .unwrap()
        .push(json!({"pos":planned["bounds"]["min"],"name":"minecraft:stone","properties":{}}));
    // Deserializing an altered expectation must not construct a capability.
    let record_path = root
        .join("assembly-instances")
        .join(format!("{}.json", id.as_str().unwrap()));
    let original_bytes = fs::read(&record_path).unwrap();
    let mut corrupted: Value = serde_json::from_slice(&original_bytes).unwrap();
    corrupted["expected"] = changed.clone();
    fs::write(&record_path, serde_json::to_vec(&corrupted).unwrap()).unwrap();
    let corrupt = call(
        &client,
        "manage_assembly",
        json!({"action":"plan_removal","instance_id":id}),
    )
    .await;
    assert_eq!(corrupt["observation"]["revalidation"]["status"], "failed");
    assert_eq!(corrupt["ok"], false);
    let diagnosis = call(
        &client,
        "manage_assembly",
        json!({"action":"diagnose","instance_id":id}),
    )
    .await;
    assert_eq!(diagnosis["diagnosis"]["status"], "reference_unverified");
    assert_eq!(
        diagnosis["diagnosis"]["reference"]["mode"],
        "saved_initial_unverified"
    );
    fs::write(&record_path, original_bytes).unwrap();
    let stale = call(
        &client,
        "manage_assembly",
        json!({"action":"plan_removal","instance_id":id}),
    )
    .await;
    assert_eq!(stale["ok"], true, "{stale}");
    assert_eq!(
        call(
            &client,
            "invoke_operation",
            json!({"operation_id":stale["operation_id"],"confirm":true})
        )
        .await["ok"],
        false
    );
    assert_eq!(
        call(
            &client,
            "show_operation",
            json!({"operation_id":stale["operation_id"]})
        )
        .await["ok"],
        true
    );
    call(
        &client,
        "manage_assembly",
        json!({"action":"observe","instance_id":id}),
    )
    .await;
    assert_eq!(
        call(
            &client,
            "invoke_operation",
            json!({"operation_id":stale["operation_id"],"confirm":true})
        )
        .await["ok"],
        false
    );
    let removal = call(
        &client,
        "manage_assembly",
        json!({"action":"plan_removal","instance_id":id}),
    )
    .await;
    assert_eq!(removal["ok"], true, "{removal}");
    let removal_id = removal["operation_id"].clone();
    assert_eq!(
        call(
            &client,
            "show_operation",
            json!({"operation_id":removal_id})
        )
        .await["ok"],
        true
    );
    fake.lock().unwrap().snapshot = Some(changed);
    assert_eq!(
        call(
            &client,
            "invoke_operation",
            json!({"operation_id":removal_id,"confirm":true})
        )
        .await["ok"],
        false
    );
    assert_eq!(
        fake.lock().unwrap().writes,
        planned["construction_steps"].as_array().unwrap().len()
    );
    fake.lock().unwrap().snapshot = Some(settled.clone());
    fake.lock().unwrap().steps = removal["removal_steps"]
        .as_array()
        .unwrap()
        .iter()
        .cloned()
        .collect();
    let undone = call(
        &client,
        "invoke_operation",
        json!({"operation_id":removal_id,"confirm":true}),
    )
    .await;
    assert_eq!(undone["ok"], true, "{undone}");
    assert_eq!(undone["instance_id"], id);
    assert!(
        fake.lock().unwrap().snapshot.as_ref().unwrap()["blocks"]
            .as_array()
            .unwrap()
            .is_empty()
    );
    assert_eq!(
        call(
            &client,
            "invoke_operation",
            json!({"operation_id":id,"confirm":true})
        )
        .await["ok"],
        false
    );
    stop(client, server).await;
    let (client, server) = connected(&root, &address).await;
    let removed = call(
        &client,
        "manage_assembly",
        json!({"action":"observe","instance_id":id}),
    )
    .await;
    assert_eq!(removed["instance"]["state"], "removed");
    assert_eq!(removed["observation"]["status"], "matches");
    assert_eq!(removed["observation"]["removal_eligible"], false);
    assert_eq!(
        call(
            &client,
            "manage_assembly",
            json!({"action":"plan_removal","instance_id":id})
        )
        .await["ok"],
        false
    );
    stop(client, server).await;
    bridge.abort();
    fs::remove_dir_all(root).unwrap();
}

#[tokio::test]
async fn same_process_undo_uses_the_durable_construction_journal() {
    let Case {
        root,
        fake,
        address: _,
        bridge,
        client,
        server,
        target,
    } = adopted_case().await;
    // The original same-process undo entry point uses the same durable journal.
    let same_process = call(&client, "new_placement", target.clone()).await;
    assert_eq!(same_process["ok"], true, "{same_process}");
    let same_process_id = same_process["operation_id"].clone();
    call(
        &client,
        "show_operation",
        json!({"operation_id":same_process_id}),
    )
    .await;
    fake.lock().unwrap().steps = same_process["construction_steps"]
        .as_array()
        .unwrap()
        .iter()
        .cloned()
        .collect();
    let same_process_applied = call(
        &client,
        "invoke_operation",
        json!({"operation_id":same_process_id,"confirm":true}),
    )
    .await;
    assert_eq!(same_process_applied["ok"], true, "{same_process_applied}");
    fake.lock().unwrap().steps = same_process["undo_steps"]
        .as_array()
        .unwrap()
        .iter()
        .cloned()
        .collect();
    let same_process_undone = call(
        &client,
        "undo_operation",
        json!({"operation_id":same_process_id,"confirm":true}),
    )
    .await;
    assert_eq!(same_process_undone["ok"], true, "{same_process_undone}");
    let same_process_record = call(
        &client,
        "manage_assembly",
        json!({"action":"get","instance_id":same_process_id}),
    )
    .await;
    assert_eq!(same_process_record["instance"]["state"], "removed");
    stop(client, server).await;
    bridge.abort();
    fs::remove_dir_all(root).unwrap();
}

#[tokio::test]
async fn interrupted_construction_and_reconstruction_preserve_attempts_across_restarts() {
    let Case {
        root,
        fake,
        address,
        bridge,
        client,
        server,
        target,
    } = adopted_case().await;
    // A lost/partial post-write observation is retained across another restart.
    let second = call(&client, "new_placement", target).await;
    assert_eq!(second["ok"], true, "{second}");
    let second_id = second["operation_id"].clone();
    let settled = second["construction_steps"]
        .as_array()
        .unwrap()
        .last()
        .unwrap()["expected"]
        .clone();
    call(&client, "show_operation", json!({"operation_id":second_id})).await;
    fake.lock().unwrap().steps = second["construction_steps"]
        .as_array()
        .unwrap()
        .iter()
        .cloned()
        .collect();
    let writes = fake.lock().unwrap().writes;
    fake.lock().unwrap().fail_after_write = Some(writes + 2);
    let partial = call(
        &client,
        "invoke_operation",
        json!({"operation_id":second_id,"confirm":true}),
    )
    .await;
    assert_eq!(partial["status"], "needs_inspection", "{partial}");
    assert_eq!(partial["verified_steps"], 1);
    let interrupted_snapshot = fake.lock().unwrap().snapshot.clone().unwrap();
    stop(client, server).await;
    let (client, server) = connected(&root, &address).await;
    fake.lock().unwrap().fail_after_write = None;
    // Even a matching later snapshot does not promote an uncertain attempt.
    fake.lock().unwrap().snapshot = Some(settled);
    let uncertain = call(
        &client,
        "manage_assembly",
        json!({"action":"plan_removal","instance_id":second_id}),
    )
    .await;
    assert_eq!(uncertain["instance"]["state"], "needs_inspection");
    assert_eq!(uncertain["observation"]["status"], "matches");
    assert_eq!(uncertain["ok"], false);
    assert_eq!(fake.lock().unwrap().writes, writes + 2);

    // Reconstruction is a new, explicitly previewed operation. It preserves
    // the uncertain old attempt, starts from fresh observations, and can itself
    // stop and be replanned after restart without blindly resending a stage.
    fake.lock().unwrap().snapshot = Some(interrupted_snapshot.clone());
    fake.lock().unwrap().partial = true;
    let unavailable = call(
        &client,
        "manage_assembly",
        json!({"action":"plan_reconstruction","instance_id":second_id}),
    )
    .await;
    assert_eq!(unavailable["ok"], false, "{unavailable}");
    fake.lock().unwrap().partial = false;
    let mut conflict = interrupted_snapshot.clone();
    conflict["blocks"].as_array_mut().unwrap().push(json!({
        "pos":second["bounds"]["min"],"name":"minecraft:diamond_block","properties":{}
    }));
    fake.lock().unwrap().snapshot = Some(conflict.clone());
    let refused = call(
        &client,
        "manage_assembly",
        json!({"action":"plan_reconstruction","instance_id":second_id}),
    )
    .await;
    assert_eq!(refused["ok"], false, "{refused}");
    assert_eq!(refused["diagnosis"]["summary"]["by_kind"]["unexpected"], 1);
    assert_eq!(refused["diagnosis"]["repair"]["status"], "blocked");
    conflict["blocks"]
        .as_array_mut()
        .unwrap()
        .last_mut()
        .unwrap()["name"] = json!("minecraft:moving_piston");
    fake.lock().unwrap().snapshot = Some(conflict);
    let moving = call(
        &client,
        "manage_assembly",
        json!({"action":"plan_reconstruction","instance_id":second_id}),
    )
    .await;
    assert_eq!(moving["ok"], false, "{moving}");
    fake.lock().unwrap().snapshot = Some(interrupted_snapshot.clone());
    let repair = call(
        &client,
        "manage_assembly",
        json!({"action":"plan_reconstruction","instance_id":second_id}),
    )
    .await;
    assert_eq!(repair["ok"], true, "{repair}");
    assert_eq!(
        repair["reconstruction_conditions"]["server_readiness_proven"],
        false
    );
    assert!(!repair["differences"].as_array().unwrap().is_empty());
    let repair_id = repair["operation_id"].clone();
    assert_eq!(
        call(
            &client,
            "invoke_operation",
            json!({"operation_id":repair_id,"confirm":true})
        )
        .await["ok"],
        false
    );
    assert_eq!(
        call(&client, "show_operation", json!({"operation_id":repair_id})).await["ok"],
        true
    );
    fake.lock().unwrap().snapshot = Some(
        second["construction_steps"]
            .as_array()
            .unwrap()
            .last()
            .unwrap()["expected"]
            .clone(),
    );
    assert_eq!(
        call(
            &client,
            "invoke_operation",
            json!({"operation_id":repair_id,"confirm":true})
        )
        .await["ok"],
        false
    );
    fake.lock().unwrap().snapshot = Some(interrupted_snapshot);
    call(
        &client,
        "manage_assembly",
        json!({"action":"observe","instance_id":second_id}),
    )
    .await;
    assert_eq!(
        call(
            &client,
            "invoke_operation",
            json!({"operation_id":repair_id,"confirm":true})
        )
        .await["ok"],
        false
    );
    assert_eq!(
        fake.lock().unwrap().writes,
        writes + 2,
        "all guards must precede writes"
    );
    let repair = call(
        &client,
        "manage_assembly",
        json!({"action":"plan_reconstruction","instance_id":second_id}),
    )
    .await;
    assert_eq!(repair["ok"], true, "{repair}");
    let repair_id = repair["operation_id"].clone();
    call(&client, "show_operation", json!({"operation_id":repair_id})).await;
    fake.lock().unwrap().steps = repair["reconstruction"]["steps"]
        .as_array()
        .unwrap()
        .iter()
        .cloned()
        .collect();
    fake.lock().unwrap().lose_write_reply_at = Some(writes + 4);
    let partial_repair = call(
        &client,
        "invoke_operation",
        json!({"operation_id":repair_id,"confirm":true}),
    )
    .await;
    assert_eq!(
        partial_repair["status"], "needs_inspection",
        "{partial_repair}"
    );
    assert_eq!(partial_repair["verified_steps"], 1);
    stop(client, server).await;
    let (client, server) = connected(&root, &address).await;
    fake.lock().unwrap().fail_after_write = None;
    fake.lock().unwrap().lose_write_reply_at = None;
    let repair = call(
        &client,
        "manage_assembly",
        json!({"action":"plan_reconstruction","instance_id":second_id}),
    )
    .await;
    assert_eq!(repair["ok"], true, "{repair}");
    let repair_id = repair["operation_id"].clone();
    call(&client, "show_operation", json!({"operation_id":repair_id})).await;
    fake.lock().unwrap().steps = repair["reconstruction"]["steps"]
        .as_array()
        .unwrap()
        .iter()
        .cloned()
        .collect();
    let repaired = call(
        &client,
        "invoke_operation",
        json!({"operation_id":repair_id,"confirm":true}),
    )
    .await;
    assert_eq!(repaired["ok"], true, "{repaired}");
    assert_eq!(repaired["instance_id"], second_id);
    assert_eq!(
        call(
            &client,
            "invoke_operation",
            json!({"operation_id":repair_id,"confirm":true})
        )
        .await["ok"],
        false
    );
    stop(client, server).await;
    let (client, server) = connected(&root, &address).await;
    let restored = call(
        &client,
        "manage_assembly",
        json!({"action":"observe","instance_id":second_id}),
    )
    .await;
    assert_eq!(restored["instance"]["state"], "applied");
    assert_eq!(restored["observation"]["status"], "matches");
    let attempts = restored["instance"]["attempts"].as_array().unwrap();
    assert_eq!(attempts.len(), 3);
    assert_eq!(attempts[0]["verified_steps"], 1);
    assert_eq!(attempts[1]["verified_steps"], 1);
    assert!(attempts[0]["reconstruction"].is_null());
    assert_eq!(attempts[2]["reconstruction"], repair["reconstruction"]);
    assert_eq!(attempts[2]["verified_steps"], attempts[2]["total_steps"]);
    stop(client, server).await;
    bridge.abort();
    fs::remove_dir_all(root).unwrap();
}
