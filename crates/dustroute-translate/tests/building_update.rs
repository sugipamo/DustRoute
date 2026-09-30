#[allow(dead_code)]
#[path = "support/runtime_blueprint.rs"]
mod fixture;
use dustroute_library::blueprint::*;
use dustroute_library::building::{BuildingDesignRequest, BuildingDesignUpdateRequest};
use dustroute_translate::blueprint_update::BlueprintUpdates;
use dustroute_translate::building::{generate_building_design, generate_building_design_update};
use serde_json::{Value, json};

fn design(namespace: &str, glass: &str) -> BuildingDesignRequest {
    serde_json::from_value(json!({"namespace":namespace,"name":"Small design",
        "known_region":{"min":{"x":-1,"y":-1,"z":-1},"max":{"x":4,"y":3,"z":3}},
        "parts":[{"name":"floor","shapes":[{"kind":"fill","material":"stone",
            "region":{"min":{"x":0,"y":0,"z":0},"max":{"x":2,"y":0,"z":0}}}]},
            {"name":"window","shapes":[{"kind":"blocks","material":glass,"positions":[{"x":1,"y":1,"z":1}]}]}],
        "spaces":[{"name":"gap","region":{"min":{"x":2,"y":1,"z":1},"max":{"x":2,"y":1,"z":1}}}]})).unwrap()
}

#[test]
fn updates_descend_from_the_selected_base_retain_pins_and_survive_restart() {
    let previous = design("test.update.before", "glass");
    let original = generate_building_design(previous.clone(), None).unwrap();
    let mut updates = BlueprintUpdates::new(original.records.catalog().unwrap());
    updates.create(original.request.clone()).unwrap();
    updates.adopt(&original.request.id).unwrap();
    let base = updates
        .catalog()
        .assembly(&original.request.candidate_state.id)
        .unwrap()
        .clone();
    let after = design("test.update.after", "tinted_glass");
    let generated = generate_building_design_update(
        BuildingDesignUpdateRequest {
            base_assembly_revision_id: base.id.clone(),
            previous,
            design: after.clone(),
        },
        updates.catalog(),
        None,
        None,
    )
    .unwrap();
    assert_eq!(generated.design.request.base_state, base.id);
    assert_eq!(
        generated.design.request.previous_child,
        original.request.next_child
    );
    assert_eq!(generated.diff.blocks.len(), 1);
    assert_eq!(
        generated.retained_revisions["floor"].as_str(),
        "test.update.before.floor.v1"
    );
    assert_eq!(
        generated.retained_revisions["space.gap"].as_str(),
        "test.update.before.space.gap.v1"
    );
    assert!(!generated.placed_instances_modified);
    let changed = generated
        .design
        .request
        .revisions
        .iter()
        .find(|r| r.id.as_str() == "test.update.after.window.v1")
        .unwrap();
    assert_eq!(changed.parents[0].as_str(), "test.update.before.window.v1");
    for definition in generated.design.records.types {
        if updates.catalog().type_revision(&definition.id).is_none() {
            updates.append_type(definition).unwrap();
        }
    }
    updates.create(generated.design.request.clone()).unwrap();
    let mut updates = BlueprintUpdates::from_json(&updates.to_json().unwrap()).unwrap();
    updates.adopt(&generated.design.request.id).unwrap();
    assert_eq!(updates.catalog().assembly(&base.id), Some(&base));
    let final_design = design("test.update.final", "glass");
    let next = generate_building_design_update(
        BuildingDesignUpdateRequest {
            base_assembly_revision_id: generated.design.request.candidate_state.id,
            previous: after,
            design: final_design,
        },
        updates.catalog(),
        None,
        None,
    )
    .unwrap();
    assert_eq!(
        next.retained_revisions["floor"].as_str(),
        "test.update.before.floor.v1"
    );
}

#[test]
fn wrong_previous_input_cannot_silently_drop_air_or_custom_requirements() {
    let previous = design("test.update.mismatch", "glass");
    let original = generate_building_design(previous.clone(), None).unwrap();
    let mut catalog = original.records.catalog().unwrap();
    catalog
        .insert_revisions(original.request.revisions.clone())
        .unwrap();
    catalog
        .insert_assembly(original.request.candidate_state.clone())
        .unwrap();
    let mut wrong = previous.clone();
    wrong.spaces.clear();
    let error = generate_building_design_update(
        BuildingDesignUpdateRequest {
            base_assembly_revision_id: original.request.candidate_state.id.clone(),
            previous: wrong,
            design: design("test.update.candidate", "glass"),
        },
        &catalog,
        None,
        None,
    )
    .unwrap_err();
    assert_eq!(error.code, "base_design_mismatch");
    let mut body = catalog
        .revision(&original.request.next_child)
        .unwrap()
        .clone();
    body.id = BlueprintRevisionId::new("test.update.custom-body.v1").unwrap();
    body.parents = vec![original.request.next_child.clone()];
    let extra = TypeRevision {
        id: TypeRevisionId::new("test.update.extra-type.v1").unwrap(),
        name: "Retained extra obligation".into(),
        contract: TypeContract::BlockKind {
            block_kind: dustroute_minecraft::BlockKind::Solid,
        },
    };
    body.static_type_bindings.push(StaticTypeBinding {
        port: "layout".into(),
        type_revision: extra.id.clone(),
    });
    catalog.insert_type(extra).unwrap();
    let mut parent = catalog
        .revision(&original.request.candidate_parent)
        .unwrap()
        .clone();
    parent.parents = vec![parent.id.clone()];
    parent.id = BlueprintRevisionId::new("test.update.custom-parent.v1").unwrap();
    parent.inclusions[0].revision = body.id.clone();
    let mut base = original.request.candidate_state.clone();
    base.parents = vec![base.id.clone()];
    base.id = AssemblyRevisionId::new("test.update.custom-state.v1").unwrap();
    base.assembly.instances[0].revision = parent.id.clone();
    catalog.insert_revisions(vec![body, parent]).unwrap();
    catalog.insert_assembly(base.clone()).unwrap();
    let error = generate_building_design_update(
        BuildingDesignUpdateRequest {
            base_assembly_revision_id: base.id,
            previous: previous.clone(),
            design: design("test.update.no-silent-drop", "glass"),
        },
        &catalog,
        None,
        None,
    )
    .unwrap_err();
    assert_eq!(error.code, "base_design_mismatch");
    assert_eq!(error.item.as_deref(), Some("root/building"));
    let error = generate_building_design_update(
        BuildingDesignUpdateRequest {
            base_assembly_revision_id: AssemblyRevisionId::new("unknown.v1").unwrap(),
            previous: previous.clone(),
            design: previous,
        },
        &catalog,
        None,
        None,
    )
    .unwrap_err();
    assert_eq!(error.code, "immutable_namespace");
}

#[test]
fn unchanged_equipment_keeps_original_nested_pins_in_the_combined_candidate() {
    let f = fixture::fixture(false, false);
    let mut catalog = f.catalog;
    catalog.insert_revisions(f.request.revisions).unwrap();
    catalog
        .insert_assembly(f.request.candidate_state.clone())
        .unwrap();
    let previous:BuildingDesignRequest=serde_json::from_value(json!({"namespace":"test.equipment.before","name":"Marker and engine",
        "known_region":f.context.known_region,
        "parts":[{"name":"marker","shapes":[{"kind":"blocks","material":"glass","positions":[{"x":5,"y":1,"z":3}]}]}],
        "component":{"name":"engine","assembly_revision_id":f.request.candidate_state.id,
            "source_anchor":{"x":0,"y":0,"z":0},"target_anchor":{"x":0,"y":0,"z":0},"rotation":"r0",
            "reserved_space":{"min":{"x":-1,"y":0,"z":0},"max":{"x":3,"y":1,"z":0}}}})).unwrap();
    let original =
        generate_building_design(previous.clone(), Some((&catalog, &f.context))).unwrap();
    let mut updates = BlueprintUpdates::new(original.records.catalog().unwrap());
    updates.create(original.request.clone()).unwrap();
    updates.adopt(&original.request.id).unwrap();
    let mut value = serde_json::to_value(&previous).unwrap();
    value["namespace"] = json!("test.equipment.after");
    value["parts"][0]["shapes"][0]["material"] = json!("tinted_glass");
    let generated = generate_building_design_update(
        BuildingDesignUpdateRequest {
            base_assembly_revision_id: original.request.candidate_state.id,
            previous,
            design: serde_json::from_value::<BuildingDesignRequest>(value).unwrap(),
        },
        updates.catalog(),
        Some(&f.context),
        Some(&f.context),
    )
    .unwrap();
    assert_eq!(
        generated.retained_revisions["component.engine"].as_str(),
        "test.equipment.before.component.engine.v1"
    );
    let serialized: Value = serde_json::to_value(generated.design.request).unwrap();
    assert_eq!(serialized["base_state"], "test.equipment.before.state.v2");
}
