use dustroute_library::blueprint::{BlueprintRecords, TypeContract};
use dustroute_library::building::BuildingRequest;
use dustroute_minecraft::{Block, BlockKind, Pos, RotationY};
use dustroute_translate::assembly_transform::AssemblyTransform;
use dustroute_translate::building::generate_building;
use dustroute_translate::piston_construction::{ElectricalConstruction, construction_batches};
use dustroute_translate::promotion::{CheckStatus, review_assembly_with_context};
use serde_json::json;

fn specification() -> BuildingRequest {
    serde_json::from_value(json!({"namespace":"test.enclosure","width":5,"depth":5,"height":4,"wall_material":"glass"})).unwrap()
}

#[test]
fn enclosure_parts_clearance_and_shared_construction_are_exact() {
    let generated = generate_building(specification()).unwrap();
    assert_eq!(generated.verification.status, CheckStatus::Passed);
    assert!(!generated.verification.live_world_verified);
    assert_eq!(
        (
            generated.parts["floor"],
            generated.parts["walls"],
            generated.parts["roof"]
        ),
        (25, 30, 25)
    );
    assert_eq!(generated.expected.blocks.len(), 80);
    assert_eq!(
        (
            generated.verification.construction_steps,
            generated.verification.construction_batches
        ),
        (80, 3)
    );
    assert_eq!(
        (
            generated.verification.removal_steps,
            generated.verification.removal_batches
        ),
        (80, 3)
    );
    let indexed = generated
        .expected
        .blocks
        .iter()
        .map(|b| (b.pos, b.name.as_str()))
        .collect::<std::collections::BTreeMap<_, _>>();
    for x in 0..5 {
        for z in 0..5 {
            for y in 0..4 {
                let expected = if y == 0 || y == 3 {
                    Some("minecraft:stone")
                } else if (x == 0 || x == 4 || z == 0 || z == 4) && !(z == 0 && x == 2) {
                    Some("minecraft:glass")
                } else {
                    None
                };
                assert_eq!(indexed.get(&Pos::new(x, y, z)).copied(), expected);
            }
        }
    }
    let pattern = generated
        .records
        .types
        .iter()
        .find(|t| t.id.as_str().ends_with("clearance.pattern.v1"))
        .unwrap();
    let TypeContract::BlockPattern { blocks } = &pattern.contract else {
        panic!("structural pattern required")
    };
    assert_eq!(blocks.len(), 7 * 6 * 7);
    assert!(
        blocks
            .iter()
            .any(|b| b.position == Pos::new(2, 1, 0) && b.block.kind == BlockKind::Air)
    );
    let saved: BlueprintRecords =
        serde_json::from_value(serde_json::to_value(&generated.records).unwrap()).unwrap();
    assert_eq!(
        saved.catalog().unwrap(),
        generated.records.catalog().unwrap()
    );
}

#[test]
fn missing_material_and_occupied_passage_fail_structural_requirements() {
    let generated = generate_building(specification()).unwrap();
    let mut catalog = generated.records.catalog().unwrap();
    catalog
        .insert_revisions(generated.request.revisions.clone())
        .unwrap();
    for position in [
        Pos::new(0, 0, 0),
        Pos::new(2, 1, 0),
        Pos::new(2, 1, 2),
        Pos::new(-1, 1, 2),
    ] {
        let mut candidate = generated.request.candidate_state.assembly.clone();
        if position == Pos::new(0, 0, 0) {
            candidate.blocks.retain(|b| b.position != position);
        } else {
            candidate
                .blocks
                .push(dustroute_library::blueprint::PositionedBlock {
                    position,
                    block: Block::new(BlockKind::Solid),
                });
        }
        let report = review_assembly_with_context(
            &catalog,
            &candidate,
            Some(&generated.context.clone().into()),
            Default::default(),
        )
        .unwrap();
        assert_eq!(
            report.status(),
            CheckStatus::Failed,
            "position {position:?}"
        );
    }
}

#[test]
fn supported_cube_materials_keep_exact_identity_in_structure_and_construction() {
    for materials in [
        ["stone", "cobblestone", "smooth_stone"],
        ["smooth_quartz", "glass", "tinted_glass"],
    ] {
        let request = serde_json::from_value(json!({
            "namespace":"test.palette","width":3,"depth":3,"height":4,
            "floor_material":materials[0],"wall_material":materials[1],"roof_material":materials[2]
        }))
        .unwrap();
        let generated = generate_building(request).unwrap();
        assert_eq!(generated.verification.status, CheckStatus::Passed);
        let mut catalog = generated.records.catalog().unwrap();
        catalog
            .insert_revisions(generated.request.revisions.clone())
            .unwrap();
        let mut changed = generated.request.candidate_state.assembly.clone();
        // Glass and cubes share geometry, but substituting the roof for one floor
        // block violates the native-material contract even when placement is valid.
        let roof = changed
            .blocks
            .iter()
            .find(|b| b.position == Pos::new(0, 3, 0))
            .unwrap()
            .block
            .clone();
        changed
            .blocks
            .iter_mut()
            .find(|b| b.position == Pos::default())
            .unwrap()
            .block = roof;
        let report = review_assembly_with_context(
            &catalog,
            &changed,
            Some(&generated.context.clone().into()),
            Default::default(),
        )
        .unwrap();
        assert_eq!(report.status(), CheckStatus::Failed);
    }
}

#[test]
fn exactly_256_blocks_are_admitted_and_a_larger_design_is_rejected() {
    let mut request: BuildingRequest = serde_json::from_value(json!({
        "namespace":"test.budget","width":9,"depth":8,"height":6,
        "entrance":{"offset":2,"width":4,"height":2}
    }))
    .unwrap();
    let generated = generate_building(request.clone()).unwrap();
    assert_eq!(generated.expected.blocks.len(), 256);
    assert_eq!(generated.verification.construction_batches, 8);
    request.entrance.as_mut().unwrap().width = 3;
    assert!(
        generate_building(request)
            .unwrap_err()
            .detail
            .contains("256-block")
    );
}

#[test]
fn every_rotation_and_relocation_retains_parts_and_pattern_obligations() {
    let generated = generate_building(specification()).unwrap();
    let mut catalog = generated.records.catalog().unwrap();
    catalog
        .insert_revisions(generated.request.revisions.clone())
        .unwrap();
    for rotation in [
        RotationY::R0,
        RotationY::R90,
        RotationY::R180,
        RotationY::R270,
    ] {
        let transform = AssemblyTransform {
            source_anchor: Pos::default(),
            target_anchor: Pos::new(-27, 180, 35),
            rotation,
        };
        let (assembly, context) = transform
            .apply(
                &generated.request.candidate_state.assembly,
                &generated.context,
            )
            .unwrap();
        let report = review_assembly_with_context(
            &catalog,
            &assembly,
            Some(&context.clone().into()),
            Default::default(),
        )
        .unwrap();
        assert_eq!(report.status(), CheckStatus::Passed, "{rotation:?}");
        let world = assembly.inspect(&catalog).unwrap().proposed_world();
        let construction =
            ElectricalConstruction::new(&world, context.known_region, context.root_limits).unwrap();
        assert_eq!(construction.settled().blocks.len(), 80);
        assert_eq!(construction_batches(construction.build_steps()).count(), 3);
        assert!(
            construction
                .remove_steps()
                .last()
                .unwrap()
                .expected
                .is_empty()
        );
    }
}

#[test]
fn malformed_oversized_and_unsupported_buildings_are_not_candidates() {
    for value in [
        json!({"namespace":"bad","width":2,"depth":5,"height":4}),
        json!({"namespace":"bad","width":16,"depth":16,"height":16}),
        json!({"namespace":"bad","width":5,"depth":5,"height":3}),
        json!({"namespace":"bad","width":5,"depth":5,"height":4,"entrance":{"offset":4,"width":1,"height":2}}),
        json!({"namespace":"BAD namespace","width":5,"depth":5,"height":4}),
    ] {
        assert!(generate_building(serde_json::from_value(value).unwrap()).is_err());
    }
    assert!(
        serde_json::from_value::<BuildingRequest>(
            json!({"namespace":"bad","width":5,"depth":5,"height":4,"wall_material":"oak_planks"})
        )
        .is_err()
    );
    let request=serde_json::from_value(json!({"namespace":"open","width":4,"depth":6,"height":3,"roof":"none","entrance":{"offset":1,"width":2,"height":2},"floor_material":"smooth_quartz"})).unwrap();
    let generated = generate_building(request).unwrap();
    assert!(!generated.parts.contains_key("roof"));
    assert!(generated.expected.blocks.iter().all(|b| b.pos.y < 3));
}

#[test]
fn structural_review_cannot_pass_when_initialization_budget_is_exhausted() {
    use dustroute_translate::behavior_type::BehaviorBudget;
    let generated = generate_building(specification()).unwrap();
    let mut catalog = generated.records.catalog().unwrap();
    catalog
        .insert_revisions(generated.request.revisions.clone())
        .unwrap();
    for budget in [
        BehaviorBudget {
            max_steps: 0,
            ..Default::default()
        },
        BehaviorBudget {
            max_states: 1,
            ..Default::default()
        },
        BehaviorBudget {
            max_elapsed: std::time::Duration::ZERO,
            ..Default::default()
        },
    ] {
        let report = review_assembly_with_context(
            &catalog,
            &generated.request.candidate_state.assembly,
            Some(&generated.context.clone().into()),
            budget,
        )
        .unwrap();
        assert_eq!(report.status(), CheckStatus::Undetermined);
    }
}

fn device_pattern_fixture(
    snapshot: serde_json::Value,
    position: Pos,
) -> (
    dustroute_library::blueprint::BlueprintCatalog,
    dustroute_library::assembly::Assembly,
    dustroute_library::runtime_behavior::RuntimeBehaviorContext,
) {
    use dustroute_library::blueprint::*;
    let generated = generate_building(specification()).unwrap();
    let mut catalog = generated.records.catalog().unwrap();
    let snapshot: dustroute_translate::snapshot::MinecraftSnapshot =
        serde_json::from_value(snapshot).unwrap();
    let region = dustroute_minecraft::Region::new(snapshot.min, snapshot.max);
    let mut assembly = dustroute_translate::snapshot::assembly_from_snapshot(
        &snapshot,
        "Uncontrolled device",
        vec![region],
    )
    .unwrap();
    let mut revision = generated
        .request
        .revisions
        .iter()
        .find(|r| r.id.as_str().ends_with("floor.v1"))
        .unwrap()
        .clone();
    revision.id = BlueprintRevisionId::new("test.device.v1").unwrap();
    revision.blocks = assembly.blocks.clone();
    revision.ports[0].position = position;
    let type_id = TypeRevisionId::new("test.device.pattern.v1").unwrap();
    catalog
        .insert_type(TypeRevision {
            id: type_id.clone(),
            name: "Exact device state".into(),
            contract: TypeContract::BlockPattern {
                blocks: vec![PositionedBlock {
                    position: Pos::default(),
                    block: assembly
                        .blocks
                        .iter()
                        .find(|b| b.position == position)
                        .unwrap()
                        .block
                        .clone(),
                }],
            },
        })
        .unwrap();
    revision.static_type_bindings[0].type_revision = type_id;
    assembly.instances = vec![BlueprintInclusion {
        instance: InstanceId::new("device").unwrap(),
        revision: revision.id.clone(),
        origin: Pos::default(),
        rotation: RotationY::R0,
    }];
    catalog.insert_revision(revision).unwrap();
    (
        catalog,
        assembly,
        dustroute_library::runtime_behavior::RuntimeBehaviorContext::fresh_pistons(region, vec![]),
    )
}

#[test]
fn undeclared_input_behavior_does_not_become_a_structural_pass() {
    let position = Pos::new(0, 1, 0);
    let (catalog, assembly, mut context) = device_pattern_fixture(
        json!({"min":{"x":-1,"y":-1,"z":-1},"max":{"x":2,"y":3,"z":2},"blocks":[
            {"pos":{"x":0,"y":0,"z":0},"name":"minecraft:stone"},
            {"pos":{"x":0,"y":1,"z":0},"name":"minecraft:lever","properties":{"face":"floor","facing":"north","powered":"false"}}
        ]}),
        position,
    );
    let report = review_assembly_with_context(
        &catalog,
        &assembly,
        Some(&context.clone().into()),
        Default::default(),
    )
    .unwrap();
    assert_eq!(report.status(), CheckStatus::Passed);
    context.input_levers = vec![position];
    let report = review_assembly_with_context(
        &catalog,
        &assembly,
        Some(&context.into()),
        Default::default(),
    )
    .unwrap();
    assert_eq!(report.status(), CheckStatus::Undetermined);
}

#[test]
fn structural_patterns_also_detect_device_changes_during_initialization() {
    let (catalog, assembly, context) = device_pattern_fixture(
        json!({"min":{"x":-1,"y":-1,"z":-1},"max":{"x":3,"y":2,"z":2},"blocks":[
            {"pos":{"x":0,"y":0,"z":0},"name":"minecraft:redstone_lamp","properties":{"lit":"false"}},
            {"pos":{"x":1,"y":0,"z":0},"name":"minecraft:redstone_block"}
        ]}),
        Pos::default(),
    );
    let report = review_assembly_with_context(
        &catalog,
        &assembly,
        Some(&context.into()),
        Default::default(),
    )
    .unwrap();
    assert_eq!(report.status(), CheckStatus::Failed);
    assert!(
        report
            .occurrences
            .values()
            .flat_map(|r| &r.checks)
            .any(
                |c| c.kind == dustroute_translate::promotion::CheckKind::StaticType
                    && c.status == CheckStatus::Failed
                    && c.detail.contains("runtime")
            )
    );
}
