use dustroute_library::blueprint::*;
use dustroute_translate::blueprint::*;
use dustroute_translate::{
    cell_library::default_cell_library, cell_library::verify_cell, cells::baseline_cell_for,
    cells::not_cell, cells::not_top_cell, ir::GateKind, world::Block, world::BlockKind, world::Pos,
};

fn id(name: &str) -> BlueprintRevisionId {
    BlueprintRevisionId::new(name).unwrap()
}

#[test]
fn frozen_archive_is_reproduced_by_independent_authoring_recipes() {
    let generated =
        dustroute_translate::blueprint_generation::generate_builtin_blueprints().unwrap();
    let loaded = builtin_blueprints();
    assert_eq!(generated.to_json().unwrap(), loaded.to_json().unwrap());
    for record in generated.revisions() {
        assert_eq!(
            blueprint_cell(&generated, &record.id).unwrap(),
            blueprint_cell(loaded, &record.id).unwrap()
        );
    }
}

#[test]
fn frozen_compilation_preserves_the_authoring_geometry_and_validation() {
    let compiler = dustroute_translate::compiler::BaselineCompiler::new(Default::default());
    let circuit = dustroute_translate::circuits::half_adder();
    let frozen = compiler.compile(&circuit).unwrap();
    let generated = compiler
        .compile_with_cell_source(
            &circuit,
            dustroute_translate::cell_generators::primitive_cell_for,
        )
        .unwrap();
    assert_eq!(frozen.world, generated.world);
    assert_eq!(frozen.input_positions, generated.input_positions);
    assert_eq!(frozen.output_positions, generated.output_positions);
    assert!(frozen.legality.valid());
    assert!(generated.legality.valid());
}

#[test]
fn compiler_and_replacement_candidates_resolve_the_same_persistable_not_realizations() {
    let catalog = builtin_not_blueprints();
    let saved = catalog.to_json().unwrap();
    let loaded = BlueprintCatalog::from_json(&saved).unwrap();
    let classification = ClassificationRevisionId::new(NOT_CLASSIFICATION_REVISION).unwrap();
    assert_eq!(loaded.candidates(&classification).len(), 2);
    let top = blueprint_cell(&loaded, &id(NOT_TOP_REVISION)).unwrap();
    let side = blueprint_cell(&loaded, &id(NOT_SIDE_REVISION)).unwrap();
    assert_eq!(top, not_top_cell());
    assert_eq!(side, not_cell());
    assert!(verify_cell(GateKind::Not, &top).valid);
    assert!(verify_cell(GateKind::Not, &side).valid);
    assert_eq!(baseline_cell_for(GateKind::Not), Some(top.clone()));
    let library = default_cell_library();
    assert_eq!(library.candidates_for(GateKind::Not), [top, side]);
}

#[test]
fn parent_can_keep_raw_blocks_and_shared_interpretations_without_duplicating_geometry() {
    let mut catalog = builtin_not_blueprints().clone();
    let mut parent = catalog.revision(&id(NOT_TOP_REVISION)).unwrap().clone();
    parent.id = id("parent.v1");
    parent.name = "A parent with two interpretations".into();
    parent.classifications.clear();
    parent.blocks = vec![PositionedBlock {
        position: Pos::new(9, 0, 0),
        block: Block::new(BlockKind::Solid),
    }];
    parent.inclusions = ["b", "c"]
        .into_iter()
        .map(|name| BlueprintInclusion {
            rotation: Default::default(),
            instance: InstanceId::new(name).unwrap(),
            revision: id(NOT_TOP_REVISION),
            origin: Pos::default(),
        })
        .collect();
    catalog.insert_revision(parent.clone()).unwrap();
    let expanded = catalog.expand(&parent.id).unwrap();
    assert_eq!(
        expanded.blocks.len(),
        not_top_cell().world.iter().count() + 1
    );
    assert_eq!(expanded.membership[&Pos::default()].len(), 3);
    let cell = blueprint_cell(&catalog, &parent.id).unwrap();
    assert!(verify_cell(GateKind::Not, &cell).valid);
    let old = catalog.to_json().unwrap();
    let mut replacement = parent.clone();
    replacement.id = id("parent.v2");
    replacement.parents = vec![parent.id.clone()];
    replacement.blocks = not_cell()
        .world
        .iter()
        .map(|(position, block)| PositionedBlock {
            position: *position,
            block: block.clone(),
        })
        .collect();
    replacement.ports = catalog
        .revision(&id(NOT_SIDE_REVISION))
        .unwrap()
        .ports
        .clone();
    replacement.inclusions.clear();
    catalog.insert_revision(replacement.clone()).unwrap();
    assert!(
        verify_cell(
            GateKind::Not,
            &blueprint_cell(&catalog, &replacement.id).unwrap()
        )
        .valid
    );
    assert_eq!(catalog.expand(&parent.id).unwrap(), expanded);
    assert!(
        BlueprintCatalog::from_json(&old)
            .unwrap()
            .revision(&replacement.id)
            .is_none()
    );
}

#[test]
fn legacy_adapter_never_drops_requirements_it_cannot_preserve() {
    let mut catalog = builtin_not_blueprints().clone();
    let mut constrained = catalog.revision(&id(NOT_TOP_REVISION)).unwrap().clone();
    constrained.id = id("constrained.v1");
    constrained.ports[0].required_source_types = vec![
        TypeRevisionId::new(dustroute_library::builtin_blueprints::WIRE_TYPE_REVISION).unwrap(),
    ];
    catalog.insert_revision(constrained.clone()).unwrap();
    assert!(blueprint_cell(&catalog, &constrained.id).is_err());
    let mut air = catalog.revision(&id(NOT_TOP_REVISION)).unwrap().clone();
    air.id = id("air.v1");
    air.blocks.push(PositionedBlock {
        position: Pos::new(9, 0, 0),
        block: Block::new(BlockKind::Air),
    });
    catalog.insert_revision(air.clone()).unwrap();
    assert!(blueprint_cell(&catalog, &air.id).is_err());
    let mut mechanical = catalog.revision(&id(NOT_TOP_REVISION)).unwrap().clone();
    mechanical.id = id("mechanical.v1");
    mechanical.ports[1].kind = BlueprintPortKind::BlockState;
    catalog.insert_revision(mechanical.clone()).unwrap();
    assert!(blueprint_cell(&catalog, &mechanical.id).is_err());
}
