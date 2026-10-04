//! Public-boundary regressions with immutable synthetic data and local mock sockets.
use super::super::test_support::{
    DurableRegistry, decode_reply, start_construction_bridge, temporary,
};
use super::super::*;

#[tokio::test]
async fn conversion_and_focused_health_preserve_absence_and_discovery_limits() {
    use dustroute_translate::snapshot::MinecraftSnapshot;
    use dustroute_translate::world_reverse::RegionBounds;
    let root = temporary();
    let mut service =
        DustRouteMcp::with_test_transport_and_player("127.0.0.1:1", McpPolicy::default(), "Tester");
    service.state_store = PlanStateStore::new(root.clone(), 3600);
    let bounds = RegionBounds::new(Pos::new(0, 0, 0), Pos::new(1, 1, 1));
    // Explicitly synthetic acquisition facts exercise dispatch independently
    // of physical extraction. No observation or server authority is fabricated.
    for (count, complete, focus, infer) in [
        (None, true, None, false),
        (Some(512), false, Some(Pos::new(0, 0, 0)), false),
        (Some(513), false, None, false),
        (Some(513), true, Some(Pos::new(0, 0, 0)), false),
        (Some(513), true, None, true),
    ] {
        let id = service
            .store_circuit(StoredCircuit {
                player: "Tester".into(),
                dimension: "minecraft:overworld".into(),
                bounds,
                target: focus,
                snapshot: service
                    .bridge
                    .share_snapshot(MinecraftSnapshot {
                        min: bounds.min,
                        max: bounds.max,
                        blocks: vec![],
                    })
                    .unwrap(),
                expansion: ExpansionEvidence::ExplicitSelectedRegion {
                    components_loaded: count,
                    component_limit: None,
                    limit_reached: !complete,
                },
                complete,
                expires_at: Instant::now() + CIRCUIT_SNAPSHOT_TTL,
            })
            .await;
        let response = decode_reply(
            &service
                .convert_from_circuit(Parameters(
                    serde_json::from_value(json!({
                        "circuit_id":id,"include_truth_table":infer
                    }))
                    .unwrap(),
                ))
                .await,
        )
        .unwrap();
        assert_eq!(response["ok"], true, "{response}");
        assert_eq!(response["circuit_id"], id.to_string());
        assert_eq!(response["mechanisms"], json!([]));
        assert_eq!(response["analysis_complete"], complete);
        assert_eq!(response["diagnostic"]["observation_complete"], complete);
        assert_eq!(response["circuit_identity"]["analysis_complete"], complete);
        assert!(response.get("execution_progress").is_none());
        if count.is_some_and(|n| n > 512) && !infer {
            assert_eq!(response["analysis_mode"], "hierarchical_local_first");
            assert_eq!(response["expansion"]["components_loaded"], 513);
            assert_eq!(response["focused_explanation"].is_null(), focus.is_none());
            assert!(response.get("macro_replacement_candidates").is_none());
            assert!(response.get("discovery").is_none());
        } else {
            assert!(response.get("analysis_mode").is_none());
            assert_eq!(response["macro_replacement_candidates"], Value::Null);
            assert_eq!(response["focused_component"].is_null(), focus.is_none());
            assert_eq!(
                response.get("focused_explanation").is_none(),
                focus.is_none()
            );
            assert_eq!(response["physical"]["analysis_complete"], complete);
            assert_eq!(response["discovery"]["seed"], json!(focus));
            assert_eq!(response["discovery"]["bounds"], json!(bounds));
        }
        let health = decode_reply(
            &service
                .test_circuit(Parameters(
                    serde_json::from_value(json!({"circuit_id":id})).unwrap(),
                ))
                .await,
        )
        .unwrap();
        assert_eq!(health["analysis_mode"], "focused_fast");
        assert_eq!(health["mutation_performed"], false);
        assert_eq!(health["target"], json!(focus));
        assert_eq!(health["focused_explanation"].is_null(), focus.is_none());
        assert_eq!(health["diagnostic"]["observation_complete"], complete);
        assert_eq!(
            health["expansion"].get("components_loaded").is_none(),
            count.is_none()
        );
        assert!(health.get("macro_replacement_candidates").is_none());
    }
    assert!(service.operations.list().await.is_empty());
    assert!(service.plans.placements().lock().await.is_empty());
    assert!(!root.exists());
}

#[test]
fn selected_conversion_adds_only_capture_fields_to_the_native_report() {
    use crate::recorded_analysis::reports::{hierarchical_report, reverse_report, tests::fixture};
    use dustroute_translate::world_reverse::RegionBounds;
    let (bounds, reverse) = fixture(0);
    let id = uuid::Uuid::nil();
    let mut expected = reverse_result_json(bounds, &reverse);
    expected["circuit_id"] = json!(id);
    expected["mechanisms"] = json!([]);
    let value = serde_json::to_value(Conversion {
        report: reverse_report(bounds, &reverse),
        captured: Capture {
            circuit_id: id,
            mechanisms: vec![],
        },
        detail: Selected {},
    })
    .unwrap();
    assert_eq!(value, expected);
    for key in [
        "circuit_identity",
        "diagnostic",
        "next_tools",
        "discovery",
        "focused_explanation",
        "macro_replacement_candidates",
        "analysis_complete",
    ] {
        assert!(value.get(key).is_none(), "{key}");
    }
    let hierarchy = dustroute_ir::derive_hierarchy(&reverse.analysis.scene);
    let bounds = RegionBounds::new(bounds.min, bounds.max);
    let value = serde_json::to_value(Conversion {
        report: hierarchical_report(
            bounds,
            &hierarchy,
            None,
            ExpansionEvidence::ExplicitSelectedRegion {
                components_loaded: Some(513),
                component_limit: None,
                limit_reached: false,
            }
            .recorded(),
            None,
        ),
        captured: Capture {
            circuit_id: id,
            mechanisms: vec![],
        },
        detail: Selected {},
    })
    .unwrap();
    assert_eq!(value["analysis_mode"], "hierarchical_local_first");
    assert_eq!(value["focused_explanation"], Value::Null);
    assert_eq!(value["expansion"]["components_loaded"], 513);
    for key in [
        "circuit_identity",
        "diagnostic",
        "next_tools",
        "discovery",
        "macro_replacement_candidates",
    ] {
        assert!(value.get(key).is_none(), "{key}");
    }
}

#[tokio::test]
async fn raw_capture_keeps_gaze_frontier_separate_from_complete_work_region() {
    let root = temporary();
    let (fake, address, bridge) =
        start_construction_bridge(DurableRegistry::Assemblies(root.join("world"))).await;
    let snapshot = super::super::electrical_edit_tests::machine();
    {
        let mut state = fake.lock().unwrap();
        state.snapshot = Some(snapshot.clone());
        state.gaze_target = Some(json!({"x":101,"y":101,"z":105}));
        state.resize_scan = true;
        state.include_air = true;
    }
    let mut service =
        DustRouteMcp::with_test_transport_and_player(address, McpPolicy::default(), "Tester");
    service.state_store = PlanStateStore::new(root.clone(), 3600);
    for (max_components, limited) in [(1, true), (64, false)] {
        let reply = service
            .get_world(Parameters(
                serde_json::from_value(json!({
                    "max_components":max_components,"max_listed_blocks":1
                }))
                .unwrap(),
            ))
            .await;
        let response = decode_reply(&reply).unwrap();
        assert_eq!(response["ok"], true, "{response}");
        assert_eq!(response["inference_applied"], false);
        assert_eq!(
            response["expansion"]["strategy"],
            "adjacent_component_flood_fill"
        );
        assert_eq!(response["expansion"]["limit_reached"], limited);
        assert_eq!(response["expansion"]["complete"], !limited);
        assert_eq!(response["scan"]["complete"], !limited);
        assert_eq!(
            response["boundary"]["component_frontier_remaining"],
            limited
        );
        assert!(response["boundary"].get("non_air_blocks").is_none());
        assert_eq!(response["boundary"]["guidance"].is_null(), !limited);
        assert!(response["blocks"].is_null());
        assert_eq!(response["blocks_truncated"], false);
        assert_eq!(response["redstone_blocks"].as_array().unwrap().len(), 1);
        assert_eq!(response["redstone_blocks_truncated"], true);
        assert!(response.get("circuit_id").is_none());
        assert!(response.get("execution_progress").is_none());
    }
    let region = json!({"min":snapshot["min"],"max":snapshot["max"]});
    let response = decode_reply(
        &service
            .get_world(Parameters(
                serde_json::from_value(json!({
                    "region":region,"include_block_list":true,"max_listed_blocks":2
                }))
                .unwrap(),
            ))
            .await,
    )
    .unwrap();
    assert_eq!(response["ok"], true, "{response}");
    assert_eq!(response["target"], Value::Null);
    assert_eq!(response["target_block"], Value::Null);
    assert_eq!(response["scan"]["complete"], true);
    assert_eq!(response["counts"]["non_air"], 6);
    assert_eq!(response["blocks"].as_array().unwrap().len(), 2);
    assert_eq!(response["blocks_truncated"], true);
    assert!(
        response["boundary"]
            .get("component_frontier_remaining")
            .is_none()
    );
    assert_eq!(response["expansion"]["strategy"], "explicit_work_region");
    assert_eq!(response["mutation_authorized"], false);
    assert_eq!(
        response["circuit_expires_in_seconds"],
        CIRCUIT_SNAPSHOT_TTL.as_secs()
    );
    assert!(response["observation_id"].is_string());
    assert!(response["readback"].is_object());
    assert!(response["observation_capabilities"].is_object());
    let id = response["circuit_id"].as_str().unwrap();
    let (_, captured) = service.load_circuit(id, "Tester").await.unwrap();
    assert!(captured.complete);
    assert!(captured.target.is_none());
    assert_eq!(response["content_id"], captured.snapshot.id().to_string());
    assert_eq!(service.circuits.lock().await.len(), 1);
    assert!(service.operations.list().await.is_empty());
    assert!(service.plans.placements().lock().await.is_empty());
    assert_eq!(fake.lock().unwrap().writes, 0);

    let rejected = decode_reply(
        &service
            .get_world(Parameters(
                serde_json::from_value(json!({
                    "region":region,"max_components":64
                }))
                .unwrap(),
            ))
            .await,
    )
    .unwrap();
    assert_eq!(rejected["ok"], false);
    assert_eq!(rejected["scan_complete"], false);
    assert!(rejected.get("observation").is_none());
    assert!(rejected["failure"]["progress"].is_null());
    assert_eq!(service.circuits.lock().await.len(), 1);

    fake.lock().unwrap().gaze_target = None;
    let rejected = decode_reply(
        &service
            .get_world(Parameters(serde_json::from_value(json!({})).unwrap()))
            .await,
    )
    .unwrap();
    assert_eq!(rejected["ok"], false);
    assert!(rejected["observation"].is_object());
    assert!(rejected.get("scan_complete").is_none());
    assert!(rejected["failure"]["progress"].is_null());
    assert_eq!(fake.lock().unwrap().writes, 0);
    bridge.abort();
    assert!(!root.exists());
}

#[tokio::test]
async fn player_override_keeps_known_cause_and_legacy_message_distinct() {
    let root = temporary();
    let mut service =
        DustRouteMcp::with_test_transport_and_player("127.0.0.1:1", McpPolicy::default(), "Tester");
    service.state_store = PlanStateStore::new(root.clone(), 3600);
    let replies = [
        (
            service
                .get_world(Parameters(
                    serde_json::from_value(json!({
                        "player":"AnotherPlayer","max_components":0
                    }))
                    .unwrap(),
                ))
                .await,
            "permission_denied",
        ),
        (
            service
                .get_circuit_ir(Parameters(
                    serde_json::from_value(json!({
                        "player":"AnotherPlayer","circuit_id":"invalid"
                    }))
                    .unwrap(),
                ))
                .await,
            "permission_denied",
        ),
        (
            service
                .convert_from_circuit(Parameters(
                    serde_json::from_value(json!({
                        "player":"AnotherPlayer","circuit_id":"invalid"
                    }))
                    .unwrap(),
                ))
                .await,
            "permission_denied",
        ),
        (
            service
                .test_circuit(Parameters(
                    serde_json::from_value(json!({
                        "player":"AnotherPlayer","circuit_id":"invalid"
                    }))
                    .unwrap(),
                ))
                .await,
            "permission_denied",
        ),
        // Revision's old String-only path did not retain a category or phase.
        (
            service
                .test_circuit_change(Parameters(
                    serde_json::from_value(json!({
                        "player":"AnotherPlayer","revision_id":"invalid","changes":[]
                    }))
                    .unwrap(),
                ))
                .await,
            "internal",
        ),
    ];
    for (reply, code) in replies {
        assert_eq!(reply.is_error, Some(true));
        let response = decode_reply(&reply).unwrap();
        assert_eq!(response["error_code"], code);
        let message = "player override is not allowed; configured assist player is \"Tester\"";
        let expected = if code == "permission_denied" {
            // Original boundary encoded the native cause, including no assigned phase.
            decode_reply(&json_reply(
                json!({"ok":false,"error":FailureCause::new(CauseKind::PermissionDenied, message)}),
            ))
            .unwrap()
        } else {
            decode_reply(&json_reply(json!({"ok":false,"error":message}))).unwrap()
        };
        assert_eq!(response, expected);
        assert!(response["failure"]["progress"].is_null());
        assert!(!response["error"].as_str().unwrap().starts_with('{'));
    }
    assert!(service.operations.list().await.is_empty());
    assert!(service.circuits.lock().await.is_empty());
    assert!(!root.exists());
}

#[tokio::test]
async fn observation_and_revision_admission_refuse_before_transport_or_storage() {
    let root = temporary();
    let mut service =
        DustRouteMcp::with_test_transport_and_player("127.0.0.1:1", McpPolicy::default(), "Tester");
    service.state_store = PlanStateStore::new(root.clone(), 3600);
    service.policy.allowed_players = ["AnotherPlayer".into()].into();
    let replies = [
        service
            .get_world(Parameters(
                serde_json::from_value(json!({"max_components":0})).unwrap(),
            ))
            .await,
        service
            .test_circuit_change(Parameters(
                serde_json::from_value(
                    json!({"revision_id":"invalid","changes":[],"simulation_ticks":257}),
                )
                .unwrap(),
            ))
            .await,
        service
            .get_circuit_revision(Parameters(GetCircuitRevisionParams {
                blueprint: None,
                revision_id: "invalid".into(),
                include_snapshot: Some(true),
            }))
            .await,
    ];
    for reply in replies {
        assert_eq!(reply.is_error, Some(true));
        let response = decode_reply(&reply).unwrap();
        assert_eq!(response["error_code"], "permission_denied");
        assert_eq!(response["failure"]["primary"]["phase"], "admission");
        assert!(response["failure"]["progress"].is_null());
        assert!(!response["error"].as_str().unwrap().starts_with('{'));
    }
    assert!(service.operations.list().await.is_empty());
    assert!(service.circuits.lock().await.is_empty());
    assert!(!root.exists());
}
