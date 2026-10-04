#[path = "support/archive_fixture.rs"]
mod archive_fixture;
use archive_fixture::FixtureJson;
#[allow(dead_code)]
#[path = "support/reference_door_blueprint.rs"]
mod fixture;

use dustroute_library::blueprint::*;
use dustroute_library::building::BuildingDesignRequest;
use dustroute_minecraft::{Block, BlockKind, Pos};
use dustroute_translate::blueprint_update::BlueprintUpdates;
use dustroute_translate::building::generate_building_design;
use dustroute_translate::promotion::{CheckStatus, review_assembly_with_context};
use serde_json::{Value, json};

fn design() -> Value {
    json!({"namespace":"test.design","name":"Windowed room",
        "known_region":{"min":{"x":-1,"y":-1,"z":-1},"max":{"x":5,"y":4,"z":5}},
        "parts":[
            {"name":"shell","shapes":[{"kind":"shell","material":"stone",
                "region":{"min":{"x":0,"y":0,"z":0},"max":{"x":4,"y":3,"z":4}}}],
                "cutouts":[
                    {"min":{"x":2,"y":1,"z":0},"max":{"x":2,"y":2,"z":0}},
                    {"min":{"x":4,"y":1,"z":2},"max":{"x":4,"y":2,"z":2}}]},
            {"name":"window","shapes":[{"kind":"blocks","material":"glass",
                "positions":[{"x":4,"y":1,"z":2},{"x":4,"y":2,"z":2}]}]}],
        "spaces":[
            {"name":"room","region":{"min":{"x":1,"y":1,"z":1},"max":{"x":3,"y":2,"z":3}}},
            {"name":"entrance","region":{"min":{"x":2,"y":1,"z":0},"max":{"x":2,"y":2,"z":0}}}]})
}
fn request(value: Value) -> BuildingDesignRequest {
    serde_json::from_value(value).unwrap()
}

#[test]
fn explicit_shell_cutouts_windows_and_named_air_compile_to_shared_blueprints() {
    let generated = generate_building_design(request(design()), None).unwrap();
    assert_eq!(generated.verification.status, CheckStatus::Passed);
    assert!(!generated.verification.live_world_verified);
    assert_eq!(generated.unique_blocks, 80);
    assert_eq!(generated.parts["shell"], 78);
    assert_eq!(generated.parts["window"], 2);
    let world: std::collections::BTreeMap<_, _> = generated
        .expected
        .blocks
        .iter()
        .map(|b| (b.pos, b.name.as_str()))
        .collect();
    for x in -1..=5 {
        for y in -1..=4 {
            for z in -1..=5 {
                let p = Pos::new(x, y, z);
                let expected = if x == 4 && (1..=2).contains(&y) && z == 2 {
                    Some("minecraft:glass")
                } else if (0..=4).contains(&x)
                    && (0..=3).contains(&y)
                    && (0..=4).contains(&z)
                    && (x == 0 || x == 4 || y == 0 || y == 3 || z == 0 || z == 4)
                    && !(x == 2 && (1..=2).contains(&y) && z == 0)
                {
                    Some("minecraft:stone")
                } else {
                    None
                };
                assert_eq!(world.get(&p).copied(), expected, "{p:?}");
            }
        }
    }
    assert!(
        generated
            .records
            .types
            .iter()
            .any(|t| t.id.as_str() == "test.design.space.room.pattern.v1")
    );
    let mut updates = BlueprintUpdates::new(generated.records.catalog().unwrap());
    updates.create(generated.request.clone()).unwrap();
    let mut updates = archive_fixture::updates(&updates.fixture_json().unwrap()).unwrap();
    updates.adopt(&generated.request.id).unwrap();
    let updates = archive_fixture::updates(&updates.fixture_json().unwrap()).unwrap();
    let mut changed = updates
        .catalog()
        .assembly(&generated.request.candidate_state.id)
        .unwrap()
        .assembly
        .clone();
    changed.blocks.push(PositionedBlock {
        position: Pos::new(2, 1, 2),
        block: Block::new(BlockKind::Solid),
    });
    assert_eq!(
        review_assembly_with_context(
            updates.catalog(),
            &changed,
            Some(&generated.context.into()),
            Default::default()
        )
        .unwrap()
        .status(),
        CheckStatus::Failed
    );
}

#[test]
fn non_rectangular_floors_and_same_material_overlap_have_unique_physical_writes() {
    let mut value = design();
    value["spaces"] = json!([]);
    value["parts"] = json!([
        {"name":"long","shapes":[{"kind":"fill","material":"stone",
            "region":{"min":{"x":0,"y":0,"z":0},"max":{"x":2,"y":0,"z":4}}}]},
        {"name":"wide","shapes":[{"kind":"fill","material":"stone",
            "region":{"min":{"x":0,"y":0,"z":0},"max":{"x":4,"y":0,"z":2}}}]}]);
    let generated = generate_building_design(request(value.clone()), None).unwrap();
    assert_eq!(generated.unique_blocks, 21);
    assert_eq!(generated.verification.construction_steps, 21);
    assert_eq!(generated.parts.values().sum::<usize>(), 30);
    for x in 0..5 {
        for z in 0..5 {
            assert_eq!(
                generated
                    .expected
                    .blocks
                    .iter()
                    .any(|b| b.pos == Pos::new(x, 0, z)),
                x < 3 || z < 3
            );
        }
    }
    value["parts"].as_array_mut().unwrap().reverse();
    let reversed = generate_building_design(request(value), None).unwrap();
    assert_eq!(generated.expected, reversed.expected);
}

#[test]
fn conflicts_include_the_item_and_coordinate_without_returning_a_candidate() {
    let mut value = design();
    value["parts"]
        .as_array_mut()
        .unwrap()
        .push(json!({"name":"wrong","shapes":[{
        "kind":"blocks","material":"glass","positions":[{"x":0,"y":0,"z":0}]}]}));
    let error = generate_building_design(request(value), None).unwrap_err();
    assert_eq!(error.code, "material_conflict");
    assert_eq!(error.item.as_deref(), Some("wrong"));
    assert_eq!(error.position, Some(Pos::default()));
    assert!(error.detail.contains("shell"));
    let mut value = design();
    value["spaces"][0]["region"]["min"] = json!({"x":0,"y":1,"z":1});
    let error = generate_building_design(request(value), None).unwrap_err();
    assert_eq!(error.code, "occupied_space");
    assert_eq!(error.item.as_deref(), Some("room"));
}

#[test]
fn invalid_bounds_names_shapes_and_resource_budgets_are_actionable() {
    let mut cases = Vec::new();
    let mut value = design();
    value["known_region"]["max"]["x"] = json!(i32::MAX);
    cases.push((value, "known_cell_budget"));
    let mut value = design();
    value["known_region"]["max"]["y"] = json!(-2);
    cases.push((value, "invalid_region"));
    let mut value = design();
    value["parts"][0]["name"] = json!("building");
    cases.push((value, "invalid_name"));
    let mut value = design();
    value["parts"][1]["name"] = json!("shell");
    cases.push((value, "duplicate_name"));
    let mut value = design();
    value["parts"][1]["shapes"][0]["positions"][0]["x"] = json!(5);
    cases.push((value, "missing_air_guard"));
    let mut value = design();
    value["parts"][0]["cutouts"] = json!([{"min":{"x":0,"y":0,"z":0},"max":{"x":4,"y":3,"z":4}}]);
    cases.push((value, "empty_part"));
    let mut value = design();
    value["parts"][0]["shapes"] = json!(vec![value["parts"][0]["shapes"][0].clone(); 200]);
    cases.push((value, "shape_work_budget"));
    cases.push((
        json!({"namespace":"test.large","name":"257 cells",
        "known_region":{"min":{"x":-1,"y":-1,"z":-1},"max":{"x":258,"y":1,"z":1}},
        "parts":[{"name":"line","shapes":[{"kind":"fill","material":"stone",
            "region":{"min":{"x":0,"y":0,"z":0},"max":{"x":256,"y":0,"z":0}}}]}]}),
        "block_budget",
    ));
    for (value, code) in cases {
        assert_eq!(
            generate_building_design(request(value), None)
                .unwrap_err()
                .code,
            code
        );
    }
    for field in ["command", "width", "allow_unknown"] {
        let mut value = design();
        value[field] = json!(true);
        assert!(serde_json::from_value::<BuildingDesignRequest>(value).is_err());
    }
    let mut value = design();
    value["parts"][0]["shapes"][0]["material"] = json!("oak_planks");
    assert!(serde_json::from_value::<BuildingDesignRequest>(value).is_err());
}

fn door_design() -> Value {
    json!({"namespace":"test.free-door","name":"Explicit equipment mount",
        "known_region":{"min":{"x":-1,"y":-5,"z":-2},"max":{"x":9,"y":8,"z":3}},
        "parts":[{"name":"floor","shapes":[{"kind":"fill","material":"stone",
            "region":{"min":{"x":0,"y":0,"z":1},"max":{"x":8,"y":0,"z":2}}}]}],
        "component":{"name":"door","assembly_revision_id":"reference-door.state.v3",
            "source_anchor":{"x":0,"y":5,"z":0},"target_anchor":{"x":4,"y":0,"z":0},"rotation":"r270",
            "reserved_space":{"min":{"x":0,"y":2,"z":-3},"max":{"x":0,"y":11,"z":3}},
            "exports":[{"name":"control","port":{"instance":["root","mechanism"],"port":"control"}}]}})
}

#[test]
fn pinned_door_composition_rechecks_the_whole_world_and_handles_nonzero_anchors() {
    let mut f = fixture::ordinary_fixture();
    f.catalog.insert_revisions(f.request.revisions).unwrap();
    f.catalog
        .insert_assembly(f.request.candidate_state)
        .unwrap();
    let original = f.catalog.clone();
    let generated =
        generate_building_design(request(door_design()), Some((&f.catalog, &f.context))).unwrap();
    assert_eq!(f.catalog, original);
    assert_eq!(generated.unique_blocks, 61);
    let attached = generated.component.as_ref().unwrap();
    assert_eq!(attached.origin, Pos::new(4, -5, 0));
    assert_eq!(attached.terminals["door.control"], Pos::new(4, 6, 0));
    let mut updates = BlueprintUpdates::new(generated.records.catalog().unwrap());
    updates.create(generated.request.clone()).unwrap();
    let mut updates = archive_fixture::updates(&updates.fixture_json().unwrap()).unwrap();
    updates.adopt(&generated.request.id).unwrap();
    assert_eq!(
        updates
            .catalog()
            .assembly(&attached.source_assembly_revision),
        original.assembly(&attached.source_assembly_revision)
    );
    let assembly = &updates
        .catalog()
        .assembly(&generated.request.candidate_state.id)
        .unwrap()
        .assembly;
    let view = assembly.inspect(updates.catalog()).unwrap();
    assert_eq!(
        view.resolved_port(&assembly.boundaries[0].port)
            .unwrap()
            .0
            .position,
        Pos::new(4, 6, 0)
    );
    // A passing source cannot hide a failed retained child in the combined world.
    let mut bad = f.catalog.clone();
    let revision = bad
        .revision(&BlueprintRevisionId::new("reference-door.mechanism.v3").unwrap())
        .unwrap();
    let mut impossible = revision.clone();
    impossible.static_type_bindings.push(StaticTypeBinding {
        type_revision: TypeRevisionId::new("test.impossible.v1").unwrap(),
        port: "control".into(),
    });
    let mut records = BlueprintRecords {
        types: bad.type_revisions().cloned().collect(),
        revisions: bad
            .revisions()
            .filter(|r| r.id != impossible.id)
            .cloned()
            .collect(),
        assemblies: bad.assemblies().cloned().collect(),
        ..Default::default()
    };
    records.types.push(TypeRevision {
        id: TypeRevisionId::new("test.impossible.v1").unwrap(),
        name: "Must be air at lever".into(),
        contract: TypeContract::BlockPattern {
            blocks: vec![PositionedBlock {
                position: Pos::default(),
                block: Block::new(BlockKind::Air),
            }],
        },
    });
    records.revisions.push(impossible);
    bad = records.catalog().unwrap();
    let error =
        generate_building_design(request(door_design()), Some((&bad, &f.context))).unwrap_err();
    assert_eq!(error.code, "verification_not_established");
}

#[test]
fn equipment_never_silently_carves_parts_or_weakens_permanent_air() {
    let mut f = fixture::ordinary_fixture();
    f.catalog.insert_revisions(f.request.revisions).unwrap();
    f.catalog
        .insert_assembly(f.request.candidate_state)
        .unwrap();
    let mut value = door_design();
    value["parts"][0]["shapes"][0]["region"]["min"]["z"] = json!(0);
    assert_eq!(
        generate_building_design(request(value), Some((&f.catalog, &f.context)))
            .unwrap_err()
            .code,
        "component_collision"
    );
    let mut value = door_design();
    value["spaces"] =
        json!([{"name":"aperture","region":{"min":{"x":3,"y":1,"z":0},"max":{"x":5,"y":3,"z":0}}}]);
    let error =
        generate_building_design(request(value), Some((&f.catalog, &f.context))).unwrap_err();
    assert_eq!(error.code, "invalid_component");
    assert!(error.detail.contains("permanent air"));
    assert_eq!(
        generate_building_design(request(door_design()), None)
            .unwrap_err()
            .code,
        "invalid_component"
    );
}
