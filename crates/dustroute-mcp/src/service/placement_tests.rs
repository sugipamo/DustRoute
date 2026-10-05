//! Placement proof, preview and fresh-state admission contracts.
use super::*;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::net::TcpListener;

#[test]
fn boundary_record_keeps_static_identity_and_ignores_live_power() {
    let record = boundary_block_record(&dustroute_translate::snapshot::MinecraftSnapshotBlock {
        pos: Pos::new(1, 64, 2),
        name: "minecraft:repeater".to_owned(),
        properties: BTreeMap::from([
            ("facing".to_owned(), "west".to_owned()),
            ("delay".to_owned(), "2".to_owned()),
            ("powered".to_owned(), "true".to_owned()),
            ("locked".to_owned(), "false".to_owned()),
        ]),
    });
    assert_eq!(record.static_properties["facing"], "west");
    assert_eq!(record.static_properties["delay"], "2");
    assert!(!record.static_properties.contains_key("powered"));
    assert!(!record.static_properties.contains_key("locked"));
}

#[test]
fn placement_baseline_rejects_dynamic_state_changes_between_preview_and_apply() {
    let mut expected = dustroute_physical::Block::new(BlockKind::Lever);
    expected.facing = Some(dustroute_physical::Facing::North);
    expected.support_offset = Some(Pos::new(0, -1, 0));
    expected.powered = Some(false);
    let mut actual = expected.clone();
    actual.powered = Some(true);
    assert!(!placement_baseline_matches(Some(&actual), &expected));
    assert!(placement_baseline_matches(Some(&expected), &expected));
    assert!(placement_baseline_matches(
        None,
        &dustroute_physical::Block::new(BlockKind::Air)
    ));
}

#[test]
fn placement_baseline_accepts_observed_equivalent_synthetic_blocks() {
    let mut expected = dustroute_physical::Block::new(BlockKind::RedstoneTorch);
    expected.facing = Some(dustroute_physical::Facing::East);
    expected.support_offset = Some(Pos::new(-1, 0, 0));
    let mut actual = expected.clone();
    actual.observed_name = Some("minecraft:redstone_wall_torch".to_owned());
    actual.observed_properties = BTreeMap::from([
        ("facing".to_owned(), "east".to_owned()),
        ("lit".to_owned(), "true".to_owned()),
    ]);
    actual.observation_classification = dustroute_physical::ObservationClassification::Exact;
    actual.facing = None;
    actual.powered = Some(true);
    assert!(placement_baseline_matches(Some(&actual), &expected));
}

#[tokio::test]
async fn tampered_placement_is_revalidated_before_any_bridge_write() {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap().to_string();
    let server = tokio::spawn(async move {
        // Baseline verification followed by placement-context validation.
        // Any write request is a regression, even if live verification
        // would later discover the broken placement.
        for _ in 0..2 {
            let (mut stream, _) = listener.accept().await.unwrap();
            let mut line = String::new();
            BufReader::new(&mut stream)
                .read_line(&mut line)
                .await
                .unwrap();
            let request: Value = serde_json::from_str(&line).unwrap();
            assert_eq!(request["method"], "scan_region");
            let response = json!({ "id": request["id"], "result": crate::bridge::test_readback_response(&request,json!({
                "min": request["params"]["min"], "max": request["params"]["max"], "blocks": []
            }))});
            stream
                .write_all(format!("{response}\n").as_bytes())
                .await
                .unwrap();
        }
    });
    let service = DustRouteMcp::with_test_transport_and_policy(
        address,
        McpPolicy {
            read_only: false,
            preview_required: false,
            ..McpPolicy::default()
        },
    );
    let mut proposed = dustroute_translate::world::World::new();
    proposed.place(BlockKind::Solid, Pos::new(0, 1, 0));
    let mut plan = plan_world_overlay(
        &dustroute_translate::world::World::new(),
        &dustroute_translate::world::ValidatedWorld::try_from(proposed).unwrap(),
        Pos::new(0, 0, 0),
        10,
    )
    .unwrap();
    plan.changes[0].after = dustroute_translate::world::Block::new(BlockKind::Repeater);
    let id = plan.operation_id;
    service
        .plans
        .placements()
        .lock()
        .await
        .insert(plan, "minecraft:overworld".into(), None);
    let response: Value = test_support::decode_reply(
        &service
            .mutate_placement(
                ConfirmedOperationParams {
                    operation_id: id.to_string(),
                    confirm: true,
                },
                false,
            )
            .await,
    )
    .unwrap();
    assert_eq!(response["ok"], false);
    assert!(response["error"].as_str().unwrap().contains("validation"));
    server.await.unwrap();
    assert!(!service.plans.placements().lock().await.is_applied(&id));
}

#[tokio::test]
async fn placement_requires_show_before_invoke_when_preview_is_required() {
    let policy = McpPolicy {
        read_only: false,
        preview_required: true,
        ..McpPolicy::default()
    };
    let service = DustRouteMcp::with_test_transport_and_policy("127.0.0.1:1", policy);
    let plan = plan_world_overlay(
        &dustroute_physical::World::new(),
        &dustroute_translate::world::ValidatedWorld::try_from(dustroute_physical::World::new())
            .unwrap(),
        Pos::new(0, 0, 0),
        1,
    )
    .unwrap();
    let operation_id = plan.operation_id;
    service
        .plans
        .placements()
        .lock()
        .await
        .insert(plan, "minecraft:overworld".into(), None);

    let rejected: Value = test_support::decode_reply(
        &service
            .invoke_operation(Parameters(InvokeOperationParams {
                blueprint_decision: None,
                operation_id: operation_id.to_string(),
                confirm: true,
                contracts: None,
            }))
            .await,
    )
    .unwrap();
    assert_eq!(rejected["error_code"], "invalid_state");
    assert!(
        !service
            .plans
            .placements()
            .lock()
            .await
            .get(&operation_id)
            .is_some_and(|plan| plan.previewed)
    );

    let shown: Value = test_support::decode_reply(
        &service
            .show_operation(Parameters(ShowOperationParams {
                operation_id: operation_id.to_string(),
                player: None,
            }))
            .await,
    )
    .unwrap();
    assert_eq!(shown["ok"], true);
    assert_eq!(shown["plan"]["previewed"], true);
    assert!(
        service
            .plans
            .placements()
            .lock()
            .await
            .get(&operation_id)
            .is_some_and(|plan| plan.previewed)
    );
}

#[tokio::test]
async fn compiled_placement_keeps_pinned_sources_and_composed_state_in_existing_api() {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap().to_string();
    let origin = Pos::new(10, 64, -20);
    let server = tokio::spawn(async move {
        loop {
            let (mut stream, _) = listener.accept().await.unwrap();
            let mut line = String::new();
            BufReader::new(&mut stream)
                .read_line(&mut line)
                .await
                .unwrap();
            let req: Value = serde_json::from_str(&line).unwrap();
            let result = match req["method"].as_str().unwrap() {
                "observe_player" => {
                    json!({"player":"builder","eye_position":{"x":10.0,"y":67.0,"z":-20.0},"yaw":0.0,"pitch":0.0,"targeted_block":origin,"dimension":"minecraft:overworld"})
                }
                "scan_region" => {
                    json!({"min":req["params"]["min"],"max":req["params"]["max"],"blocks":[]})
                }
                other => panic!("unexpected request (planning must not write): {other}"),
            };
            let reply =
                json!({"id":req["id"],"result":crate::bridge::test_readback_response(&req,result)});
            stream
                .write_all(format!("{reply}\n").as_bytes())
                .await
                .unwrap();
        }
    });
    let service =
        DustRouteMcp::with_test_transport_and_player(address, McpPolicy::default(), "builder");
    for optimize in [false, true] {
        let proposed: Value = test_support::decode_reply(
            &service
                .new_placement(Parameters(
                    serde_json::from_value(json!({"circuit":"half-adder","optimize":optimize}))
                        .unwrap(),
                ))
                .await,
        )
        .unwrap();
        assert_eq!(proposed["ok"], true, "{proposed}");
        let operation_id =
            uuid::Uuid::parse_str(proposed["operation_id"].as_str().unwrap()).unwrap();
        let history = service.operations.get(operation_id).await.unwrap();
        let result = history.result.unwrap();
        assert!(matches!(
            &result,
            crate::operations::OperationResult::BuiltinPlacementPreview(_)
        ));
        assert!(result.progress().is_none());
        assert!(!result.consumed());
        // Both sides pass through the MCP codec; optimizer scores stay
        // native f64 internally rather than being rounded by JSON parsing.
        assert_eq!(
            test_support::decode_reply(&typed_reply(result)).unwrap(),
            proposed
        );
        assert!(service.operations.activity(operation_id).await.is_none());
        if optimize {
            assert_eq!(
                proposed["optimization"]["strategy"],
                "directional_x_toward_minimum_then_global"
            );
            assert!(proposed["optimization"]["safety_details"].is_string());
            assert!(proposed["optimization"]["phases"].is_array());
        } else {
            assert!(proposed.as_object().unwrap().contains_key("optimization"));
            assert!(proposed["optimization"].is_null());
        }
        let full: Value = test_support::decode_reply(
            &service
                .get_circuit_placement(Parameters(OperationParams {
                    operation_id: proposed["operation_id"].as_str().unwrap().into(),
                }))
                .await,
        )
        .unwrap();
        let plan: PlacementPlan = serde_json::from_value(full["plan"].clone()).unwrap();
        let saved = plan.assembly.as_ref().unwrap();
        assert_eq!(saved.coordinate_origin, origin);
        assert_eq!(
            json!(saved.revision.id),
            proposed["assembly_state"]["assembly_revision_id"]
        );
        let view = saved
            .revision
            .assembly
            .inspect(dustroute_library::builtin_blueprints::builtin_blueprints())
            .unwrap();
        assert!(!view.source_differences().is_empty());
        assert!(
            view.occurrences
                .values()
                .any(|occurrence| occurrence.revision.as_str()
                    == dustroute_library::builtin_blueprints::NOT_TOP_REVISION)
        );
        let world = view.proposed_world();
        assert_eq!(world.iter().count(), plan.changes.len());
        for change in &plan.changes {
            let local = change.pos.offset(-origin.x, -origin.y, -origin.z);
            assert_eq!(world.get(local), Some(&change.after));
        }
        assert!(!plan.previewed);
        let shown: Value = test_support::decode_reply(
            &service
                .show_operation(Parameters(ShowOperationParams {
                    operation_id: proposed["operation_id"].as_str().unwrap().into(),
                    player: None,
                }))
                .await,
        )
        .unwrap();
        assert_eq!(shown["ok"], true);
        assert_eq!(shown["plan"]["assembly"], full["plan"]["assembly"]);
        assert_eq!(shown["plan"]["previewed"], true);
        let mut legacy = full["plan"].clone();
        legacy.as_object_mut().unwrap().remove("assembly");
        assert!(
            serde_json::from_value::<PlacementPlan>(legacy)
                .unwrap()
                .assembly
                .is_none()
        );
    }
    server.abort();
}
