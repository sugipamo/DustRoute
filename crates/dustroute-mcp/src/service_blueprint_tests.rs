//! Exercise public tool routing and durable decisions through a real MCP session.
use super::*;
use crate::blueprint_mcp::{AssemblyGrounding, BlueprintRecords, BlueprintWrite, Command, execute};
use dustroute_library::PortDirection;
use dustroute_library::assembly::*;
use dustroute_library::blueprint::*;
use dustroute_library::builtin_blueprints::*;
use dustroute_translate::{Block, Region, RotationY};
use rmcp::{
    ServiceExt,
    model::{CallToolRequestParams, ClientInfo, ContentBlock},
};
use std::{
    fs,
    path::{Path, PathBuf},
};

#[path = "../../dustroute-translate/tests/support/runtime_blueprint.rs"]
pub(crate) mod runtime_fixture;

#[allow(dead_code)]
#[path = "../../dustroute-translate/tests/support/reference_door_blueprint.rs"]
mod door_fixture;

#[test]
fn ordinary_door_command_initialization_connects_to_full_readback_gates() {
    use crate::piston_assembly::ValidatedAssemblyPlacement;
    use dustroute_translate::assembly_transform::AssemblyTransform;
    let mut fixture = door_fixture::ordinary_fixture();
    fixture
        .catalog
        .insert_revisions(fixture.request.revisions)
        .unwrap();
    let transform = AssemblyTransform {
        source_anchor: Pos::default(),
        target_anchor: Pos::new(60_000, 180, 1000),
        rotation: RotationY::R90,
    };
    let bounds = transform.region(fixture.context.known_region).unwrap();
    let empty = dustroute_translate::MinecraftSnapshot {
        min: bounds.min,
        max: bounds.max,
        blocks: vec![],
    };
    let plan = ValidatedAssemblyPlacement::new(
        &fixture.catalog,
        &fixture.request.candidate_state.assembly,
        &fixture.context,
        transform,
        &empty,
        "1.21.11",
    )
    .unwrap();
    assert_eq!(plan.steps(false).len(), 43);
    assert_eq!(plan.steps(true).len(), 43);
    assert!(
        plan.steps(false)
            .iter()
            .filter(|s| s.state.starts_with("minecraft:observer["))
            .all(|s| s.state.contains("powered=true"))
    );
    plan.validate_before(&empty, "1.21.11", false).unwrap();
    plan.validate_after(plan.settled(), "1.21.11", false)
        .unwrap();
    assert!(plan.validate_after(&empty, "1.21.11", false).is_err());
    assert!(
        plan.validate_before(plan.settled(), "1.21.11", false)
            .is_err()
    );
    let restored = ValidatedAssemblyPlacement::review_target(
        &fixture.catalog,
        &fixture.request.candidate_state.assembly,
        &fixture.context,
        transform,
    )
    .unwrap();
    assert_eq!(
        serde_json::to_value(plan.steps(false)).unwrap(),
        serde_json::to_value(restored.steps(false)).unwrap()
    );
}

#[tokio::test]
async fn ordinary_door_public_proposal_survives_restart_and_is_freshly_adopted() {
    let fixture = door_fixture::ordinary_fixture();
    let root = temporary();
    let (client, server) = start(&root).await;
    let imported = call(
        &client,
        "test_circuit_change",
        json!({"blueprint":{
            "action":"import", "records":{
                "types":fixture.catalog.type_revisions().collect::<Vec<_>>(),
                "classifications":fixture.catalog.classifications().collect::<Vec<_>>(),
                "revisions":fixture.catalog.revisions().collect::<Vec<_>>(),
                "assemblies":[fixture.base]
            }
        }}),
    )
    .await;
    assert_eq!(imported["ok"], true, "{imported}");
    let mut request = serde_json::to_value(&fixture.request).unwrap();
    request.as_object_mut().unwrap().remove("id");
    let created = call(
        &client,
        "test_circuit_change",
        json!({"blueprint":{
            "action":"propose_update", "request":request
        }}),
    )
    .await;
    assert_eq!(created["ok"], true, "{created}");
    let operation = created["operation_id"].clone();
    let shown = call(&client, "show_operation", json!({"operation_id":operation})).await;
    assert_eq!(shown["can_adopt"], true, "{shown}");
    assert_eq!(shown["validation"]["behavior_status"], "passed");
    let saved = call(
        &client,
        "get_circuit_revision",
        json!({"blueprint":{"kind":"archive"}}),
    )
    .await;
    assert!(
        saved
            .to_string()
            .contains("dustroute.blueprint-catalog.v11")
    );
    stop(client, server).await;
    let (client, server) = start(&root).await;
    let adopted = call(
        &client,
        "invoke_operation",
        json!({"operation_id":operation,"confirm":true,
        "blueprint_decision":{"action":"adopt"}}),
    )
    .await;
    assert_eq!(adopted["ok"], true, "{adopted}");
    assert_eq!(adopted["writes_minecraft"], false);
    stop(client, server).await;
    let (client, server) = start(&root).await;
    let checked = call(
        &client,
        "get_circuit_revision",
        json!({"blueprint":{
            "kind":"assembly", "id":fixture.request.candidate_state.id,
            "validate":true, "behavior_context":fixture.context
        }}),
    )
    .await;
    assert_eq!(
        checked["result"]["validation"]["status"], "passed",
        "{checked}"
    );
    assert_eq!(
        checked["result"]["validation"]["live_world_verified"],
        false
    );
    assert_eq!(checked["result"]["lifecycle"]["adopted"], true);
    assert_eq!(
        checked["result"]["record"],
        serde_json::to_value(&fixture.request.candidate_state).unwrap()
    );
    let base = call(
        &client,
        "get_circuit_revision",
        json!({"blueprint":{"kind":"assembly","id":fixture.base.id}}),
    )
    .await;
    assert_eq!(
        base["result"]["record"],
        serde_json::to_value(&fixture.base).unwrap()
    );
    stop(client, server).await;
    fs::remove_dir_all(root).unwrap();
}

#[tokio::test]
async fn custom_electrical_construction_rechecks_adoption_baseline_settings_and_each_write() {
    use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
    use tokio::net::TcpListener;
    #[derive(Default)]
    struct Fake {
        snapshot: Option<Value>,
        steps: VecDeque<Value>,
        writes: usize,
        partial: bool,
        unknown_features: bool,
        wrong_target: bool,
        fail_after_write: Option<usize>,
        lose_write_reply_at: Option<usize>,
    }
    let root = temporary();
    let durable_root = root.join("assembly-instances");
    let fake = Arc::new(std::sync::Mutex::new(Fake::default()));
    let transport = fake.clone();
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap().to_string();
    let bridge = tokio::spawn(async move {
        loop {
            let (mut stream, _) = listener.accept().await.unwrap();
            let mut line = String::new();
            BufReader::new(&mut stream)
                .read_line(&mut line)
                .await
                .unwrap();
            let req: Value = serde_json::from_str(&line).unwrap();
            let result = {
                let mut state = transport.lock().unwrap();
                match req["method"].as_str().unwrap() {
                    "status" => {
                        json!({"connected":true,"username":"bot","host":if state.wrong_target {"other-server"}else{"localhost"},"port":25565,"version":"1.21.11","dimension":"minecraft:overworld","enabled_features":if state.unknown_features {Value::Null}else{json!(["minecraft:vanilla"])}})
                    }
                    "observe_player" => {
                        json!({"player":"Tester","eye_position":{"x":0.0,"y":180.0,"z":0.0},"yaw":0.0,"pitch":0.0,"dimension":"minecraft:overworld"})
                    }
                    "scan_region" => {
                        let mut snapshot = state.snapshot.clone().unwrap_or_else(|| json!({"min":req["params"]["min"],"max":req["params"]["max"],"blocks":[]}));
                        if state.partial
                            || state.fail_after_write.is_some_and(|n| state.writes >= n)
                        {
                            snapshot["min"]["x"] =
                                json!(snapshot["min"]["x"].as_i64().unwrap() + 1);
                        }
                        snapshot
                    }
                    "preview_region" => json!({"particle_corners":8}),
                    "wait_ticks" => json!({"waited":true}),
                    "write_blocks" => {
                        // Transport stub only: physical callback conformance is
                        // covered by independent server captures, not this mock.
                        let expected = state.steps.pop_front().expect("unexpected write");
                        let active: Vec<Value> = fs::read_dir(&durable_root)
                            .unwrap()
                            .filter_map(|entry| {
                                let path = entry.unwrap().path();
                                if path.extension().and_then(|s| s.to_str()) != Some("json") {
                                    return None;
                                }
                                let record: Value =
                                    serde_json::from_slice(&fs::read(path).unwrap()).unwrap();
                                (record["state"] == "needs_inspection").then_some(record)
                            })
                            .collect();
                        assert_eq!(
                            active.len(),
                            1,
                            "durable intent must exist before every write"
                        );
                        let attempt = active[0]["attempts"].as_array().unwrap().last().unwrap();
                        assert_eq!(
                            attempt["verified_steps"].as_u64().unwrap() as usize,
                            attempt["total_steps"].as_u64().unwrap() as usize
                                - state.steps.len()
                                - 1
                        );
                        assert_eq!(
                            req["params"]["changes"],
                            json!([{"pos":expected["position"],"state":expected["state"]}])
                        );
                        state.snapshot = Some(expected["expected"].clone());
                        state.writes += 1;
                        json!({"submitted_changes":1})
                    }
                    method => panic!("unexpected transport method {method}"),
                }
            };
            if req["method"] == "write_blocks" {
                let state = transport.lock().unwrap();
                if state.lose_write_reply_at == Some(state.writes) {
                    // The simulated server applied the command but the TCP
                    // reply is lost. No process or host fault is injected.
                    continue;
                }
            }
            stream
                .write_all(format!("{}\n", json!({"id":req["id"],"result":result})).as_bytes())
                .await
                .unwrap();
        }
    });
    async fn connected(root: &Path, address: &str) -> (Client, tokio::task::JoinHandle<()>) {
        let mut service = DustRouteMcp::with_policy_and_player(
            address,
            McpPolicy {
                read_only: false,
                ..Default::default()
            },
            "Tester",
        );
        service.state_store = PlanStateStore::new(root.to_path_buf(), 3600);
        let (server_io, client_io) = tokio::io::duplex(1024 * 1024);
        let server = tokio::spawn(async move {
            service
                .serve(server_io)
                .await
                .unwrap()
                .waiting()
                .await
                .unwrap();
        });
        (
            ClientInfo::default().serve(client_io).await.unwrap(),
            server,
        )
    }
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
        "dustroute.piston-electrical-root-exploration.v13"
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
    let settled = fake.lock().unwrap().snapshot.clone().unwrap();
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
    // The original same-process undo entry point uses the same durable journal.
    let legacy = call(&client, "new_placement", target.clone()).await;
    assert_eq!(legacy["ok"], true, "{legacy}");
    let legacy_id = legacy["operation_id"].clone();
    call(&client, "show_operation", json!({"operation_id":legacy_id})).await;
    fake.lock().unwrap().steps = legacy["construction_steps"]
        .as_array()
        .unwrap()
        .iter()
        .cloned()
        .collect();
    let legacy_applied = call(
        &client,
        "invoke_operation",
        json!({"operation_id":legacy_id,"confirm":true}),
    )
    .await;
    assert_eq!(legacy_applied["ok"], true, "{legacy_applied}");
    fake.lock().unwrap().steps = legacy["undo_steps"]
        .as_array()
        .unwrap()
        .iter()
        .cloned()
        .collect();
    let legacy_undone = call(
        &client,
        "undo_operation",
        json!({"operation_id":legacy_id,"confirm":true}),
    )
    .await;
    assert_eq!(legacy_undone["ok"], true, "{legacy_undone}");
    let legacy_record = call(
        &client,
        "manage_assembly",
        json!({"action":"get","instance_id":legacy_id}),
    )
    .await;
    assert_eq!(legacy_record["instance"]["state"], "removed");
    // A lost/partial post-write observation is retained across another restart.
    let second = call(&client, "new_placement", target).await;
    assert_eq!(second["ok"], true, "{second}");
    let second_id = second["operation_id"].clone();
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

#[tokio::test]
async fn runtime_blueprint_review_and_adoption_preserve_child_failures_after_restart() {
    for broken_child in [false, true] {
        let fixture = runtime_fixture::fixture(broken_child, false);
        let root = temporary();
        let known = fixture.base.assembly.known_regions[0];
        let grounding_snapshot =
            snapshot_from_grounded_assembly(&fixture.base, known.min, known.max).unwrap();
        let grounding_revision = uuid::Uuid::new_v4();
        execute(
            &PlanStateStore::new(root.clone(), 3600),
            "Tester",
            Command::Write(BlueprintWrite::Import {
                records: BlueprintRecords {
                    types: fixture.catalog.type_revisions().cloned().collect(),
                    classifications: fixture.catalog.classifications().cloned().collect(),
                    revisions: fixture.catalog.revisions().cloned().collect(),
                    assemblies: vec![],
                },
            }),
        )
        .unwrap();
        execute(
            &PlanStateStore::new(root.clone(), 3600),
            "Tester",
            Command::Capture {
                record: Box::new(fixture.base.clone()),
                grounding: AssemblyGrounding {
                    assembly_revision_id: fixture.base.id.clone(),
                    circuit_revision_id: grounding_revision,
                    base_observation_id: uuid::Uuid::new_v4(),
                    dimension: "minecraft:overworld".into(),
                    complete: true,
                    base_snapshot: grounding_snapshot,
                },
            },
        )
        .unwrap();
        let (client, server) = start(&root).await;
        let tools = client.list_tools(None).await.unwrap();
        assert_eq!(tools.tools.len(), 22);
        let schema = serde_json::to_string(
            &tools
                .tools
                .iter()
                .find(|t| t.name == "get_circuit_revision")
                .unwrap()
                .input_schema,
        )
        .unwrap();
        assert!(schema.contains("dustroute.piston-electrical-root-exploration.v13"));
        assert!(!schema.contains("dustroute.horizontal-piston-root-exploration.v1"));
        let imported=call(&client,"test_circuit_change",json!({"blueprint":{"action":"import","records":{
            "types":fixture.catalog.type_revisions().collect::<Vec<_>>(),
            "classifications":fixture.catalog.classifications().collect::<Vec<_>>(),
            "revisions":fixture.catalog.revisions().collect::<Vec<_>>(),"assemblies":[fixture.base]
        }}})).await;
        assert_eq!(imported["ok"], true, "{imported}");
        let without_context = call(
            &client,
            "get_circuit_revision",
            json!({"blueprint":{"kind":"assembly","id":fixture.base.id,"validate":true}}),
        )
        .await;
        assert_eq!(
            without_context["result"]["validation"]["status"],
            "undetermined"
        );
        let inspected=call(&client,"get_circuit_revision",json!({"blueprint":{"kind":"assembly","id":fixture.base.id,"validate":true,"behavior_context":fixture.context}})).await;
        assert_eq!(
            inspected["result"]["validation"]["status"],
            if broken_child { "failed" } else { "passed" },
            "{inspected}"
        );
        assert_eq!(
            inspected["result"]["validation"]["scope"],
            "whole_realization_declared_obligations_in_moving_world_model"
        );
        assert_eq!(
            inspected["result"]["validation"]["placement_validation_profile"],
            "dustroute.piston-electrical-callbacks.java-1-21-11.v13"
        );
        assert_eq!(
            inspected["result"]["world_execution_context"],
            serde_json::to_value(fixture.context.execution_context()).unwrap()
        );
        assert_eq!(
            inspected["result"]["validation"]["live_world_verified"],
            false
        );
        assert_eq!(inspected["writes_minecraft"], false);
        let mut request = serde_json::to_value(&fixture.request).unwrap();
        request.as_object_mut().unwrap().remove("id");
        let created = call(
            &client,
            "test_circuit_change",
            json!({"blueprint":{"action":"propose_update","request":request}}),
        )
        .await;
        assert_eq!(created["ok"], true, "{created}");
        let operation = created["operation_id"].clone();
        let shown = call(&client, "show_operation", json!({"operation_id":operation})).await;
        assert_eq!(shown["can_adopt"], !broken_child, "{shown}");
        assert_eq!(shown["validation"]["behavior_status"], "passed");
        assert_eq!(
            shown["validation"]["declared_behavior_verified"],
            !broken_child
        );
        if broken_child {
            assert!(shown.to_string().contains("MovingPiston"));
        }
        let saved = call(
            &client,
            "get_circuit_revision",
            json!({"blueprint":{"kind":"archive"}}),
        )
        .await;
        assert_eq!(
            saved["result"]["archive"]["schema"],
            "dustroute.blueprint-updates.v5"
        );
        stop(client, server).await;
        let (client, server) = start(&root).await;
        let decided=call(&client,"invoke_operation",json!({"operation_id":operation,"confirm":true,"blueprint_decision":{"action":"adopt"}})).await;
        assert_eq!(decided["ok"], !broken_child, "{decided}");
        assert_eq!(decided["writes_minecraft"], false);
        let retained = call(&client, "get_operation", json!({"operation_id":operation})).await;
        assert_eq!(
            retained["operation"]["status"],
            if broken_child { "open" } else { "adopted" }
        );
        assert_eq!(
            retained["operation"]["request"]["behavior_context"],
            serde_json::to_value(&fixture.context).unwrap()
        );
        let base = call(
            &client,
            "get_circuit_revision",
            json!({"blueprint":{"kind":"assembly","id":fixture.base.id}}),
        )
        .await;
        assert_eq!(
            base["result"]["record"],
            serde_json::to_value(&fixture.base).unwrap()
        );
        assert_eq!(base["result"]["lifecycle"]["adopted"], false);
        assert_eq!(base["result"]["lifecycle"]["placement_eligible"], false);
        let candidate = call(
            &client,
            "get_circuit_revision",
            json!({"blueprint":{"kind":"assembly","id":fixture.request.candidate_state.id}}),
        )
        .await;
        assert_eq!(candidate["ok"], !broken_child, "{candidate}");
        if !broken_child {
            assert_eq!(
                candidate["result"]["record"],
                serde_json::to_value(&fixture.request.candidate_state).unwrap()
            );
            assert_eq!(candidate["result"]["lifecycle"]["adopted"], true);
            assert_eq!(
                candidate["result"]["lifecycle"]["adopted_by"],
                json!([operation])
            );
            assert_eq!(candidate["result"]["lifecycle"]["placement_eligible"], true);
            let basis = execute(
                &PlanStateStore::new(root.clone(), 3600),
                "Tester",
                Command::PlacementBasis(fixture.request.candidate_state.id.clone()),
            )
            .unwrap()
            .unwrap();
            assert_eq!(
                basis["grounding"]["circuit_revision_id"],
                grounding_revision.to_string()
            );
            assert_eq!(basis["fresh_review"]["status"], "passed");
        }
        stop(client, server).await;
        fs::remove_dir_all(root).unwrap();
    }
}

fn id(value: &str) -> BlueprintRevisionId {
    BlueprintRevisionId::new(value).unwrap()
}
fn inclusion(name: &str, revision: &str) -> BlueprintInclusion {
    BlueprintInclusion {
        instance: InstanceId::new(name).unwrap(),
        revision: id(revision),
        origin: Pos::default(),
        rotation: RotationY::R0,
    }
}
fn regions() -> Vec<Region> {
    vec![Region::new(Pos::new(-4, -4, -4), Pos::new(10, 4, 4))]
}
fn parent(
    name: &str,
    previous: Option<&str>,
    layout: &str,
    child_name: &str,
    child: &str,
    shared: bool,
) -> BlueprintRevision {
    let mut record = builtin_blueprints().revision(&id(layout)).unwrap().clone();
    record.id = id(name);
    record.name = name.into();
    record.parents = previous.into_iter().map(id).collect();
    record.initial_layout = Some(BlueprintLayout {
        blocks: record.blocks.clone(),
        known_regions: regions(),
    });
    record.blocks.clear();
    record.inclusions = vec![inclusion(child_name, child)];
    if shared {
        record.inclusions.push(inclusion("shared", "shared.v1"));
    }
    record.port_bindings = record
        .ports
        .iter()
        .map(|p| BlueprintPortBinding {
            name: p.name.clone(),
            port: BlueprintPortRef {
                instance: vec![InstanceId::new(child_name).unwrap()],
                port: p.name.clone(),
            },
        })
        .collect();
    record
}

pub(crate) fn fixture(broken: bool) -> (Value, Value) {
    let mut shared = builtin_blueprints()
        .revision(&id(NOT_TOP_REVISION))
        .unwrap()
        .clone();
    shared.id = id("shared.v1");
    shared.name = "Shared block and clearance".into();
    shared.classifications.clear();
    shared.blocks = vec![
        PositionedBlock {
            position: Pos::default(),
            block: Block::new(BlockKind::Solid),
        },
        PositionedBlock {
            position: if broken {
                Pos::new(2, 0, 0)
            } else {
                Pos::new(9, 0, 0)
            },
            block: Block::new(BlockKind::Air),
        },
    ];
    shared.ports = vec![BlueprintPort {
        name: "tap".into(),
        direction: PortDirection::Output,
        position: Pos::default(),
        kind: BlueprintPortKind::BlockPower,
        facing: None,
        required_source_types: vec![],
    }];
    let inner = parent(
        "inner.v1",
        None,
        NOT_TOP_REVISION,
        "b",
        NOT_TOP_REVISION,
        true,
    );
    let outer = parent(
        "parent.v1",
        None,
        NOT_TOP_REVISION,
        "inner",
        "inner.v1",
        false,
    );
    let next_inner = parent(
        "inner.v2",
        Some("inner.v1"),
        NOT_SIDE_REVISION,
        "replacement",
        NOT_SIDE_REVISION,
        true,
    );
    let next_outer = parent(
        "parent.v2",
        Some("parent.v1"),
        NOT_SIDE_REVISION,
        "inner",
        "inner.v2",
        false,
    );
    let actual = |layout: &str, source: &str| Assembly {
        name: "Nested shared NOT".into(),
        instances: vec![inclusion("root", source), inclusion("outside", "shared.v1")],
        blocks: builtin_blueprints()
            .revision(&id(layout))
            .unwrap()
            .blocks
            .clone(),
        known_regions: regions(),
        connections: vec![],
        boundaries: vec![],
    };
    let base = AssemblyRevision {
        id: AssemblyRevisionId::new("state.v1").unwrap(),
        parents: vec![],
        assembly: actual(NOT_TOP_REVISION, "parent.v1"),
    };
    let candidate = AssemblyRevision {
        id: AssemblyRevisionId::new("state.v2").unwrap(),
        parents: vec![base.id.clone()],
        assembly: actual(NOT_SIDE_REVISION, "parent.v2"),
    };
    // Reverse dependency order checks that import and proposal creation resolve exact pins.
    (
        json!({"blueprint":{"action":"import","records":{"revisions":[outer,inner,shared],"assemblies":[base]}}}),
        json!({"blueprint":{"action":"propose_update","request":{"title":"Use side NOT","description":"Change nested decomposition while retaining shared interpretations","base_state":"state.v1","base_parent":"parent.v1","candidate_parent":"parent.v2","parent_instance":["root"],"child_before":["inner","b"],"previous_child":NOT_TOP_REVISION,"child_after":["inner","replacement"],"next_child":NOT_SIDE_REVISION,"revisions":[next_outer,next_inner],"candidate_state":candidate}}}),
    )
}

pub(crate) fn placement_fixture() -> (Value, Value) {
    let positioned = |x| PositionedBlock {
        position: Pos::new(x, 0, 0),
        block: Block::new(BlockKind::Solid),
    };
    let layout = |x| BlueprintLayout {
        blocks: vec![positioned(x)],
        known_regions: vec![Region::new(Pos::new(-2, -2, -2), Pos::new(2, 2, 2))],
    };
    let mut child = builtin_blueprints()
        .revision(&id(NOT_TOP_REVISION))
        .unwrap()
        .clone();
    child.id = id("placement.child.v1");
    child.parents.clear();
    child.name = "Placement child v1".into();
    child.classifications.clear();
    child.behavior_bindings.clear();
    child.static_type_bindings.clear();
    child.required_laws.clear();
    child.blocks = layout(0).blocks.clone();
    child.initial_layout = None;
    child.inclusions.clear();
    child.ports.clear();
    child.connections.clear();
    child.port_bindings.clear();
    let mut next_child = child.clone();
    next_child.id = id("placement.child.v2");
    next_child.parents = vec![child.id.clone()];
    next_child.name = "Placement child v2".into();
    next_child.blocks = layout(1).blocks.clone();
    let mut parent = child.clone();
    parent.id = id("placement.parent.v1");
    parent.name = "Placement parent v1".into();
    parent.blocks.clear();
    parent.initial_layout = Some(layout(0));
    parent.inclusions = vec![inclusion("child", "placement.child.v1")];
    let mut next_parent = parent.clone();
    next_parent.id = id("placement.parent.v2");
    next_parent.parents = vec![parent.id.clone()];
    next_parent.name = "Placement parent v2".into();
    next_parent.initial_layout = Some(layout(1));
    next_parent.inclusions[0].revision = next_child.id.clone();
    let assembly = |name: &str, revision: &str, x| Assembly {
        name: name.into(),
        instances: vec![inclusion("root", revision)],
        blocks: layout(x).blocks,
        known_regions: layout(x).known_regions,
        connections: vec![],
        boundaries: vec![],
    };
    let base = AssemblyRevision {
        id: AssemblyRevisionId::new("placement.state.v1").unwrap(),
        parents: vec![],
        assembly: assembly("Placement state v1", "placement.parent.v1", 0),
    };
    let candidate = AssemblyRevision {
        id: AssemblyRevisionId::new("placement.state.v2").unwrap(),
        parents: vec![base.id.clone()],
        assembly: assembly("Placement state v2", "placement.parent.v2", 1),
    };
    (
        json!({"blueprint":{"action":"import","records":{"revisions":[parent,child],"assemblies":[base]}}}),
        json!({"blueprint":{"action":"propose_update","request":{
            "title":"Move one block","description":"Minimal adopted placement path fixture",
            "base_state":"placement.state.v1","base_parent":"placement.parent.v1",
            "candidate_parent":"placement.parent.v2","parent_instance":["root"],
            "child_before":["child"],"previous_child":"placement.child.v1",
            "child_after":["child"],"next_child":"placement.child.v2",
            "revisions":[next_parent,next_child],"candidate_state":candidate
        }}}),
    )
}

type Client = rmcp::service::RunningService<rmcp::RoleClient, ClientInfo>;
async fn start(root: &Path) -> (Client, tokio::task::JoinHandle<()>) {
    // A dead bridge endpoint proves these are local catalog operations, even with read-only world policy.
    let mut service =
        DustRouteMcp::with_policy_and_player("127.0.0.1:1", McpPolicy::default(), "Tester");
    service.state_store = PlanStateStore::new(root.to_path_buf(), 3600);
    let (server_io, client_io) = tokio::io::duplex(1024 * 1024);
    let server = tokio::spawn(async move {
        service
            .serve(server_io)
            .await
            .unwrap()
            .waiting()
            .await
            .unwrap();
    });
    (
        ClientInfo::default().serve(client_io).await.unwrap(),
        server,
    )
}
async fn call(client: &Client, name: &str, args: Value) -> Value {
    let result = client
        .call_tool(
            CallToolRequestParams::new(name.to_owned())
                .with_arguments(args.as_object().unwrap().clone()),
        )
        .await
        .unwrap();
    let ContentBlock::Text(text) = &result.content[0] else {
        panic!("expected JSON text")
    };
    serde_json::from_str(&text.text)
        .unwrap_or_else(|e| panic!("{name} returned non-JSON: {} ({e})", text.text))
}
fn temporary() -> PathBuf {
    std::env::temp_dir().join(format!("dustroute-mcp-blueprints-{}", uuid::Uuid::new_v4()))
}
async fn stop(client: Client, server: tokio::task::JoinHandle<()>) {
    client.cancel().await.unwrap();
    server.await.unwrap();
}

#[tokio::test]
async fn blueprint_mcp_round_trip_adopts_after_restart_without_minecraft() {
    let root = temporary();
    let (client, server) = start(&root).await;
    let tools = client.list_tools(None).await.unwrap();
    assert_eq!(tools.tools.len(), 22);
    let schema = serde_json::to_string(
        &tools
            .tools
            .iter()
            .find(|t| t.name == "test_circuit_change")
            .unwrap()
            .input_schema,
    )
    .unwrap();
    for field in [
        "candidate_state",
        "required_source_types",
        "known_regions",
        "behavior_context",
        "behavior_bindings",
        "static_type_bindings",
        "required_laws",
        "periodic",
    ] {
        assert!(schema.contains(field), "missing {field}");
    }
    let catalog = call(
        &client,
        "get_circuit_revision",
        json!({"blueprint":{"kind":"catalog","classification_id":NOT_CLASSIFICATION_REVISION}}),
    )
    .await;
    assert_eq!(catalog["result"]["total_blueprints"], 2);
    let (records, proposal) = fixture(false);
    assert_eq!(
        call(&client, "test_circuit_change", records).await["ok"],
        true
    );
    let created = call(&client, "test_circuit_change", proposal).await;
    assert_eq!(created["ok"], true, "{created}");
    let operation = created["operation_id"].clone();
    let shown = call(&client, "show_operation", json!({"operation_id":operation})).await;
    assert_eq!(shown["validation"]["status"], "passed", "{shown}");
    assert_eq!(shown["can_adopt"], true);
    assert!(shown["diff"]["blocks"].as_array().unwrap().len() > 1);
    assert_eq!(call(&client,"invoke_operation",json!({"operation_id":operation,"confirm":false,"blueprint_decision":{"action":"adopt"}})).await["ok"],false);
    assert_eq!(
        call(
            &client,
            "invoke_operation",
            json!({"operation_id":operation,"confirm":true})
        )
        .await["ok"],
        false
    );
    stop(client, server).await;
    let (client, server) = start(&root).await;
    let restored = call(&client, "get_operation", json!({"operation_id":operation})).await;
    assert_eq!(restored["operation"]["status"], "open");
    assert_eq!(restored["stored_history_is_validation_proof"], false);
    let adopted = call(
        &client,
        "invoke_operation",
        json!({"operation_id":operation,"confirm":true,"blueprint_decision":{"action":"adopt"}}),
    )
    .await;
    assert_eq!(adopted["ok"], true, "{adopted}");
    assert_eq!(adopted["operation"]["status"], "adopted");
    assert_eq!(adopted["writes_minecraft"], false);
    assert_eq!(adopted["operation"]["history"].as_array().unwrap().len(), 3);
    let old = call(
        &client,
        "get_circuit_revision",
        json!({"blueprint":{"kind":"blueprint","id":"inner.v1"}}),
    )
    .await;
    assert_eq!(
        old["result"]["record"]["inclusions"][0]["revision"],
        NOT_TOP_REVISION
    );
    let state = call(
        &client,
        "get_circuit_revision",
        json!({"blueprint":{"kind":"assembly","id":"state.v2","validate":true}}),
    )
    .await;
    assert_eq!(state["result"]["validation"]["status"], "passed");
    assert_eq!(state["result"]["validation"]["live_world_verified"], false);
    assert_eq!(
        call(&client, "show_operation", json!({"operation_id":operation})).await["can_adopt"],
        false
    );
    assert_eq!(call(&client,"invoke_operation",json!({"operation_id":operation,"confirm":true,"blueprint_decision":{"action":"reject","reason":"too late"}})).await["ok"],false);
    assert_eq!(
        call(
            &client,
            "undo_operation",
            json!({"operation_id":operation,"confirm":true})
        )
        .await["ok"],
        false
    );
    let archive = call(
        &client,
        "get_circuit_revision",
        json!({"blueprint":{"kind":"archive"}}),
    )
    .await;
    let loaded = dustroute_translate::blueprint_update::BlueprintUpdates::from_json(
        &archive["result"]["archive"].to_string(),
    )
    .unwrap();
    assert_eq!(loaded.proposals().count(), 1);
    stop(client, server).await;
    let (client, server) = start(&root).await;
    assert_eq!(
        call(&client, "get_operation", json!({"operation_id":operation})).await["operation"]["status"],
        "adopted"
    );
    stop(client, server).await;
    fs::remove_dir_all(root).unwrap();
}

#[tokio::test]
async fn blueprint_mcp_rechecks_wire_to_block_arms_after_restart() {
    use dustroute_translate::{Facing, WireConnection};
    for arm in [false, true] {
        let root = temporary();
        let (client, server) = start(&root).await;
        let (mut records, mut proposal) = fixture(false);
        let mut world = dustroute_translate::World::new();
        let dust = Pos::new(4, 1, 2);
        let sink = Pos::new(5, 1, 2);
        world.set(Pos::new(4, 0, 2), Block::new(BlockKind::Solid));
        world.set(Pos::new(4, 1, 1), Block::new(BlockKind::RedstoneBlock));
        world.set(sink, Block::new(BlockKind::Solid));
        world.place(BlockKind::RedstoneWire, dust).wire_connections = Some(
            [
                (Facing::North, WireConnection::Side),
                (Facing::South, WireConnection::Side),
                (
                    Facing::East,
                    if arm {
                        WireConnection::Side
                    } else {
                        WireConnection::None
                    },
                ),
                (Facing::West, WireConnection::None),
            ]
            .into_iter()
            .collect(),
        );
        let mut route = builtin_blueprints()
            .revision(&id(TERMINAL_REVISION))
            .unwrap()
            .clone();
        route.id = id("route.v1");
        route.classifications.clear();
        route.blocks = world
            .iter()
            .map(|(position, block)| PositionedBlock {
                position: *position,
                block: block.clone(),
            })
            .collect();
        route.ports = vec![
            BlueprintPort {
                name: "out".into(),
                direction: PortDirection::Output,
                position: dust,
                kind: BlueprintPortKind::Wire,
                facing: None,
                required_source_types: vec![],
            },
            BlueprintPort {
                name: "in".into(),
                direction: PortDirection::Input,
                position: sink,
                kind: BlueprintPortKind::BlockPower,
                facing: Some(Facing::West),
                required_source_types: vec![],
            },
        ];
        let actual = &mut proposal["blueprint"]["request"]["candidate_state"]["assembly"];
        actual["blocks"].as_array_mut().unwrap().extend(
            route
                .blocks
                .iter()
                .map(|b| serde_json::to_value(b).unwrap()),
        );
        actual["instances"]
            .as_array_mut()
            .unwrap()
            .push(serde_json::to_value(inclusion("route", "route.v1")).unwrap());
        actual["connections"] = json!([AssemblyConnection {
            source: AssemblyPortRef {
                instance: vec![InstanceId::new("route").unwrap()],
                port: "out".into()
            },
            sink: AssemblyPortRef {
                instance: vec![InstanceId::new("route").unwrap()],
                port: "in".into()
            },
            path: vec![dust, sink],
        }]);
        records["blueprint"]["records"]["revisions"]
            .as_array_mut()
            .unwrap()
            .push(serde_json::to_value(&route).unwrap());
        let original = proposal["blueprint"]["request"]["candidate_state"].clone();
        assert_eq!(
            call(&client, "test_circuit_change", records).await["ok"],
            true
        );
        let created = call(&client, "test_circuit_change", proposal).await;
        assert_eq!(created["ok"], true, "{created}");
        let operation = created["operation_id"].clone();
        let shown = call(
            &client,
            "show_operation",
            json!({"operation_id": operation}),
        )
        .await;
        assert_eq!(shown["can_adopt"], arm, "{shown}");
        if !arm {
            assert!(
                shown["validation"]["checks"]
                    .to_string()
                    .contains("DisconnectedStep")
            );
        }
        stop(client, server).await;
        let (client, server) = start(&root).await;
        let result = call(&client, "invoke_operation", json!({"operation_id": operation, "confirm": true, "blueprint_decision": {"action":"adopt"}})).await;
        assert_eq!(result["ok"], arm, "{result}");
        if !arm {
            assert_eq!(result["error_code"], "verification_failed");
            assert_eq!(result["operation"]["request"]["candidate_state"], original);
        }
        assert_eq!(
            call(
                &client,
                "get_circuit_revision",
                json!({"blueprint":{"kind":"assembly","id":"state.v2"}})
            )
            .await["ok"],
            arm
        );
        assert_eq!(
            call(
                &client,
                "get_circuit_revision",
                json!({"blueprint":{"kind":"assembly","id":"state.v1"}})
            )
            .await["ok"],
            true
        );
        stop(client, server).await;
        fs::remove_dir_all(root).unwrap();
    }
}

#[tokio::test]
async fn blueprint_mcp_rejects_obstructed_wire_rise_after_restart_and_preserves_literal_candidate()
{
    let root = temporary();
    let (client, server) = start(&root).await;
    let (records, mut proposal) = fixture(false);
    let mut extra = dustroute_translate::World::new();
    extra.set(Pos::new(4, 0, 2), Block::new(BlockKind::Solid));
    extra.set(Pos::new(5, 1, 2), Block::new(BlockKind::Solid));
    extra.place(BlockKind::RedstoneWire, Pos::new(4, 1, 2));
    extra.place(BlockKind::RedstoneWire, Pos::new(5, 2, 2));
    dustroute_translate::update_wire_shapes(&mut extra);
    extra.set(Pos::new(4, 2, 2), Block::new(BlockKind::Solid));
    proposal["blueprint"]["request"]["candidate_state"]["assembly"]["blocks"]
        .as_array_mut()
        .unwrap()
        .extend(extra.iter().map(|(pos, block)| {
            serde_json::to_value(PositionedBlock {
                position: *pos,
                block: block.clone(),
            })
            .unwrap()
        }));
    let original = proposal["blueprint"]["request"]["candidate_state"].clone();
    assert_eq!(
        call(&client, "test_circuit_change", records).await["ok"],
        true
    );
    let created = call(&client, "test_circuit_change", proposal).await;
    assert_eq!(created["ok"], true, "{created}");
    let operation = created["operation_id"].clone();
    let shown = call(&client, "show_operation", json!({"operation_id":operation})).await;
    assert_eq!(shown["validation"]["status"], "failed", "{shown}");
    assert_eq!(
        shown["validation"]["placement_validation_profile"],
        dustroute_translate::ValidatedWorld::PROFILE
    );
    assert_eq!(shown["can_adopt"], false);
    assert!(
        shown["validation"]["checks"]
            .to_string()
            .contains("obstructed")
    );
    stop(client, server).await;
    let (client, server) = start(&root).await;
    let refused = call(
        &client,
        "invoke_operation",
        json!({"operation_id":operation,"confirm":true,"blueprint_decision":{"action":"adopt"}}),
    )
    .await;
    assert_eq!(refused["ok"], false, "{refused}");
    assert_eq!(refused["error_code"], "verification_failed");
    assert_eq!(refused["operation"]["request"]["candidate_state"], original);
    assert_eq!(
        call(
            &client,
            "get_circuit_revision",
            json!({"blueprint":{"kind":"assembly","id":"state.v2"}})
        )
        .await["ok"],
        false
    );
    assert_eq!(
        call(
            &client,
            "get_circuit_revision",
            json!({"blueprint":{"kind":"assembly","id":"state.v1"}})
        )
        .await["ok"],
        true
    );
    stop(client, server).await;
    fs::remove_dir_all(root).unwrap();
}

#[tokio::test]
async fn blueprint_mcp_revalidates_forged_saved_pass_and_retains_rejected_old_revisions() {
    let root = temporary();
    let (client, server) = start(&root).await;
    let (records, proposal) = fixture(true);
    assert_eq!(
        call(&client, "test_circuit_change", records).await["ok"],
        true
    );
    let created = call(&client, "test_circuit_change", proposal).await;
    assert_eq!(created["ok"], true, "{created}");
    let operation = created["operation_id"].clone();
    let shown = call(&client, "show_operation", json!({"operation_id":operation})).await;
    assert_eq!(shown["validation"]["status"], "failed", "{shown}");
    assert_eq!(shown["can_adopt"], false);
    let checks = shown["validation"]["checks"]["occurrences"]
        .as_array()
        .unwrap();
    let parent = checks
        .iter()
        .find(|entry| entry[1]["revision"] == "parent.v2")
        .unwrap();
    assert!(
        parent[1]["checks"]
            .as_array()
            .unwrap()
            .iter()
            .all(|check| check["status"] == "passed")
    );
    stop(client, server).await;
    // Stored diagnostics deliberately claim a pass. Adoption must compute a new review.
    let path = PlanStateStore::new(root.clone(), 1)
        .blueprint_root("Tester")
        .join("catalog.json");
    let mut saved: Value = serde_json::from_slice(&fs::read(&path).unwrap()).unwrap();
    saved["archive"]["proposals"][0]["events"][0]["report"] =
        json!({"occurrences":[],"arrangement":[]});
    fs::write(&path, serde_json::to_vec(&saved).unwrap()).unwrap();
    let (client, server) = start(&root).await;
    let refused = call(
        &client,
        "invoke_operation",
        json!({"operation_id":operation,"confirm":true,"blueprint_decision":{"action":"adopt"}}),
    )
    .await;
    assert_eq!(refused["ok"], false, "{refused}");
    assert_eq!(refused["error_code"], "verification_failed");
    assert_eq!(refused["operation"]["status"], "open");
    assert_eq!(
        call(
            &client,
            "get_circuit_revision",
            json!({"blueprint":{"kind":"blueprint","id":"parent.v2"}})
        )
        .await["ok"],
        false
    );
    let rejected=call(&client,"invoke_operation",json!({"operation_id":operation,"confirm":true,"blueprint_decision":{"action":"reject","reason":"Keep the previous shared layout"}})).await;
    assert_eq!(rejected["operation"]["status"], "rejected", "{rejected}");
    stop(client, server).await;
    let (client, server) = start(&root).await;
    assert_eq!(
        call(&client, "get_operation", json!({"operation_id":operation})).await["operation"]["status"],
        "rejected"
    );
    assert_eq!(
        call(
            &client,
            "get_circuit_revision",
            json!({"blueprint":{"kind":"assembly","id":"state.v1"}})
        )
        .await["ok"],
        true
    );
    stop(client, server).await;
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn blueprint_store_is_atomic_scoped_and_locked_across_instances() {
    use crate::blueprint_mcp::{BlueprintRead, Command, execute};
    let root = temporary();
    let store = PlanStateStore::new(root.clone(), 1);
    let (records, _) = fixture(false);
    execute(
        &store,
        "Tester",
        Command::Write(serde_json::from_value(records["blueprint"].clone()).unwrap()),
    )
    .unwrap();
    let path = store.blueprint_root("Tester").join("catalog.json");
    let before = fs::read(&path).unwrap();
    let mut valid = builtin_blueprints()
        .revision(&id(NOT_TOP_REVISION))
        .unwrap()
        .clone();
    valid.id = id("new.v1");
    let mut rebound = builtin_blueprints()
        .revision(&id(NOT_TOP_REVISION))
        .unwrap()
        .clone();
    rebound.name = "Rebound".into();
    let write =
        serde_json::from_value(json!({"action":"import","records":{"revisions":[valid,rebound]}}))
            .unwrap();
    assert!(execute(&store, "Tester", Command::Write(write)).is_err());
    assert_eq!(fs::read(&path).unwrap(), before);
    assert!(
        execute(
            &store,
            "Other",
            Command::Read(BlueprintRead::Blueprint {
                id: id("parent.v1")
            })
        )
        .is_err()
    );
    let lock = fs::OpenOptions::new()
        .read(true)
        .write(true)
        .open(store.blueprint_root("Tester").join("catalog.lock"))
        .unwrap();
    fs2::FileExt::try_lock_exclusive(&lock).unwrap();
    assert!(
        execute(&store, "Tester", Command::Read(BlueprintRead::Archive))
            .unwrap_err()
            .contains("busy")
    );
    drop(lock);
    let other = PlanStateStore::new(root.clone(), 1);
    assert!(
        execute(&other, "Tester", Command::Read(BlueprintRead::Archive))
            .unwrap()
            .is_some()
    );
    let mut legacy: Value = serde_json::from_slice(&fs::read(&path).unwrap()).unwrap();
    legacy.as_object_mut().unwrap().remove("groundings");
    fs::write(&path, serde_json::to_vec(&legacy).unwrap()).unwrap();
    assert!(
        execute(&other, "Tester", Command::Read(BlueprintRead::Archive))
            .unwrap()
            .is_some()
    );
    fs::write(&path, "broken archive").unwrap();
    assert!(execute(&other, "Tester", Command::Read(BlueprintRead::Archive)).is_err());
    assert_eq!(fs::read_to_string(&path).unwrap(), "broken archive");
    fs::remove_dir_all(root).unwrap();
}

#[tokio::test]
async fn blueprint_mcp_unknown_clearance_blocks_adoption() {
    let root = temporary();
    let (client, server) = start(&root).await;
    let (records, mut proposal) = fixture(false);
    proposal["blueprint"]["request"]["candidate_state"]["assembly"]["known_regions"] = json!([]);
    assert_eq!(
        call(&client, "test_circuit_change", records).await["ok"],
        true
    );
    let created = call(&client, "test_circuit_change", proposal).await;
    assert_eq!(created["ok"], true, "{created}");
    let refused = call(&client, "invoke_operation", json!({"operation_id":created["operation_id"],"confirm":true,"blueprint_decision":{"action":"adopt"}})).await;
    assert_eq!(refused["error_code"], "verification_failed", "{refused}");
    assert_eq!(refused["validation"]["status"], "undetermined");
    assert_eq!(refused["operation"]["status"], "open");
    stop(client, server).await;
    fs::remove_dir_all(root).unwrap();
}

#[tokio::test]
async fn blueprint_mcp_captures_saved_assembly_and_rejects_mixed_operations() {
    let root = temporary();
    let mut service =
        DustRouteMcp::with_policy_and_player("127.0.0.1:1", McpPolicy::default(), "Tester");
    service.state_store = PlanStateStore::new(root.clone(), 3600);
    let snapshot = serde_json::from_value(json!({"min":{"x":0,"y":0,"z":0},"max":{"x":1,"y":1,"z":1},"blocks":[{"pos":{"x":0,"y":0,"z":0},"name":"minecraft:stone","properties":{}}]})).unwrap();
    let observed = service
        .store_circuit(StoredCircuit {
            player: "Tester".into(),
            dimension: "minecraft:overworld".into(),
            bounds: dustroute_translate::RegionBounds::new(Pos::default(), Pos::new(1, 1, 1)),
            target: None,
            snapshot,
            expansion: json!({}),
            complete: true,
            expires_at: Instant::now() + Duration::from_secs(300),
        })
        .await;
    let draft: Value = serde_json::from_str(
        &service
            .test_circuit_change(Parameters(
                serde_json::from_value(json!({"circuit_id":observed,"changes":[]})).unwrap(),
            ))
            .await,
    )
    .unwrap();
    assert_eq!(draft["ok"], true, "{draft}");
    let (client, server) = start(&root).await;
    let captured = call(
        &client,
        "test_circuit_change",
        json!({"blueprint":{"action":"capture_revision","revision_id":draft["revision_id"]}}),
    )
    .await;
    assert_eq!(captured["ok"], true, "{captured}");
    assert_eq!(
        captured["assembly_revision_id"],
        draft["assembly_state"]["assembly_revision_id"]
    );
    let result = call(
        &client,
        "get_circuit_revision",
        json!({"blueprint":{"kind":"assembly","id":captured["assembly_revision_id"]}}),
    )
    .await;
    assert_eq!(
        result["result"]["record"]["assembly"]["instances"],
        json!([])
    );
    assert_eq!(
        result["result"]["record"]["assembly"]["blocks"]
            .as_array()
            .unwrap()
            .len(),
        1
    );
    assert_eq!(result["result"]["lifecycle"]["adopted"], false);
    assert_eq!(
        result["result"]["lifecycle"]["status"],
        "catalog_only_unadopted"
    );
    // Retained catalog state no longer depends on the source's short-lived file.
    fs::remove_dir_all(root.join("circuit_revisions")).unwrap();
    assert_eq!(
        call(
            &client,
            "get_circuit_revision",
            json!({"blueprint":{"kind":"assembly","id":captured["assembly_revision_id"]}})
        )
        .await["ok"],
        true
    );
    assert_eq!(
        call(
            &client,
            "get_circuit_revision",
            json!({"revision_id":draft["revision_id"],"blueprint":{"kind":"catalog"}})
        )
        .await["ok"],
        false
    );
    assert_eq!(
        call(
            &client,
            "test_circuit_change",
            json!({"revision_id":draft["revision_id"],"blueprint":{"action":"import","records":{}}})
        )
        .await["ok"],
        false
    );
    assert_eq!(call(&client, "invoke_operation", json!({"operation_id":uuid::Uuid::new_v4(),"confirm":true,"contracts":[],"blueprint_decision":{"action":"adopt"}})).await["ok"], false);
    stop(client, server).await;
    fs::remove_dir_all(root).unwrap();
}

#[tokio::test]
async fn periodic_blueprint_uses_existing_tools_and_reverifies_after_restart() {
    use dustroute_library::behavior_type::{Periodic, PhysicalBehaviorProfile};
    behavior_blueprint_round_trip(
        TypeContract::Periodic {
            requirement: Periodic {
                output: "signal".into(),
            },
        },
        PhysicalBehaviorProfile::DustTorchSynchronousGameTickV1,
        PhysicalBehaviorProfile::DustSingleTorchBlockEffectsV1,
    )
    .await;
}

#[tokio::test]
async fn finite_burst_blueprint_uses_existing_tools_and_reverifies_after_restart() {
    use dustroute_library::behavior_type::{FiniteBurst, PhysicalBehaviorProfile};
    behavior_blueprint_round_trip(
        TypeContract::FiniteBurst {
            requirement: FiniteBurst {
                output: "signal".into(),
            },
        },
        PhysicalBehaviorProfile::DustSingleTorchBlockEffectsV1,
        PhysicalBehaviorProfile::DustTorchSynchronousGameTickV1,
    )
    .await;
}

async fn behavior_blueprint_round_trip(
    contract: TypeContract,
    passing_profile: dustroute_library::behavior_type::PhysicalBehaviorProfile,
    failing_profile: dustroute_library::behavior_type::PhysicalBehaviorProfile,
) {
    use dustroute_library::behavior_type::{BehaviorInitialCondition, PhysicalBehaviorContext};
    use dustroute_library::builtin_laws::{DUST_LAW_REVISION, TORCH_LAW_REVISION, builtin_laws};
    let repeated = matches!(contract, TypeContract::RepeatedSettling { .. });
    let expected_scope = if repeated || matches!(contract, TypeContract::FiniteBurst { .. }) {
        "placement_connections_and_declared_behavioral_obligations"
    } else {
        "placement_connections_and_declared_periodic_obligations"
    };
    let root = temporary();
    let mut world = dustroute_translate::World::new();
    world.set(Pos::new(0, 0, 0), Block::new(BlockKind::Solid));
    world.set(Pos::new(1, 1, 0), Block::new(BlockKind::Solid));
    let torch = world.place(BlockKind::RedstoneTorch, Pos::new(1, 0, 0));
    torch.facing = Some(dustroute_translate::Facing::East);
    torch.support_offset = Some(Pos::new(-1, 0, 0));
    if repeated {
        let lever = world.place(BlockKind::Lever, Pos::new(-1, 0, 0));
        lever.support_offset = Some(Pos::new(1, 0, 0));
        lever.powered = Some(false);
    } else {
        world.place(BlockKind::RedstoneWire, Pos::new(0, 1, 0));
    }
    let definition = TypeRevision {
        id: TypeRevisionId::new("mcp.clock.type.v1").unwrap(),
        name: "Declared autonomous behavior".into(),
        contract,
    };
    let mut clock = builtin_blueprints()
        .revision(&id(TERMINAL_REVISION))
        .unwrap()
        .clone();
    clock.id = id("mcp.clock.v1");
    clock.classifications.clear();
    clock.blocks = world
        .iter()
        .map(|(p, b)| PositionedBlock {
            position: *p,
            block: b.clone(),
        })
        .collect();
    clock.ports = vec![BlueprintPort {
        name: "pulse".into(),
        direction: PortDirection::Output,
        position: Pos::new(0, 1, 0),
        kind: BlueprintPortKind::Wire,
        facing: None,
        required_source_types: vec![],
    }];
    clock.behavior_bindings = vec![BehaviorBinding::Autonomous {
        behavior_type: definition.id.clone(),
        output_port: "pulse".into(),
    }];
    if repeated {
        clock.ports[0].position = Pos::new(1, 1, 0);
        clock.ports[0].kind = BlueprintPortKind::BlockPower;
        clock.ports.push(BlueprintPort {
            name: "input".into(),
            direction: PortDirection::Input,
            position: Pos::default(),
            kind: BlueprintPortKind::BlockPower,
            facing: None,
            required_source_types: vec![],
        });
        clock.behavior_bindings = vec![BehaviorBinding::RepeatedSettling {
            behavior_type: definition.id.clone(),
            inputs: std::collections::BTreeMap::from([("a".into(), "input".into())]),
            outputs: std::collections::BTreeMap::from([("signal".into(), "pulse".into())]),
        }];
    }
    let mut next_clock = clock.clone();
    next_clock.id = id("mcp.clock.v2");
    next_clock.parents = vec![clock.id.clone()];
    let mut parent = clock.clone();
    parent.id = id("mcp.parent.v1");
    parent.blocks.clear();
    parent.ports.clear();
    parent.behavior_bindings.clear();
    parent.inclusions = vec![inclusion("clock", "mcp.clock.v1")];
    let mut next_parent = parent.clone();
    next_parent.id = id("mcp.parent.v2");
    next_parent.parents = vec![parent.id.clone()];
    next_parent.inclusions[0].revision = next_clock.id.clone();
    let base = AssemblyRevision {
        id: AssemblyRevisionId::new("mcp.clock.state.v1").unwrap(),
        parents: vec![],
        assembly: Assembly {
            name: "Autonomous clock assembly".into(),
            instances: vec![inclusion("root", "mcp.parent.v1")],
            blocks: clock.blocks.clone(),
            known_regions: regions(),
            connections: vec![],
            boundaries: vec![],
        },
    };
    let mut candidate = base.clone();
    candidate.id = AssemblyRevisionId::new("mcp.clock.state.v2").unwrap();
    candidate.parents = vec![base.id.clone()];
    candidate.assembly.instances[0].revision = next_parent.id.clone();
    let context = PhysicalBehaviorContext {
        profile: passing_profile,
        initial_condition: BehaviorInitialCondition::FreshConstruction,
        dust_law: id(DUST_LAW_REVISION),
        torch_law: id(TORCH_LAW_REVISION),
        max_electrical_iterations: 128,
        input_drivers: if repeated {
            vec![dustroute_library::behavior_type::PhysicalInputDriver {
                port_position: Pos::default(),
                port_kind: BlueprintPortKind::BlockPower,
                lever_position: Pos::new(-1, 0, 0),
            }]
        } else {
            vec![]
        },
    };
    let mut revisions: Vec<_> = builtin_laws().revisions().cloned().collect();
    revisions.extend([clock.clone(), next_clock.clone(), parent.clone()]);
    let (client, server) = start(&root).await;
    let imported = call(&client, "test_circuit_change", json!({"blueprint":{"action":"import","records":{"types":[definition],"revisions":revisions,"assemblies":[base]}}})).await;
    assert_eq!(imported["ok"], true, "{imported}");
    let snapshot = call(
        &client,
        "get_circuit_revision",
        json!({"blueprint":{"kind":"assembly","id":base.id,"validate":true}}),
    )
    .await;
    assert_eq!(
        snapshot["result"]["validation"]["status"], "undetermined",
        "{snapshot}"
    );
    assert_eq!(snapshot["result"]["validation"]["behavior_verified"], false);
    let verified = call(&client, "get_circuit_revision", json!({"blueprint":{"kind":"assembly","id":base.id,"validate":true,"behavior_context":context}})).await;
    assert_eq!(
        verified["result"]["world_execution_context"],
        serde_json::to_value(context.execution_context()).unwrap()
    );
    assert_eq!(
        verified["result"]["validation_context"],
        serde_json::to_value(&context).unwrap()
    );
    assert_eq!(
        verified["result"]["validation"]["status"], "passed",
        "{verified}"
    );
    assert_eq!(
        verified["result"]["validation"]["declared_behavior_verified"],
        true
    );
    assert_eq!(
        verified["result"]["validation"]["live_world_verified"],
        false
    );
    // Context and type are independent: recheck under a profile that fails
    // this obligation, retaining all immutable source pins and both requests.
    let failing_context = PhysicalBehaviorContext {
        profile: failing_profile,
        ..context.clone()
    };
    let effects = call(&client, "get_circuit_revision", json!({"blueprint":{"kind":"assembly","id":base.id,"validate":true,"behavior_context":failing_context}})).await;
    assert_eq!(
        effects["result"]["validation"]["status"],
        if repeated { "undetermined" } else { "failed" },
        "{effects}"
    );
    assert_eq!(
        effects["result"]["validation"]["declared_behavior_verified"],
        false
    );
    let mut failing_parent = next_parent.clone();
    failing_parent.id = id("mcp.parent.effects.v2");
    let mut failing_state = candidate.clone();
    failing_state.id = AssemblyRevisionId::new("mcp.clock.state.effects.v2").unwrap();
    failing_state.assembly.instances[0].revision = failing_parent.id.clone();
    let refused = call(&client, "test_circuit_change", json!({"blueprint":{"action":"propose_update","request":{
        "title":"Proposal under a failing context","description":"Retain explicit failing behavior context",
        "base_state":base.id,"base_parent":parent.id,"candidate_parent":failing_parent.id,"parent_instance":["root"],
        "child_before":["clock"],"previous_child":clock.id,"child_after":["clock"],"next_child":next_clock.id,
        "revisions":[failing_parent],"candidate_state":failing_state,"behavior_context":failing_context
    }}})).await;
    assert_eq!(refused["ok"], true, "{refused}");
    let failing_operation = refused["operation_id"].clone();
    let shown = call(
        &client,
        "show_operation",
        json!({"operation_id":failing_operation}),
    )
    .await;
    assert_eq!(shown["can_adopt"], false, "{shown}");
    assert_eq!(shown["validation"]["declared_behavior_verified"], false);
    let created = call(&client, "test_circuit_change", json!({"blueprint":{"action":"propose_update","request":{
        "title":"Clock revision proposal","description":"Explicit autonomous obligations",
        "base_state":base.id,"base_parent":parent.id,"candidate_parent":next_parent.id,"parent_instance":["root"],
        "child_before":["clock"],"previous_child":clock.id,"child_after":["clock"],"next_child":next_clock.id,
        "revisions":[next_parent],"candidate_state":candidate,"behavior_context":context
    }}})).await;
    assert_eq!(created["ok"], true, "{created}");
    let operation = created["operation_id"].clone();
    let shown = call(&client, "show_operation", json!({"operation_id":operation})).await;
    assert_eq!(shown["can_adopt"], true, "{shown}");
    assert_eq!(shown["validation"]["declared_behavior_verified"], true);
    assert_eq!(shown["validation"]["scope"], expected_scope);
    stop(client, server).await;
    let (client, server) = start(&root).await;
    let refused_after_restart = call(&client, "invoke_operation", json!({"operation_id":failing_operation,"confirm":true,"blueprint_decision":{"action":"adopt"}})).await;
    assert_eq!(
        refused_after_restart["ok"], false,
        "{refused_after_restart}"
    );
    let retained = call(
        &client,
        "show_operation",
        json!({"operation_id":failing_operation}),
    )
    .await;
    assert_eq!(
        retained["operation"]["request"]["behavior_context"],
        serde_json::to_value(&failing_context).unwrap()
    );
    assert_eq!(retained["can_adopt"], false);
    let adopted = call(
        &client,
        "invoke_operation",
        json!({"operation_id":operation,"confirm":true,"blueprint_decision":{"action":"adopt"}}),
    )
    .await;
    assert_eq!(adopted["ok"], true, "{adopted}");
    assert_eq!(adopted["operation"]["status"], "adopted");
    assert_eq!(
        adopted["operation"]["request"]["behavior_context"],
        serde_json::to_value(context).unwrap()
    );
    assert_eq!(adopted["writes_minecraft"], false);
    stop(client, server).await;
    fs::remove_dir_all(root).unwrap();
}

#[tokio::test]
async fn repeated_settling_blueprint_reverifies_port_drivers_and_adoption_after_restart() {
    use dustroute_library::behavior_type::{BooleanRow, PhysicalBehaviorProfile, RepeatedSettling};
    behavior_blueprint_round_trip(
        TypeContract::RepeatedSettling {
            relation: RepeatedSettling {
                inputs: vec!["a".into()],
                outputs: vec!["signal".into()],
                rows: [false, true]
                    .into_iter()
                    .map(|b| BooleanRow {
                        inputs: vec![b],
                        outputs: vec![!b],
                    })
                    .collect(),
            },
        },
        PhysicalBehaviorProfile::DustSingleTorchBlockEffectsV1,
        PhysicalBehaviorProfile::DustTorchSynchronousGameTickV1,
    )
    .await;
}

#[tokio::test]
async fn blueprint_optimization_moves_ports_and_reenters_persisted_explicit_adoption() {
    use dustroute_library::behavior_type::{
        BehaviorInitialCondition, BooleanRow, PhysicalBehaviorContext, PhysicalBehaviorProfile,
        PhysicalInputDriver, RepeatedSettling,
    };
    use dustroute_library::builtin_laws::{DUST_LAW_REVISION, TORCH_LAW_REVISION, builtin_laws};
    use dustroute_optimize::blueprint_reduction::{
        BlueprintReductionCandidate, BlueprintReductionRequest,
    };
    let root = temporary();
    let type_id = TypeRevisionId::new("mcp.reduced-not.type.v1").unwrap();
    let definition = TypeRevision {
        id: type_id.clone(),
        name: "Repeated NOT".into(),
        contract: TypeContract::RepeatedSettling {
            relation: RepeatedSettling {
                inputs: vec!["a".into()],
                outputs: vec!["out".into()],
                rows: [false, true]
                    .into_iter()
                    .map(|b| BooleanRow {
                        inputs: vec![b],
                        outputs: vec![!b],
                    })
                    .collect(),
            },
        },
    };
    let mut world = dustroute_translate::World::new();
    for p in [Pos::default(), Pos::new(1, 1, 0), Pos::new(-2, -1, 0)] {
        world.set(p, Block::new(BlockKind::Solid));
    }
    let lever = world.place(BlockKind::Lever, Pos::new(-1, 0, 0));
    lever.support_offset = Some(Pos::new(1, 0, 0));
    lever.powered = Some(false);
    let torch = world.place(BlockKind::RedstoneTorch, Pos::new(1, 0, 0));
    torch.support_offset = Some(Pos::new(-1, 0, 0));
    torch.facing = Some(dustroute_translate::Facing::East);
    world.place(BlockKind::RedstoneWire, Pos::new(-2, 0, 0));
    world.place(BlockKind::RedstoneWire, Pos::new(1, 2, 0));
    let mut source = builtin_blueprints()
        .revision(&id(TERMINAL_REVISION))
        .unwrap()
        .clone();
    source.id = id("mcp.reduction.source.v1");
    source.classifications.clear();
    source.blocks = world
        .iter()
        .map(|(p, b)| PositionedBlock {
            position: *p,
            block: b.clone(),
        })
        .collect();
    source.ports = [
        ("input", Pos::new(-2, 0, 0), PortDirection::Input),
        ("output", Pos::new(1, 2, 0), PortDirection::Output),
    ]
    .into_iter()
    .map(|(name, position, direction)| BlueprintPort {
        name: name.into(),
        position,
        direction,
        kind: BlueprintPortKind::Wire,
        facing: None,
        required_source_types: vec![],
    })
    .collect();
    source.behavior_bindings = vec![BehaviorBinding::RepeatedSettling {
        behavior_type: type_id,
        inputs: std::collections::BTreeMap::from([("a".into(), "input".into())]),
        outputs: std::collections::BTreeMap::from([("out".into(), "output".into())]),
    }];
    let mut parent = source.clone();
    parent.id = id("mcp.reduction.parent.v1");
    parent.blocks.clear();
    parent.ports.clear();
    parent.behavior_bindings.clear();
    parent.inclusions = vec![
        inclusion("child", source.id.as_str()),
        inclusion("shared", source.id.as_str()),
    ];
    let base = AssemblyRevision {
        id: AssemblyRevisionId::new("mcp.reduction.base.v1").unwrap(),
        parents: vec![],
        assembly: Assembly {
            name: "NOT with shared interpretations".into(),
            instances: vec![inclusion("root", parent.id.as_str())],
            blocks: source.blocks.clone(),
            known_regions: regions(),
            connections: vec![],
            boundaries: vec![],
        },
    };
    let context = PhysicalBehaviorContext {
        profile: PhysicalBehaviorProfile::DustSingleTorchBlockEffectsV1,
        initial_condition: BehaviorInitialCondition::FreshConstruction,
        dust_law: id(DUST_LAW_REVISION),
        torch_law: id(TORCH_LAW_REVISION),
        max_electrical_iterations: 128,
        input_drivers: vec![PhysicalInputDriver {
            port_position: Pos::new(-2, 0, 0),
            port_kind: BlueprintPortKind::Wire,
            lever_position: Pos::new(-1, 0, 0),
        }],
    };
    let primitives = dustroute_library::builtin_primitives::builtin_primitives();
    let mut equipment = inclusion(
        "control",
        dustroute_library::builtin_primitives::LEVER_REVISION,
    );
    equipment.origin = Pos::new(-1, 0, 0);
    let request = BlueprintReductionRequest {
        scope: dustroute_optimize::blueprint_reduction::BlueprintReductionScope::Component {
            body_positions: base
                .assembly
                .blocks
                .iter()
                .filter(|b| b.block.kind != BlockKind::Lever)
                .map(|b| b.position)
                .collect(),
            environment_instances: vec![equipment.clone()],
        },
        base_state: base.id.clone(),
        target_instance: vec![
            InstanceId::new("root").unwrap(),
            InstanceId::new("child").unwrap(),
        ],
        target: source.behavior_bindings[0].clone(),
        candidate_revision: id("mcp.reduction.source.v2"),
        candidate_state: AssemblyRevisionId::new("mcp.reduction.state.v2").unwrap(),
        behavior_context: context,
        alternatives: vec![],
    };
    let mut revisions: Vec<_> = builtin_laws().revisions().cloned().collect();
    revisions.extend([source.clone(), parent.clone()]);
    revisions.extend(primitives.revisions().cloned());
    let mut types = vec![definition];
    types.extend(primitives.type_revisions().cloned());
    let (client, server) = start(&root).await;
    let imported=call(&client,"test_circuit_change",json!({"blueprint":{"action":"import","records":{"types":types,"revisions":revisions,"assemblies":[base]}}})).await;
    assert_eq!(imported["ok"], true, "{imported}");
    let enumerated = call(&client, "test_circuit_change", json!({"blueprint":{
        "action":"enumerate_layouts", "request":{"search":request,"support_position":Pos::default()}
    }})).await;
    assert_eq!(enumerated["ok"], true, "{enumerated}");
    assert_eq!(enumerated["catalog_changed"], false);
    assert_eq!(enumerated["result"]["family_exhausted"], true);
    let layouts = enumerated["result"]["candidates"].as_array().unwrap();
    assert_eq!(layouts.len(), 5);
    assert_eq!(
        layouts.iter().filter(|c| c["status"] == "passed").count(),
        4,
        "{enumerated}"
    );
    assert_eq!(
        layouts.iter().find(|c| c["placement"] == "west").unwrap()["status"],
        "undetermined"
    );
    let result = call(
        &client,
        "test_circuit_change",
        json!({"blueprint":{"action":"optimize","request":request}}),
    )
    .await;
    assert_eq!(result["ok"], true, "{result}");
    assert_eq!(result["result"]["baseline_blocks"], 6);
    assert_eq!(result["result"]["best"]["occupied_blocks"], 2);
    assert_eq!(result["result"]["baseline_total_blocks"], 7);
    assert_eq!(result["result"]["best"]["total_occupied_blocks"], 3);
    assert_eq!(result["result"]["global_minimality_proven"], false);
    assert_eq!(result["catalog_changed"], false);
    assert_eq!(result["writes_minecraft"], false);
    assert_eq!(result["adoption_authorized"], false);
    let best: BlueprintReductionCandidate =
        serde_json::from_value(result["result"]["best"]["candidate"].clone()).unwrap();
    assert!(best.blueprint.ports.iter().all(|port| port.kind
        == if port.direction == PortDirection::Input {
            BlueprintPortKind::BlockPower
        } else {
            BlueprintPortKind::DeviceOutput
        }));
    let not_published = call(
        &client,
        "get_circuit_revision",
        json!({"blueprint":{"kind":"blueprint","id":best.blueprint.id}}),
    )
    .await;
    assert_eq!(not_published["ok"], false);
    let mut next_parent = parent.clone();
    next_parent.id = id("mcp.reduction.parent.v2");
    next_parent.parents = vec![parent.id.clone()];
    next_parent.inclusions = vec![inclusion("child", best.blueprint.id.as_str())];
    let mut next_state = best.state.clone();
    next_state.assembly.instances = vec![inclusion("root", next_parent.id.as_str()), equipment];
    for boundary in &mut next_state.assembly.boundaries {
        boundary.port.instance = vec![
            InstanceId::new("root").unwrap(),
            InstanceId::new("child").unwrap(),
        ];
    }
    let proposal=call(&client,"test_circuit_change",json!({"blueprint":{"action":"propose_update","request":{
        "title":"Explicit adoption of a smaller NOT","description":"Review both moved terminals and the removed shared interpretation",
        "base_state":base.id,"base_parent":parent.id,"candidate_parent":next_parent.id,"parent_instance":["root"],"child_before":["child"],"previous_child":source.id,
        "child_after":["child"],"next_child":best.blueprint.id,"revisions":[next_parent,best.blueprint],"candidate_state":next_state,"behavior_context":best.behavior_context
    }}})).await;
    assert_eq!(proposal["ok"], true, "{proposal}");
    let operation = proposal["operation_id"].clone();
    let checked = call(&client, "show_operation", json!({"operation_id":operation})).await;
    assert_eq!(checked["can_adopt"], true, "{checked}");
    stop(client, server).await;
    let (client, server) = start(&root).await;
    let adopted = call(
        &client,
        "invoke_operation",
        json!({"operation_id":operation,"confirm":true,"blueprint_decision":{"action":"adopt"}}),
    )
    .await;
    assert_eq!(adopted["ok"], true, "{adopted}");
    assert_eq!(adopted["operation"]["status"], "adopted");
    let old = call(
        &client,
        "get_circuit_revision",
        json!({"blueprint":{"kind":"blueprint","id":parent.id}}),
    )
    .await;
    assert_eq!(
        old["result"]["record"],
        serde_json::to_value(parent).unwrap()
    );
    stop(client, server).await;
    fs::remove_dir_all(root).unwrap();
}
