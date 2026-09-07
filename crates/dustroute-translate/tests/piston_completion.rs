use dustroute_minecraft::time::PhysicsEngine;
use dustroute_minecraft::{
    Block, BlockKind, Facing, PistonAction, PistonState, PistonVariant, Pos, World, piston_state,
    plan_piston,
};
fn world() -> World {
    let mut world = World::new();
    for z in [0, 10] {
        let mut piston = Block::new(BlockKind::Piston);
        piston.facing = Some(Facing::East);
        piston.piston_variant = Some(PistonVariant::Sticky);
        world.set(Pos::new(0, 1, z), piston);
        world.set(Pos::new(1, 1, z), Block::new(BlockKind::Solid));
    }
    let mut lever = Block::new(BlockKind::Lever);
    lever.powered = Some(false);
    world.set(Pos::new(20, 1, 20), lever);
    world
}

#[test]
fn independent_overlapping_motion_completes_in_both_orders() {
    for action in [PistonAction::Extend, PistonAction::Retract] {
        for gap in 0..=3 {
            for order in [[0, 10], [10, 0]] {
                let mut w = world();
                if action == PistonAction::Retract {
                    for z in [0, 10] {
                        plan_piston(&w, Pos::new(0, 1, z), PistonAction::Extend)
                            .unwrap()
                            .apply(&mut w)
                            .unwrap();
                    }
                }
                let mut e = PhysicsEngine::new_diagnostic(w, 64);
                for (i, z) in order.into_iter().enumerate() {
                    e.schedule_piston_action(
                        if i == 0 { 0 } else { gap },
                        Pos::new(0, 1, z),
                        action,
                    );
                }
                e.run_piston_events().unwrap();
                assert_eq!(e.pending_event_count(), 0);
                for z in [0, 10] {
                    assert_eq!(
                        piston_state(e.world().get(Pos::new(0, 1, z)).unwrap()),
                        if action == PistonAction::Extend {
                            PistonState::Extended
                        } else {
                            PistonState::Retracted
                        }
                    );
                    let target = if action == PistonAction::Extend { 2 } else { 1 };
                    assert_eq!(e.world().kind_at(Pos::new(target, 1, z)), BlockKind::Solid);
                }
            }
        }
    }
}

#[test]
fn completion_rechecks_local_states_but_allows_distant_changes() {
    for action in [PistonAction::Extend, PistonAction::Retract] {
        let mut w = world();
        if action == PistonAction::Retract {
            plan_piston(&w, Pos::new(0, 1, 0), PistonAction::Extend)
                .unwrap()
                .apply(&mut w)
                .unwrap();
        }
        let plan = plan_piston(&w, Pos::new(0, 1, 0), action).unwrap();
        plan.start_delta().apply(&mut w).unwrap();
        let saved = plan.completion_plan(&w).unwrap();
        for change in plan.start_delta().changes {
            let mut changed = w.clone();
            changed.set(change.position, Block::new(BlockKind::Solid));
            let before = changed.state_id();
            assert!(
                saved.completion_plan(&changed).is_err(),
                "{:?}",
                change.position
            );
            assert_eq!(changed.state_id(), before);
        }
        w.set(Pos::new(30, 1, 30), Block::new(BlockKind::Solid));
        let mut lever = w.get(Pos::new(20, 1, 20)).unwrap().clone();
        lever.powered = Some(true);
        w.set(Pos::new(20, 1, 20), lever);
        let completion = saved.completion_plan(&w).unwrap();
        completion.world_delta().apply(&mut w).unwrap();
        assert_eq!(w.kind_at(Pos::new(30, 1, 30)), BlockKind::Solid);
        assert_eq!(w.get(Pos::new(20, 1, 20)).unwrap().powered, Some(true));
    }
}

#[test]
fn empty_pull_source_remains_a_dependency() {
    let mut w = world();
    w.set(Pos::new(1, 1, 0), Block::new(BlockKind::Air));
    plan_piston(&w, Pos::new(0, 1, 0), PistonAction::Extend)
        .unwrap()
        .apply(&mut w)
        .unwrap();
    let plan = plan_piston(&w, Pos::new(0, 1, 0), PistonAction::Retract).unwrap();
    plan.start_delta().apply(&mut w).unwrap();
    let saved = plan.completion_plan(&w).unwrap();
    w.set(Pos::new(2, 1, 0), Block::new(BlockKind::Solid));
    assert!(saved.completion_plan(&w).is_err());
}

#[test]
fn legacy_body_only_motion_checks_payload_before_rebuilding() {
    let mut w = world();
    let mut plan = plan_piston(&w, Pos::new(0, 1, 0), PistonAction::Extend).unwrap();
    plan.moving_delta = None;
    plan.start_delta().apply(&mut w).unwrap();
    let saved = plan.completion_plan(&w).unwrap();
    w.set(Pos::new(2, 1, 0), Block::new(BlockKind::Solid));
    assert!(saved.completion_plan(&w).is_err());
}

#[test]
fn overlapping_destination_is_rejected_before_second_start() {
    let mut w = world();
    let mut other = w.get(Pos::new(0, 1, 0)).unwrap().clone();
    other.facing = Some(Facing::West);
    w.set(Pos::new(4, 1, 0), other);
    w.set(Pos::new(3, 1, 0), Block::new(BlockKind::Solid));
    let first = plan_piston(&w, Pos::new(0, 1, 0), PistonAction::Extend).unwrap();
    first.start_delta().apply(&mut w).unwrap();
    let before = w.state_id();
    assert!(plan_piston(&w, Pos::new(4, 1, 0), PistonAction::Extend).is_err());
    assert_eq!(w.state_id(), before);
}

#[test]
fn two_retractions_cannot_claim_the_same_payload() {
    let mut w = world();
    for (x, facing, head_x) in [(0, Facing::East, 1), (4, Facing::West, 3)] {
        let mut piston = Block::new(BlockKind::Piston);
        piston.facing = Some(facing);
        piston.piston_variant = Some(PistonVariant::Sticky);
        piston.piston_state = Some(PistonState::Extended);
        w.set(Pos::new(x, 1, 0), piston);
        w.set(
            Pos::new(head_x, 1, 0),
            Block::piston_head(facing, PistonVariant::Sticky, false),
        );
    }
    w.set(Pos::new(2, 1, 0), Block::new(BlockKind::Solid));
    let first = plan_piston(&w, Pos::new(0, 1, 0), PistonAction::Retract).unwrap();
    first.start_delta().apply(&mut w).unwrap();
    let before = w.state_id();
    assert!(plan_piston(&w, Pos::new(4, 1, 0), PistonAction::Retract).is_err());
    assert_eq!(w.state_id(), before);
}
