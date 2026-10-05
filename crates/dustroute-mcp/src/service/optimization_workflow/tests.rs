use crate::operations::OperationResult;
use crate::service::requests::{ConfirmedOperationParams, NewMacroOptimizationParams};
use crate::service::{
    ExpansionEvidence, RepairLifecycle, StoredCircuit, test_support, typed_reply,
    world_from_snapshot_for_service,
};
use crate::state::PlanStateStore;
use crate::{DustRouteMcp, McpPolicy};
use dustroute_optimize::{
    ObservedMacroMetrics, extract_model_boundary_with_context,
    find_builtin_verified_macro_replacements, plan_macro_replacement_with_reserved,
    validate_macro_structure,
};
use dustroute_physical::{Block, BlockKind, Pos, World};
use dustroute_translate::api::ReverseRequest;
use dustroute_translate::minecraft_export::{ExportPurpose, JavaExportConfig, native_block_state};
use dustroute_translate::snapshot::{MinecraftSnapshot, MinecraftSnapshotBlock};
use dustroute_translate::world_reverse::RegionBounds;
use rmcp::handler::server::wrapper::Parameters;
use serde_json::{Value, json};
use std::collections::BTreeSet;
use tokio::time::{Duration, Instant};

fn service(root: &std::path::Path) -> DustRouteMcp {
    let mut service = DustRouteMcp::with_test_transport_and_player(
        "127.0.0.1:1",
        McpPolicy {
            read_only: false,
            ..Default::default()
        },
        "Tester",
    );
    service.state_store = PlanStateStore::new(root.to_owned(), 3600);
    service
}

/// A declared synthetic volume, never inferred coverage of a live observation.
fn fixture_snapshot(world: &World, bounds: RegionBounds) -> MinecraftSnapshot {
    MinecraftSnapshot {
        min: bounds.min,
        max: bounds.max,
        blocks: world
            .iter()
            .map(|(pos, block)| {
                let state = native_block_state(
                    block,
                    &JavaExportConfig::default(),
                    ExportPurpose::InitialPlacement,
                )
                .unwrap();
                MinecraftSnapshotBlock {
                    pos: *pos,
                    name: state.name().into(),
                    properties: state.properties().clone(),
                }
            })
            .collect(),
    }
}
async fn capture(service: &DustRouteMcp, snapshot: MinecraftSnapshot) -> uuid::Uuid {
    service
        .store_circuit(StoredCircuit {
            player: "Tester".into(),
            dimension: "minecraft:overworld".into(),
            bounds: RegionBounds::new(snapshot.min, snapshot.max),
            target: None,
            snapshot: service.bridge.share_snapshot(snapshot).unwrap(),
            expansion: ExpansionEvidence::Unspecified {},
            complete: true,
            expires_at: Instant::now() + Duration::from_secs(300),
        })
        .await
}
fn detour(devices: bool) -> (MinecraftSnapshot, RegionBounds) {
    let mut world = World::new();
    for x in -1..=5 {
        for z in 0..=2 {
            world.set(Pos::new(x, 0, z), Block::new(BlockKind::Solid));
        }
    }
    for pos in [
        Pos::new(0, 1, 0),
        Pos::new(0, 1, 1),
        Pos::new(0, 1, 2),
        Pos::new(1, 1, 2),
        Pos::new(2, 1, 2),
        Pos::new(3, 1, 2),
        Pos::new(4, 1, 2),
        Pos::new(4, 1, 1),
        Pos::new(4, 1, 0),
    ] {
        world.place(BlockKind::RedstoneWire, pos);
    }
    if devices {
        world.place(BlockKind::Lever, Pos::new(-1, 1, 0));
        world.set(Pos::new(4, 0, 0), Block::new(BlockKind::RedstoneLamp));
    }
    dustroute_translate::wire::update_wire_shapes(&mut world);
    let bounds = RegionBounds::new(Pos::new(-1, 0, 0), Pos::new(5, 2, 2));
    (
        fixture_snapshot(&world, bounds),
        RegionBounds::new(Pos::new(0, 1, 0), Pos::new(4, 1, 2)),
    )
}

async fn propose_wire(service: &DustRouteMcp, id: uuid::Uuid, focus: RegionBounds) -> Value {
    test_support::decode_reply(
        &service
            .new_optimization(Parameters(
                serde_json::from_value(
                    json!({"circuit_id":id,"focus":focus,"objective":"density_then_wire_length"}),
                )
                .unwrap(),
            ))
            .await,
    )
    .unwrap()
}

#[tokio::test]
async fn wire_candidate_preserves_contract_and_history_subset_after_restart() {
    let root = test_support::temporary();
    let service = service(&root);
    let (snapshot, focus) = detour(true);
    let id = capture(&service, snapshot).await;
    let response = propose_wire(&service, id, focus).await;
    assert_eq!(response["ok"], true, "{response}");
    assert_eq!(response["objective"], "density_then_wire_length");
    assert_eq!(response["outside_focus_fixed"], true);
    assert_eq!(response["metrics"]["wire_blocks_before"], 9);
    assert_eq!(response["metrics"]["wire_blocks_after"], 5);
    assert_eq!(response["verification"]["semantic"]["available"], true);
    assert_eq!(response["verification"]["semantic"]["equivalent"], true);
    assert_eq!(response["verification"]["steady_state"]["state"], "passed");
    assert_eq!(response["verification"]["transitions"]["state"], "passed");
    assert_eq!(response["contract_assessment"]["satisfied"], true);
    // A strength difference is still reported even when the selected contract allows it.
    assert_eq!(
        response["verification"]["boundary_strength"]["state"],
        "failed"
    );
    assert!(!response["phase_trace"].as_array().unwrap().is_empty());
    assert!(response.get("execution_progress").is_none());
    let operation_id = uuid::Uuid::parse_str(response["operation_id"].as_str().unwrap()).unwrap();
    let recorded = service
        .operations
        .get(operation_id)
        .await
        .unwrap()
        .result
        .unwrap();
    assert!(matches!(
        &recorded,
        OperationResult::WireOptimizationCandidate(_)
    ));
    assert!(recorded.progress().is_none());
    assert!(!recorded.consumed());
    assert!(!recorded.failed());
    let history = test_support::decode_reply(&typed_reply(recorded)).unwrap();
    for field in [
        "circuit_id",
        "focus",
        "patch",
        "contract",
        "contract_assessment",
    ] {
        assert_eq!(history[field], response[field]);
    }
    assert_eq!(
        history["semantic_verification"],
        response["verification"]["semantic"]
    );
    assert_eq!(history.as_object().unwrap().len(), 6);
    assert!(service.operations.activity(operation_id).await.is_none());
    let plan = service.repair_plan(operation_id).await.unwrap().unwrap();
    assert!(plan.contract_satisfied);
    assert!(plan.baseline_truth_table.is_some());
    assert_eq!(plan.lifecycle, RepairLifecycle::Draft);
    assert!(
        plan.patch
            .changes
            .iter()
            .all(|change| focus.contains(change.pos))
    );
    let restarted = self::service(&root);
    let saved = restarted.repair_plan(operation_id).await.unwrap().unwrap();
    assert_eq!(saved.patch, plan.patch);
    assert_eq!(saved.preserved_boundary, plan.preserved_boundary);
    assert_eq!(saved.baseline_truth_table, plan.baseline_truth_table);
    assert_eq!(saved.lifecycle, RepairLifecycle::Draft);
    assert!(saved.contract_satisfied);
    assert!(restarted.operations.list().await.is_empty());
    std::fs::remove_dir_all(root).unwrap();
}

#[tokio::test]
async fn unavailable_wire_semantics_remain_null_and_cannot_authorize_execution() {
    let root = test_support::temporary();
    let service = service(&root);
    let (snapshot, focus) = detour(false);
    let id = capture(&service, snapshot).await;
    let response = propose_wire(&service, id, focus).await;
    assert_eq!(response["ok"], true, "{response}");
    assert_eq!(response["verification"]["semantic"]["available"], false);
    for side in ["original", "candidate"] {
        assert_eq!(
            response["verification"]["truth_table_failures"][side]["code"],
            "no_inputs"
        );
    }
    assert!(
        response["verification"]["semantic"]
            .get("equivalent")
            .is_none()
    );
    for field in ["steady_state", "transitions", "boundary_strength"] {
        assert!(
            response["verification"]
                .as_object()
                .unwrap()
                .contains_key(field)
        );
        assert!(response["verification"][field].is_null());
    }
    assert_eq!(
        response["contract_assessment"]["logical"]["state"],
        "unavailable"
    );
    assert_eq!(response["contract_assessment"]["satisfied"], false);
    let operation_id = uuid::Uuid::parse_str(response["operation_id"].as_str().unwrap()).unwrap();
    let plan = service.repair_plan(operation_id).await.unwrap().unwrap();
    assert!(!plan.contract_satisfied);
    assert!(plan.baseline_truth_table.is_none());
    let restarted = self::service(&root);
    for owner in [&service, &restarted] {
        let refused = test_support::decode_reply(
            &owner
                .mutate_repair(
                    ConfirmedOperationParams {
                        operation_id: operation_id.to_string(),
                        confirm: true,
                    },
                    false,
                )
                .await,
        )
        .unwrap();
        assert_eq!(refused["ok"], false);
        assert_eq!(refused["error_code"], "verification_failed");
        assert!(refused["failure"]["progress"].is_null());
        assert_eq!(
            owner
                .repair_plan(operation_id)
                .await
                .unwrap()
                .unwrap()
                .lifecycle,
            RepairLifecycle::Draft
        );
        assert!(owner.operations.activity(operation_id).await.is_none());
    }
    std::fs::remove_dir_all(root).unwrap();
}

#[tokio::test]
async fn observed_macro_collision_refuses_before_plan_or_history_storage() {
    let root = test_support::temporary();
    let service = service(&root);
    let world = dustroute_translate::cells::compiled_xor_cell()
        .unwrap()
        .world;
    let (low, high) = world.bounds().unwrap();
    let bounds = RegionBounds::new(low.offset(-8, -3, -8), high.offset(8, 6, 8));
    let id = capture(&service, fixture_snapshot(&world, bounds)).await;
    let (_, circuit) = service
        .load_circuit(&id.to_string(), "Tester")
        .await
        .unwrap();
    let observed = world_from_snapshot_for_service(&circuit.snapshot).unwrap();
    let staged = service.app.analyze_physical(
        &observed,
        ReverseRequest::new(bounds)
            .with_truth_table(16)
            .with_observation_complete(true),
    );
    let model = staged.reverse.functional_network.as_ref().unwrap();
    let candidates = find_builtin_verified_macro_replacements(
        model,
        "java",
        "1.21.11",
        ObservedMacroMetrics::from_world(&observed),
    );
    let candidate = candidates
        .iter()
        .find(|candidate| {
            candidate.component_id.as_str() == dustroute_library::DUSTROUTE_COMPACT_XOR_ID
        })
        .unwrap();
    let boundary = extract_model_boundary_with_context(model, &observed, &staged.reverse.analysis);
    let reserved = boundary
        .iter()
        .filter_map(|port| port.driver_position)
        .collect();
    let boundaries = boundary
        .iter()
        .map(|port| port.position)
        .collect::<BTreeSet<_>>();
    let replaceable = observed
        .positions()
        .filter(|pos| !boundaries.contains(pos))
        .collect();
    let plan = plan_macro_replacement_with_reserved(candidate, &boundary, &reserved).unwrap();
    let structural = validate_macro_structure(&plan, &observed, &replaceable);
    // The pre-existing whole-block equality also compares observation metadata.
    // Preserve that refusal; this migration does not change source ownership or
    // physical equivalence to make a synthetic observation admissible.
    assert_eq!(structural.candidate_collisions, vec![Pos::new(50, 2, 0)]);
    assert!(structural.route_collisions.is_empty());
    assert!(structural.route_cross_net_contacts.is_empty());
    assert!(structural.candidate_support_issues.is_empty());
    assert!(structural.blocked_route_supports.is_empty());
    let position = structural.candidate_collisions[0];
    let candidate_block = plan
        .placed
        .blocks()
        .find(|(pos, _)| *pos == position)
        .unwrap()
        .1;
    let observed_block = observed.get(position).unwrap();
    assert_eq!(observed_block.kind, candidate_block.kind);
    assert_ne!(observed_block, &candidate_block);
    assert!(observed_block.observed_name.is_some());
    assert_eq!(world.get(position), Some(&candidate_block));
    assert!(validate_macro_structure(&plan, &world, &replaceable).valid());
    let reply = service
        .new_macro_optimization(Parameters(NewMacroOptimizationParams {
            player: None,
            circuit_id: id.to_string(),
            component_id: dustroute_library::DUSTROUTE_COMPACT_XOR_ID.into(),
            contract: None,
        }))
        .await;
    let response = test_support::decode_reply(&reply).unwrap();
    assert_eq!(reply.is_error, Some(true));
    assert_eq!(response["ok"], false);
    assert_eq!(response["error_code"], "verification_failed");
    assert_eq!(
        response["error"],
        "macro replacement failed structural validation"
    );
    assert!(response["failure"]["progress"].is_null());
    assert!(response.get("operation_id").is_none());
    assert!(service.operations.list().await.is_empty());
    assert!(!root.exists());
}

#[tokio::test]
async fn optimization_admission_precedes_source_loading_and_parameter_checks() {
    let root = test_support::temporary();
    let mut service = service(&root);
    service.policy.allowed_players = ["AnotherPlayer".into()].into();
    let focus = RegionBounds::new(Pos::default(), Pos::new(2, 1, 2));
    let replies = [
        service
            .new_macro_optimization(Parameters(NewMacroOptimizationParams {
                player: None,
                circuit_id: "not-an-id".into(),
                component_id: "not-a-candidate".into(),
                contract: None,
            }))
            .await,
        service
            .new_optimization(Parameters(
                serde_json::from_value(json!({
                    "circuit_id":"not-an-id", "focus":focus, "objective":"not-an-objective"
                }))
                .unwrap(),
            ))
            .await,
    ];
    for reply in replies {
        assert_eq!(reply.is_error, Some(true));
        let result = test_support::decode_reply(&reply).unwrap();
        assert_eq!(result["error_code"], "permission_denied");
        assert_eq!(result["failure"]["primary"]["phase"], "admission");
        assert!(result["failure"]["progress"].is_null());
        assert!(!result["error"].as_str().unwrap().starts_with('{'));
    }
    assert!(service.operations.list().await.is_empty());
    assert!(!root.exists());
}

#[tokio::test]
async fn invalid_focus_and_contract_size_refuse_before_plan_storage() {
    let root = test_support::temporary();
    let service = service(&root);
    let (snapshot, focus) = detour(true);
    let id = capture(&service, snapshot).await;
    for (declared_focus, contract, code) in [
        (
            RegionBounds::new(Pos::new(-20, 1, 0), Pos::new(4, 1, 2)),
            Value::Null,
            "invalid_argument",
        ),
        (
            focus,
            json!({"mutation":{"maximum_changed_blocks":1}}),
            "verification_failed",
        ),
    ] {
        let reply = service.new_optimization(Parameters(serde_json::from_value(json!({
            "circuit_id":id,"focus":declared_focus,"objective":"wire_length","contract":contract
        })).unwrap())).await;
        assert_eq!(reply.is_error, Some(true));
        let result = test_support::decode_reply(&reply).unwrap();
        assert_eq!(result["error_code"], code, "{result}");
        assert!(result["failure"]["progress"].is_null());
    }
    assert!(service.operations.list().await.is_empty());
    assert!(!root.exists());
}

#[tokio::test]
async fn exhausted_wire_search_preserves_limits_without_publishing_a_partial_plan() {
    let root = test_support::temporary();
    let service = service(&root);
    let mut world = World::new();
    for x in 0..=8 {
        for z in 0..=4 {
            world.set(Pos::new(x, 0, z), Block::new(BlockKind::Solid));
        }
    }
    for wire in (0..=8)
        .map(|x| Pos::new(x, 1, 0))
        .chain((1..=4).map(|z| Pos::new(8, 1, z)))
        .chain((4..=7).rev().map(|x| Pos::new(x, 1, 4)))
    {
        world.place(BlockKind::RedstoneWire, wire);
    }
    dustroute_translate::wire::update_wire_shapes(&mut world);
    let focus = RegionBounds::new(Pos::new(0, 1, 0), Pos::new(8, 1, 4));
    let snapshot = fixture_snapshot(
        &world,
        RegionBounds::new(Pos::new(0, 0, 0), Pos::new(8, 2, 4)),
    );
    let id = capture(&service, snapshot).await;
    let reply = service
        .new_optimization(Parameters(
            serde_json::from_value(json!({
                "circuit_id":id, "focus":focus, "objective":"density_then_wire_length",
                "contract":{"analog":{"preserve_strength":true}},
                "search":{"max_expansions":1}
            }))
            .unwrap(),
        ))
        .await;
    let response = test_support::decode_reply(&reply).unwrap();
    assert_eq!(reply.is_error, Some(true));
    assert_eq!(response["ok"], false, "{response}");
    assert_eq!(response["error_code"], "resource_limit");
    assert_eq!(response["cause"], "no_strength_preserving_route");
    assert_eq!(response["search"]["stop_reason"], "max_expansions");
    assert_eq!(response["search"]["budget"]["max_expansions"], 1);
    assert_eq!(response["search"]["expansions"], 1);
    assert_eq!(response["search"]["truncated"], true);
    assert_eq!(response["writes_minecraft"], false);
    assert_eq!(response["impossibility_proven"], false);
    assert_eq!(response["retryable"], false);
    assert!(response.get("operation_id").is_none());
    assert!(response.get("patch").is_none());
    assert!(service.operations.list().await.is_empty());
    assert!(!root.exists());
    // The same geometry has an acceptable model-only candidate at the default
    // budget. A smaller budget's refusal was not a proof of impossibility.
    let candidate =
        dustroute_optimize::optimize_physical_wire_path_with_constraints(&world, focus, true)
            .unwrap();
    assert_eq!(candidate.wire_blocks_after, 17);
    assert!(!candidate.patch.changes.is_empty());
}
