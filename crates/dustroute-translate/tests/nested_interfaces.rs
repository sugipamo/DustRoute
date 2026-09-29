use dustroute_library::PortDirection;
use dustroute_library::assembly::*;
use dustroute_library::blueprint::*;
use dustroute_library::builtin_blueprints::*;
use dustroute_translate::assembly::{AssemblyValidationError, validate_assembly};
use dustroute_translate::compiler::baseline_blueprint_selection;
use dustroute_translate::{
    cells::RotationY, compiler::BaselineCompileConfig, compiler::BaselineCompiler, ir::DagBuilder,
    ir::GateKind, world::Pos,
};

fn id(value: &str) -> BlueprintRevisionId {
    BlueprintRevisionId::new(value).unwrap()
}
fn port(path: &[&str], name: &str) -> BlueprintPortRef {
    BlueprintPortRef {
        instance: path
            .iter()
            .map(|value| InstanceId::new(*value).unwrap())
            .collect(),
        port: name.into(),
    }
}
fn inverter() -> dustroute_translate::ir::LogicDag {
    let mut builder = DagBuilder::new();
    let input = builder.input("in");
    let output = builder.gate(GateKind::Not, &[input], None);
    builder.finish([("out".into(), output)]).unwrap()
}

#[test]
fn promoted_not_reuses_internal_routes_and_nested_requirements_when_compiled_again() {
    let mut catalog = builtin_blueprints().clone();
    let mut leaf = catalog.revision(&id(NOT_TOP_REVISION)).unwrap().clone();
    leaf.id = id("typed-not.v1");
    leaf.ports
        .iter_mut()
        .find(|p| p.direction == PortDirection::Input)
        .unwrap()
        .required_source_types = vec![TypeRevisionId::new(WIRE_TYPE_REVISION).unwrap()];
    catalog.insert_revision(leaf.clone()).unwrap();
    let mut selection = baseline_blueprint_selection();
    selection.insert(GateKind::Not, leaf.id.clone());
    let compiled = BaselineCompiler::new(Default::default())
        .compile_with_blueprints(&inverter(), &catalog, &selection)
        .unwrap();
    let original = compiled.assembly.as_ref().unwrap();
    let (mut parent, _) = original
        .group_as_blueprint(
            &catalog,
            BlueprintGrouping {
                id: id("buffered-not.v1"),
                name: "Buffered NOT arrangement".into(),
                classifications: leaf.classifications.clone(),
                provenance: leaf.provenance.clone(),
            },
        )
        .unwrap();
    assert_eq!(parent.connections, original.connections);
    assert_eq!(parent.port_bindings, original.boundaries);
    // Adapt only the exposed input name to the existing primitive compiler.
    parent
        .ports
        .iter_mut()
        .find(|p| p.name == "in")
        .unwrap()
        .name = "a".into();
    parent
        .port_bindings
        .iter_mut()
        .find(|p| p.name == "in")
        .unwrap()
        .name = "a".into();
    parent
        .ports
        .iter_mut()
        .find(|p| p.name == "a")
        .unwrap()
        .required_source_types = vec![TypeRevisionId::new(WIRE_TYPE_REVISION).unwrap()];
    catalog.insert_revision(parent.clone()).unwrap();
    let restored = BlueprintCatalog::from_json(&catalog.to_json().unwrap()).unwrap();
    selection.insert(GateKind::Not, parent.id.clone());
    let reused = BaselineCompiler::new(BaselineCompileConfig {
        spacing_x: 48,
        ..Default::default()
    })
    .compile_with_blueprints(&inverter(), &restored, &selection)
    .unwrap();
    let assembly = reused.assembly.as_ref().unwrap();
    let view = assembly.inspect(&restored).unwrap();
    assert!(view.occurrences.keys().any(|path| path.len() == 2));
    assert_eq!(
        assembly.connections.len(),
        reused.routing.nets.len() + parent.connections.len()
    );
    assert_eq!(
        validate_assembly(&restored, assembly).unwrap(),
        reused.world
    );
    // Meaning is checked separately from types, using the composed circuit.
    for input in [false, true] {
        let mut world = reused.world.clone().into_world();
        if input {
            world.set(
                reused.input_positions["in"].offset(-1, 0, 0),
                dustroute_translate::world::Block::new(
                    dustroute_translate::world::BlockKind::RedstoneBlock,
                ),
            );
        }
        dustroute_translate::wire::update_wire_shapes(&mut world);
        let settled = dustroute_translate::sim::RedstoneTickSimulator::new(world)
            .unwrap()
            .settle_ticks(64)
            .unwrap();
        assert_eq!(settled.strength(reused.output_positions["out"]) > 0, !input);
    }
    assert_eq!(
        reused
            .capture_assembly_in_catalog(&restored, &assembly.name)
            .unwrap(),
        *assembly
    );
    // Even inputs with no type annotation cannot silently drop an explicit
    // source connection. Loading structure alone remains possible for drafts.
    let mut incomplete = assembly.clone();
    incomplete
        .connections
        .retain(|edge| edge.sink.instance.len() == 1);
    assert!(incomplete.inspect(&restored).is_ok());
    assert!(matches!(
        validate_assembly(&restored, &incomplete),
        Err(AssemblyValidationError::UnboundSourceConnection { .. })
    ));
    assert_eq!(
        incomplete.with_source_connections(&restored).unwrap(),
        *assembly
    );
    // Saved route proposals do not pass if the actual connected blocks break.
    let inner = assembly
        .connections
        .iter()
        .find(|edge| edge.sink.instance.len() == 2)
        .unwrap();
    let middle = inner.path[inner.path.len() / 2];
    let mut broken = assembly.clone();
    broken.blocks.retain(|record| record.position != middle);
    assert!(validate_assembly(&restored, &broken).is_err());
    assert_eq!(restored.revision(&leaf.id), Some(&leaf));
    assert_eq!(restored.revision(&parent.id), Some(&parent));
}

fn wrapper(
    catalog: &BlueprintCatalog,
    source: &BlueprintRevisionId,
    name: &str,
    rotation: RotationY,
) -> BlueprintRevision {
    let mut parent = catalog.revision(source).unwrap().clone();
    parent.id = id(name);
    parent.blocks.clear();
    parent.inclusions = vec![BlueprintInclusion {
        instance: InstanceId::new("child").unwrap(),
        revision: source.clone(),
        origin: Pos::default(),
        rotation,
    }];
    parent.connections.clear();
    parent.port_bindings = parent
        .ports
        .iter()
        .map(|p| BlueprintPortBinding {
            name: p.name.clone(),
            port: port(&["child"], &p.name),
        })
        .collect();
    for p in &mut parent.ports {
        p.position = rotation.pos(p.position);
        p.facing = p.facing.map(|f| rotation.facing(f));
        p.required_source_types.clear();
    }
    parent
}

#[test]
fn source_aliases_keep_rotated_type_frames_and_cannot_bypass_parent_or_child_requirements() {
    use dustroute_translate::blueprint_connection::{
        BlueprintEndpoint, check_blueprint_connection,
    };
    use dustroute_translate::{
        world::Block, world::BlockKind, world::ValidatedWorld, world::World,
    };
    let mut catalog = builtin_blueprints().clone();
    let mut local = World::new();
    for x in 0..5 {
        local.set(Pos::new(x, 0, 0), Block::new(BlockKind::Solid));
        local.place(BlockKind::RedstoneWire, Pos::new(x, 1, 0));
    }
    dustroute_translate::wire::update_wire_shapes(&mut local);
    let required = TypeRevisionId::new("adjacent-wire.v1").unwrap();
    catalog
        .insert_type(TypeRevision {
            id: required.clone(),
            name: "Adjacent producer-local wire".into(),
            contract: TypeContract::BlockPattern {
                blocks: vec![PositionedBlock {
                    position: Pos::new(1, 0, 0),
                    block: local.get(Pos::new(1, 1, 0)).unwrap().clone(),
                }],
            },
        })
        .unwrap();
    let mut consumer = catalog.revision(&id(TERMINAL_REVISION)).unwrap().clone();
    consumer.id = id("typed-consumer.v1");
    consumer
        .ports
        .iter_mut()
        .find(|p| p.direction == PortDirection::Input)
        .unwrap()
        .required_source_types = vec![TypeRevisionId::new(WIRE_TYPE_REVISION).unwrap()];
    catalog.insert_revision(consumer.clone()).unwrap();
    let source = wrapper(
        &catalog,
        &id(TERMINAL_REVISION),
        "source-alias.v1",
        RotationY::R90,
    );
    catalog.insert_revision(source.clone()).unwrap();
    let mut sink = wrapper(&catalog, &consumer.id, "sink-alias.v1", RotationY::R90);
    sink.ports
        .iter_mut()
        .find(|p| p.direction == PortDirection::Input)
        .unwrap()
        .required_source_types = vec![required];
    catalog.insert_revision(sink.clone()).unwrap();
    let mut actual = World::new();
    for (position, block) in local.iter() {
        actual.set(RotationY::R90.pos(*position), RotationY::R90.block(block));
    }
    let source_endpoint = BlueprintEndpoint {
        revision: &source.id,
        port: "out",
        origin: Pos::default(),
        rotation: RotationY::R0,
    };
    let sink_endpoint = BlueprintEndpoint {
        revision: &sink.id,
        port: "in",
        origin: Pos::new(0, 0, 4),
        rotation: RotationY::R0,
    };
    let path: Vec<_> = (0..5).map(|z| Pos::new(0, 1, z)).collect();
    check_blueprint_connection(
        &catalog,
        &ValidatedWorld::try_from(actual.clone()).unwrap(),
        source_endpoint,
        sink_endpoint,
        &path,
    )
    .unwrap();
    let mut assembly = Assembly {
        name: "Aliased connection".into(),
        instances: vec![
            BlueprintInclusion {
                instance: InstanceId::new("source").unwrap(),
                revision: source.id.clone(),
                origin: Pos::default(),
                rotation: RotationY::R0,
            },
            BlueprintInclusion {
                instance: InstanceId::new("sink").unwrap(),
                revision: sink.id.clone(),
                origin: Pos::new(0, 0, 4),
                rotation: RotationY::R0,
            },
        ],
        blocks: actual
            .iter()
            .map(|(position, block)| PositionedBlock {
                position: *position,
                block: block.clone(),
            })
            .collect(),
        known_regions: vec![],
        boundaries: vec![],
        connections: vec![BlueprintConnection {
            source: port(&["source"], "out"),
            sink: port(&["sink"], "in"),
            path,
        }],
    };
    validate_assembly(&catalog, &assembly).unwrap();
    // Selecting the leaf directly still has to satisfy the exposed parent's
    // requirements on the same physical terminal.
    assembly.connections[0].sink = port(&["sink", "child"], "in");
    assembly.connections[0].source = port(&["source", "child"], "out");
    validate_assembly(&catalog, &assembly).unwrap();
    assembly
        .blocks
        .iter_mut()
        .find(|record| record.position == Pos::new(0, 1, 1))
        .unwrap()
        .block
        .power_level = Some(9);
    assert!(matches!(
        validate_assembly(&catalog, &assembly),
        Err(AssemblyValidationError::Connection { .. })
    ));
}
