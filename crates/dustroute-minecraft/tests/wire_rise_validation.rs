use dustroute_minecraft::{
    Block, BlockKind, Facing, HistoricalPlacementV1, Pos, RotationY, ValidatedWorld,
    WireConnection, World, WorldValidationIssue, wire_rise_issues,
};
use std::collections::BTreeMap;

fn rise(support: Block, arm: WireConnection) -> World {
    let mut world = World::new();
    world.set(Pos::default(), Block::new(BlockKind::Solid));
    world.set(Pos::new(1, 1, 0), support);
    world.place(BlockKind::RedstoneWire, Pos::new(1, 2, 0));
    world
        .place(BlockKind::RedstoneWire, Pos::new(0, 1, 0))
        .wire_connections = Some(BTreeMap::from([
        (Facing::East, arm),
        (Facing::West, WireConnection::Side),
    ]));
    world
}

#[test]
fn all_rotations_reject_obstructed_rises_without_rewriting_old_state_or_semantics() {
    for rotation in [
        RotationY::R0,
        RotationY::R90,
        RotationY::R180,
        RotationY::R270,
    ] {
        let original = rise(Block::new(BlockKind::Solid), WireConnection::Up);
        let mut world = World::new();
        for (pos, block) in original.iter() {
            world.set(rotation.pos(*pos), rotation.block(block));
        }
        assert!(ValidatedWorld::try_from(world.clone()).is_ok());
        world.set(Pos::new(0, 2, 0), Block::new(BlockKind::Solid));
        let before = world.clone();
        let state_id = world.state_id();
        assert!(HistoricalPlacementV1::try_from(world.clone()).is_ok());
        let error = ValidatedWorld::try_from(world.clone()).unwrap_err();
        assert!(error.issues.iter().any(|issue| matches!(issue,
            WorldValidationIssue::InvalidWireConnection { position, reason, .. }
            if *position == Pos::new(0, 1, 0) && reason.contains("obstructed"))));
        assert_eq!(world, before);
        assert_eq!(world.state_id(), state_id);
    }
}

#[test]
fn unknown_clearance_is_not_air_and_known_contradictions_take_precedence() {
    let mut world = rise(Block::new(BlockKind::Solid), WireConnection::Up);
    let above = Pos::new(0, 2, 0);
    let issues = wire_rise_issues(&world, |pos| world.get(pos).cloned());
    assert!(
        matches!(&issues[..], [WorldValidationIssue::UnknownWireConnection { required_positions, .. }] if *required_positions == vec![above])
    );
    assert!(ValidatedWorld::try_from(world.clone()).is_ok()); // Complete-world API.
    world.set(Pos::new(1, 1, 0), Block::new(BlockKind::Air));
    let issues = wire_rise_issues(&world, |pos| {
        (pos != above).then(|| {
            world
                .get(pos)
                .cloned()
                .unwrap_or_else(|| Block::new(BlockKind::Air))
        })
    });
    assert!(
        matches!(&issues[..], [WorldValidationIssue::InvalidWireConnection { reason, .. }] if reason.contains("support"))
    );
    world.set(Pos::new(1, 2, 0), Block::new(BlockKind::Air));
    assert!(wire_rise_issues(&world, |pos| (pos != above).then(|| world.get(pos).cloned().unwrap_or_else(|| Block::new(BlockKind::Air)))).iter().any(|issue|
        matches!(issue, WorldValidationIssue::InvalidWireConnection { reason, .. } if reason.contains("target"))));
}

#[test]
fn top_half_side_rises_and_glass_follow_the_same_clearance_rule() {
    for (name, property, arm) in [
        (
            "minecraft:stone_slab",
            Some(("type", "top")),
            WireConnection::Side,
        ),
        (
            "minecraft:stone_stairs",
            Some(("half", "top")),
            WireConnection::Side,
        ),
        ("minecraft:glass", None, WireConnection::Up),
    ] {
        let mut support = Block::new(BlockKind::Transparent);
        support.observed_name = Some(name.into());
        if let Some((key, value)) = property {
            support.observed_properties.insert(key.into(), value.into());
        }
        if name == "minecraft:stone_slab" {
            support
                .observed_properties
                .insert("waterlogged".into(), "false".into());
        }
        let mut world = rise(support, arm);
        assert!(ValidatedWorld::try_from(world.clone()).is_ok(), "{name}");
        // Glass above the lower wire is not the solid obstruction from the audit.
        let mut glass = Block::new(BlockKind::Transparent);
        glass.observed_name = Some("minecraft:glass".into());
        world.set(Pos::new(0, 2, 0), glass);
        assert!(ValidatedWorld::try_from(world.clone()).is_ok(), "{name}");
        world.set(Pos::new(0, 2, 0), Block::new(BlockKind::Solid));
        assert!(world.placement_issues().iter().any(|issue|
            matches!(issue, WorldValidationIssue::InvalidWireConnection { reason, .. } if reason.contains("obstructed"))), "{name}");
    }
}

#[test]
fn decorative_horizontal_endpoints_and_unspecified_shapes_are_not_invented_rises() {
    let mut world = World::new();
    world.set(Pos::default(), Block::new(BlockKind::Solid));
    world.place(BlockKind::RedstoneWire, Pos::new(0, 1, 0));
    assert!(ValidatedWorld::try_from(world.clone()).is_ok());
    world.get_mut(Pos::new(0, 1, 0)).unwrap().wire_connections = Some(BTreeMap::from([
        (Facing::East, WireConnection::Side),
        (Facing::West, WireConnection::Side),
    ]));
    assert!(ValidatedWorld::try_from(world.clone()).is_ok());
    world
        .get_mut(Pos::new(0, 1, 0))
        .unwrap()
        .wire_connections
        .as_mut()
        .unwrap()
        .insert(Facing::Up, WireConnection::Side);
    assert!(ValidatedWorld::try_from(world).is_err());
}

#[test]
fn explicit_rise_overflow_is_reported_without_panicking() {
    let mut world = World::new();
    world.set(Pos::new(i32::MAX, 0, 0), Block::new(BlockKind::Solid));
    world
        .place(BlockKind::RedstoneWire, Pos::new(i32::MAX, 1, 0))
        .wire_connections = Some(BTreeMap::from([(Facing::East, WireConnection::Up)]));
    assert!(world.placement_issues().iter().any(|issue|
        matches!(issue, WorldValidationIssue::InvalidWireConnection { reason, .. } if reason.contains("coordinate bounds"))));
}
