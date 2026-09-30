#[allow(dead_code)]
#[path = "support/reference_door_blueprint.rs"]
mod fixture;

use dustroute_library::blueprint::*;
use dustroute_library::building::BuildingWithDoorRequest;
use dustroute_library::runtime_behavior::RuntimeBehaviorContext;
use dustroute_minecraft::{Pos, Region, RotationY};
use dustroute_translate::assembly_transform::AssemblyTransform;
use dustroute_translate::blueprint_update::BlueprintUpdates;
use dustroute_translate::building::generate_building_with_door;
use dustroute_translate::promotion::{CheckStatus, review_assembly_with_context};
use serde_json::json;

fn request() -> BuildingWithDoorRequest {
    serde_json::from_value(json!({
        "building":{"namespace":"test.door-house","width":9,"depth":3,"height":8,
            "entrance":{"offset":3,"width":3,"height":3}},
        "door":{"assembly_revision_id":"reference-door.state.v3","instance":["root","mechanism"],
            "behavior_type":"dustroute.type.piston-door-3x3.v1","rotation":"r270",
            "reserved_space":{"min":{"x":0,"y":2,"z":-3},"max":{"x":0,"y":11,"z":3}}}
    }))
    .unwrap()
}

fn source() -> (BlueprintCatalog, RuntimeBehaviorContext) {
    let mut f = fixture::ordinary_fixture();
    f.catalog.insert_revisions(f.request.revisions).unwrap();
    f.catalog
        .insert_assembly(f.request.candidate_state)
        .unwrap();
    (f.catalog, f.context)
}

#[test]
fn reference_door_and_enclosure_share_physics_construction_and_restart_adoption() {
    let (catalog, context) = source();
    let original = catalog.clone();
    let generated = generate_building_with_door(request(), &catalog, &context).unwrap();
    assert_eq!(catalog, original, "source records must remain immutable");
    assert_eq!(generated.verification.status, CheckStatus::Passed);
    assert!(!generated.verification.live_world_verified);
    assert_eq!(generated.expected.blocks.len(), 168);
    assert_eq!(generated.parts.values().sum::<usize>(), 168);
    assert_eq!(generated.parts["door"], 43);
    assert_eq!(generated.verification.construction_steps, 168);
    assert_eq!(generated.verification.removal_steps, 168);
    let door = generated.door.as_ref().unwrap();
    assert_eq!(door.control, Pos::new(4, 6, 0));
    assert_eq!(door.origin, Pos::new(4, -5, 0));
    assert_eq!(
        door.reserved_space,
        Region::new(Pos::new(1, -3, 0), Pos::new(7, 6, 0))
    );
    assert!(door.closed_when_powered);
    for y in 1..=3 {
        for x in 3..=5 {
            assert!(door.aperture.contains(&Pos::new(x, y, 0)));
        }
    }
    let pattern = generated
        .records
        .types
        .iter()
        .find(|t| t.id.as_str().ends_with("clearance.pattern.v1"))
        .unwrap();
    let TypeContract::BlockPattern { blocks } = &pattern.contract else {
        panic!("expected structure")
    };
    assert_eq!(blocks.len(), 924 - 70);
    assert!(
        blocks
            .iter()
            .all(|b| !door.reserved_space.contains(b.position))
    );
    assert!(blocks.iter().any(|b| b.position == Pos::new(4, 7, 0)));
    let mut updates = BlueprintUpdates::new(generated.records.catalog().unwrap());
    updates.create(generated.request.clone()).unwrap();
    let mut updates = BlueprintUpdates::from_json(&updates.to_json().unwrap()).unwrap();
    updates.adopt(&generated.request.id).unwrap();
    let updates = BlueprintUpdates::from_json(&updates.to_json().unwrap()).unwrap();
    assert_eq!(
        updates.catalog().assembly(&door.source_assembly_revision),
        catalog.assembly(&door.source_assembly_revision)
    );
    let assembly = &updates
        .catalog()
        .assembly(&generated.request.candidate_state.id)
        .unwrap()
        .assembly;
    assert_eq!(assembly.boundaries.len(), 10);
    let view = assembly.inspect(updates.catalog()).unwrap();
    for boundary in &assembly.boundaries {
        let (port, _) = view.resolved_port(&boundary.port).unwrap();
        if boundary.name == "door_control" {
            assert_eq!(port.position, door.control);
        } else {
            assert!(door.aperture.contains(&port.position));
        }
    }
    let (moved, context) = AssemblyTransform {
        source_anchor: Pos::default(),
        target_anchor: Pos::new(-96, 180, 1000),
        rotation: RotationY::R90,
    }
    .apply(assembly, &generated.context)
    .unwrap();
    assert_eq!(
        review_assembly_with_context(
            updates.catalog(),
            &moved,
            Some(&context.into()),
            Default::default()
        )
        .unwrap()
        .status(),
        CheckStatus::Passed
    );
}

#[test]
fn malformed_attachments_and_oversized_compositions_are_rejected() {
    let (catalog, context) = source();
    for invalid in 0..9 {
        let mut r = request();
        match invalid {
            0 => r.building.entrance.as_mut().unwrap().width = 1,
            1 => r.door.rotation = RotationY::R0,
            2 => r.door.reserved_space.max.y = 10,
            3 => r.door.reserved_space.max.y = 13,
            4 => r.door.instance = vec![InstanceId::new("absent").unwrap()],
            5 => {
                r.door.behavior_type =
                    TypeRevisionId::new("reference-door.aperture-repeated-settling.v1").unwrap()
            }
            6 => r.building.depth = 8,
            7 => r.door.reserved_space.min.x = 1,
            _ => r.door.reserved_space.max.y = i32::MAX,
        }
        assert!(
            generate_building_with_door(r, &catalog, &context).is_err(),
            "invalid request {invalid}"
        );
    }
    let mut excessive_context = context.clone();
    excessive_context.known_region.max.y = 1000;
    let mut extended_catalog = catalog.clone();
    let mut expanded = catalog
        .assembly(&request().door.assembly_revision_id)
        .unwrap()
        .clone();
    expanded.id = AssemblyRevisionId::new("test.expanded-door").unwrap();
    expanded.assembly.known_regions = vec![excessive_context.known_region];
    extended_catalog.insert_assembly(expanded.clone()).unwrap();
    let mut r = request();
    r.door.assembly_revision_id = expanded.id;
    assert!(
        generate_building_with_door(r, &extended_catalog, &excessive_context)
            .unwrap_err()
            .detail
            .contains("8192")
    );
}

#[test]
fn retained_door_requirements_are_not_waived_by_reservation() {
    let mut f = fixture::ordinary_fixture();
    let invariant = TypeRevision {
        id: TypeRevisionId::new("test.always-open").unwrap(),
        name: "Conflicting child invariant".into(),
        contract: TypeContract::BlockKind {
            block_kind: dustroute_minecraft::BlockKind::Air,
        },
    };
    f.request
        .revisions
        .iter_mut()
        .find(|r| r.id == f.request.next_child)
        .unwrap()
        .static_type_bindings
        .push(StaticTypeBinding {
            type_revision: invariant.id.clone(),
            port: "aperture_0_0".into(),
        });
    f.catalog.insert_type(invariant).unwrap();
    f.catalog.insert_revisions(f.request.revisions).unwrap();
    f.catalog
        .insert_assembly(f.request.candidate_state)
        .unwrap();
    let error = generate_building_with_door(request(), &f.catalog, &f.context).unwrap_err();
    assert_eq!(error.code, "verification_not_established");
    assert_eq!(error.diagnostics.unwrap().status, CheckStatus::Failed);
}
