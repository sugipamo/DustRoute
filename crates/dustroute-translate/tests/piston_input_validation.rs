use dustroute_minecraft::time::{PhysicsEngine, PhysicsEngineError};
use dustroute_minecraft::{
    Block, BlockKind, Facing, PistonError, PistonState, PistonVariant, Pos, Region, World,
};

const BODY: Pos = Pos::new(0, 1, 0);

fn world(source: Pos, powered: bool) -> World {
    let mut world = World::new();
    let piston = world.place(BlockKind::Piston, BODY);
    piston.facing = Some(Facing::East);
    piston.piston_variant = Some(PistonVariant::Normal);
    piston.piston_state = Some(PistonState::Retracted);
    world.place(BlockKind::Lever, source).powered = Some(powered);
    world
}

#[test]
fn unknown_sides_reject_both_edges_without_consuming_or_applying_the_event() {
    let sides = [Facing::North, Facing::East, Facing::South, Facing::West];
    for missing in sides {
        let offset = missing.offset();
        let mut region = Region::new(Pos::new(-2, 0, -2), Pos::new(3, 2, 2));
        match missing {
            Facing::North => region.min.z = 0,
            Facing::East => region.max.x = 0,
            Facing::South => region.max.z = 0,
            Facing::West => region.min.x = 0,
            _ => unreachable!(),
        }
        for source_direction in sides.into_iter().filter(|d| *d != missing) {
            let d = source_direction.offset();
            let source = BODY.offset(d.x, d.y, d.z);
            for powered in [false, true] {
                let world = world(source, !powered);
                let mut engine = PhysicsEngine::new_diagnostic(world.clone(), 64)
                    .with_piston_planning_region(region);
                let id = engine.schedule_redstone_input(0, source, powered);
                let error = engine.run_redstone_piston_events().unwrap_err();
                assert!(
                    matches!(error, PhysicsEngineError::Piston(e) if matches!(*e, PistonError::UnknownSpace { position } if position == BODY.offset(offset.x, offset.y, offset.z)))
                );
                assert_eq!(engine.world(), &world);
                assert_eq!(engine.pending_event_count(), 1);
                assert_eq!(engine.checkpoint().pending_events().next().unwrap().id, id);
                assert!(engine.event_trace().records.is_empty());
                assert!(engine.trace_status().is_failed());
            }
        }
    }
}

#[test]
fn known_powered_input_cannot_hide_a_later_unknown_input_state_or_shape() {
    let source = Pos::new(0, 1, -1);
    let unknown = Pos::new(0, 1, 1);
    let region = Region::new(Pos::new(-1, 0, -1), Pos::new(3, 2, 1));
    for kind in [
        BlockKind::Lever,
        BlockKind::RedstoneWire,
        BlockKind::Repeater,
    ] {
        let mut world = world(source, false);
        let mut block = Block::new(kind);
        block.observed_name = Some(
            match kind {
                BlockKind::Lever => "minecraft:lever",
                BlockKind::RedstoneWire => "minecraft:redstone_wire",
                _ => "minecraft:repeater",
            }
            .into(),
        );
        world.set(unknown, block);
        let mut engine =
            PhysicsEngine::new_diagnostic(world.clone(), 64).with_piston_planning_region(region);
        engine.schedule_redstone_input(0, source, true);
        assert!(
            matches!(engine.run_redstone_piston_events(), Err(PhysicsEngineError::Piston(e)) if matches!(*e, PistonError::UnknownInput { position, .. } if position == unknown))
        );
        assert_eq!(engine.world(), &world);
        assert_eq!(engine.pending_event_count(), 1);
        assert!(engine.event_trace().records.is_empty());
    }
}

#[test]
fn invalid_analog_inputs_do_not_become_boolean_power_or_commit_external_changes() {
    let source = Pos::new(0, 1, -1);
    let invalid = Pos::new(0, 1, 1);
    for level in [16, 255] {
        for kind in [
            BlockKind::RedstoneWire,
            BlockKind::PressurePlate,
            BlockKind::Comparator,
        ] {
            let mut world = world(source, false);
            let block = world.place(kind, invalid);
            block.facing = Some(Facing::North);
            block.power_level = Some(level);
            let mut engine = PhysicsEngine::new_diagnostic(world.clone(), 64)
                .with_piston_planning_region(Region::new(Pos::new(-2, 0, -2), Pos::new(3, 2, 2)));
            engine.schedule_redstone_input(0, source, true);
            let error = engine.run_redstone_piston_events().unwrap_err();
            assert!(error.to_string().contains("outside 0..=15"), "{error}");
            assert_eq!(engine.world(), &world);
            assert_eq!(engine.pending_event_count(), 1);
            assert!(engine.event_trace().records.is_empty());
        }
    }
}
