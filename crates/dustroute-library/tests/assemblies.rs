#[path = "support/catalog_fixture.rs"]
mod catalog_fixture;
use catalog_fixture::FixtureJson;
use std::collections::BTreeSet;

use dustroute_library::assembly::*;
use dustroute_library::blueprint::*;
use dustroute_library::builtin_blueprints::*;
use dustroute_minecraft::{Block, BlockKind, Pos, Region};

fn blueprint(value: &str) -> BlueprintRevisionId {
    BlueprintRevisionId::new(value).unwrap()
}
fn state(value: &str) -> AssemblyRevisionId {
    AssemblyRevisionId::new(value).unwrap()
}
fn instance(value: &str) -> InstanceId {
    InstanceId::new(value).unwrap()
}

fn parent(catalog: &mut BlueprintCatalog) -> BlueprintRevisionId {
    let mut parent = catalog
        .revision(&blueprint(NOT_TOP_REVISION))
        .unwrap()
        .clone();
    parent.id = blueprint("shared-parent.v1");
    parent.blocks.clear();
    parent.inclusions = ["b", "c"]
        .into_iter()
        .map(|name| BlueprintInclusion {
            rotation: Default::default(),
            instance: instance(name),
            revision: blueprint(NOT_TOP_REVISION),
            origin: Pos::default(),
        })
        .collect();
    let id = parent.id.clone();
    catalog.insert_revision(parent).unwrap();
    id
}

fn assembly(catalog: &BlueprintCatalog, source: &BlueprintRevisionId) -> Assembly {
    Assembly {
        name: "Shared physical state".into(),
        instances: vec![BlueprintInclusion {
            rotation: Default::default(),
            instance: instance("root"),
            revision: source.clone(),
            origin: Pos::default(),
        }],
        blocks: catalog
            .expand(source)
            .unwrap()
            .blocks
            .into_iter()
            .map(|(position, block)| PositionedBlock { position, block })
            .collect(),
        known_regions: vec![Region::new(Pos::new(-2, -2, -2), Pos::new(5, 5, 5))],
        connections: vec![],
        boundaries: vec![],
    }
}

#[test]
fn actual_state_is_stored_once_without_rebinding_shared_sources_or_old_states() {
    let mut catalog = builtin_blueprints().clone();
    let source = parent(&mut catalog);
    let original_source = catalog.revision(&source).unwrap().clone();
    let first = AssemblyRevision {
        id: state("state.v1"),
        parents: vec![],
        assembly: assembly(&catalog, &source),
    };
    catalog.insert_assembly(first.clone()).unwrap();
    let mut second = first.clone();
    second.id = state("state.v2");
    second.parents = vec![first.id.clone()];
    let output = Pos::new(1, 1, 0);
    second
        .assembly
        .blocks
        .iter_mut()
        .find(|record| record.position == output)
        .unwrap()
        .block
        .power_level = Some(7);
    // Raw surrounding geometry does not have to acquire a blueprint label.
    second.assembly.blocks.push(PositionedBlock {
        position: Pos::new(7, 0, 0),
        block: Block::new(BlockKind::Solid),
    });
    catalog.insert_assembly(second.clone()).unwrap();
    assert_eq!(catalog.revision(&source), Some(&original_source));
    assert_eq!(catalog.assembly(&first.id), Some(&first));
    let loaded = catalog_fixture::catalog(&catalog.fixture_json().unwrap()).unwrap();
    assert_eq!(loaded.assembly(&second.id), Some(&second));
    let view = second.assembly.inspect(&loaded).unwrap();
    assert_eq!(view.block_at(output).unwrap().power_level, Some(7));
    assert_eq!(view.source_differences().len(), 3);
    assert_eq!(
        view.affected_occurrences(&BTreeSet::from([output])),
        BTreeSet::from([
            vec![instance("root")],
            vec![instance("root"), instance("b")],
            vec![instance("root"), instance("c")],
        ])
    );
    assert_eq!(
        view.proposed_world().kind_at(Pos::new(7, 0, 0)),
        BlockKind::Solid
    );
    assert!(!view.membership.contains_key(&Pos::new(7, 0, 0)));
    assert_eq!(
        catalog.insert_assembly(second.clone()),
        Err(BlueprintError::DuplicateAssembly(second.id))
    );
}

#[test]
fn state_coverage_distinguishes_unknown_air_and_missing_source_blocks() {
    let catalog = builtin_blueprints();
    let source = blueprint(NOT_TOP_REVISION);
    let mut actual = assembly(catalog, &source);
    actual
        .blocks
        .retain(|record| record.position != Pos::default());
    let known = actual.inspect(catalog).unwrap();
    assert_eq!(
        known.block_at(Pos::default()),
        Some(Block::new(BlockKind::Air))
    );
    assert_eq!(known.block_at(Pos::new(99, 0, 0)), None);
    actual.known_regions.clear();
    assert_eq!(
        actual.inspect(catalog).unwrap().block_at(Pos::default()),
        None
    );
    actual.blocks.push(PositionedBlock {
        position: Pos::default(),
        block: Block::new(BlockKind::Air),
    });
    assert_eq!(
        actual.inspect(catalog).unwrap().block_at(Pos::default()),
        Some(Block::new(BlockKind::Air))
    );
}

#[test]
fn state_archives_reject_missing_sources_cycles_duplicate_states_and_overflow() {
    let mut catalog = builtin_blueprints().clone();
    let source = blueprint(NOT_TOP_REVISION);
    let record = AssemblyRevision {
        id: state("state.v1"),
        parents: vec![],
        assembly: assembly(&catalog, &source),
    };
    let mut invalid = record.clone();
    invalid.assembly.instances[0].revision = blueprint("absent.v1");
    assert!(matches!(
        catalog.insert_assembly(invalid),
        Err(BlueprintError::UnknownRevision(_))
    ));
    assert!(catalog.assembly(&record.id).is_none());
    let mut invalid = record.clone();
    invalid.parents = vec![invalid.id.clone()];
    assert!(matches!(
        catalog.insert_assembly(invalid),
        Err(BlueprintError::AssemblyAncestryCycle(_))
    ));
    let mut invalid = record.clone();
    invalid
        .assembly
        .blocks
        .push(invalid.assembly.blocks[0].clone());
    assert!(matches!(
        catalog.insert_assembly(invalid),
        Err(BlueprintError::Invalid(_))
    ));
    let mut invalid = record.clone();
    invalid.assembly.instances[0].origin = Pos::new(i32::MAX, 0, 0);
    assert_eq!(
        catalog.insert_assembly(invalid),
        Err(BlueprintError::CoordinateOverflow)
    );
    catalog.insert_assembly(record).unwrap();
    let mut data: serde_json::Value =
        serde_json::from_str(&catalog.fixture_json().unwrap()).unwrap();
    let duplicate = data["assemblies"][0].clone();
    data["assemblies"].as_array_mut().unwrap().push(duplicate);
    assert!(matches!(
        catalog_fixture::catalog(&data.to_string()),
        Err(BlueprintError::DuplicateAssembly(_))
    ));
}

#[test]
fn independent_source_claims_never_choose_the_actual_shared_state_by_order() {
    let mut catalog = builtin_blueprints().clone();
    let source = blueprint(NOT_TOP_REVISION);
    let mut variant = catalog.revision(&source).unwrap().clone();
    variant.id = blueprint("different-state.v1");
    variant.blocks[0].block.powered = Some(true);
    catalog.insert_revision(variant.clone()).unwrap();
    let mut actual = assembly(&catalog, &source);
    actual.instances.push(BlueprintInclusion {
        rotation: Default::default(),
        instance: instance("overlap"),
        revision: variant.id,
        origin: Pos::default(),
    });
    let before = actual.inspect(&catalog).unwrap();
    actual.instances.reverse();
    let after = actual.inspect(&catalog).unwrap();
    assert_eq!(before, after);
    assert!(!before.source_differences().is_empty());
}

#[test]
fn nested_rotations_compose_positions_port_facings_and_support_offsets() {
    use dustroute_minecraft::{Facing, RotationY};
    let mut catalog = builtin_blueprints().clone();
    let mut parent = catalog
        .revision(&blueprint(NOT_SIDE_REVISION))
        .unwrap()
        .clone();
    parent.id = blueprint("rotated-parent.v1");
    parent.blocks.clear();
    parent.ports.clear();
    parent.inclusions = vec![BlueprintInclusion {
        instance: instance("not"),
        revision: blueprint(NOT_SIDE_REVISION),
        origin: Pos::new(4, 0, 0),
        rotation: RotationY::R90,
    }];
    catalog.insert_revision(parent.clone()).unwrap();
    let mut placed = assembly(&catalog, &parent.id);
    placed.instances[0].origin = Pos::new(10, 5, 20);
    placed.instances[0].rotation = RotationY::R90;
    let view = placed.inspect(&catalog).unwrap();
    let child = vec![instance("root"), instance("not")];
    assert_eq!(view.occurrences[&child].rotation, RotationY::R180);
    let output = view
        .port(
            &catalog,
            &AssemblyPortRef {
                instance: child.clone(),
                port: "out".into(),
            },
        )
        .unwrap();
    assert_eq!(output.position, Pos::new(8, 5, 24));
    assert_eq!(output.facing, Some(Facing::West));
    let torch = &view.source_claims[&Pos::new(9, 5, 24)][&child];
    assert_eq!(torch.kind, BlockKind::RedstoneTorch);
    assert_eq!(torch.facing, Some(Facing::West));
    assert_eq!(torch.support_offset, Some(Pos::new(1, 0, 0)));
}
