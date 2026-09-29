use dustroute_library::behavior_type::PistonDoor;
use dustroute_library::blueprint::{BlueprintCatalog, TypeContract, TypeRevision, TypeRevisionId};

fn definition(requirement: PistonDoor) -> TypeRevision {
    TypeRevision {
        id: TypeRevisionId::new("dustroute.type.piston-door-3x3.v1").unwrap(),
        name: "Ordinary 3x3 piston door".into(),
        contract: TypeContract::PistonDoor {
            requirement: Box::new(requirement),
        },
    }
}

#[test]
fn registered_door_alone_requires_new_schema_and_cannot_be_downgraded() {
    let mut catalog = BlueprintCatalog::default();
    catalog
        .insert_type(definition(PistonDoor::three_by_three()))
        .unwrap();
    let saved = catalog.to_json().unwrap();
    assert!(saved.contains("dustroute.blueprint-catalog.v13"));
    assert_eq!(BlueprintCatalog::from_json(&saved).unwrap(), catalog);
    for version in 1..=10 {
        assert!(
            BlueprintCatalog::from_json(
                &saved.replace("catalog.v13", &format!("catalog.v{version}"))
            )
            .is_err()
        );
    }
}

#[test]
fn ambiguous_or_empty_observation_names_are_rejected_at_registration() {
    for duplicate_input in [false, true] {
        let mut requirement = PistonDoor::three_by_three();
        requirement.aperture[0][0].air = if duplicate_input {
            requirement.closed_input.clone()
        } else {
            String::new()
        };
        assert!(
            BlueprintCatalog::default()
                .insert_type(definition(requirement))
                .is_err()
        );
    }
    let mut requirement = PistonDoor::three_by_three();
    requirement.aperture[0][0] = requirement.aperture[1][0].clone();
    assert!(
        BlueprintCatalog::default()
            .insert_type(definition(requirement))
            .is_err()
    );
}

#[test]
fn an_initial_snapshot_cannot_certify_the_ordinary_door_contract() {
    use dustroute_library::PortDirection;
    use dustroute_library::blueprint::{BlueprintError, BlueprintPort, BlueprintPortKind};
    use dustroute_minecraft::{Block, BlockKind, Pos};
    let mut catalog = BlueprintCatalog::default();
    let definition = definition(PistonDoor::three_by_three());
    let id = definition.id.clone();
    catalog.insert_type(definition).unwrap();
    let port = BlueprintPort {
        name: "aperture".into(),
        position: Pos::default(),
        direction: PortDirection::Output,
        kind: BlueprintPortKind::BlockState,
        facing: None,
        required_source_types: vec![],
    };
    assert!(matches!(
        catalog.check_port_types(&port, &[id], |_| Some(Block::new(BlockKind::Air))),
        Err(BlueprintError::BehavioralEvidenceRequired(_))
    ));
}
