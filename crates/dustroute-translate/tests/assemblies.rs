#[path = "support/archive_fixture.rs"]
mod archive_fixture;
use archive_fixture::FixtureJson;
use dustroute_library::assembly::*;
use dustroute_library::blueprint::*;
use dustroute_library::builtin_blueprints::*;
use dustroute_translate::assembly::{AssemblyValidationError, validate_assembly};
use dustroute_translate::{
    compiler::BaselineCompiler, ir::DagBuilder, ir::GateKind, world::BlockKind,
};

fn inverter() -> dustroute_translate::ir::LogicDag {
    let mut builder = DagBuilder::new();
    let input = builder.input("in");
    let output = builder.gate(GateKind::Not, &[input], None);
    builder.finish([("out".into(), output)]).unwrap()
}

#[test]
fn captured_native_states_and_observation_coverage_are_not_inferred() {
    use dustroute_translate::snapshot::assembly_from_snapshot;
    use dustroute_translate::{
        snapshot::MinecraftSnapshot, world::Facing, world::Pos, world::Region,
        world::WireConnection,
    };
    let snapshot: MinecraftSnapshot = serde_json::from_value(serde_json::json!({
        "min": {"x": 0, "y": 0, "z": 0}, "max": {"x": 3, "y": 1, "z": 0},
        "blocks": [
            {"pos": {"x": 0, "y": 1, "z": 0}, "name": "minecraft:redstone_wire",
             "properties": {"north": "up", "east": "none", "south": "none", "west": "side", "power": "7"}},
            {"pos": {"x": 2, "y": 0, "z": 0}, "name": "minecraft:target", "properties": {"power": "3"}}
        ]
    })).unwrap();
    let catalog = BlueprintCatalog::default();
    let captured = assembly_from_snapshot(
        &snapshot,
        "Observation",
        vec![Region::new(snapshot.min, snapshot.max)],
    )
    .unwrap();
    let view = captured.inspect(&catalog).unwrap();
    let wire = view.block_at(Pos::new(0, 1, 0)).unwrap();
    assert_eq!(
        wire.wire_connections.as_ref().unwrap()[&Facing::North],
        WireConnection::Up
    );
    assert_eq!(wire.observed_properties, snapshot.blocks[0].properties);
    assert_eq!(wire.power_level, Some(7));
    assert_ne!(
        dustroute_translate::snapshot::world_from_snapshot(&snapshot)
            .unwrap()
            .get(Pos::new(0, 1, 0))
            .unwrap()
            .wire_connections,
        wire.wire_connections
    );
    assert_eq!(
        view.block_at(Pos::new(2, 0, 0))
            .unwrap()
            .observed_name
            .as_deref(),
        Some("minecraft:target")
    );
    assert_eq!(
        view.block_at(Pos::new(3, 1, 0)).unwrap().kind,
        BlockKind::Air
    );
    assert!(view.block_at(Pos::new(4, 1, 0)).is_none());
    assert!(captured.instances.is_empty());
    assert!(captured.connections.is_empty());
    let partial = assembly_from_snapshot(&snapshot, "Partial", vec![]).unwrap();
    assert!(
        partial
            .inspect(&catalog)
            .unwrap()
            .block_at(Pos::new(3, 1, 0))
            .is_none()
    );
    let mut duplicate = snapshot.clone();
    duplicate.blocks.push(snapshot.blocks[0].clone());
    assert!(assembly_from_snapshot(&duplicate, "Invalid", vec![]).is_err());
}

#[test]
fn half_adder_roundtrip_restores_composed_wire_states_and_preserves_source_revisions() {
    let mut catalog = builtin_blueprints().clone();
    let sources = catalog.fixture_json().unwrap();
    let compiled = BaselineCompiler::new(Default::default())
        .compile(&dustroute_translate::circuits::half_adder())
        .unwrap();
    let assembly = compiled.assembly.as_ref().unwrap();
    let view = assembly.inspect(&catalog).unwrap();
    assert_eq!(view.proposed_world(), compiled.world.clone().into_world());
    assert!(view.source_differences().iter().any(|difference| {
        difference.source.kind == BlockKind::RedstoneWire
            && difference.actual.as_ref().unwrap().wire_connections
                != difference.source.wire_connections
    }));
    let saved = AssemblyRevision {
        id: AssemblyRevisionId::new("half-adder.state.v1").unwrap(),
        parents: vec![],
        assembly: assembly.clone(),
    };
    catalog.insert_assembly(saved.clone()).unwrap();
    let loaded = archive_fixture::catalog(&catalog.fixture_json().unwrap()).unwrap();
    let restored = loaded.assembly(&saved.id).unwrap();
    assert_eq!(restored, &saved);
    assert_eq!(
        validate_assembly(&loaded, &restored.assembly).unwrap(),
        compiled.world
    );
    let original = archive_fixture::catalog(&sources).unwrap();
    for record in original.revisions() {
        assert_eq!(loaded.revision(&record.id), Some(record));
    }
    assert_eq!(assembly.boundaries.len(), 4);
    assert_eq!(assembly.instances.len(), compiled.physical.cells.len());
    // Promote the same physical arrangement to a nested reusable source while
    // retaining its already-composed state and connections as a separate value.
    let (parent, nested) = assembly
        .group_as_blueprint(
            &catalog,
            BlueprintGrouping {
                id: BlueprintRevisionId::new("half-adder.arrangement.v1").unwrap(),
                name: "Half adder arrangement".into(),
                classifications: vec![],
                provenance: catalog
                    .revision(&BlueprintRevisionId::new(NOT_TOP_REVISION).unwrap())
                    .unwrap()
                    .provenance
                    .clone(),
            },
        )
        .unwrap();
    assert_eq!(parent.inclusions.len(), assembly.instances.len());
    catalog.insert_revision(parent).unwrap();
    let nested_revision = AssemblyRevision {
        id: AssemblyRevisionId::new("half-adder.state.v2").unwrap(),
        parents: vec![saved.id],
        assembly: nested,
    };
    catalog.insert_assembly(nested_revision.clone()).unwrap();
    let loaded = archive_fixture::catalog(&catalog.fixture_json().unwrap()).unwrap();
    let nested = &loaded.assembly(&nested_revision.id).unwrap().assembly;
    assert_eq!(validate_assembly(&loaded, nested).unwrap(), compiled.world);
    assert!(
        nested
            .inspect(&loaded)
            .unwrap()
            .occurrences
            .keys()
            .any(|path| path.len() == 2)
    );
}

#[test]
fn replacing_a_shared_not_arrangement_can_change_its_nesting_without_mutating_history() {
    use dustroute_translate::{
        blueprint::blueprint_cell, cell_library::verify_cell, world::Pos, world::Region,
    };
    let mut catalog = builtin_blueprints().clone();
    let top_id = BlueprintRevisionId::new(NOT_TOP_REVISION).unwrap();
    let side_id = BlueprintRevisionId::new(NOT_SIDE_REVISION).unwrap();
    let mut parent = catalog.revision(&top_id).unwrap().clone();
    parent.id = BlueprintRevisionId::new("not-parent.v1").unwrap();
    parent.blocks.clear();
    parent.inclusions = ["b", "c"]
        .into_iter()
        .map(|name| BlueprintInclusion {
            rotation: Default::default(),
            instance: InstanceId::new(name).unwrap(),
            revision: top_id.clone(),
            origin: Pos::default(),
        })
        .collect();
    catalog.insert_revision(parent.clone()).unwrap();
    let first = AssemblyRevision {
        id: AssemblyRevisionId::new("not.state.v1").unwrap(),
        parents: vec![],
        assembly: Assembly {
            name: "NOT".into(),
            instances: vec![BlueprintInclusion {
                rotation: Default::default(),
                instance: InstanceId::new("root").unwrap(),
                revision: parent.id.clone(),
                origin: Pos::default(),
            }],
            blocks: catalog
                .expand(&parent.id)
                .unwrap()
                .blocks
                .into_iter()
                .map(|(position, block)| PositionedBlock { position, block })
                .collect(),
            known_regions: vec![Region::new(Pos::new(-2, -2, -2), Pos::new(5, 5, 5))],
            connections: vec![],
            boundaries: vec![],
        },
    };
    catalog.insert_assembly(first.clone()).unwrap();
    let mut next = first.clone();
    next.id = AssemblyRevisionId::new("not.state.v2").unwrap();
    next.parents = vec![first.id.clone()];
    next.assembly.instances[0].revision = side_id.clone();
    next.assembly.blocks = catalog.revision(&side_id).unwrap().blocks.clone();
    catalog.insert_assembly(next.clone()).unwrap();
    let before = first.assembly.inspect(&catalog).unwrap();
    let after = next.assembly.inspect(&catalog).unwrap();
    let impact = after.changes_from(&before);
    assert!(impact.affected_occurrences.contains(&vec![
        InstanceId::new("root").unwrap(),
        InstanceId::new("c").unwrap()
    ]));
    assert_eq!(before.occurrences.len(), 3);
    assert_eq!(after.occurrences.len(), 1);
    for (record, source) in [(&first, &parent.id), (&next, &side_id)] {
        let mut physical = blueprint_cell(&catalog, source).unwrap();
        physical.world = validate_assembly(&catalog, &record.assembly)
            .unwrap()
            .into_world();
        assert!(verify_cell(GateKind::Not, &physical).valid);
    }
    assert_eq!(catalog.assembly(&first.id), Some(&first));
    assert_eq!(catalog.revision(&parent.id), Some(&parent));
    let restored = archive_fixture::catalog(&catalog.fixture_json().unwrap()).unwrap();
    assert_eq!(restored.assembly(&next.id), Some(&next));
}

#[test]
fn loading_a_broken_state_does_not_restore_placement_or_connection_proof() {
    let mut catalog = builtin_blueprints().clone();
    let compiled = BaselineCompiler::new(Default::default())
        .compile(&inverter())
        .unwrap();
    let mut assembly = compiled.assembly.unwrap();
    let path = &assembly.connections[0].path;
    let middle = path[path.len() / 2];
    assembly.blocks.retain(|block| block.position != middle);
    let revision = AssemblyRevision {
        id: AssemblyRevisionId::new("broken.state.v1").unwrap(),
        parents: vec![],
        assembly,
    };
    catalog.insert_assembly(revision.clone()).unwrap();
    let loaded = archive_fixture::catalog(&catalog.fixture_json().unwrap()).unwrap();
    assert!(validate_assembly(&loaded, &loaded.assembly(&revision.id).unwrap().assembly).is_err());
}

#[test]
fn exact_type_requirements_are_checked_against_actual_state_after_loading() {
    let mut catalog = builtin_blueprints().clone();
    let compiled = BaselineCompiler::new(Default::default())
        .compile(&inverter())
        .unwrap();
    let mut actual = compiled.assembly.unwrap();
    let first = actual.connections[0].clone();
    let source_pos = first.path[0];
    let actual_source = actual
        .blocks
        .iter()
        .find(|record| record.position == source_pos)
        .unwrap()
        .block
        .clone();
    let required = TypeRevisionId::new("exact-output-state.v1").unwrap();
    catalog
        .insert_type(TypeRevision {
            id: required.clone(),
            name: "Exact producer state".into(),
            contract: TypeContract::BlockPattern {
                blocks: vec![PositionedBlock {
                    position: Default::default(),
                    block: actual_source,
                }],
            },
        })
        .unwrap();
    let sink_instance = actual
        .instances
        .iter_mut()
        .find(|instance| instance.instance == first.sink.instance[0])
        .unwrap();
    let mut typed = catalog.revision(&sink_instance.revision).unwrap().clone();
    typed.id = BlueprintRevisionId::new("typed-consumer.v1").unwrap();
    typed
        .ports
        .iter_mut()
        .find(|port| port.name == first.sink.port)
        .unwrap()
        .required_source_types = vec![required.clone()];
    sink_instance.revision = typed.id.clone();
    catalog.insert_revision(typed).unwrap();
    validate_assembly(&catalog, &actual).unwrap();
    // Requirements use producer-local coordinates/states, while connections
    // and placed blocks rotate together in the actual world.
    let rotation = dustroute_translate::cells::RotationY::R90;
    for instance in &mut actual.instances {
        instance.origin = rotation.pos(instance.origin);
        instance.rotation = rotation.then(instance.rotation);
    }
    for record in &mut actual.blocks {
        record.position = rotation.pos(record.position);
        record.block = rotation.block(&record.block);
    }
    for region in &mut actual.known_regions {
        *region = dustroute_translate::world::Region::new(
            rotation.pos(region.min),
            rotation.pos(region.max),
        );
    }
    for connection in &mut actual.connections {
        for position in &mut connection.path {
            *position = rotation.pos(*position);
        }
    }
    validate_assembly(&catalog, &actual).unwrap();
    let source_pos = rotation.pos(source_pos);
    actual
        .blocks
        .iter_mut()
        .find(|record| record.position == source_pos)
        .unwrap()
        .block
        .power_level = Some(1);
    let revision = AssemblyRevision {
        id: AssemblyRevisionId::new("changed-state.v1").unwrap(),
        parents: vec![],
        assembly: actual,
    };
    catalog.insert_assembly(revision.clone()).unwrap();
    let loaded = archive_fixture::catalog(&catalog.fixture_json().unwrap()).unwrap();
    assert!(
        matches!(validate_assembly(&loaded, &loaded.assembly(&revision.id).unwrap().assembly),
        Err(AssemblyValidationError::Connection { error: dustroute_translate::blueprint_connection::BlueprintConnectionError::Blueprint(BlueprintError::UnsatisfiedSourceType(id)), .. }) if id == required)
    );
}
