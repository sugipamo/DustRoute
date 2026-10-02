use super::*;
use crate::service::test_support::{Client, call, serve, start, stop, temporary};
use crate::survival_construction::native_roof_trial as fixture;
use std::io::Write;

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
        json!({"operation_id":id,"confirmed":true}),
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
async fn public_grounded_adoption_is_required_and_observer_is_explicit() {
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
    assert_eq!(
        missing["error"]["code"], "observer_not_configured",
        "{missing}"
    );
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
        McpConfig::new("127.0.0.1:25572", "Tester", "127.0.0.1:1").unwrap(),
        McpPolicy {
            read_only: false,
            ..Default::default()
        },
        "NatMineBot",
    )
    .await
    .unwrap();
    service = service.with_survival_observer("NatMineView").await.unwrap();
    service.state_store = PlanStateStore::new(root.clone(), 3600);
    let native = service.bridge.survival_bridge().unwrap();
    let initial = native.lease_survival().unwrap();
    let bot = initial.source();
    initial.release(bot.clone());
    let observer = service.survival_observer.clone().unwrap();
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
    std::fs::write(&output,serde_json::to_vec_pretty(&json!({"execute":true,"error":error,"events":[{"phase":"public_plan","result":plan},{"phase":"public_start","result":started_response},{"phase":"public_final","result":final_result}]})).unwrap()).unwrap();
    assert!(error.is_null(), "{error}");
    assert_eq!(final_result["completed_steps"], 115);
    assert_eq!(
        final_result["diagnosis"]["record"]["events"]
            .as_array()
            .unwrap()
            .last()
            .unwrap()["evidence"]["independently_checked_cells"],
        3120
    );
    let restored = native.lease_survival().unwrap();
    let current = restored.source();
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
    stop(reopened, server).await;
}
