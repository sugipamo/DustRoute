use dustroute_library::PortDirection;
use dustroute_library::blueprint::*;
use dustroute_library::builtin_blueprints::*;
use dustroute_translate::blueprint_connection::{
    BlueprintConnectionError, BlueprintEndpoint, check_blueprint_connection,
};
use dustroute_translate::{Block, BlockKind, Facing, Pos, ValidatedWorld, World};

fn id(value: &str) -> BlueprintRevisionId {
    BlueprintRevisionId::new(value).unwrap()
}

fn constrained_consumer(catalog: &mut BlueprintCatalog) -> BlueprintRevisionId {
    let mut consumer = catalog.revision(&id(TERMINAL_REVISION)).unwrap().clone();
    consumer.id = id("consumer.v1");
    consumer.ports[0].required_source_types =
        vec![TypeRevisionId::new(WIRE_TYPE_REVISION).unwrap()];
    let id = consumer.id.clone();
    catalog.insert_revision(consumer).unwrap();
    id
}

fn wire_world(start: Pos, length: i32) -> ValidatedWorld {
    let mut world = World::new();
    for x in 0..length {
        world.set(start.offset(x, -1, 0), Block::new(BlockKind::Solid));
        world.place(BlockKind::RedstoneWire, start.offset(x, 0, 0));
    }
    dustroute_translate::wire::update_wire_shapes(&mut world);
    ValidatedWorld::try_from(world).unwrap()
}

#[test]
fn directional_interfaces_retain_their_physical_faces_under_spatial_laws() {
    use dustroute_translate::connectivity::{
        comparator_input_pos, comparator_output_pos, observer_input_pos, observer_output_pos,
        repeater_input_pos, repeater_output_pos,
    };
    let pos = Pos::new(3, 4, 5);
    for kind in [
        BlockKind::Repeater,
        BlockKind::Comparator,
        BlockKind::Observer,
    ] {
        for facing in [
            None,
            Some(Facing::North),
            Some(Facing::East),
            Some(Facing::South),
            Some(Facing::West),
            Some(Facing::Up),
            Some(Facing::Down),
        ] {
            let mut world = World::new();
            world.place(kind, pos).facing = facing;
            let actual = match kind {
                BlockKind::Repeater => (
                    repeater_input_pos(&world, pos),
                    repeater_output_pos(&world, pos),
                ),
                BlockKind::Comparator => (
                    comparator_input_pos(&world, pos),
                    comparator_output_pos(&world, pos),
                ),
                _ => (
                    observer_input_pos(&world, pos),
                    observer_output_pos(&world, pos),
                ),
            };
            let valid = facing
                .filter(|f| kind == BlockKind::Observer || !matches!(f, Facing::Up | Facing::Down));
            let expected = valid.map(|f| f.offset());
            assert_eq!(
                actual,
                (
                    expected.map(|d| pos.offset(-d.x, -d.y, -d.z)),
                    expected.map(|d| pos.offset(d.x, d.y, d.z))
                )
            );
        }
    }
}

#[test]
fn not_and_gate_outputs_share_connection_types_without_reading_their_meaning() {
    let mut catalog = builtin_blueprints().clone();
    let sink_id = constrained_consumer(&mut catalog);
    for revision in [NOT_TOP_REVISION, AND_REVISION] {
        let source_id = id(revision);
        let record = catalog.revision(&source_id).unwrap();
        let output = record
            .ports
            .iter()
            .find(|port| port.direction == PortDirection::Output)
            .unwrap();
        let start = output.position;
        let path: Vec<_> = (0..4).map(|x| start.offset(x, 0, 0)).collect();
        let world = wire_world(start, 4);
        // Only selected physical terminals are needed for connection checking;
        // this deliberately supplies no functional NOT or AND circuit/evidence.
        check_blueprint_connection(
            &catalog,
            &world,
            BlueprintEndpoint {
                rotation: Default::default(),
                revision: &source_id,
                port: &output.name,
                origin: Pos::default(),
            },
            BlueprintEndpoint {
                rotation: Default::default(),
                revision: &sink_id,
                port: "in",
                origin: path[3].offset(0, -1, 0),
            },
            &path,
        )
        .unwrap();
    }
}

#[test]
fn matching_types_cannot_certify_a_missing_wire_or_reversed_repeater() {
    let mut catalog = builtin_blueprints().clone();
    let sink_id = constrained_consumer(&mut catalog);
    let source_id = id(TERMINAL_REVISION);
    let source = BlueprintEndpoint {
        rotation: Default::default(),
        revision: &source_id,
        port: "out",
        origin: Pos::default(),
    };
    let sink = BlueprintEndpoint {
        rotation: Default::default(),
        revision: &sink_id,
        port: "in",
        origin: Pos::new(4, 0, 0),
    };
    let path: Vec<_> = (0..5).map(|x| Pos::new(x, 1, 0)).collect();
    let original = wire_world(path[0], 5);
    check_blueprint_connection(&catalog, &original, source, sink, &path).unwrap();
    let mut broken = original.clone().into_world();
    broken.set(path[2], Block::new(BlockKind::Air));
    let broken = ValidatedWorld::try_from(broken).unwrap();
    assert!(matches!(
        check_blueprint_connection(&catalog, &broken, source, sink, &path),
        Err(BlueprintConnectionError::DisconnectedStep { .. })
    ));
    for (facing, accepted) in [(Facing::East, true), (Facing::West, false)] {
        let mut world = original.clone().into_world();
        let repeater = world.place(BlockKind::Repeater, path[2]);
        repeater.facing = Some(facing);
        repeater.delay = Some(1);
        dustroute_translate::wire::update_wire_shapes(&mut world);
        let world = ValidatedWorld::try_from(world).unwrap();
        assert_eq!(
            check_blueprint_connection(&catalog, &world, source, sink, &path).is_ok(),
            accepted
        );
    }
    let mut fabricated = original.into_world();
    fabricated.set(path[0], Block::new(BlockKind::Solid));
    assert!(
        check_blueprint_connection(
            &catalog,
            &ValidatedWorld::try_from(fabricated).unwrap(),
            source,
            sink,
            &path
        )
        .is_err()
    );
}

#[test]
fn block_power_consumer_can_require_a_wire_source_without_matching_port_kinds() {
    let mut catalog = builtin_blueprints().clone();
    let mut sink = catalog.revision(&id(NOT_SIDE_REVISION)).unwrap().clone();
    sink.id = id("block-consumer.v1");
    sink.ports[0].required_source_types = vec![TypeRevisionId::new(WIRE_TYPE_REVISION).unwrap()];
    let sink_id = sink.id.clone();
    catalog.insert_revision(sink).unwrap();
    let source_id = id(TERMINAL_REVISION);
    let mut world = wire_world(Pos::new(0, 1, 0), 3).into_world();
    world.set(Pos::new(3, 1, 0), Block::new(BlockKind::Solid));
    dustroute_translate::wire::update_wire_shapes(&mut world);
    check_blueprint_connection(
        &catalog,
        &ValidatedWorld::try_from(world).unwrap(),
        BlueprintEndpoint {
            rotation: Default::default(),
            revision: &source_id,
            port: "out",
            origin: Pos::default(),
        },
        BlueprintEndpoint {
            rotation: Default::default(),
            revision: &sink_id,
            port: "a",
            origin: Pos::new(3, 1, 0),
        },
        &(0..4).map(|x| Pos::new(x, 1, 0)).collect::<Vec<_>>(),
    )
    .unwrap();
}

#[test]
fn block_requirements_use_the_selected_ports_frame_and_the_actual_snapshot() {
    let mut catalog = builtin_blueprints().clone();
    let required_id = TypeRevisionId::new("solid-below-output.v1").unwrap();
    catalog
        .insert_type(TypeRevision {
            id: required_id.clone(),
            name: "Solid support".into(),
            contract: TypeContract::BlockPattern {
                blocks: vec![PositionedBlock {
                    position: Pos::new(0, -1, 0),
                    block: Block::new(BlockKind::Solid),
                }],
            },
        })
        .unwrap();
    let source_id = id(TERMINAL_REVISION);
    let mut consumer = catalog.revision(&source_id).unwrap().clone();
    consumer.id = id("position-consumer.v1");
    consumer.ports[0].required_source_types = vec![required_id.clone()];
    let sink_id = consumer.id.clone();
    catalog.insert_revision(consumer).unwrap();
    let source = BlueprintEndpoint {
        rotation: Default::default(),
        revision: &source_id,
        port: "out",
        origin: Pos::new(10, 4, -6),
    };
    let sink = BlueprintEndpoint {
        rotation: Default::default(),
        revision: &sink_id,
        port: "in",
        origin: Pos::new(12, 4, -6),
    };
    let path: Vec<_> = (10..13).map(|x| Pos::new(x, 5, -6)).collect();
    let world = wire_world(path[0], 3);
    check_blueprint_connection(&catalog, &world, source, sink, &path).unwrap();
    let mut changed = world.into_world();
    changed.set(Pos::new(10, 4, -6), Block::new(BlockKind::Transparent));
    assert_eq!(
        check_blueprint_connection(
            &catalog,
            &ValidatedWorld::try_from(changed).unwrap(),
            source,
            sink,
            &path
        ),
        Err(BlueprintConnectionError::Blueprint(
            BlueprintError::UnsatisfiedSourceType(required_id)
        )),
    );
}

#[test]
fn selecting_a_port_does_not_apply_other_output_requirements_and_directions_are_checked() {
    let mut catalog = builtin_blueprints().clone();
    let mut terminal = catalog.revision(&id(TERMINAL_REVISION)).unwrap().clone();
    terminal.id = id("multiple-outputs.v1");
    let mut mechanical = terminal.ports[1].clone();
    mechanical.name = "mechanical".into();
    mechanical.kind = BlueprintPortKind::BlockState;
    terminal.ports.push(mechanical);
    catalog.insert_revision(terminal.clone()).unwrap();
    let mut source = terminal.ports[1].clone();
    let mut consumer = terminal.ports[0].clone();
    consumer.required_source_types = vec![TypeRevisionId::new(WIRE_TYPE_REVISION).unwrap()];
    let states = |_| Some(Block::new(BlockKind::RedstoneWire));
    catalog
        .check_source_requirements(&source, &consumer, states)
        .unwrap();
    source.kind = BlueprintPortKind::BlockPower;
    assert!(
        catalog
            .check_source_requirements(&source, &consumer, |_| Some(Block::new(BlockKind::Solid)))
            .is_err()
    );
    source.kind = BlueprintPortKind::Wire;
    source.direction = PortDirection::Input;
    assert!(
        catalog
            .check_source_requirements(&source, &consumer, states)
            .is_err()
    );
    source.direction = PortDirection::Output;
    consumer.kind = BlueprintPortKind::BlockState;
    assert!(
        catalog
            .check_source_requirements(&source, &consumer, states)
            .is_err()
    );
}

#[test]
fn compiler_enforces_consumer_types_and_pins_selected_revisions() {
    use dustroute_translate::compiler::baseline_blueprint_selection;
    use dustroute_translate::{BaselineCompiler, CompileError, DagBuilder, GateKind};
    let mut catalog = builtin_blueprints().clone();
    let mut consumer = catalog.revision(&id(NOT_TOP_REVISION)).unwrap().clone();
    consumer.id = id("typed-not.v1");
    consumer.ports[0].required_source_types =
        vec![TypeRevisionId::new(WIRE_TYPE_REVISION).unwrap()];
    catalog.insert_revision(consumer.clone()).unwrap();
    let mut selection = baseline_blueprint_selection();
    selection.insert(GateKind::Not, consumer.id.clone());
    let mut dag = DagBuilder::new();
    let input = dag.input("a");
    let output = dag.gate(GateKind::Not, &[input], None);
    let dag = dag.finish([("out".into(), output)]).unwrap();
    let compiler = BaselineCompiler::new(Default::default());
    let result = compiler
        .compile_with_blueprints(&dag, &catalog, &selection)
        .unwrap();
    assert_eq!(
        result.blueprint_revisions.len(),
        result.physical.cells.len()
    );
    assert!(
        result
            .blueprint_revisions
            .values()
            .any(|id| id == &consumer.id)
    );
    assert!(dustroute_translate::blueprint::blueprint_cell(&catalog, &consumer.id).is_err());
    // A new incompatible contract must fail after composition; it must not be
    // erased by conversion to the compiler's older physical cell format.
    consumer.parents = vec![consumer.id.clone()];
    consumer.id = id("typed-not.v2");
    let wrong_type = TypeRevisionId::new(BLOCK_POWER_TYPE_REVISION).unwrap();
    consumer.ports[0].required_source_types = vec![wrong_type.clone()];
    catalog.insert_revision(consumer.clone()).unwrap();
    selection.insert(GateKind::Not, consumer.id);
    assert!(
        matches!(compiler.compile_with_blueprints(&dag, &catalog, &selection),
        Err(CompileError::Connection(BlueprintConnectionError::Blueprint(BlueprintError::UnsatisfiedSourceType(id)))) if id == wrong_type)
    );
}

#[test]
fn compiler_does_not_assume_requirements_on_external_inputs_are_satisfied() {
    use dustroute_translate::compiler::baseline_blueprint_selection;
    use dustroute_translate::{BaselineCompiler, CompileError, DagBuilder, GateKind};
    let mut catalog = builtin_blueprints().clone();
    let mut boundary = catalog.revision(&id(BUFFER_REVISION)).unwrap().clone();
    boundary.id = id("typed-boundary.v1");
    boundary.ports[0].required_source_types =
        vec![TypeRevisionId::new(WIRE_TYPE_REVISION).unwrap()];
    catalog.insert_revision(boundary.clone()).unwrap();
    let mut selection = baseline_blueprint_selection();
    selection.insert(GateKind::Input, boundary.id);
    let mut dag = DagBuilder::new();
    let input = dag.input("a");
    let dag = dag.finish([("out".into(), input)]).unwrap();
    assert!(
        matches!(BaselineCompiler::new(Default::default()).compile_with_blueprints(&dag, &catalog, &selection),
        Err(CompileError::Blueprint(BlueprintError::Invalid(message))) if message.contains("unconnected port"))
    );
}

#[test]
fn direct_device_output_connections_share_the_physical_power_directions() {
    let mut catalog = builtin_blueprints().clone();
    let mut producer = dustroute_library::builtin_primitives::builtin_primitives()
        .revisions()
        .next()
        .unwrap()
        .clone();
    producer.id = id("test.device-output.v1");
    let source_id = producer.id.clone();
    catalog.insert_revision(producer).unwrap();
    let mut consumer = catalog.revision(&id(NOT_SIDE_REVISION)).unwrap().clone();
    consumer.id = id("test.any-input-source.v1");
    consumer.ports[0].facing = None;
    let sink_id = consumer.id.clone();
    catalog.insert_revision(consumer).unwrap();
    let source_pos = Pos::new(-1, 0, 0);
    // The NOT input does not require a lever: another signal source can connect.
    for (kind, sink_pos, accepted) in [
        (BlockKind::Lever, Pos::default(), true),
        (BlockKind::RedstoneBlock, Pos::default(), true),
        (BlockKind::RedstoneTorch, Pos::default(), false),
        (BlockKind::RedstoneTorch, Pos::new(-1, 1, 0), true),
    ] {
        let mut world = World::new();
        world.set(Pos::default(), Block::new(BlockKind::Solid));
        world.set(sink_pos, Block::new(BlockKind::Solid));
        world.set(source_pos.offset(0, -1, 0), Block::new(BlockKind::Solid));
        let block = world.place(kind, source_pos);
        if kind == BlockKind::Lever {
            block.support_offset = Some(Pos::new(1, 0, 0));
            block.powered = Some(false);
        } else if kind == BlockKind::RedstoneTorch {
            block.support_offset = Some(Pos::new(0, -1, 0));
            block.facing = Some(Facing::Up);
        }
        let world = ValidatedWorld::try_from(world).unwrap();
        let result = check_blueprint_connection(
            &catalog,
            &world,
            BlueprintEndpoint {
                revision: &source_id,
                port: "out",
                origin: source_pos,
                rotation: Default::default(),
            },
            BlueprintEndpoint {
                revision: &sink_id,
                port: "a",
                origin: sink_pos,
                rotation: Default::default(),
            },
            &[source_pos, sink_pos],
        );
        assert_eq!(
            result.is_ok(),
            accepted,
            "{kind:?} to {sink_pos:?}: {result:?}"
        );
    }
}
