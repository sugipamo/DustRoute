use dustroute_library::blueprint::BlueprintRevisionId;
use dustroute_library::builtin_blueprints::*;
use dustroute_library::{ComponentId, ComponentQuery, REDSTONE_COMPILER_XOR_ID, builtin_catalog};
use dustroute_minecraft::BlockKind;

#[test]
fn typed_definitions_preserve_every_pinned_record_from_independent_fixtures() {
    for (fixture, actual) in [
        (
            include_str!("fixtures/builtin-blueprints/builtin-v1.json"),
            builtin_blueprints(),
        ),
        (
            include_str!("fixtures/builtin-blueprints/primitives-v2.json"),
            dustroute_library::builtin_primitives::builtin_primitives(),
        ),
    ] {
        let frozen: dustroute_library::blueprint::BlueprintCatalog =
            serde_json::from_str(fixture).unwrap();
        assert_eq!(&frozen, actual);
        for record in frozen.revisions() {
            assert_eq!(
                frozen.expand(&record.id).unwrap(),
                actual.expand(&record.id).unwrap()
            );
        }
    }
}

#[test]
fn builtins_load_without_a_translation_or_compiler_dependency() {
    let catalog = builtin_blueprints();
    for record in catalog.revisions() {
        assert!(!catalog.expand(&record.id).unwrap().blocks.is_empty());
    }
    assert_eq!(catalog.revisions().count(), 10);
    assert_eq!(catalog.classifications().count(), 6);
    assert_eq!(catalog.type_revisions().count(), 2);
}

#[test]
fn frozen_geometry_agrees_with_the_existing_catalogs_metrics_and_behavior_claims() {
    let blueprints = builtin_blueprints();
    for component in builtin_catalog().search(&ComponentQuery {
        require_physical: true,
        require_automatic_replacement: true,
        ..Default::default()
    }) {
        let reference = component.layout_reference.as_ref().unwrap();
        let revision =
            BlueprintRevisionId::new(reference.strip_prefix("blueprint:").unwrap()).unwrap();
        let record = blueprints.revision(&revision).unwrap();
        let classification = blueprints
            .classification(&record.classifications[0])
            .unwrap();
        assert_eq!(
            classification.logical_claim.as_ref(),
            Some(&component.logical)
        );
        let world = blueprints.expand(&revision).unwrap().proposed_world();
        let (min, max) = world.bounds().unwrap();
        let metrics = component.physical.as_ref().unwrap();
        assert_eq!(
            metrics.bounding_size,
            [
                (max.x - min.x + 1) as usize,
                (max.y - min.y + 1) as usize,
                (max.z - min.z + 1) as usize
            ]
        );
        assert_eq!(metrics.occupied_blocks, world.iter().count());
        assert_eq!(
            metrics.dust_blocks,
            world
                .iter()
                .filter(|(_, block)| block.kind == BlockKind::RedstoneWire)
                .count()
        );
        assert_eq!(
            metrics.repeater_count,
            world
                .iter()
                .filter(|(_, block)| block.kind == BlockKind::Repeater)
                .count()
        );
    }
    let metadata = builtin_catalog();
    let rejected = metadata
        .get(&ComponentId::new(REDSTONE_COMPILER_XOR_ID).unwrap())
        .unwrap();
    assert!(!rejected.may_automatically_replace_physical_circuit());
    assert_eq!(
        blueprints
            .revision(&BlueprintRevisionId::new(EXTERNAL_XOR_REVISION).unwrap())
            .unwrap()
            .provenance,
        rejected.provenance
    );
}
