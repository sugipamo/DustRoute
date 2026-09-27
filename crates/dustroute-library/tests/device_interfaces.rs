use dustroute_library::PortDirection;
use dustroute_library::blueprint::*;
use dustroute_library::builtin_primitives::*;
use dustroute_minecraft::{Block, BlockKind};

#[test]
fn independent_lever_type_checks_identity_without_fixing_powered_state() {
    let catalog = builtin_primitives();
    let revision = catalog
        .revision(&BlueprintRevisionId::new(LEVER_REVISION).unwrap())
        .unwrap();
    assert_eq!(revision.blocks.len(), 1);
    let source = &revision.ports[0];
    let mut sink = source.clone();
    sink.direction = PortDirection::Input;
    sink.kind = BlueprintPortKind::BlockPower;
    sink.required_source_types = vec![TypeRevisionId::new(LEVER_TYPE_REVISION).unwrap()];
    for powered in [false, true] {
        let mut block = revision.blocks[0].block.clone();
        block.powered = Some(powered);
        catalog
            .check_source_requirements(source, &sink, |_| Some(block.clone()))
            .unwrap();
    }
    assert!(
        catalog
            .check_source_requirements(source, &sink, |_| Some(Block::new(
                BlockKind::RedstoneTorch
            )))
            .is_err()
    );
    assert!(
        catalog
            .check_source_requirements(source, &sink, |_| None)
            .is_err()
    );
    // NOT can accept the signal interface without requiring lever identity.
    sink.required_source_types.clear();
    catalog
        .check_source_requirements(source, &sink, |_| {
            Some(Block::new(BlockKind::RedstoneTorch))
        })
        .unwrap();
    let mut wire = source.clone();
    wire.kind = BlueprintPortKind::Wire;
    assert!(
        catalog
            .check_source_requirements(&wire, &sink, |_| Some(Block::new(BlockKind::RedstoneTorch)))
            .is_err()
    );
}

#[test]
fn new_interfaces_are_output_only_and_archives_cannot_hide_their_version() {
    let original = builtin_primitives().to_json().unwrap();
    assert!(original.contains("dustroute.blueprint-catalog.v8"));
    assert_eq!(
        BlueprintCatalog::from_json(&original)
            .unwrap()
            .to_json()
            .unwrap(),
        original
    );
    for version in 1..=7 {
        assert!(
            BlueprintCatalog::from_json(&original.replace(
                "blueprint-catalog.v8",
                &format!("blueprint-catalog.v{version}")
            ))
            .is_err()
        );
    }
    let mut catalog = builtin_primitives().clone();
    let mut lever = catalog
        .revision(&BlueprintRevisionId::new(LEVER_REVISION).unwrap())
        .unwrap()
        .clone();
    lever.id = BlueprintRevisionId::new("bad.input-device.v1").unwrap();
    lever.ports[0].direction = PortDirection::Input;
    assert!(catalog.insert_revision(lever).is_err());
    assert!(!BlueprintPortKind::DeviceOutput.matches_signal_block(&Block::new(BlockKind::Solid)));
    assert!(
        !BlueprintPortKind::DeviceOutput.matches_signal_block(&Block::new(BlockKind::RedstoneWire))
    );
}

#[test]
fn static_bindings_are_explicit_versioned_and_reject_incomplete_or_behavioral_declarations() {
    let legacy =
        BlueprintCatalog::from_json(include_str!("../blueprints/primitives-v1.json")).unwrap();
    assert!(legacy.to_json().unwrap().contains("blueprint-catalog.v7"));
    assert_eq!(
        legacy.revision(&BlueprintRevisionId::new(LEGACY_LEVER_REVISION).unwrap()),
        builtin_primitives().revision(&BlueprintRevisionId::new(LEGACY_LEVER_REVISION).unwrap())
    );
    let mut catalog = builtin_primitives().clone();
    let original = catalog
        .revision(&BlueprintRevisionId::new(LEVER_REVISION).unwrap())
        .unwrap()
        .clone();
    assert_eq!(
        original.static_type_bindings,
        vec![StaticTypeBinding {
            type_revision: TypeRevisionId::new(LEVER_TYPE_REVISION).unwrap(),
            port: "out".into()
        }]
    );
    for mutation in 0..3 {
        let mut bad = original.clone();
        bad.id = BlueprintRevisionId::new(format!("invalid-binding.{mutation}")).unwrap();
        match mutation {
            0 => bad.static_type_bindings[0].port = "absent".into(),
            1 => {
                bad.static_type_bindings[0].type_revision =
                    TypeRevisionId::new("absent.type").unwrap()
            }
            _ => bad
                .static_type_bindings
                .push(bad.static_type_bindings[0].clone()),
        }
        assert!(catalog.insert_revision(bad).is_err());
    }
    let behavioral = TypeRevisionId::new("test.behavior.v1").unwrap();
    catalog
        .insert_type(TypeRevision {
            id: behavioral.clone(),
            name: "Behavior".into(),
            contract: TypeContract::RepeatedSettling {
                relation: dustroute_library::behavior_type::RepeatedSettling {
                    inputs: vec!["a".into()],
                    outputs: vec!["out".into()],
                    rows: [false, true]
                        .into_iter()
                        .map(|b| dustroute_library::behavior_type::BooleanRow {
                            inputs: vec![b],
                            outputs: vec![!b],
                        })
                        .collect(),
                },
            },
        })
        .unwrap();
    let mut bad = original;
    bad.id = BlueprintRevisionId::new("invalid-binding.behavior").unwrap();
    bad.static_type_bindings[0].type_revision = behavioral;
    assert!(catalog.insert_revision(bad).is_err());
}
