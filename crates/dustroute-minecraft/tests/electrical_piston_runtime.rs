use dustroute_minecraft::piston_electrical::{ElectricalWorld, HORIZONTAL};
use dustroute_minecraft::time::piston_runtime::*;
use dustroute_minecraft::time::runtime::*;
use dustroute_minecraft::{
    BlockKind, Facing, PistonState, PistonVariant, Pos, Region, WireConnection, World,
};

fn region() -> Region {
    Region::new(Pos::new(-6, -2, -5), Pos::new(22, 12, 5))
}
fn piston(world: &mut World, pos: Pos, facing: Facing) {
    let b = world.place(BlockKind::Piston, pos);
    b.facing = Some(facing);
    b.piston_state = Some(PistonState::Retracted);
    b.piston_variant = Some(PistonVariant::Sticky);
    let d = facing.offset();
    world.place(BlockKind::Solid, pos.offset(d.x, d.y, d.z));
}
fn lever(world: &mut World, pos: Pos, support: Facing) {
    let d = support.offset();
    world.place(BlockKind::Solid, pos.offset(d.x, d.y, d.z));
    let b = world.place(BlockKind::Lever, pos);
    b.support_offset = Some(d);
    b.powered = Some(false);
}
fn wire(world: &mut World, pos: Pos) {
    world.place(BlockKind::Solid, pos.offset(0, -1, 0));
    let b = world.place(BlockKind::RedstoneWire, pos);
    b.support_offset = Some(Facing::Down.offset());
    b.power_level = Some(0);
    b.wire_connections = Some(
        HORIZONTAL
            .into_iter()
            .map(|d| (d, WireConnection::Side))
            .collect(),
    );
}
fn repeater(world: &mut World, pos: Pos, delay: u8) {
    world.place(BlockKind::Solid, pos.offset(0, -1, 0));
    let b = world.place(BlockKind::Repeater, pos);
    b.support_offset = Some(Facing::Down.offset());
    b.facing = Some(Facing::East);
    b.powered = Some(false);
    b.delay = Some(delay);
}
fn shapes(world: &mut World) {
    let updated: Vec<_> = world
        .iter()
        .filter(|(_, b)| b.kind == BlockKind::RedstoneWire)
        .map(|(p, _)| {
            (
                *p,
                ElectricalWorld::new(world, region())
                    .unwrap()
                    .wire_shape(*p)
                    .unwrap(),
            )
        })
        .collect();
    for (pos, shape) in updated {
        world.get_mut(pos).unwrap().wire_connections = Some(shape);
    }
}
fn run(world: World) -> ElectricalPistonRuntime {
    new_piston_runtime(world, region(), RuntimeLimits::default()).unwrap()
}

#[test]
fn dust_repeater_and_conductor_drive_mixed_facings_in_one_world() {
    let mut world = World::new();
    let bodies = [Pos::new(0, 4, 0), Pos::new(8, 4, 0), Pos::new(16, 4, 0)];
    for (pos, direction) in bodies
        .into_iter()
        .zip([Facing::East, Facing::Up, Facing::Down])
    {
        piston(&mut world, pos, direction);
    }
    wire(&mut world, bodies[0].offset(-1, 0, 0));
    let a = bodies[0].offset(-2, 0, 0);
    lever(&mut world, a, Facing::Down);
    repeater(&mut world, bodies[1].offset(-1, 0, 0), 1);
    wire(&mut world, bodies[1].offset(-2, 0, 0));
    let b = bodies[1].offset(-3, 0, 0);
    lever(&mut world, b, Facing::Down);
    let c = bodies[2].offset(-2, 0, 0);
    lever(&mut world, c, Facing::East);
    shapes(&mut world);
    let mut rt = run(world);
    rt.run_until_idle().unwrap();
    for input in [a, b, c] {
        rt.input_now(input, true).unwrap();
        rt.step().unwrap();
    }
    rt.run_until_idle().unwrap();
    for body in bodies {
        assert_eq!(
            rt.view().block(body).unwrap().piston_state,
            Some(PistonState::Extended)
        );
    }
    for input in [a, b, c] {
        rt.input_now(input, false).unwrap();
        rt.step().unwrap();
    }
    rt.run_until_idle().unwrap();
    for body in bodies {
        assert_eq!(
            rt.view().block(body).unwrap().piston_state,
            Some(PistonState::Retracted)
        );
    }
    assert_eq!(rt.view().adapter_revision(), ELECTRICAL_PROFILE);
}

#[test]
fn short_repeater_pulses_survive_input_loss_at_each_delay() {
    for delay in 1..=4 {
        let mut world = World::new();
        let body = Pos::new(0, 4, 0);
        piston(&mut world, body, Facing::Up);
        let rep = body.offset(-1, 0, 0);
        repeater(&mut world, rep, delay);
        let input = body.offset(-2, 0, 0);
        lever(&mut world, input, Facing::Down);
        let mut rt = run(world);
        schedule_electrical_input(&mut rt, 1, input, true).unwrap();
        schedule_electrical_input(&mut rt, 2, input, false).unwrap();
        rt.run_until_idle().unwrap();
        let transitions: Vec<_> = rt
            .trace()
            .iter()
            .flat_map(|r| {
                r.delta
                    .iter()
                    .flat_map(|d| &d.changes)
                    .filter(move |c| c.position == rep)
                    .map(move |c| (r.invocation.time.game_tick, c.after.powered))
            })
            .collect();
        assert_eq!(
            transitions,
            vec![
                (1 + u64::from(delay) * 2, Some(true)),
                (1 + u64::from(delay) * 4, Some(false))
            ]
        );
        assert_eq!(
            rt.view().block(body).unwrap().piston_state,
            Some(PistonState::Retracted)
        );
    }
}

#[test]
fn quasi_connectivity_waits_for_a_real_neighbor_callback() {
    let mut world = World::new();
    let body = Pos::new(0, 4, 0);
    piston(&mut world, body, Facing::Down);
    let quasi = body.offset(1, 1, 0);
    lever(&mut world, quasi, Facing::Up);
    let direct = body.offset(0, 0, -1);
    lever(&mut world, direct, Facing::Down);
    let mut rt = run(world);
    rt.run_until_idle().unwrap();
    rt.input_now(quasi, true).unwrap();
    rt.run_until_idle().unwrap();
    assert_eq!(
        rt.view().block(body).unwrap().piston_state,
        Some(PistonState::Retracted)
    );
    rt.input_now(direct, true).unwrap();
    rt.run_until_idle().unwrap();
    rt.input_now(direct, false).unwrap();
    rt.run_until_idle().unwrap();
    assert_eq!(
        rt.view().block(body).unwrap().piston_state,
        Some(PistonState::Extended)
    );
    rt.input_now(quasi, false).unwrap();
    rt.run_until_idle().unwrap();
    assert_eq!(
        rt.view().block(body).unwrap().piston_state,
        Some(PistonState::Extended)
    );
    rt.input_now(direct, true).unwrap();
    rt.run_until_idle().unwrap();
    rt.input_now(direct, false).unwrap();
    rt.run_until_idle().unwrap();
    assert_eq!(
        rt.view().block(body).unwrap().piston_state,
        Some(PistonState::Retracted)
    );
}

#[test]
fn pending_electrical_callbacks_resume_with_identical_future() {
    let mut world = World::new();
    let rep = Pos::new(0, 4, 0);
    repeater(&mut world, rep, 2);
    let input = rep.offset(-1, 0, 0);
    lever(&mut world, input, Facing::Down);
    let mut rt = run(world);
    rt.run_until_idle().unwrap();
    rt.input_now(input, true).unwrap();
    rt.step().unwrap();
    let checkpoint = rt.checkpoint();
    let behavior = rt.behavior_state().unwrap();
    let mut exact = ElectricalPistonRuntime::from_checkpoint(&checkpoint).unwrap();
    let mut resumed = ElectricalPistonRuntime::from_behavior_state(&behavior).unwrap();
    rt.run_until_idle().unwrap();
    exact.run_until_idle().unwrap();
    resumed.run_until_idle().unwrap();
    assert_eq!(rt.state_key(), exact.state_key());
    assert_eq!(rt.view().world(), resumed.view().world());
    assert_eq!(
        rt.behavior_state().unwrap(),
        resumed.behavior_state().unwrap()
    );
}

#[test]
fn moving_a_power_source_updates_dust_shape_and_power_then_restores_them() {
    let mut world = World::new();
    let body = Pos::new(0, 4, 0);
    piston(&mut world, body, Facing::East);
    world.place(BlockKind::RedstoneBlock, body.offset(1, 0, 0));
    let input = body.offset(0, 0, 1);
    lever(&mut world, input, Facing::Down);
    let dust = body.offset(1, 0, -1);
    wire(&mut world, dust);
    shapes(&mut world);
    let mut rt = run(world);
    rt.run_until_idle().unwrap();
    let original = rt.view().block(dust).unwrap();
    assert_eq!(original.power_level, Some(15));
    assert_eq!(
        original.wire_connections.as_ref().unwrap()[&Facing::East],
        WireConnection::None
    );
    rt.input_now(input, true).unwrap();
    rt.run_until_idle().unwrap();
    let moved = rt.view().block(dust).unwrap();
    assert_eq!(moved.power_level, Some(0));
    assert!(
        moved
            .wire_connections
            .as_ref()
            .unwrap()
            .values()
            .all(|c| *c == WireConnection::Side)
    );
    rt.input_now(input, false).unwrap();
    rt.run_until_idle().unwrap();
    assert_eq!(rt.view().block(dust).unwrap(), original);
}

#[test]
fn locking_at_delivery_suppresses_a_pending_tick_and_unlocking_requests_another() {
    let mut world = World::new();
    let main = Pos::new(0, 4, 0);
    repeater(&mut world, main, 2);
    let input = main.offset(-1, 0, 0);
    lever(&mut world, input, Facing::Down);
    let lock = main.offset(0, 0, -1);
    repeater(&mut world, lock, 1);
    world.get_mut(lock).unwrap().facing = Some(Facing::South);
    let lock_input = lock.offset(0, 0, -1);
    lever(&mut world, lock_input, Facing::Down);
    let mut rt = run(world);
    schedule_electrical_input(&mut rt, 1, input, true).unwrap();
    schedule_electrical_input(&mut rt, 1, lock_input, true).unwrap();
    schedule_electrical_input(&mut rt, 6, lock_input, false).unwrap();
    rt.run_until_idle().unwrap();
    let changes: Vec<_> = rt
        .trace()
        .iter()
        .flat_map(|r| {
            r.delta
                .iter()
                .flat_map(|d| &d.changes)
                .filter(move |c| c.position == main)
                .map(move |c| {
                    (
                        r.invocation.time.game_tick,
                        c.after.powered,
                        c.after.observed_properties.get("locked").cloned(),
                    )
                })
        })
        .collect();
    assert_eq!(
        changes,
        vec![
            (3, Some(false), Some("true".into())),
            (8, Some(false), Some("false".into())),
            (12, Some(true), Some("false".into()))
        ]
    );
}
