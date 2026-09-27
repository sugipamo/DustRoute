//! Compare current route checks with actual weak power in the same snapshot.
//! This diagnostic does not modify production rules or establish live
//! Minecraft evidence. A passing route alone does not prove behavior.
use std::collections::BTreeMap;

use dustroute_library::PortDirection;
use dustroute_library::assembly::{Assembly, AssemblyConnection, AssemblyPortRef};
use dustroute_library::blueprint::{
    BlueprintCatalog, BlueprintInclusion, BlueprintPort, BlueprintPortKind, BlueprintRevision,
    BlueprintRevisionId, InstanceId, PositionedBlock,
};
use dustroute_library::builtin_blueprints::{TERMINAL_REVISION, builtin_blueprints};
use dustroute_translate::assembly::validate_assembly;
use dustroute_translate::electrical::{DeviceOutputState, solve_instantaneous};
use dustroute_translate::promotion::review_assembly;
use dustroute_translate::{
    Block, BlockKind, Facing, Pos, Region, RotationY, WireConnection, World,
};

fn terminal(name: &str, block: Block, port: BlueprintPort) -> BlueprintRevision {
    let mut revision = builtin_blueprints()
        .revision(&BlueprintRevisionId::new(TERMINAL_REVISION).unwrap())
        .unwrap()
        .clone();
    revision.id = BlueprintRevisionId::new(format!("audit.{name}.v1")).unwrap();
    revision.name = name.into();
    revision.classifications.clear();
    revision.blocks = vec![PositionedBlock {
        position: port.position,
        block,
    }];
    revision.ports = vec![port];
    revision
}

fn main() {
    let dust = Pos::new(0, 1, 0);
    let receiver = Pos::new(1, 1, 0);
    let mut wire = Block::new(BlockKind::RedstoneWire);
    wire.support_offset = Some(Pos::new(0, -1, 0));
    wire.power_level = Some(0);
    wire.wire_connections = Some(BTreeMap::from([
        (Facing::North, WireConnection::Side),
        (Facing::East, WireConnection::None),
        (Facing::South, WireConnection::Side),
        (Facing::West, WireConnection::None),
    ]));
    let source = terminal(
        "wire",
        wire.clone(),
        BlueprintPort {
            name: "out".into(),
            direction: PortDirection::Output,
            position: dust,
            kind: BlueprintPortKind::Wire,
            facing: None,
            required_source_types: vec![],
        },
    );
    let sink = terminal(
        "receiver",
        Block::new(BlockKind::Solid),
        BlueprintPort {
            name: "in".into(),
            direction: PortDirection::Input,
            position: Pos::default(),
            kind: BlueprintPortKind::BlockPower,
            facing: Some(Facing::West),
            required_source_types: vec![],
        },
    );
    let source_id = source.id.clone();
    let sink_id = sink.id.clone();
    let mut catalog = BlueprintCatalog::default();
    catalog.insert_revisions(vec![source, sink]).unwrap();
    let source_instance = InstanceId::new("source").unwrap();
    let sink_instance = InstanceId::new("sink").unwrap();
    for rotation in [
        RotationY::R0,
        RotationY::R90,
        RotationY::R180,
        RotationY::R270,
    ] {
        for arm in [false, true] {
            let mut world = World::new();
            world.set(Pos::new(0, 0, 0), Block::new(BlockKind::Solid));
            world.set(Pos::new(0, 1, -1), Block::new(BlockKind::RedstoneBlock));
            world.set(receiver, Block::new(BlockKind::Solid));
            let mut actual_wire = wire.clone();
            if arm {
                actual_wire
                    .wire_connections
                    .as_mut()
                    .unwrap()
                    .insert(Facing::East, WireConnection::Side);
            }
            world.set(dust, actual_wire);
            let assembly = Assembly {
                name: "wire-to-block route diagnostic".into(),
                instances: vec![
                    BlueprintInclusion {
                        instance: source_instance.clone(),
                        revision: source_id.clone(),
                        origin: Pos::default(),
                        rotation,
                    },
                    BlueprintInclusion {
                        instance: sink_instance.clone(),
                        revision: sink_id.clone(),
                        origin: rotation.pos(receiver),
                        rotation,
                    },
                ],
                blocks: world
                    .iter()
                    .map(|(position, block)| PositionedBlock {
                        position: rotation.pos(*position),
                        block: rotation.block(block),
                    })
                    .collect(),
                // Explicitly complete synthetic fixture, not inferred coverage.
                known_regions: vec![Region::new(Pos::new(-3, -3, -3), Pos::new(3, 3, 3))],
                connections: vec![AssemblyConnection {
                    source: AssemblyPortRef {
                        instance: vec![source_instance.clone()],
                        port: "out".into(),
                    },
                    sink: AssemblyPortRef {
                        instance: vec![sink_instance.clone()],
                        port: "in".into(),
                    },
                    path: vec![rotation.pos(dust), rotation.pos(receiver)],
                }],
                boundaries: vec![],
            };
            let snapshot = assembly.inspect(&catalog).unwrap().proposed_world();
            let solved = solve_instantaneous(&snapshot, &DeviceOutputState::default(), 128)
                .expect("finite electrical solve");
            assert_eq!(solved.signal(rotation.pos(dust)), 15);
            println!(
                "rotation={rotation:?}; arm_toward_receiver={arm}; assembly_passes={}; review={:?}; wire_level={}; receiver_weak={}; receiver_strong={}",
                validate_assembly(&catalog, &assembly).is_ok(),
                review_assembly(&catalog, &assembly).unwrap().status(),
                solved.signal(rotation.pos(dust)),
                solved.power(rotation.pos(receiver)).weak,
                solved.power(rotation.pos(receiver)).strong,
            );
        }
    }
}
