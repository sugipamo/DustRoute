//! These are model/regression assertions, not retained Vanilla observations.
use super::*;
use crate::{BlockKind, CapabilityLevel, ObservationClassification};

const ROOT: Pos = Pos::new(0, 1, 0);
const WATER: Pos = Pos::new(1, 0, 0);
fn region() -> Region {
    Region::new(Pos::new(-8, -4, -8), Pos::new(8, 8, 8))
}
fn native(name: &str, properties: &[(&str, &str)]) -> Block {
    let (kind, classification) = crate::physical::classify(name);
    let mut b = Block::new(kind);
    b.observed_name = Some(format!("minecraft:{name}"));
    b.observation_classification = classification;
    b.observed_properties = properties
        .iter()
        .map(|(k, v)| (k.to_string(), v.to_string()))
        .collect();
    b
}
fn cane(age: &str) -> Block {
    native("sugar_cane", &[("age", age)])
}
fn scene() -> World {
    let mut world = World::new();
    for side in [
        Facing::Down,
        Facing::North,
        Facing::East,
        Facing::South,
        Facing::West,
    ] {
        let d = side.offset();
        world.place(BlockKind::Solid, WATER.offset(d.x, d.y, d.z));
    }
    world.set(Pos::new(0, 0, 0), native("dirt", &[]));
    world.set(WATER, native("water", &[("level", "0")]));
    for y in 1..=3 {
        world.set(Pos::new(0, y, 0), cane("0"));
    }
    world
}
fn settled(world: World) -> ElectricalPistonRuntime {
    let mut run = new_piston_runtime(world, region(), Default::default()).unwrap();
    run.run_until_idle().unwrap();
    run
}
fn enqueue(run: &mut ElectricalPistonRuntime, pos: Pos, payload: PistonEvent) {
    run.0
        .enqueue(QueueRequest::External {
            game_tick: 0,
            call: call(pos, payload),
        })
        .unwrap();
}

#[test]
fn complete_native_environment_is_partial_geometry_not_a_glass_or_solid_fallback() {
    let world = scene();
    let run = settled(world.clone());
    assert_eq!(run.view().world(), &world);
    let water = world.get(WATER).unwrap();
    let plant = world.get(ROOT).unwrap();
    assert!(!crate::piston_electrical::conducts(water));
    for side in crate::piston_electrical::SIDES {
        assert!(!crate::piston_electrical::full_face(water, side));
        assert!(!crate::piston_electrical::full_face(plant, side));
    }
    assert_eq!(plant.capabilities().temporal, CapabilityLevel::Unsupported);
    assert_eq!(run.view().observe_location(WATER).unwrap().block(), water);
    assert!(crate::ValidatedWorld::try_from(world).is_err());
}

#[test]
fn strict_age_and_source_water_state_reject_incomplete_unknown_and_flowing_records() {
    for age in ["", "16", "-1", "01", "+1"] {
        let mut world = scene();
        world.set(ROOT, cane(age));
        assert!(
            new_piston_runtime(world, region(), Default::default()).is_err(),
            "{age}"
        );
    }
    for age in ["0", "7", "15"] {
        let mut w = scene();
        w.set(ROOT, cane(age));
        settled(w);
    }
    for level in ["1", "8", "15", "00"] {
        let mut world = scene();
        world.set(WATER, native("water", &[("level", level)]));
        assert!(new_piston_runtime(world, region(), Default::default()).is_err());
    }
    for (pos, block) in [
        (ROOT, native("sugar_cane", &[])),
        (
            ROOT,
            native("sugar_cane", &[("age", "0"), ("extra", "false")]),
        ),
        (WATER, native("water", &[])),
        (WATER, native("lava", &[("level", "0")])),
    ] {
        let mut w = scene();
        w.set(pos, block);
        assert!(new_piston_runtime(w, region(), Default::default()).is_err());
    }
    let mut w = scene();
    let b = w.get_mut(ROOT).unwrap();
    b.observed_name = Some("other:sugar_cane".into());
    b.observation_classification = ObservationClassification::Coarse;
    assert!(new_piston_runtime(w, region(), Default::default()).is_err());
}

#[test]
fn a_root_requires_known_declared_soil_and_adjacent_contained_water() {
    for soil in ["dirt", "coarse_dirt", "rooted_dirt"] {
        let mut w = scene();
        w.set(Pos::new(0, 0, 0), native(soil, &[]));
        settled(w);
    }
    let mut w = scene();
    w.set(Pos::new(0, 0, 0), Block::new(BlockKind::Solid));
    assert!(new_piston_runtime(w, region(), Default::default()).is_err());
    let mut w = scene();
    w.set(WATER, Block::new(BlockKind::Solid));
    assert!(new_piston_runtime(w, region(), Default::default()).is_err());
    let mut w = scene();
    w.set(WATER.offset(1, 0, 0), Block::new(BlockKind::Air));
    assert!(new_piston_runtime(w, region(), Default::default()).is_err());
    assert!(
        new_piston_runtime(
            scene(),
            Region::new(Pos::new(-1, 0, 0), Pos::new(1, 3, 0)),
            Default::default()
        )
        .is_err()
    );
}

#[test]
fn cutting_the_middle_keeps_the_root_and_removes_the_top_at_the_next_tick() {
    let mut run = settled(scene());
    run.remove_now(ROOT.offset(0, 1, 0)).unwrap();
    let mut checkpoint = None;
    let mut behavior = None;
    while run.microstep().unwrap().is_some() {
        if checkpoint.is_none()
            && run.view().time().game_tick == 0
            && run.at_input_boundary()
            && run.pending_count() > 0
        {
            assert_eq!(run.view().block(ROOT.offset(0, 2, 0)).unwrap(), cane("0"));
            checkpoint = Some(run.checkpoint());
            behavior = Some(run.behavior_state().unwrap());
        }
    }
    assert_eq!(run.view().block(ROOT).unwrap(), cane("0"));
    assert_eq!(
        run.view().block(ROOT.offset(0, 2, 0)).unwrap().kind,
        BlockKind::Air
    );
    let removed = run
        .trace()
        .iter()
        .find(|r| matches!(r.invocation.call.payload, PistonEvent::SupportTick))
        .unwrap();
    assert_eq!(removed.invocation.time.game_tick, 1);
    let mut replay = ElectricalPistonRuntime::from_checkpoint(&checkpoint.unwrap()).unwrap();
    replay.run_until_idle().unwrap();
    assert_eq!(replay.state_key(), run.state_key());
    let mut explored = ElectricalPistonRuntime::from_behavior_state(&behavior.unwrap()).unwrap();
    explored.run_until_idle().unwrap();
    assert_eq!(
        explored.behavior_state().unwrap(),
        run.behavior_state().unwrap()
    );
}

#[test]
fn removing_the_root_cascades_upward_one_scheduled_tick_at_a_time() {
    let mut run = settled(scene());
    run.remove_now(ROOT).unwrap();
    run.run_until_idle().unwrap();
    let ticks = run
        .trace()
        .iter()
        .filter(|r| matches!(r.invocation.call.payload, PistonEvent::SupportTick))
        .map(|r| (r.invocation.call.target, r.invocation.time.game_tick))
        .collect::<Vec<_>>();
    assert_eq!(
        ticks,
        [(ROOT.offset(0, 1, 0), 1), (ROOT.offset(0, 2, 0), 2)]
    );
    assert_eq!(
        run.view().block(WATER).unwrap(),
        native("water", &[("level", "0")])
    );
}

#[test]
fn restored_support_before_delivery_preserves_the_column_and_age() {
    let mut run = settled(scene());
    enqueue(&mut run, ROOT, PistonEvent::ElectricalRemove);
    enqueue(
        &mut run,
        ROOT,
        PistonEvent::ElectricalInstall {
            block: Box::new(cane("7")),
        },
    );
    run.run_until_idle().unwrap();
    assert_eq!(run.view().block(ROOT).unwrap(), cane("7"));
    assert_eq!(run.view().block(ROOT.offset(0, 1, 0)).unwrap(), cane("0"));
    assert_eq!(
        run.trace()
            .iter()
            .filter(|r| matches!(r.invocation.call.payload, PistonEvent::SupportTick))
            .count(),
        1
    );
}

#[test]
fn replaced_plant_cannot_be_removed_by_a_stale_support_tick() {
    let mut run = settled(scene());
    enqueue(&mut run, ROOT, PistonEvent::ElectricalRemove);
    enqueue(
        &mut run,
        ROOT.offset(0, 1, 0),
        PistonEvent::ElectricalRemove,
    );
    enqueue(
        &mut run,
        ROOT.offset(0, 1, 0),
        PistonEvent::ElectricalInstall {
            block: Box::new(Block::new(BlockKind::Solid)),
        },
    );
    run.run_until_idle().unwrap();
    assert_eq!(
        run.view().block(ROOT.offset(0, 1, 0)).unwrap().kind,
        BlockKind::Solid
    );
    assert!(run.trace().iter().any(|r| matches!(
        r.invocation.call.payload,
        PistonEvent::SupportTick
    ) && r.result == DeliveryResult::BlockReplaced));
}

#[test]
fn fluid_boundary_changes_stop_before_committing_the_unmodeled_write() {
    for pos in [WATER, WATER.offset(1, 0, 0), WATER.offset(0, -1, 0)] {
        let mut run = settled(scene());
        let before = run.view().world().clone();
        run.remove_now(pos).unwrap();
        assert!(run.run_until_idle().is_err());
        assert_eq!(run.view().world(), &before);
    }
}

#[test]
fn introducing_a_plant_without_known_support_is_rejected_before_insertion() {
    let mut run = settled(scene());
    let before = run.view().world().clone();
    run.install_now(Pos::new(4, 4, 4), cane("0")).unwrap();
    assert!(run.run_until_idle().is_err());
    assert_eq!(run.view().world(), &before);
}

#[test]
fn a_piston_bar_breaks_the_middle_and_cascades_the_top_without_moving_the_root() {
    let mut world = scene();
    let base = Pos::new(-2, 2, 0);
    world.place(BlockKind::Piston, base).facing = Some(Facing::East);
    world.place(BlockKind::Solid, Pos::new(-1, 2, 0));
    let input = Pos::new(-2, 2, -1);
    world.place(BlockKind::Solid, input.offset(0, -1, 0));
    let lever = world.place(BlockKind::Lever, input);
    lever.powered = Some(false);
    lever.facing = Some(Facing::East);
    let mut run = settled(world);
    schedule_electrical_input_after_tick(&mut run, 1, input, true).unwrap();
    run.run_until_idle().unwrap();
    assert_eq!(run.view().block(ROOT).unwrap(), cane("0"));
    assert_eq!(
        run.view().block(ROOT.offset(0, 1, 0)).unwrap().kind,
        BlockKind::Solid
    );
    assert_eq!(
        run.view().block(ROOT.offset(0, 2, 0)).unwrap().kind,
        BlockKind::Air
    );
}
