use dustroute_minecraft::piston_electrical::*;
use dustroute_minecraft::time::runtime::RuntimeError;
use dustroute_minecraft::{
    Block, BlockKind, Facing, PistonState, Pos, Region, WireConnection, World,
};
use std::collections::BTreeMap;

const P: Pos = Pos::new(0, 4, 0);
fn region() -> Region {
    Region::new(Pos::new(-20, -20, -20), Pos::new(20, 20, 20))
}
fn query(world: &World) -> ElectricalWorld<'_> {
    ElectricalWorld::new(world, region()).unwrap()
}
fn piston(world: &mut World, facing: Facing) {
    let b = world.place(BlockKind::Piston, P);
    b.facing = Some(facing);
    b.piston_state = Some(PistonState::Retracted);
}
fn lever(world: &mut World, pos: Pos, support: Facing, powered: bool) {
    let b = world.place(BlockKind::Lever, pos);
    b.support_offset = Some(support.offset());
    b.powered = Some(powered);
}
fn wire(world: &mut World, pos: Pos, level: u8, dot: bool) {
    let b = world.place(BlockKind::RedstoneWire, pos);
    b.power_level = Some(level);
    b.support_offset = Some(Facing::Down.offset());
    b.wire_connections = Some(
        HORIZONTAL
            .into_iter()
            .map(|d| {
                (
                    d,
                    if dot {
                        WireConnection::None
                    } else {
                        WireConnection::Side
                    },
                )
            })
            .collect(),
    );
}
fn repeater(world: &mut World, pos: Pos, output: Facing, powered: bool) {
    let b = world.place(BlockKind::Repeater, pos);
    b.powered = Some(powered);
    b.delay = Some(1);
    b.facing = Some(output);
    b.support_offset = Some(Facing::Down.offset());
}

#[test]
fn all_body_facings_accept_direct_and_quasi_sources_with_front_exclusion() {
    for facing in SIDES {
        for side in SIDES {
            let mut world = World::new();
            piston(&mut world, facing);
            world.place(BlockKind::RedstoneBlock, along(P, side).unwrap());
            assert_eq!(
                query(&world).piston_powered(P).unwrap(),
                side != facing,
                "{facing:?}/{side:?}"
            );
        }
        let mut world = World::new();
        piston(&mut world, facing);
        world.place(BlockKind::RedstoneBlock, P.offset(0, 2, 0));
        assert!(query(&world).piston_powered(P).unwrap(), "QC {facing:?}");
    }
}

#[test]
fn strong_lever_power_transfers_once_and_full_cube_does_not_imply_conduction() {
    let mut world = World::new();
    piston(&mut world, Facing::Up);
    let conductor = P.offset(1, 0, 0);
    world.place(BlockKind::Solid, conductor);
    lever(&mut world, P.offset(1, -1, 0), Facing::Up, true);
    assert!(query(&world).piston_powered(P).unwrap());
    // An adjacent redstone block emits weak, not strong; it cannot energize stone.
    world.set(P.offset(1, -1, 0), Block::new(BlockKind::RedstoneBlock));
    assert!(!query(&world).piston_powered(P).unwrap());
    // Two stone blocks do not recursively relay strong input.
    world.set(P.offset(1, -1, 0), Block::new(BlockKind::Solid));
    lever(&mut world, P.offset(1, -2, 0), Facing::Up, true);
    assert!(!query(&world).piston_powered(P).unwrap());
    assert!(!conducts(&Block::new(BlockKind::RedstoneBlock)));
    assert!(!conducts(world.get(P).unwrap()));
    assert!(full_face(world.get(P).unwrap(), Facing::Up));
}

#[test]
fn wire_vertical_emission_dot_line_and_disabled_feedback_follow_java_queries() {
    let mut world = World::new();
    wire(&mut world, P, 12, true);
    let q = query(&world);
    assert_eq!(q.emission(P, Facing::Up, true).unwrap().strong, 12);
    assert_eq!(q.emission(P, Facing::Down, true).unwrap().weak, 0);
    assert_eq!(q.emission(P, Facing::East, true).unwrap().weak, 0);
    assert_eq!(q.emission(P, Facing::Up, false).unwrap().strong, 0);
    // One north connection renders a north/south line, preserving east/west zero.
    wire(&mut world, P.offset(0, 0, -1), 13, false);
    let q = query(&world);
    assert_eq!(q.emission(P, Facing::North, true).unwrap().weak, 12);
    assert_eq!(q.emission(P, Facing::South, true).unwrap().weak, 12);
    assert_eq!(q.emission(P, Facing::East, true).unwrap().weak, 0);
    // Adjacent stored wire power propagates even if its retained arms are stale.
    assert_eq!(q.wire_power(P).unwrap(), 12);
    world.set(P.offset(0, 0, -1), Block::new(BlockKind::Air));
    world.place(BlockKind::Solid, P.offset(0, -1, 0));
    // The wire cannot re-power itself through strong input into its support.
    assert_eq!(query(&world).wire_power(P).unwrap(), 0);
}

#[test]
fn dust_receives_conductor_input_and_climbs_but_an_overhead_solid_blocks_climb() {
    let mut world = World::new();
    wire(&mut world, P, 0, false);
    world.place(BlockKind::Solid, P.offset(1, 0, 0));
    wire(&mut world, P.offset(1, 1, 0), 9, false);
    assert_eq!(query(&world).wire_power(P).unwrap(), 8);
    assert_eq!(
        query(&world).wire_shape(P).unwrap()[&Facing::East],
        WireConnection::Up
    );
    world.place(BlockKind::Solid, P.offset(0, 1, 0));
    assert_eq!(query(&world).wire_power(P).unwrap(), 0);
    lever(&mut world, P.offset(1, -1, 0), Facing::Up, true);
    assert_eq!(query(&world).wire_power(P).unwrap(), 15);
}

#[test]
fn repeater_reads_raw_rear_wire_and_only_aligned_side_gates_lock() {
    let mut world = World::new();
    repeater(&mut world, P, Facing::East, false);
    wire(&mut world, P.offset(-1, 0, 0), 7, true);
    assert!(query(&world).gate_input_powered(P).unwrap());
    world.place(BlockKind::RedstoneBlock, P.offset(0, 0, -1));
    assert!(!query(&world).side_gate_powered(P).unwrap());
    repeater(&mut world, P.offset(0, 0, -1), Facing::South, true);
    assert!(query(&world).side_gate_powered(P).unwrap());
    repeater(&mut world, P.offset(0, 0, -1), Facing::North, true);
    assert!(!query(&world).side_gate_powered(P).unwrap());
}

#[test]
fn known_power_does_not_mask_unknown_quasi_or_conductor_cells() {
    let mut world = World::new();
    piston(&mut world, Facing::East);
    world.place(BlockKind::RedstoneBlock, P.offset(-1, 0, 0));
    let region = Region::new(Pos::new(-3, 0, -3), Pos::new(3, 5, 3));
    assert!(matches!(
        ElectricalWorld::new(&world, region)
            .unwrap()
            .piston_powered(P),
        Err(RuntimeError::UnknownSpace(_))
    ));
}

#[test]
fn incomplete_or_contradictory_electrical_metadata_is_rejected() {
    let mut world = World::new();
    wire(&mut world, P, 4, false);
    let b = world.get_mut(P).unwrap();
    b.observed_name = Some("minecraft:redstone_wire".into());
    b.observed_properties = BTreeMap::from([("power".into(), "4".into())]);
    assert!(ElectricalWorld::new(&world, region()).is_err());
    for d in HORIZONTAL {
        set_observed_arm(&mut world, d);
    }
    assert!(ElectricalWorld::new(&world, region()).is_ok());
    world.get_mut(P).unwrap().power_level = Some(16);
    assert!(ElectricalWorld::new(&world, region()).is_err());
    fn set_observed_arm(world: &mut World, d: Facing) {
        let n = match d {
            Facing::North => "north",
            Facing::South => "south",
            Facing::East => "east",
            Facing::West => "west",
            _ => unreachable!(),
        };
        world
            .get_mut(P)
            .unwrap()
            .observed_properties
            .insert(n.into(), "side".into());
    }
}

#[test]
fn notification_centers_match_java_hashset_at_actual_and_negative_coordinates() {
    #[derive(serde::Deserialize)]
    struct Oracle {
        origin: Pos,
        centers: Vec<Pos>,
    }
    for line in include_str!("fixtures/java_wire_notification_order_v1.jsonl").lines() {
        let expected: Oracle = serde_json::from_str(line).unwrap();
        assert_eq!(
            wire_notification_centers(expected.origin).unwrap(),
            expected.centers
        );
    }
    let a = wire_notification_centers(Pos::new(0, 4, 0)).unwrap();
    let translated: Vec<_> = a.iter().map(|p| p.offset(4, 0, 0)).collect();
    assert_ne!(
        translated,
        wire_notification_centers(Pos::new(4, 4, 0)).unwrap()
    );
}
