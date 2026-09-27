use dustroute_library::assembly::*;
use dustroute_library::blueprint::*;
use dustroute_library::builtin_primitives::*;
use dustroute_translate::assembly::validate_assembly;
use dustroute_translate::promotion::*;
use dustroute_translate::{Block, BlockKind, Pos, Region, RotationY};

fn id(s: &str) -> BlueprintRevisionId {
    BlueprintRevisionId::new(s).unwrap()
}
fn path(s: &str) -> InstancePath {
    vec![InstanceId::new(s).unwrap()]
}
fn fixture(powered: bool, rotation: RotationY) -> (BlueprintCatalog, Assembly) {
    let catalog = builtin_primitives().clone();
    let revision = catalog.revision(&id(LEVER_REVISION)).unwrap();
    let mut lever = revision.blocks[0].block.clone();
    lever.powered = Some(powered);
    let origin = Pos::new(7, 2, 3);
    let offset = rotation.pos(Pos::new(1, 0, 0));
    let assembly = Assembly {
        name: "Independent rotated lever".into(),
        instances: vec![BlueprintInclusion {
            instance: path("lever")[0].clone(),
            revision: revision.id.clone(),
            origin,
            rotation,
        }],
        blocks: vec![
            PositionedBlock {
                position: origin,
                block: rotation.block(&lever),
            },
            PositionedBlock {
                position: origin.offset(offset.x, offset.y, offset.z),
                block: Block::new(BlockKind::Solid),
            },
        ],
        known_regions: vec![Region::new(Pos::new(0, 0, 0), Pos::new(10, 5, 5))],
        connections: vec![],
        boundaries: vec![],
    };
    (catalog, assembly)
}
fn grouping(catalog: &BlueprintCatalog) -> BlueprintGrouping {
    BlueprintGrouping {
        id: id("parent.v1"),
        name: "Lever parent".into(),
        classifications: vec![],
        provenance: catalog
            .revision(&id(LEVER_REVISION))
            .unwrap()
            .provenance
            .clone(),
    }
}

#[test]
fn own_lever_identity_is_checked_without_consumers_after_reload_and_rotation() {
    for rotation in [
        RotationY::R0,
        RotationY::R90,
        RotationY::R180,
        RotationY::R270,
    ] {
        for powered in [false, true] {
            let (catalog, assembly) = fixture(powered, rotation);
            let catalog = BlueprintCatalog::from_json(&catalog.to_json().unwrap()).unwrap();
            let review = review_assembly(&catalog, &assembly).unwrap();
            assert_eq!(review.status(), CheckStatus::Passed);
            assert!(
                review.occurrences[&path("lever")]
                    .checks
                    .iter()
                    .any(|c| c.kind == CheckKind::StaticType && c.status == CheckStatus::Passed)
            );
            validate_assembly(&catalog, &assembly).unwrap();
            let mut changed = assembly.clone();
            changed.blocks[0].block = Block::new(BlockKind::RedstoneBlock);
            let review = review_assembly(&catalog, &changed).unwrap();
            assert_eq!(review.status(), CheckStatus::Failed);
            assert!(
                review.occurrences[&path("lever")]
                    .checks
                    .iter()
                    .any(|c| c.kind == CheckKind::StaticType && c.status == CheckStatus::Failed)
            );
            assert!(validate_assembly(&catalog, &changed).is_err());
            // The old revision did not assert lever identity: preserve its meaning.
            changed.instances[0].revision = id(LEGACY_LEVER_REVISION);
            assert_eq!(
                review_assembly(&catalog, &changed).unwrap().status(),
                CheckStatus::Passed
            );
        }
    }
}

#[test]
fn parent_pass_cannot_hide_failed_or_unknown_child_and_adoption_pins_static_types() {
    let (mut catalog, assembly) = fixture(true, RotationY::R90);
    let before = catalog.to_json().unwrap();
    for unknown in [false, true] {
        let mut bad = assembly.clone();
        if unknown {
            bad.blocks.remove(0);
            bad.known_regions.clear();
        } else {
            bad.blocks[0].block = Block::new(BlockKind::RedstoneBlock);
        }
        let candidate = PromotionCandidate::prepare(&catalog, &bad, grouping(&catalog)).unwrap();
        let review = candidate.validate(&catalog).unwrap();
        let status = if unknown {
            CheckStatus::Undetermined
        } else {
            CheckStatus::Failed
        };
        assert_eq!(review.report().status(), status);
        assert_eq!(
            review.report().occurrences[&path("root")].status(),
            CheckStatus::Passed
        );
        assert!(review.adopt(&mut catalog).is_err());
        assert_eq!(catalog.to_json().unwrap(), before);
    }
    let candidate = PromotionCandidate::prepare(&catalog, &assembly, grouping(&catalog)).unwrap();
    let review = candidate.validate(&catalog).unwrap();
    // A different archive may reuse the ID with a weaker contract. Even though
    // the current physical state would pass it, a previous review cannot adopt it.
    let mut changed: serde_json::Value = serde_json::from_str(&before).unwrap();
    for definition in changed["types"].as_array_mut().unwrap() {
        if definition["id"] == LEVER_TYPE_REVISION {
            definition["contract"] =
                serde_json::json!({"kind":"signal", "port_kind":"device_output"});
        }
    }
    let mut changed = BlueprintCatalog::from_json(&changed.to_string()).unwrap();
    assert!(matches!(
        review.clone().adopt(&mut changed),
        Err(PromotionError::ChangedDependency(_))
    ));
    let grouped = review.adopt(&mut catalog).unwrap();
    let restored = BlueprintCatalog::from_json(&catalog.to_json().unwrap()).unwrap();
    assert_eq!(
        review_assembly(&restored, &grouped).unwrap().status(),
        CheckStatus::Passed
    );
    assert_eq!(
        catalog.revision(&id(LEVER_REVISION)),
        builtin_primitives().revision(&id(LEVER_REVISION))
    );
}

#[test]
fn self_pattern_uses_rotated_terminal_frame_and_known_contradiction_wins_over_unknown() {
    let (mut catalog, mut assembly) = fixture(false, RotationY::R90);
    let pattern = TypeRevisionId::new("test.support-pattern.v1").unwrap();
    catalog
        .insert_type(TypeRevision {
            id: pattern.clone(),
            name: "Support and clearance".into(),
            contract: TypeContract::BlockPattern {
                blocks: vec![
                    PositionedBlock {
                        position: Pos::new(1, 0, 0),
                        block: Block::new(BlockKind::Solid),
                    },
                    PositionedBlock {
                        position: Pos::new(0, 0, 1),
                        block: Block::new(BlockKind::Air),
                    },
                ],
            },
        })
        .unwrap();
    let mut source = catalog.revision(&id(LEVER_REVISION)).unwrap().clone();
    source.parents = vec![source.id.clone()];
    source.id = id("lever-with-clearance.v1");
    source.static_type_bindings.push(StaticTypeBinding {
        type_revision: pattern,
        port: "out".into(),
    });
    catalog.insert_revision(source.clone()).unwrap();
    assembly.instances[0].revision = source.id;
    assert_eq!(
        review_assembly(&catalog, &assembly).unwrap().status(),
        CheckStatus::Passed
    );
    assembly.known_regions.clear();
    assert_eq!(
        review_assembly(&catalog, &assembly).unwrap().status(),
        CheckStatus::Undetermined
    );
    assembly.blocks[1].block = Block::new(BlockKind::RedstoneBlock);
    let review = review_assembly(&catalog, &assembly).unwrap();
    assert!(
        review.occurrences[&path("lever")]
            .checks
            .iter()
            .any(|c| c.kind == CheckKind::StaticType && c.status == CheckStatus::Failed)
    );
}
