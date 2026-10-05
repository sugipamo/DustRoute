//! Public MCP encoding, authorization, dispatch and tool-surface contracts.
use super::*;
use rmcp::model::ContentBlock;

#[tokio::test]
async fn builtin_and_repair_planning_denial_keeps_cause_before_transport_or_storage() {
    let root = test_support::temporary();
    let mut service = DustRouteMcp::with_test_transport_and_player(
        "127.0.0.1:1",
        McpPolicy {
            allowed_players: ["AnotherPlayer".to_owned()].into(),
            ..Default::default()
        },
        "Tester",
    );
    service.state_store = PlanStateStore::new(root.clone(), 3600);
    let circuit_id = uuid::Uuid::new_v4().to_string();
    let replies = [
        service
            .new_placement(Parameters(PreviewPlacementParams {
                circuit: "half-adder".into(),
                ..Default::default()
            }))
            .await,
        service
            .new_repair(Parameters(ProposeRepairsParams {
                player: None,
                circuit_id: circuit_id.clone(),
                max_gap: None,
            }))
            .await,
        service
            .get_repair_context(Parameters(GetRepairContextParams {
                player: None,
                circuit_id,
                operation_id: None,
                max_gap: None,
            }))
            .await,
    ];
    for reply in replies {
        assert_eq!(reply.is_error, Some(true));
        let response = test_support::decode_reply(&reply).unwrap();
        assert_eq!(response["error_code"], "permission_denied");
        assert_eq!(response["failure"]["primary"]["phase"], "admission");
        assert!(response["failure"]["progress"].is_null());
        assert!(!response["error"].as_str().unwrap().starts_with('{'));
    }
    assert!(service.operations.list().await.is_empty());
    assert!(!root.exists());
}

#[tokio::test]
async fn grounded_planning_denial_keeps_admission_cause_before_loading_or_transport() {
    let service = DustRouteMcp::with_test_transport_and_player(
        "127.0.0.1:1",
        McpPolicy {
            allowed_players: ["AnotherPlayer".to_owned()].into(),
            ..Default::default()
        },
        "Tester",
    );
    for adopted in [false, true] {
        let params = if adopted {
            PreviewPlacementParams {
                assembly_revision_id: Some(
                    dustroute_library::blueprint::AssemblyRevisionId::new("missing-source")
                        .unwrap(),
                ),
                ..Default::default()
            }
        } else {
            PreviewPlacementParams {
                revision_id: Some("invalid-source".into()),
                ..Default::default()
            }
        };
        let reply = if adopted {
            service.plan_adopted_assembly_placement(params).await
        } else {
            service.plan_revision_placement(params).await
        };
        assert_eq!(reply.is_error, Some(true));
        let response = test_support::decode_reply(&reply).unwrap();
        assert_eq!(response["error_code"], "permission_denied");
        assert_eq!(response["failure"]["primary"]["phase"], "admission");
        assert!(response["failure"]["progress"].is_null());
        assert!(response["error"].as_str().unwrap().contains("Tester"));
    }
}

#[test]
fn mcp_reply_outcome_is_set_before_text_encoding() {
    for (value, failed) in [
        (
            serde_json::json!({"ok":false,"error":"legacy failure"}),
            true,
        ),
        (
            serde_json::json!({"ok":true,"operation":{"status":"failed","result":{"ok":false}}}),
            false,
        ),
        (
            serde_json::json!({"operation_id":"legacy proposal without ok"}),
            false,
        ),
    ] {
        let reply = super::json_reply(value.clone());
        assert_eq!(reply.is_error, Some(failed));
        assert!(reply.structured_content.is_none());
        let public = super::test_support::decode_reply(&reply).unwrap();
        for (key, expected) in value.as_object().unwrap() {
            assert_eq!(&public[key], expected);
        }
    }
}

#[test]
fn mcp_reply_encoding_failure_is_a_tool_error() {
    struct Unencodable;
    impl serde::Serialize for Unencodable {
        fn serialize<S: serde::Serializer>(&self, _: S) -> Result<S::Ok, S::Error> {
            Err(serde::ser::Error::custom("test encoding failure"))
        }
    }
    let reply = super::typed_reply(Unencodable);
    assert_eq!(reply.is_error, Some(true));
    let public = super::test_support::decode_reply(&reply).unwrap();
    assert_eq!(public["ok"], false);
    assert_eq!(public["error_code"], "serialization_failed");
    assert!(public["failure"]["progress"].is_null());
}

#[tokio::test]
async fn mcp_envelope_marks_mutation_refusal_but_keeps_failed_history_query_successful() {
    let root = test_support::temporary();
    let mut service =
        DustRouteMcp::with_test_transport_and_player("127.0.0.1:9", McpPolicy::default(), "Tester");
    service.state_store = PlanStateStore::new(root.clone(), 3600);
    let id = uuid::Uuid::new_v4();
    service
        .operations
        .record_completed(
            id,
            OperationKind::PlacementApply,
            crate::operations::mutation::PlacementAttempt {
                operation_id: id.to_string(),
                outcome: crate::operations::mutation::PlacementOutcome::refused("reply lost"),
            }
            .into(),
        )
        .await;
    let (client, server) = test_support::serve(service).await;
    let query = client
        .call_tool(
            rmcp::model::CallToolRequestParams::new("get_operation").with_arguments(
                serde_json::json!({"operation_id":id})
                    .as_object()
                    .unwrap()
                    .clone(),
            ),
        )
        .await
        .unwrap();
    assert_ne!(query.is_error, Some(true));
    let refused = client
        .call_tool(
            rmcp::model::CallToolRequestParams::new("invoke_operation").with_arguments(
                serde_json::json!({"operation_id":uuid::Uuid::new_v4(),"confirm":true})
                    .as_object()
                    .unwrap()
                    .clone(),
            ),
        )
        .await
        .unwrap();
    assert_eq!(refused.is_error, Some(true));
    test_support::stop(client, server).await;
    if root.exists() {
        std::fs::remove_dir_all(root).unwrap();
    }
}

#[test]
fn unstructured_error_information_remains_unknown_without_claiming_zero_writes() {
    let response: serde_json::Value = test_support::decode_reply(&super::json_reply(
        serde_json::json!({"ok":false,"error":"timeout resource limit"}),
    ))
    .unwrap();
    assert_eq!(response["failure"]["primary"]["kind"], "unknown");
    assert_eq!(response["failure"]["primary"]["phase"], "unknown");
    assert!(response["failure"]["progress"].is_null());
    assert!(response["recovery"]["reobserve_required"].is_null());
    assert_eq!(response["recovery"]["same_operation_replay_allowed"], false);
}

#[test]
fn typed_transition_trace_uses_arrays_for_coordinate_keyed_state() {
    let position = Pos::new(1, 64, -2);
    let trace = dustroute_translate::scenario::ScenarioTrace {
        duration_redstone_ticks: 2,
        duration_game_ticks: None,
        time_unit: dustroute_ir::TraceTimeUnit::RedstoneTick,
        events: Vec::new(),
        final_strengths: BTreeMap::from([(position, 15)]),
        final_powered: BTreeMap::from([(position, true)]),
        status: dustroute_ir::TraceStatus::Complete,
    };

    let value = serde_json::to_value(crate::api::TransitionTraceResponse::from(&trace)).unwrap();
    assert_eq!(value["final_strengths"][0]["position"], json!(position));
    assert_eq!(value["final_strengths"][0]["strength"], 15);
    assert_eq!(value["final_powered"][0]["powered"], true);
    assert!(serde_json::to_string(&value).is_ok());
}

#[test]
fn unstructured_tool_errors_receive_the_common_error_contract() {
    let value: Value = test_support::decode_reply(&json_reply(json!({
        "ok": false,
        "error": "legacy failure"
    })))
    .unwrap();
    assert_eq!(value["schema_version"], crate::api::ERROR_SCHEMA_V1);
    assert_eq!(value["error_code"], "internal");
    assert_eq!(value["retryable"], false);
    assert_eq!(value["error"], "legacy failure");
}

#[tokio::test]
async fn unified_operation_tools_reject_invalid_ids_with_argument_error() {
    let service = DustRouteMcp::with_test_transport("127.0.0.1:1");
    let result: Value = test_support::decode_reply(
        &service
            .invoke_operation(Parameters(InvokeOperationParams {
                blueprint_decision: None,
                operation_id: "not-a-uuid".into(),
                confirm: true,
                contracts: None,
            }))
            .await,
    )
    .unwrap();

    assert_eq!(result["ok"], false);
    assert_eq!(result["error_code"], "invalid_argument");
    assert_eq!(result["retryable"], false);

    let get_result: Value = test_support::decode_reply(
        &service
            .get_operation(Parameters(OperationParams {
                operation_id: "not-a-uuid".into(),
            }))
            .await,
    )
    .unwrap();
    assert_eq!(get_result["error_code"], "invalid_argument");

    let show_result: Value = test_support::decode_reply(
        &service
            .show_operation(Parameters(ShowOperationParams {
                operation_id: "not-a-uuid".into(),
                player: None,
            }))
            .await,
    )
    .unwrap();
    assert_eq!(show_result["error_code"], "invalid_argument");

    let undo_result: Value = test_support::decode_reply(
        &service
            .undo_operation(Parameters(ConfirmedOperationParams {
                operation_id: "not-a-uuid".into(),
                confirm: true,
            }))
            .await,
    )
    .unwrap();
    assert_eq!(undo_result["error_code"], "invalid_argument");

    let stop_result: Value = test_support::decode_reply(
        &service
            .stop_operation(Parameters(OperationParams {
                operation_id: "not-a-uuid".into(),
            }))
            .await,
    )
    .unwrap();
    assert_eq!(stop_result["error_code"], "invalid_argument");
}

#[tokio::test]
async fn collaboration_prompt_requires_gaze_grounding_and_preview() {
    let prompt = DustRouteMcp::with_test_transport("127.0.0.1:1")
        .collaboration_prompt()
        .await;
    let ContentBlock::Text(text) = &prompt.messages[0].content else {
        panic!("expected text prompt");
    };
    assert!(text.text.contains("get_world"));
    assert!(text.text.contains("test_circuit"));
    assert!(text.text.contains("get_circuit_ir"));
    assert!(text.text.contains("get_repair_context"));
    assert!(text.text.contains("new_optimization"));
    assert!(text.text.contains("show_operation"));
    assert!(text.text.contains("confirmation"));
}

#[test]
fn tool_profiles_keep_low_level_operations_out_of_the_default_surface() {
    let default = DustRouteMcp::with_test_transport_and_profile(
        "127.0.0.1:1",
        McpPolicy::default(),
        ToolProfile::Default,
    );
    let debug = DustRouteMcp::with_test_transport_and_profile(
        "127.0.0.1:1",
        McpPolicy::default(),
        ToolProfile::Debug,
    );
    let default_names = default
        .tool_router
        .list_all()
        .into_iter()
        .map(|tool| tool.name.to_string())
        .collect::<BTreeSet<_>>();
    let debug_names = debug
        .tool_router
        .list_all()
        .into_iter()
        .map(|tool| tool.name.to_string())
        .collect::<BTreeSet<_>>();

    let native_tools = usize::from(cfg!(feature = "voxrig"));
    assert_eq!(default_names.len(), 23 + native_tools);
    assert_eq!(
        default_names.contains("survival_construction"),
        cfg!(feature = "voxrig")
    );
    assert!(default_names.contains("manage_construction_job"));
    assert!(default_names.contains("manage_assembly"));
    assert!(!default_names.contains("get_piston_door_state"));
    assert!(!debug_names.contains("get_piston_door_state"));
    assert_eq!(debug_names.len(), 30 + native_tools);
    assert!(default_names.contains("test_circuit"));
    assert!(default_names.contains("get_circuit_ir"));
    assert!(default_names.contains("test_circuit_change"));
    assert!(default_names.contains("get_circuit_revision"));
    assert!(default_names.contains("get_repair_context"));
    assert!(default_names.contains("new_optimization"));
    assert!(default_names.contains("new_macro_optimization"));
    assert!(default_names.contains("invoke_operation"));
    assert!(!default_names.contains("invoke_repair"));
    for name in DEBUG_ONLY_TOOLS {
        assert!(!default_names.contains(name), "{name}");
        assert!(debug_names.contains(name), "{name}");
    }
}

#[tokio::test]
async fn rejects_an_override_of_the_configured_assist_player() {
    let service = DustRouteMcp::with_test_transport_and_player(
        "127.0.0.1:1",
        McpPolicy::default(),
        "builder",
    );
    let result = service
        .get_player_gaze(Parameters(ObserveParams {
            player: Some("someone_else".to_owned()),
            max_distance: None,
        }))
        .await;
    assert!(test_support::reply_text(&result).contains("player override is not allowed"));
}
