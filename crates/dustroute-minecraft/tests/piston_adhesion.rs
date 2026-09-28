use dustroute_minecraft::physical::{self, Adhesion};
use dustroute_minecraft::piston_electrical::{SIDES, validate_evidence};
use dustroute_minecraft::time::piston_runtime::{
    ElectricalPistonRuntime, new_piston_runtime, schedule_electrical_input_after_tick,
};
use dustroute_minecraft::time::runtime::RuntimeLimits;
use dustroute_minecraft::{
    Block, BlockKind, Facing, ObservationClassification, PistonState, PistonVariant, Pos, Region,
    World,
};

const ZERO: Pos = Pos::new(0, 0, 0);
fn at(p: Pos, d: Facing, n: i32) -> Pos {
    let v = d.offset();
    Pos::new(p.x + n * v.x, p.y + n * v.y, p.z + n * v.z)
}
fn material(name: &str) -> Block {
    let mut b = Block::new(physical::classify(name).0);
    b.observed_name = Some(format!("minecraft:{name}"));
    b.observation_classification = ObservationClassification::Exact;
    b
}
fn world(dir: Facing) -> (World, Pos) {
    let mut w = World::new();
    let mut b = Block::new(BlockKind::Piston);
    b.facing = Some(dir);
    b.piston_variant = Some(PistonVariant::Sticky);
    b.piston_state = Some(PistonState::Retracted);
    w.set(ZERO, b);
    let input = at(ZERO, dir, -1);
    w.place(BlockKind::Solid, at(ZERO, dir, -2));
    let b = w.place(BlockKind::Lever, input);
    b.powered = Some(false);
    b.support_offset = Some(dir.opposite().offset());
    (w, input)
}
fn run(w: World) -> ElectricalPistonRuntime {
    new_piston_runtime(
        w,
        Region::new(Pos::new(-18, -18, -18), Pos::new(18, 18, 18)),
        RuntimeLimits::default(),
    )
    .unwrap()
}
fn on(r: &mut ElectricalPistonRuntime, input: Pos) {
    schedule_electrical_input_after_tick(r, 1, input, true).unwrap();
    r.run_until_idle().unwrap();
}

#[test]
fn materials_keep_native_support_conduction_and_pair_rules_separate() {
    let slime = material("slime_block");
    let honey = material("honey_block");
    for b in [&slime, &honey] {
        validate_evidence(b).unwrap();
    }
    let a = physical::of_block(&slime).unwrap();
    let b = physical::of_block(&honey).unwrap();
    assert!(a.conducts(&slime));
    assert!(!b.conducts(&honey));
    for side in SIDES {
        assert!(a.full_face(&slime, side));
        assert!(!b.full_face(&honey, side));
        assert_eq!(b.center_face(&honey, side), side == Facing::Down);
    }
    for (left, right, expected) in [
        (Adhesion::Slime, Adhesion::Honey, false),
        (Adhesion::Honey, Adhesion::Slime, false),
        (Adhesion::Slime, Adhesion::Slime, true),
        (Adhesion::Honey, Adhesion::Honey, true),
        (Adhesion::None, Adhesion::Slime, true),
        (Adhesion::Honey, Adhesion::None, true),
        (Adhesion::None, Adhesion::None, false),
    ] {
        assert_eq!(left.sticks_to(right), expected);
    }
    let mut bad = honey;
    bad.observed_properties
        .insert("waterlogged".into(), "false".into());
    assert!(validate_evidence(&bad).is_err());
}

#[test]
fn older_execution_profiles_do_not_gain_adhesion_from_material_registration() {
    use dustroute_minecraft::execution_context::{WorldExecutionContext, WorldExecutionProfile::*};
    for name in ["slime_block", "honey_block"] {
        let mut w = World::new();
        w.set(ZERO, material(name));
        for profile in [
            DustTorchSynchronousGameTickV1,
            DustSingleTorchBlockEffectsV1,
            RedstoneCompatibilityBoundaryV1,
            BoundedRedstoneEventsV1,
        ] {
            assert!(
                WorldExecutionContext::for_profile(profile)
                    .validate_world_kinds(&w)
                    .is_err()
            );
        }
        WorldExecutionContext::for_profile(UnifiedPistonElectricalCallbacksJava12111V17)
            .validate_world_kinds(&w)
            .unwrap();
        let mut malformed = material(name);
        malformed
            .observed_properties
            .insert("unknown".into(), "true".into());
        assert!(!BoundedRedstoneEventsV1.admits_block(&malformed));
        let (mut legacy, _) = world(Facing::East);
        legacy.set(Pos::new(1, 0, 0), material(name));
        assert!(
            dustroute_minecraft::plan_piston(
                &legacy,
                ZERO,
                dustroute_minecraft::PistonAction::Extend
            )
            .is_err()
        );
    }
}

#[test]
fn branched_push_and_pull_clear_every_vacated_cell_in_all_six_directions() {
    for dir in SIDES {
        let side = if matches!(dir, Facing::Up | Facing::Down) {
            Facing::East
        } else {
            Facing::Up
        };
        for name in ["slime_block", "honey_block"] {
            let (mut w, input) = world(dir);
            let core = at(ZERO, dir, 1);
            let wing = at(core, side, 1);
            let tail = at(wing, dir, -1);
            w.set(core, material(name));
            w.set(wing, material(name));
            w.set(tail, material("stone"));
            let original = w.clone();
            let mut r = run(w);
            on(&mut r, input);
            for p in [core, wing, tail] {
                assert_eq!(
                    r.view().block(at(p, dir, 1)).unwrap(),
                    *original.get(p).unwrap(),
                    "{name} {dir:?} {p:?}"
                );
            }
            assert_eq!(r.view().block(tail).unwrap().kind, BlockKind::Air);
            r.input_now(input, false).unwrap();
            r.run_until_idle().unwrap();
            assert_eq!(r.view().world(), &original, "{name} {dir:?}");
        }
    }
}

#[test]
fn nonadhesive_contact_is_distinct_from_a_blocked_destination() {
    for (neighbor, forward, moves) in [
        ("honey_block", false, true),
        ("obsidian", false, true),
        ("stone", true, false),
    ] {
        let (mut w, input) = world(Facing::East);
        w.set(Pos::new(1, 0, 0), material("slime_block"));
        w.set(Pos::new(1, 1, 0), material(neighbor));
        if forward {
            w.set(Pos::new(2, 1, 0), material("obsidian"));
        }
        let original = w.clone();
        let mut r = run(w);
        on(&mut r, input);
        assert_eq!(
            r.view().block(ZERO).unwrap().piston_state,
            Some(if moves {
                PistonState::Extended
            } else {
                PistonState::Retracted
            })
        );
        assert_eq!(
            r.view().block(Pos::new(1, 1, 0)).unwrap(),
            *original.get(Pos::new(1, 1, 0)).unwrap()
        );
    }
}

#[test]
fn converging_branches_move_each_block_once_and_share_the_twelve_block_limit() {
    for count in [4, 12, 13] {
        let (mut w, input) = world(Facing::East);
        for n in 0..count {
            w.set(Pos::new(1 + n % 2, n / 2, 0), material("slime_block"));
        }
        let original = w.clone();
        let mut r = run(w);
        on(&mut r, input);
        for (p, b) in original
            .iter()
            .filter(|(_, b)| b.observed_name.as_deref() == Some("minecraft:slime_block"))
        {
            assert_eq!(
                r.view()
                    .block(at(*p, Facing::East, i32::from(count <= 12)))
                    .unwrap(),
                *b
            );
        }
        assert_eq!(
            r.view().block(ZERO).unwrap().piston_state,
            Some(if count <= 12 {
                PistonState::Extended
            } else {
                PistonState::Retracted
            })
        );
        r.input_now(input, false).unwrap();
        r.run_until_idle().unwrap();
        assert_eq!(r.view().world(), &original);
    }
}

#[test]
fn blocked_pull_retracts_body_without_moving_the_structure() {
    let (mut w, input) = world(Facing::East);
    w.set(Pos::new(1, 0, 0), material("honey_block"));
    w.set(Pos::new(1, 1, 0), material("stone"));
    let mut r = run(w);
    on(&mut r, input);
    r.install_now(Pos::new(1, 1, 0), material("obsidian"))
        .unwrap();
    r.run_until_idle().unwrap();
    r.input_now(input, false).unwrap();
    r.run_until_idle().unwrap();
    assert_eq!(
        r.view().block(ZERO).unwrap().piston_state,
        Some(PistonState::Retracted)
    );
    assert_eq!(
        r.view().block(Pos::new(1, 0, 0)).unwrap().kind,
        BlockKind::Air
    );
    assert_eq!(
        r.view().block(Pos::new(2, 0, 0)).unwrap(),
        material("honey_block")
    );
    assert_eq!(
        r.view().block(Pos::new(2, 1, 0)).unwrap(),
        material("stone")
    );
}

#[test]
fn newly_attached_thirteenth_block_prevents_pull_but_not_body_retraction() {
    let (mut w, input) = world(Facing::East);
    for n in 0..12 {
        w.set(Pos::new(1 + n % 2, n / 2, 0), material("slime_block"));
    }
    let mut r = run(w);
    on(&mut r, input);
    r.install_now(Pos::new(2, 6, 0), material("stone")).unwrap();
    r.run_until_idle().unwrap();
    r.input_now(input, false).unwrap();
    r.run_until_idle().unwrap();
    assert_eq!(
        r.view().block(ZERO).unwrap().piston_state,
        Some(PistonState::Retracted)
    );
    for n in 0..12 {
        assert_eq!(
            r.view().block(Pos::new(2 + n % 2, n / 2, 0)).unwrap(),
            material("slime_block")
        );
    }
    assert_eq!(
        r.view().block(Pos::new(1, 0, 0)).unwrap().kind,
        BlockKind::Air
    );
}

#[test]
fn direct_destruction_is_rejected_before_any_motion_instead_of_becoming_a_blocked_push() {
    let (mut w, input) = world(Facing::East);
    w.set(Pos::new(1, 0, 0), material("slime_block"));
    w.set(Pos::new(2, -1, 0), material("stone"));
    let b = w.place(BlockKind::Lever, Pos::new(2, 0, 0));
    b.powered = Some(false);
    b.support_offset = Some(Facing::Down.offset());
    let mut r = run(w);
    schedule_electrical_input_after_tick(&mut r, 1, input, true).unwrap();
    assert!(r.run_until_idle().is_err());
    assert!(
        !r.view()
            .world()
            .iter()
            .any(|(_, b)| b.kind == BlockKind::MovingPiston)
    );
    assert_eq!(
        r.view().block(Pos::new(2, 0, 0)).unwrap().kind,
        BlockKind::Lever
    );
}

#[test]
fn support_loss_skips_the_attachment_instead_of_moving_it() {
    let (mut w, input) = world(Facing::East);
    w.set(Pos::new(1, 0, 0), material("slime_block"));
    let lever = w.place(BlockKind::Lever, Pos::new(1, 1, 0));
    lever.support_offset = Some(Facing::Down.offset());
    lever.powered = Some(false);
    let mut r = run(w);
    on(&mut r, input);
    assert_eq!(
        r.view().block(Pos::new(1, 1, 0)).unwrap().kind,
        BlockKind::Air
    );
    assert_eq!(
        r.view().block(Pos::new(2, 1, 0)).unwrap().kind,
        BlockKind::Air
    );
}

#[test]
fn interrupted_branched_movement_restores_exact_and_behavior_continuations() {
    let (mut w, input) = world(Facing::East);
    for p in [Pos::new(1, 0, 0), Pos::new(1, 1, 0), Pos::new(2, 1, 0)] {
        w.set(p, material("slime_block"));
    }
    let mut r = run(w);
    for (tick, power) in [(1, true), (2, false), (3, true), (16, false)] {
        schedule_electrical_input_after_tick(&mut r, tick, input, power).unwrap();
    }
    let (mut exact, mut behavior) = (None, None);
    while r.microstep().unwrap().is_some() {
        if r.view()
            .world()
            .iter()
            .filter(|(_, b)| b.kind == BlockKind::MovingPiston)
            .count()
            >= 2
        {
            if !r.at_input_boundary() && exact.is_none() {
                exact = Some(r.checkpoint());
            }
            if r.at_input_boundary() && behavior.is_none() {
                behavior = Some(r.behavior_state().unwrap());
            }
        }
    }
    let mut a = ElectricalPistonRuntime::from_checkpoint(&exact.unwrap()).unwrap();
    a.run_until_idle().unwrap();
    assert_eq!(a.state_key(), r.state_key());
    let mut b = ElectricalPistonRuntime::from_behavior_state(&behavior.unwrap()).unwrap();
    b.run_until_idle().unwrap();
    assert_eq!(b.behavior_state().unwrap(), r.behavior_state().unwrap());
}
