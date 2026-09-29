use dustroute_library::blueprint::BlueprintRevisionId;
use dustroute_library::builtin_blueprints::{XOR_COMPACT_REVISION, builtin_blueprints};
use dustroute_optimize::{resolve_blueprint_layout, resolve_builtin_layout};

#[test]
fn legacy_layout_references_and_revision_references_load_the_same_frozen_geometry() {
    let legacy = resolve_builtin_layout("dustroute-translate:compact_compiled_xor_cell").unwrap();
    let pinned = resolve_builtin_layout(&format!("blueprint:{XOR_COMPACT_REVISION}")).unwrap();
    assert_eq!(legacy, pinned);
    assert!(resolve_builtin_layout("dustroute-translate:arbitrary_generator").is_err());
    assert!(resolve_builtin_layout("blueprint:missing.v1").is_err());
}

#[test]
fn caller_catalogs_are_data_sources_without_registering_executable_resolvers() {
    let mut catalog = builtin_blueprints().clone();
    let mut revision = catalog
        .revision(&BlueprintRevisionId::new(XOR_COMPACT_REVISION).unwrap())
        .unwrap()
        .clone();
    revision.parents = vec![revision.id.clone()];
    revision.id = BlueprintRevisionId::new("local.xor.v1").unwrap();
    revision.name = "Local XOR copy".into();
    catalog.insert_revision(revision).unwrap();
    assert!(resolve_builtin_layout("blueprint:local.xor.v1").is_err());
    let local = resolve_blueprint_layout("blueprint:local.xor.v1", &catalog).unwrap();
    assert_eq!(local.name, "Local XOR copy");
    assert_eq!(
        local.world,
        resolve_builtin_layout(&format!("blueprint:{XOR_COMPACT_REVISION}"))
            .unwrap()
            .world
    );
}

#[test]
fn same_named_not_candidates_are_replaced_by_revision_and_retained_after_rotation() {
    use dustroute_library::builtin_blueprints::{NOT_SIDE_REVISION, NOT_TOP_REVISION};
    use dustroute_optimize::{apply_mutation, candidate_mutations};
    use dustroute_translate::{
        cell_library::CellLibrary, cells::PlacedCell, cells::RotationY, cells::not_cell,
        cells::not_top_cell, ir::GateKind, physical::PlacementCircuit, world::Pos,
    };
    let mut top = not_top_cell();
    let mut side = not_cell();
    top.name = "same display name".into();
    side.name = top.name.clone();
    let mut library = CellLibrary::default();
    library.add(GateKind::Not, top.clone());
    library.add(GateKind::Not, side);
    let mut placed = PlacementCircuit::new();
    let cell = placed.add_cell(
        GateKind::Not,
        PlacedCell {
            cell: top,
            origin: Pos::new(10, 3, 20),
            rotation: RotationY::R90,
        },
    );
    let mutation = candidate_mutations(&placed, &library, 1)
        .into_iter()
        .find(|candidate| {
            candidate
                .candidate_revision
                .as_ref()
                .is_some_and(|id| id.as_str() == NOT_SIDE_REVISION)
        })
        .unwrap();
    let changed = apply_mutation(&placed, &mutation, &library);
    assert_eq!(
        placed.cells[&cell]
            .placed
            .cell
            .source_revision
            .as_ref()
            .unwrap()
            .as_str(),
        NOT_TOP_REVISION
    );
    assert_eq!(
        changed.cells[&cell]
            .placed
            .cell
            .source_revision
            .as_ref()
            .unwrap()
            .as_str(),
        NOT_SIDE_REVISION
    );
    let world = changed.cell_world().unwrap();
    let captured = dustroute_translate::assembly::capture_placement_assembly(
        &changed,
        &Default::default(),
        &world,
        "Rotated replacement",
    )
    .unwrap();
    assert_eq!(captured.instances[0].revision.as_str(), NOT_SIDE_REVISION);
    assert_eq!(captured.instances[0].rotation, RotationY::R90);
    assert_eq!(
        dustroute_translate::assembly::validate_assembly(builtin_blueprints(), &captured)
            .unwrap()
            .into_world(),
        world
    );
}
