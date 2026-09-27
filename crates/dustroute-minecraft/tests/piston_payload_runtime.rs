//! Movable piston bodies share the normal world queue and carrier lifecycle.
use dustroute_minecraft::time::piston_runtime::{
    ElectricalPistonRuntime, PistonEvent, new_piston_runtime, schedule_electrical_input_after_tick,
};
use dustroute_minecraft::time::runtime::{DeliveryResult, RuntimeLimits};
use dustroute_minecraft::{
    Block, BlockKind, Facing, PistonState, PistonVariant, Pos, Region, World,
};

const ZERO: Pos = Pos::new(0, 0, 0);
const DIRECTIONS: [Facing; 6] = [
    Facing::Down,
    Facing::Up,
    Facing::North,
    Facing::South,
    Facing::West,
    Facing::East,
];

fn along(p: Pos, d: Facing, n: i32) -> Pos {
    let v = d.offset();
    Pos::new(p.x + n * v.x, p.y + n * v.y, p.z + n * v.z)
}

fn body(d: Facing, sticky: bool) -> Block {
    let mut b = Block::new(BlockKind::Piston);
    b.facing = Some(d);
    b.piston_variant = Some(if sticky {
        PistonVariant::Sticky
    } else {
        PistonVariant::Normal
    });
    b.piston_state = Some(PistonState::Retracted);
    b
}

fn lever(world: &mut World, p: Pos, support: Facing) {
    world.place(BlockKind::Solid, along(p, support, 1));
    let b = world.place(BlockKind::Lever, p);
    b.powered = Some(false);
    b.support_offset = Some(support.offset());
}

fn driver(world: &mut World, d: Facing, sticky: bool) -> Pos {
    world.set(ZERO, body(d, sticky));
    let input = along(
        ZERO,
        if matches!(d, Facing::East | Facing::West) {
            Facing::North
        } else {
            Facing::East
        },
        1,
    );
    lever(
        world,
        input,
        if d == Facing::Down {
            Facing::Up
        } else {
            Facing::Down
        },
    );
    input
}

fn runtime(world: World) -> ElectricalPistonRuntime {
    new_piston_runtime(
        world,
        Region::new(Pos::new(-17, -17, -17), Pos::new(17, 17, 17)),
        RuntimeLimits::default(),
    )
    .unwrap()
}

#[test]
fn all_body_facings_and_variants_can_be_pushed_and_pulled_in_every_direction() {
    for movement in DIRECTIONS {
        for facing in DIRECTIONS {
            for sticky in [false, true] {
                let mut world = World::new();
                let input = driver(&mut world, movement, true);
                let payload = body(facing, sticky);
                let origin = along(ZERO, movement, 1);
                let destination = along(ZERO, movement, 2);
                world.set(origin, payload.clone());
                let mut run = runtime(world);
                schedule_electrical_input_after_tick(&mut run, 1, input, true).unwrap();
                run.run_until_idle().unwrap();
                assert_eq!(
                    run.view().block(destination).unwrap(),
                    payload,
                    "{movement:?}/{facing:?}/{sticky}"
                );
                run.input_now(input, false).unwrap();
                run.run_until_idle().unwrap();
                assert_eq!(run.view().block(origin).unwrap(), payload);
                assert_eq!(run.view().block(destination).unwrap().kind, BlockKind::Air);
            }
        }
    }
}

#[test]
fn mixed_material_and_multiple_piston_chains_obey_the_twelve_block_limit() {
    for count in [3, 12, 13] {
        let mut world = World::new();
        let input = driver(&mut world, Facing::Up, false);
        for n in 1..=count {
            world.set(
                along(ZERO, Facing::Up, n),
                if n % 3 == 0 {
                    Block::new(BlockKind::Solid)
                } else {
                    body(
                        if n % 2 == 0 { Facing::Down } else { Facing::Up },
                        n % 2 == 0,
                    )
                },
            );
        }
        let original = world.clone();
        let mut run = runtime(world);
        schedule_electrical_input_after_tick(&mut run, 1, input, true).unwrap();
        run.run_until_idle().unwrap();
        assert_eq!(
            run.view().block(ZERO).unwrap().piston_state,
            Some(if count <= 12 {
                PistonState::Extended
            } else {
                PistonState::Retracted
            })
        );
        for n in 1..=count {
            assert_eq!(
                run.view()
                    .block(along(ZERO, Facing::Up, n + i32::from(count <= 12)))
                    .unwrap(),
                *original.get(along(ZERO, Facing::Up, n)).unwrap()
            );
        }
    }
}

#[test]
fn moving_a_powered_retracted_body_invalidates_its_old_queued_event() {
    let mut world = World::new();
    let input = driver(&mut world, Facing::East, false);
    world.set(Pos::new(1, 0, 0), body(Facing::Up, true));
    let second = Pos::new(1, 0, 1);
    lever(&mut world, second, Facing::Down);
    let mut run = runtime(world);
    schedule_electrical_input_after_tick(&mut run, 1, input, true).unwrap();
    schedule_electrical_input_after_tick(&mut run, 1, second, true).unwrap();
    run.run_until_idle().unwrap();
    assert_eq!(
        run.view().block(Pos::new(2, 0, 0)).unwrap(),
        body(Facing::Up, true)
    );
    assert!(
        run.trace()
            .iter()
            .any(|r| r.invocation.call.target == Pos::new(1, 0, 0)
                && matches!(r.invocation.call.payload, PistonEvent::Block { .. })
                && r.result != DeliveryResult::Executed)
    );
}

#[test]
fn a_moved_body_activates_from_power_at_its_new_position() {
    let mut world = World::new();
    let input = driver(&mut world, Facing::East, false);
    world.set(Pos::new(1, 0, 0), body(Facing::Up, true));
    world.place(BlockKind::RedstoneBlock, Pos::new(3, 0, 0));
    world.place(BlockKind::Solid, Pos::new(2, 1, 0));
    let mut run = runtime(world);
    schedule_electrical_input_after_tick(&mut run, 1, input, true).unwrap();
    run.run_until_idle().unwrap();
    assert_eq!(
        run.view().block(Pos::new(2, 0, 0)).unwrap().piston_state,
        Some(PistonState::Extended)
    );
    assert_eq!(
        run.view().block(Pos::new(2, 2, 0)).unwrap().kind,
        BlockKind::Solid
    );
}

#[test]
fn an_extended_body_remains_a_physical_obstruction() {
    let mut world = World::new();
    let input = driver(&mut world, Facing::East, false);
    let mut obstacle = body(Facing::Up, true);
    obstacle.piston_state = Some(PistonState::Extended);
    world.set(Pos::new(1, 0, 0), obstacle.clone());
    world.set(
        Pos::new(1, 1, 0),
        Block::piston_head(Facing::Up, PistonVariant::Sticky, false),
    );
    world.place(BlockKind::RedstoneBlock, Pos::new(2, 0, 0));
    let mut run = runtime(world);
    schedule_electrical_input_after_tick(&mut run, 1, input, true).unwrap();
    run.run_until_idle().unwrap();
    assert_eq!(
        run.view().block(ZERO).unwrap().piston_state,
        Some(PistonState::Retracted)
    );
    assert_eq!(run.view().block(Pos::new(1, 0, 0)).unwrap(), obstacle);
}

#[test]
fn double_extender_recovers_its_payload_and_resumes_during_body_movement() {
    let mut world = World::new();
    world.set(ZERO, body(Facing::Up, true));
    world.set(Pos::new(0, 1, 0), body(Facing::Up, true));
    world.place(BlockKind::Solid, Pos::new(0, 2, 0));
    let a = Pos::new(0, 0, -1);
    let high = Pos::new(1, 2, 0);
    let low = Pos::new(-1, 1, 0);
    for p in [a, high, low] {
        lever(&mut world, p, Facing::Down);
    }
    let original = world.clone();
    let mut run = runtime(world);
    for (tick, input, power) in [
        (1, a, true),
        (9, high, true),
        (17, high, false),
        (25, a, false),
        (33, low, true),
        (41, low, false),
    ] {
        schedule_electrical_input_after_tick(&mut run, tick, input, power).unwrap();
    }
    let mut checkpoint = None;
    let mut behavior = None;
    while run.microstep().unwrap().is_some() {
        let moving_body = run.view().world().iter().any(|(_, b)| {
            b.piston_entity
                .as_ref()
                .is_some_and(|e| !e.source && e.pushed_block.kind == BlockKind::Piston)
        });
        if moving_body && checkpoint.is_none() && !run.at_input_boundary() {
            checkpoint = Some(run.checkpoint());
        }
        if moving_body && behavior.is_none() && run.at_input_boundary() {
            behavior = Some(run.behavior_state().unwrap());
        }
    }
    assert_eq!(run.view().world(), &original);
    let mut exact = ElectricalPistonRuntime::from_checkpoint(&checkpoint.unwrap()).unwrap();
    exact.run_until_idle().unwrap();
    assert_eq!(exact.state_key(), run.state_key());
    let mut representative =
        ElectricalPistonRuntime::from_behavior_state(&behavior.unwrap()).unwrap();
    representative.run_until_idle().unwrap();
    assert_eq!(
        representative.behavior_state().unwrap(),
        run.behavior_state().unwrap()
    );
}
