//! Bounded inference and read-only observation contracts over offline fixtures.
use super::test_support::snapshot_block;
use super::*;
use rmcp::ServiceExt;
use rmcp::model::{CallToolRequestParams, ClientInfo, ContentBlock};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::net::TcpListener;

#[test]
fn truth_table_request_is_explicit_and_budgeted_for_large_circuits() {
    let bounds =
        dustroute_translate::world_reverse::RegionBounds::new(Pos::new(0, 0, 0), Pos::new(4, 4, 4));
    let request = reverse_request_for_truth_table(
        bounds,
        TruthTableRequestOptions {
            include_truth_table: true,
            ..TruthTableRequestOptions::default()
        },
    )
    .expect("default truth-table request");
    assert!(request.infer_truth_table);
    assert_eq!(request.max_inputs, 16);
    assert_eq!(request.settle_ticks, 60);
    assert_eq!(request.truth_table_budget, TruthTableBudget::DEFAULT);

    let request = reverse_request_for_truth_table(
        bounds,
        TruthTableRequestOptions {
            include_truth_table: true,
            max_solver_iterations: Some(2_000_000),
            max_elapsed_millis: Some(30_000),
            ..TruthTableRequestOptions::default()
        },
    )
    .expect("runtime budgets within the protocol bound");
    assert_eq!(request.truth_table_budget.max_solver_iterations, 2_000_000);
    assert_eq!(request.truth_table_budget.max_elapsed_millis, Some(30_000));

    let error = reverse_request_for_truth_table(
        bounds,
        TruthTableRequestOptions {
            include_truth_table: true,
            max_inputs: Some(17),
            ..TruthTableRequestOptions::default()
        },
    )
    .expect_err("input count above the protocol bound must fail");
    assert!(error.contains("truth_table_max_inputs"));

    let error = reverse_request_for_truth_table(
        bounds,
        TruthTableRequestOptions {
            include_truth_table: true,
            max_solver_iterations: Some(MAX_TRUTH_TABLE_SOLVER_ITERATIONS + 1),
            ..TruthTableRequestOptions::default()
        },
    )
    .expect_err("solver iteration count above the protocol bound must fail");
    assert!(error.contains("truth_table_max_solver_iterations"));
}

#[test]
fn truth_table_budget_failure_is_structured_in_reverse_json() {
    let compiled = dustroute_translate::compiler::BaselineCompiler::new(
        dustroute_translate::compiler::BaselineCompileConfig::default(),
    )
    .compile(&dustroute_translate::circuits::half_adder())
    .expect("half-adder compiles");
    let (min, max) = compiled.world.bounds().expect("compiled bounds");
    let bounds = dustroute_translate::world_reverse::RegionBounds::new(min, max);
    let translated = dustroute_translate::api::Translator.reverse(
        &compiled.world,
        ReverseRequest::new(bounds)
            .with_truth_table(16)
            .with_truth_table_budget(TruthTableBudget::new(1, u128::MAX)),
    );
    let value = reverse_result_json(bounds, &translated);
    assert_eq!(value["truth_table_status"], "budget_exceeded");
    assert_eq!(
        value["truth_table_error_details"]["code"],
        "budget_exceeded"
    );
    assert_eq!(value["truth_table_error_details"]["rows"], 4);
    assert_eq!(value["truth_table_error_details"]["max_rows"], 1);
}

#[test]
fn truth_table_runtime_budget_failure_is_structured_in_reverse_json() {
    let compiled = dustroute_translate::compiler::BaselineCompiler::new(
        dustroute_translate::compiler::BaselineCompileConfig::default(),
    )
    .compile(&dustroute_translate::circuits::half_adder())
    .expect("half-adder compiles");
    let (min, max) = compiled.world.bounds().expect("compiled bounds");
    let bounds = dustroute_translate::world_reverse::RegionBounds::new(min, max);
    let translated = dustroute_translate::api::Translator.reverse(
        &compiled.world,
        ReverseRequest::new(bounds)
            .with_truth_table(16)
            .with_truth_table_budget(
                TruthTableBudget::new(usize::MAX, u128::MAX)
                    .with_max_solver_iterations(0)
                    .with_max_elapsed_millis(None),
            ),
    );
    let value = reverse_result_json(bounds, &translated);
    assert_eq!(value["truth_table_status"], "budget_exceeded");
    assert_eq!(
        value["truth_table_error_details"]["code"],
        "runtime_budget_exceeded"
    );
    assert_eq!(value["truth_table_error_details"]["completed_rows"], 0);
    assert_eq!(value["truth_table"], Value::Null);
}

#[tokio::test]
async fn exposes_gaze_tools_prompt_and_fake_bot_status_over_mcp() {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap().to_string();
    tokio::spawn(async move {
        let (mut stream, _) = listener.accept().await.unwrap();
        let mut request = String::new();
        BufReader::new(&mut stream)
            .read_line(&mut request)
            .await
            .unwrap();
        let request: Value = serde_json::from_str(&request).unwrap();
        let response = json!({
            "id": request["id"],
            "result": {
                "connected": true,
                "username": "DustRouteBot",
                "host": "test",
                "port": 25565,
                "version": "1.21.11",
                "dimension": "minecraft:overworld"
            }
        });
        stream
            .write_all(format!("{response}\n").as_bytes())
            .await
            .unwrap();
    });

    let (server_transport, client_transport) = tokio::io::duplex(16 * 1024);
    let server = tokio::spawn(async move {
        DustRouteMcp::with_test_transport(address)
            .serve(server_transport)
            .await
            .unwrap()
            .waiting()
            .await
            .unwrap();
    });
    let client = ClientInfo::default().serve(client_transport).await.unwrap();
    let tools = client.list_tools(None).await.unwrap();
    assert!(
        !tools
            .tools
            .iter()
            .any(|tool| tool.name == "resolve_looked_at_circuit")
    );
    assert!(
        tools
            .tools
            .iter()
            .any(|tool| tool.name == "convert_from_circuit")
    );
    assert!(tools.tools.iter().any(|tool| tool.name == "test_circuit"));
    assert!(tools.tools.iter().any(|tool| tool.name == "get_world"));
    assert!(tools.tools.iter().all(|tool| tool.output_schema.is_none()));
    let prompts = client.list_prompts(None).await.unwrap();
    assert!(
        prompts
            .prompts
            .iter()
            .any(|prompt| prompt.name == "collaborate-on-redstone-circuit")
    );
    let result = client
        .call_tool(CallToolRequestParams::new("get_bot_status"))
        .await
        .unwrap();
    let ContentBlock::Text(text) = &result.content[0] else {
        panic!("expected text tool result");
    };
    assert!(text.text.contains("DustRouteBot"));
    client.cancel().await.unwrap();
    server.await.unwrap();
}

#[tokio::test]
async fn runs_two_gaze_points_preview_and_reverse_analysis_over_mcp() {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap().to_string();
    tokio::spawn(async move {
        let mut observation = 0;
        for _ in 0..5 {
            let (mut stream, _) = listener.accept().await.unwrap();
            let mut request = String::new();
            BufReader::new(&mut stream)
                .read_line(&mut request)
                .await
                .unwrap();
            let request: Value = serde_json::from_str(&request).unwrap();
            let result = match request["method"].as_str().unwrap() {
                "observe_player" => {
                    let target = if observation == 0 {
                        json!({ "x": 0, "y": 64, "z": 0 })
                    } else {
                        json!({ "x": 3, "y": 66, "z": 1 })
                    };
                    observation += 1;
                    json!({
                        "player": "builder",
                        "eye_position": { "x": 0.5, "y": 65.62, "z": 4.5 },
                        "yaw": 0.0,
                        "pitch": 0.0,
                        "targeted_block": target,
                        "targeted_face": "up",
                        "distance": 4.0,
                        "dimension": "minecraft:overworld"
                    })
                }
                "preview_region" => json!({ "particle_corners": 8 }),
                "scan_region" => json!({
                    "min": { "x": 0, "y": 64, "z": 0 },
                    "max": { "x": 3, "y": 66, "z": 1 },
                    "blocks": []
                }),
                method => panic!("unexpected fake bridge method {method}"),
            };
            let response = json!({ "id": request["id"], "result": crate::bridge::test_readback_response(&request,result) });
            stream
                .write_all(format!("{response}\n").as_bytes())
                .await
                .unwrap();
        }
    });

    let (server_transport, client_transport) = tokio::io::duplex(64 * 1024);
    let server = tokio::spawn(async move {
        DustRouteMcp::with_test_transport_and_player(address, McpPolicy::default(), "builder")
            .serve(server_transport)
            .await
            .unwrap()
            .waiting()
            .await
            .unwrap();
    });
    let client = ClientInfo::default().serve(client_transport).await.unwrap();
    for corner in ["first", "second"] {
        let arguments = serde_json::from_value(json!({
            "corner": corner
        }))
        .unwrap();
        let result = client
            .call_tool(CallToolRequestParams::new("set_region").with_arguments(arguments))
            .await
            .unwrap();
        let ContentBlock::Text(text) = &result.content[0] else {
            panic!("expected text tool result");
        };
        assert!(text.text.contains("\"ok\": true"));
    }
    let shown = client
        .call_tool(CallToolRequestParams::new("show_region").with_arguments(serde_json::Map::new()))
        .await
        .unwrap();
    let ContentBlock::Text(shown_text) = &shown.content[0] else {
        panic!("expected text tool result");
    };
    let shown_value: Value = serde_json::from_str(&shown_text.text).unwrap();
    let circuit_id = shown_value["circuit_id"].as_str().unwrap();
    let ir = client
        .call_tool(
            CallToolRequestParams::new("get_circuit_ir").with_arguments(
                serde_json::from_value::<serde_json::Map<String, Value>>(json!({
                    "circuit_id": circuit_id
                }))
                .unwrap(),
            ),
        )
        .await
        .unwrap();
    let ContentBlock::Text(ir_text) = &ir.content[0] else {
        panic!("expected text tool result");
    };
    assert!(ir_text.text.contains("\"ok\": true"));
    assert!(ir_text.text.contains(circuit_id));
    let converted = client
        .call_tool(
            CallToolRequestParams::new("convert_from_circuit").with_arguments(
                serde_json::from_value::<serde_json::Map<String, Value>>(json!({
                    "scope": "selected_region"
                }))
                .unwrap(),
            ),
        )
        .await
        .unwrap();
    let ContentBlock::Text(converted_text) = &converted.content[0] else {
        panic!("expected text tool result");
    };
    assert!(converted_text.text.contains("\"ok\": true"));
    client.cancel().await.unwrap();
    server.await.unwrap();
}

#[tokio::test]
async fn what_is_this_returns_physical_gate_and_boundary_views() {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap().to_string();
    tokio::spawn(async move {
        for _ in 0..11 {
            let (mut stream, _) = listener.accept().await.unwrap();
            let mut request = String::new();
            BufReader::new(&mut stream)
                .read_line(&mut request)
                .await
                .unwrap();
            let request: Value = serde_json::from_str(&request).unwrap();
            let result = match request["method"].as_str().unwrap() {
                "observe_player" => json!({
                    "player": "builder",
                    "eye_position": { "x": 0.5, "y": 3.0, "z": 0.5 },
                    "yaw": 0.0,
                    "pitch": -1.0,
                    "targeted_block": { "x": 1, "y": 1, "z": 0 },
                    "targeted_face": "up",
                    "distance": 2.0,
                    "dimension": "minecraft:overworld"
                }),
                "scan_region" => json!({
                    "min": request["params"]["min"],
                    "max": request["params"]["max"],
                    "blocks": [
                        { "pos": { "x": 0, "y": 0, "z": 0 }, "name": "minecraft:stone", "properties": {} },
                        { "pos": { "x": 1, "y": 0, "z": 0 }, "name": "minecraft:stone", "properties": {} },
                        { "pos": { "x": 2, "y": 0, "z": 0 }, "name": "minecraft:stone", "properties": {} },
                        { "pos": { "x": 0, "y": 1, "z": 0 }, "name": "minecraft:redstone_wire", "properties": { "east": "side", "power": "0" } },
                        { "pos": { "x": 1, "y": 1, "z": 0 }, "name": "minecraft:repeater", "properties": { "facing": "west", "delay": "1", "powered": "false" } },
                        { "pos": { "x": 2, "y": 1, "z": 0 }, "name": "minecraft:redstone_wire", "properties": { "west": "side", "power": "0" } }
                    ]
                }),
                method => panic!("unexpected fake bridge method {method}"),
            };
            stream
                .write_all(
                    format!("{}\n", json!({ "id": request["id"], "result": crate::bridge::test_readback_response(&request,crate::bridge::test_scan_world(&request,result)) }))
                        .as_bytes(),
                )
                .await
                .unwrap();
        }
    });
    let service =
        DustRouteMcp::with_test_transport_and_player(address, McpPolicy::default(), "builder");
    let result = service
        .convert_from_circuit(Parameters(AnalyzeLookedAtParams {
            player: None,
            max_components: Some(64),
            fragment_gap: Some(2),
            include_truth_table: Some(false),
            truth_table_max_inputs: None,
            truth_table_settle_ticks: None,
            truth_table_max_rows: None,
            truth_table_max_work_units: None,
            truth_table_max_solver_iterations: None,
            truth_table_max_elapsed_millis: None,
            scope: None,
            circuit_id: None,
        }))
        .await;
    let value: Value = test_support::decode_reply(&result).unwrap();
    assert_eq!(value["ok"], true, "{result:?}");
    assert_eq!(value["mechanisms"], json!([]));
    assert_eq!(value["focused_component"]["block"], "Repeater");
    assert_eq!(
        value["focused_component"]["observed_name"],
        "minecraft:repeater"
    );
    assert_eq!(
        value["focused_component"]["observed_properties"]["delay"],
        "1"
    );
    assert_eq!(
        value["focused_component"]["capabilities"]["temporal"],
        "partial"
    );
    assert_eq!(
        value["focused_component"]["recognized_gates"][0]["kind"],
        "buffer"
    );
    assert!(!value["gate_view"]["gates"].as_array().unwrap().is_empty());
    assert_eq!(
        value["circuit_identity"]["classification_level"],
        "local_gate_network"
    );
    assert_eq!(value["circuit_identity"]["local_gate_counts"]["buffer"], 1);
    assert_eq!(value["circuit_identity"]["local_gate_count"], 1);
    assert_eq!(
        value["circuit_identity"]["local_gate_samples"][0]["kind"],
        "buffer"
    );
    assert!(value["circuit_identity"].get("local_gates").is_none());
    assert_eq!(value["circuit_identity"]["analysis_complete"], true);
    assert!(value["circuit_identity"]["primary_candidate"].is_null());
    assert!(
        value["circuit_identity"]["uncertainty_reasons"]
            .as_array()
            .unwrap()
            .contains(&Value::String(
                "no_registered_higher_level_pattern_matched".to_owned()
            ))
    );
    assert!(value["physical"]["observation"].is_object());
    assert!(value["physical"]["block_capabilities"]["groups"].is_array());
    assert!(value["stages"]["physical_scene"].is_object());
    assert_eq!(value["next_tools"]["repair_planning"], "new_repair");
    assert!(value.get("transition_scenarios").is_none());
    assert!(value.get("repair_proposals").is_none());
    assert_eq!(value["diagnostic"]["observation_complete"], true);
    assert!(value["diagnostic"]["counts"].is_object());
    assert!(value["diagnostic"]["recommended_next_action"].is_object());
    assert!(value["focused_explanation"]["role"].is_object());
    assert!(value["focused_explanation"]["incoming"].is_array());
    assert!(value["focused_explanation"]["paths_to_outputs"].is_array());
    let circuit_id = value["circuit_id"].as_str().unwrap().to_owned();
    let ir: Value = test_support::decode_reply(
        &service
            .get_circuit_ir(Parameters(GetLookedAtCircuitIrParams {
                player: None,
                max_components: None,
                fragment_gap: None,
                node_id: None,
                analysis_id: None,
                circuit_id: Some(circuit_id.clone()),
            }))
            .await,
    )
    .unwrap();
    assert_eq!(ir["ok"], true);
    assert_eq!(ir["circuit_id"], circuit_id);
    assert_eq!(
        ir["analysis_id_schema"],
        crate::snapshot_content::VALIDATION_KEY_SCHEMA
    );
    let node = ir["mixed_ir"]["nodes"][0]["id"].as_u64().unwrap() as usize;
    for supplied in [
        None,
        Some("obsolete-analysis-id".to_owned()),
        ir["analysis_id"].as_str().map(str::to_owned),
    ] {
        let matches = supplied.as_deref() == ir["analysis_id"].as_str();
        let expanded: Value = test_support::decode_reply(
            &service
                .get_circuit_ir(Parameters(GetLookedAtCircuitIrParams {
                    player: None,
                    max_components: None,
                    fragment_gap: None,
                    node_id: Some(node),
                    analysis_id: supplied,
                    circuit_id: Some(circuit_id.clone()),
                }))
                .await,
        )
        .unwrap();
        assert_eq!(expanded["ok"], matches);
        assert_eq!(
            expanded["analysis_id_schema"],
            crate::snapshot_content::VALIDATION_KEY_SCHEMA
        );
        if matches {
            assert_eq!(expanded["analysis_id"], ir["analysis_id"]);
            assert_eq!(expanded["mutation_performed"], false);
            assert!(expanded["mixed_ir"]["expanded_node"].is_object());
        } else {
            assert_eq!(expanded["current_analysis_id"], ir["analysis_id"]);
            assert_eq!(expanded["retryable"], true);
        }
    }
    let invalid_node = service
        .get_circuit_ir(Parameters(GetLookedAtCircuitIrParams {
            player: None,
            max_components: None,
            fragment_gap: None,
            node_id: Some(usize::MAX),
            analysis_id: ir["analysis_id"].as_str().map(str::to_owned),
            circuit_id: Some(circuit_id),
        }))
        .await;
    assert_eq!(invalid_node.is_error, Some(true));
    let invalid_node = test_support::decode_reply(&invalid_node).unwrap();
    assert_eq!(invalid_node["ok"], false);
    assert_eq!(
        invalid_node["available_node_count"],
        ir["mixed_ir"]["node_count"]
    );
    assert!(invalid_node["failure"]["progress"].is_null());
    assert!(invalid_node.get("circuit_id").is_none());
    assert!(invalid_node.get("expanded_node").is_none());
}

#[tokio::test]
async fn focused_diagnostic_returns_a_compact_read_only_contract() {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap().to_string();
    let bridge = tokio::spawn(async move {
        loop {
            let (mut stream, _) = listener.accept().await.unwrap();
            let mut request = String::new();
            BufReader::new(&mut stream)
                .read_line(&mut request)
                .await
                .unwrap();
            let request: Value = serde_json::from_str(&request).unwrap();
            let result = match request["method"].as_str().unwrap() {
                "observe_player" => json!({
                    "player": "builder",
                    "eye_position": { "x": 0.5, "y": 3.0, "z": 0.5 },
                    "yaw": 0.0,
                    "pitch": -1.0,
                    "targeted_block": { "x": 1, "y": 1, "z": 0 },
                    "targeted_face": "up",
                    "distance": 2.0,
                    "dimension": "minecraft:overworld"
                }),
                "scan_region" => json!({
                    "min": request["params"]["min"],
                    "max": request["params"]["max"],
                    "blocks": [
                        { "pos": { "x": 0, "y": 0, "z": 0 }, "name": "minecraft:stone", "properties": {} },
                        { "pos": { "x": 1, "y": 0, "z": 0 }, "name": "minecraft:stone", "properties": {} },
                        { "pos": { "x": 0, "y": 1, "z": 0 }, "name": "minecraft:redstone_wire", "properties": { "east": "side", "power": "0" } },
                        { "pos": { "x": 1, "y": 1, "z": 0 }, "name": "minecraft:repeater", "properties": { "facing": "west", "delay": "1", "powered": "false" } }
                    ]
                }),
                method => panic!("unexpected fake bridge method {method}"),
            };
            stream
                .write_all(
                    format!("{}\n", json!({ "id": request["id"], "result": crate::bridge::test_readback_response(&request,crate::bridge::test_scan_world(&request,result)) }))
                        .as_bytes(),
                )
                .await
                .unwrap();
        }
    });
    let service =
        DustRouteMcp::with_test_transport_and_player(address, McpPolicy::default(), "builder");
    let result = service
        .test_circuit(Parameters(DiagnoseLookedAtParams {
            player: None,
            max_components: Some(64),
            fragment_gap: Some(2),
            circuit_id: None,
        }))
        .await;
    bridge.abort();

    let value: Value = test_support::decode_reply(&result).unwrap();
    assert_eq!(value["ok"], true, "{result:?}");
    assert_eq!(value["schema_version"], "dustroute.diagnostic.v1");
    assert_eq!(value["analysis_mode"], "focused_fast");
    let shared: dustroute_translate::diagnostic::report::Diagnosis =
        serde_json::from_value(value["diagnostic"].clone()).unwrap();
    assert_eq!(
        shared.method,
        dustroute_translate::diagnostic::report::DiagnosticMethod::Connectivity
    );
    assert!(!shared.world_writes && !shared.repair.permission_granted);
    assert_eq!(
        shared.repair.status,
        dustroute_translate::diagnostic::report::RepairStatus::NotAssessed
    );
    assert_eq!(value["mutation_performed"], false);
    assert!(value["diagnostic"]["counts"].is_object());
    assert!(value["diagnostic"]["findings"].is_array());
    assert!(value["diagnostic"]["recommended_next_action"].is_object());
    assert!(value["focused_explanation"]["role"].is_object());
    assert!(value["focused_explanation"]["caveats"].is_array());
}

#[test]
fn raw_inspection_preserves_states_and_reports_scan_boundaries() {
    let snapshot: dustroute_translate::snapshot::MinecraftSnapshot =
        serde_json::from_value(json!({
            "min": { "x": 0, "y": 0, "z": 0 },
            "max": { "x": 2, "y": 1, "z": 0 },
            "blocks": [
                snapshot_block(0, 0, 0, "minecraft:stone", json!({})),
                snapshot_block(1, 0, 0, "minecraft:stone", json!({})),
                snapshot_block(2, 0, 0, "minecraft:stone", json!({})),
                snapshot_block(1, 1, 0, "minecraft:redstone_wire", json!({
                    "north": "none", "east": "side", "south": "none",
                    "west": "side", "power": "7"
                })),
                snapshot_block(2, 1, 0, "minecraft:repeater", json!({
                    "facing": "east", "delay": "3", "powered": "true"
                }))
            ]
        }))
        .unwrap();
    let result = raw_world_inspection(
        &snapshot,
        Pos::new(1, 1, 0),
        "minecraft:overworld",
        false,
        16,
    );
    let result = serde_json::to_value(result).unwrap();
    assert_eq!(result["inference_applied"], false);
    assert_eq!(result["scan"]["volume"], 6);
    assert_eq!(result["counts"]["air"], 1);
    assert_eq!(result["counts"]["redstone_candidates"], 2);
    assert_eq!(result["counts"]["modeled_redstone"], 2);
    assert_eq!(result["boundary"]["redstone_touches_boundary"], true);
    assert_eq!(result["target_block"]["properties"]["power"], "7");
    assert_eq!(result["redstone_blocks"][1]["properties"]["delay"], "3");
    assert!(result["blocks"].is_null());
}
