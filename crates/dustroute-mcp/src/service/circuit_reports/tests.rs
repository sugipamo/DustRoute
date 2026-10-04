//! Public-boundary regressions with immutable synthetic data and local mock sockets.
use super::super::test_support::{
    DurableRegistry, decode_reply, start_construction_bridge, temporary,
};
use super::super::*;

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
