use dustroute_minecraft::device_program::program;
use dustroute_minecraft::piston_electrical::{ElectricalWorld, HORIZONTAL, SIDES};
use dustroute_minecraft::time::piston_runtime::{
    ElectricalPistonRuntime, new_piston_runtime, schedule_electrical_input_after_tick,
};
use dustroute_minecraft::time::runtime::BlockIdentity;
use dustroute_minecraft::{Block, BlockKind, Facing, Pos, Region, WireConnection, World};

const S: Pos = Pos::new(0, 3, 0);
fn region() -> Region {
    Region::new(Pos::new(-8, -4, -8), Pos::new(8, 12, 8))
}
fn along(p: Pos, d: Facing, n: i32) -> Pos {
    let d = d.offset();
    p.offset(d.x * n, d.y * n, d.z * n)
}
fn torch(support: Facing, lit: bool) -> Block {
    let mut b = Block::new(BlockKind::RedstoneTorch);
    b.support_offset = Some(support.offset());
    b.powered = Some(lit);
    b
}
fn lever(w: &mut World, p: Pos, support: Facing) {
    let b = w.place(BlockKind::Lever, p);
    b.powered = Some(false);
    b.support_offset = Some(support.offset());
}
fn scene(support: Facing) -> (World, Pos, Pos) {
    let mut w = World::new();
    w.place(BlockKind::Solid, S);
    let pos = along(S, support, -1);
    w.set(pos, torch(support, true));
    let input_side = if support == Facing::Down {
        Facing::West
    } else {
        support
    };
    let input = along(S, input_side, 1);
    lever(&mut w, input, input_side.opposite());
    (w, pos, input)
}
fn lit_writes(rt: &ElectricalPistonRuntime, pos: Pos) -> Vec<(u64, bool)> {
    rt.trace()
        .iter()
        .flat_map(|r| {
            r.delta.iter().flat_map(move |d| {
                d.changes
                    .iter()
                    .filter(move |c| c.position == pos && c.before.powered != c.after.powered)
                    .map(move |c| (r.invocation.time.game_tick, c.after.powered.unwrap()))
            })
        })
        .collect()
}

#[test]
fn support_power_and_directional_emission_cover_standing_and_four_wall_mounts() {
    for support in [
        Facing::Down,
        Facing::North,
        Facing::East,
        Facing::South,
        Facing::West,
    ] {
        let (w, pos, input) = scene(support);
        let query = ElectricalWorld::new(&w, region()).unwrap();
        for side in SIDES {
            let signal = query.emission(pos, side, true).unwrap();
            let weak = if side == support.opposite() { 0 } else { 15 };
            assert_eq!(
                (signal.weak, signal.strong),
                (weak, if side == Facing::Down { weak } else { 0 })
            );
        }
        let mut rt = new_piston_runtime(w, region(), Default::default()).unwrap();
        rt.run_until_idle().unwrap();
        rt.input_now(input, true).unwrap();
        rt.run_until_idle().unwrap();
        assert_eq!(lit_writes(&rt, pos), vec![(2, false)]);
        rt.input_now(input, false).unwrap();
        rt.run_until_idle().unwrap();
        assert_eq!(lit_writes(&rt, pos), vec![(2, false), (4, true)]);
    }
}

#[test]
fn shared_runtime_matches_all_retained_isolated_torch_samples() {
    let observation: serde_json::Value = serde_json::from_str(include_str!(
        "../../dustroute-translate/tests/fixtures/torch_burnout_1_21_11.json"
    ))
    .unwrap();
    assert_eq!(observation["evidence"], "observed_server_block_state");
    let mut samples = 0;
    for case in observation["cases"].as_array().unwrap() {
        let support = if case["orientation"] == "standing" {
            Facing::Down
        } else {
            Facing::West
        };
        let (w, pos, input) = scene(support);
        let mut rt = new_piston_runtime(w, region(), Default::default()).unwrap();
        for change in case["inputs"].as_array().unwrap() {
            schedule_electrical_input_after_tick(
                &mut rt,
                change["game_tick"].as_u64().unwrap(),
                input,
                change["powered"].as_bool().unwrap(),
            )
            .unwrap();
        }
        rt.run_until_idle().unwrap();
        let writes = lit_writes(&rt, pos);
        for sample in case["samples"].as_array().unwrap() {
            let tick = sample["game_tick"].as_u64().unwrap();
            let actual = writes
                .iter()
                .rev()
                .find(|(t, _)| *t <= tick)
                .is_none_or(|(_, lit)| *lit);
            assert_eq!(
                actual,
                sample["lit"].as_bool().unwrap(),
                "{} {} tick {tick}",
                case["name"],
                case["orientation"]
            );
            samples += 1;
        }
    }
    assert_eq!(samples, 3888);
}

#[test]
fn feedback_reserves_the_short_tick_before_burnout_recovery() {
    let lower = Pos::new(0, 0, 0);
    let upper = Pos::new(1, 1, 0);
    let wire = Pos::new(0, 1, 0);
    let pos = Pos::new(1, 0, 0);
    let mut w = World::new();
    w.place(BlockKind::Solid, lower);
    w.place(BlockKind::Solid, upper);
    let dust = w.place(BlockKind::RedstoneWire, wire);
    dust.power_level = Some(15);
    dust.support_offset = Some(Facing::Down.offset());
    dust.wire_connections = Some(HORIZONTAL.map(|d| (d, WireConnection::None)).into());
    w.set(pos, torch(Facing::West, true));
    let mut rt = new_piston_runtime(w, region(), Default::default()).unwrap();
    rt.run_until_idle().unwrap();
    let observation: serde_json::Value = serde_json::from_str(include_str!(
        "../../dustroute-translate/tests/fixtures/periodic_clock_1_21_11.json"
    ))
    .unwrap();
    let writes = lit_writes(&rt, pos);
    for sample in observation["samples"].as_array().unwrap() {
        let tick = sample["game_tick"].as_u64().unwrap();
        let actual = writes
            .iter()
            .rev()
            .find(|(t, _)| *t <= tick)
            .is_none_or(|(_, lit)| *lit);
        assert_eq!(
            actual,
            sample["torch_lit"].as_bool().unwrap(),
            "feedback tick {tick}"
        );
    }
    assert_eq!(writes.len(), 15);
    assert_eq!(writes.last(), Some(&(30, false)));
    assert_eq!(rt.view().time().game_tick, 32);
    assert_eq!(rt.pending_count(), 0);
    let rule = program(&rt.view().block(pos).unwrap())
        .unwrap()
        .definition()
        .history
        .clone()
        .unwrap();
    assert_eq!(rt.view().history_count(&rule, pos).unwrap(), 8);
}

#[test]
fn concrete_mount_identity_changes_without_erasing_position_owned_history() {
    let (mut w, pos, input) = scene(Facing::West);
    w.place(BlockKind::Solid, along(pos, Facing::Down, 1));
    let second = pos.offset(1, -1, 0);
    lever(&mut w, second, Facing::West);
    let mut rt = new_piston_runtime(w, region(), Default::default()).unwrap();
    rt.run_until_idle().unwrap();
    for _ in 0..7 {
        for powered in [true, false] {
            rt.input_now(input, powered).unwrap();
            rt.run_until_idle().unwrap();
        }
    }
    let rule = program(&rt.view().block(pos).unwrap())
        .unwrap()
        .definition()
        .history
        .clone()
        .unwrap();
    assert_eq!(rt.view().history_count(&rule, pos).unwrap(), 7);
    let old = rt.view().block(pos).unwrap();
    let replacement = torch(Facing::Down, true);
    assert_ne!(BlockIdentity::of(&old), BlockIdentity::of(&replacement));
    assert_eq!(
        BlockIdentity::of(&old),
        BlockIdentity::of(&torch(Facing::North, false))
    );
    rt.remove_now(pos).unwrap();
    rt.run_until_idle().unwrap();
    assert_eq!(rt.view().history_count(&rule, pos).unwrap(), 7);
    rt.install_now(pos, replacement).unwrap();
    rt.run_until_idle().unwrap();
    rt.input_now(second, true).unwrap();
    while rt.view().history_count(&rule, pos).unwrap() < 8 {
        assert!(rt.step().unwrap());
    }
    assert_eq!(rt.view().block(pos).unwrap().powered, Some(false));
    assert_eq!(rt.pending_count(), 1);
    let mut restored =
        ElectricalPistonRuntime::from_behavior_state(&rt.behavior_state().unwrap()).unwrap();
    assert_eq!(restored.view().history_count(&rule, pos).unwrap(), 8);
    rt.run_until_idle().unwrap();
    restored.run_until_idle().unwrap();
    assert_eq!(
        rt.behavior_state().unwrap(),
        restored.behavior_state().unwrap()
    );
}

#[test]
fn burnout_effects_restore_at_every_microstep_after_a_normalized_history() {
    let (w, pos, input) = scene(Facing::Down);
    let mut rt = new_piston_runtime(w, region(), Default::default()).unwrap();
    rt.run_until_idle().unwrap();
    for _ in 0..7 {
        for powered in [true, false] {
            rt.input_now(input, powered).unwrap();
            rt.run_until_idle().unwrap();
        }
    }
    // Begin a new diagnostic segment while preserving live ages and all future work.
    let mut rt =
        ElectricalPistonRuntime::from_behavior_state(&rt.behavior_state().unwrap()).unwrap();
    let rule = program(&rt.view().block(pos).unwrap())
        .unwrap()
        .definition()
        .history
        .clone()
        .unwrap();
    rt.input_now(input, true).unwrap();
    let mut checkpoints = vec![];
    let mut reached_burnout = false;
    while rt.microstep().unwrap().is_some() {
        reached_burnout |= rt.view().history_count(&rule, pos).unwrap() == 8;
        checkpoints.push(rt.checkpoint());
    }
    assert!(reached_burnout);
    for checkpoint in checkpoints {
        let mut restored = ElectricalPistonRuntime::from_checkpoint(&checkpoint).unwrap();
        restored.run_until_idle().unwrap();
        assert_eq!(restored.state_key(), rt.state_key());
    }
}

#[test]
fn multiple_torches_share_the_scheduler_but_keep_independent_position_histories() {
    let (mut w, a, input_a) = scene(Facing::Down);
    let b = a.offset(4, 0, 0);
    let support = S.offset(4, 0, 0);
    let input_b = support.offset(-1, 0, 0);
    w.place(BlockKind::Solid, support);
    w.set(b, torch(Facing::Down, true));
    lever(&mut w, input_b, Facing::East);
    let mut rt = new_piston_runtime(w, region(), Default::default()).unwrap();
    rt.run_until_idle().unwrap();
    for _ in 0..7 {
        for powered in [true, false] {
            rt.input_now(input_a, powered).unwrap();
            rt.run_until_idle().unwrap();
        }
    }
    let rule = program(&rt.view().block(a).unwrap())
        .unwrap()
        .definition()
        .history
        .clone()
        .unwrap();
    assert_eq!(rt.view().history_count(&rule, a).unwrap(), 7);
    assert_eq!(rt.view().history_count(&rule, b).unwrap(), 0);
    let now = rt.view().time().game_tick;
    schedule_electrical_input_after_tick(&mut rt, now, input_a, true).unwrap();
    schedule_electrical_input_after_tick(&mut rt, now, input_b, true).unwrap();
    while rt.view().history_count(&rule, a).unwrap() < 8 {
        assert!(rt.step().unwrap());
    }
    let mut restored =
        ElectricalPistonRuntime::from_behavior_state(&rt.behavior_state().unwrap()).unwrap();
    rt.run_until_idle().unwrap();
    restored.run_until_idle().unwrap();
    assert_eq!(
        rt.behavior_state().unwrap(),
        restored.behavior_state().unwrap()
    );
    assert_eq!(rt.view().block(a).unwrap().powered, Some(false));
    assert_eq!(rt.view().block(b).unwrap().powered, Some(false));
    rt.input_now(input_b, false).unwrap();
    rt.run_until_idle().unwrap();
    assert_eq!(rt.view().block(b).unwrap().powered, Some(true));
}

#[test]
fn root_normalization_preserves_the_inclusive_sixty_tick_expiry_boundary() {
    let (w, pos, input) = scene(Facing::Down);
    let mut rt = new_piston_runtime(w, region(), Default::default()).unwrap();
    rt.run_until_idle().unwrap();
    for _ in 0..7 {
        for powered in [true, false] {
            rt.input_now(input, powered).unwrap();
            rt.run_until_idle().unwrap();
        }
    }
    let rule = program(&rt.view().block(pos).unwrap())
        .unwrap()
        .definition()
        .history
        .clone()
        .unwrap();
    while rt.view().time().game_tick < 62 {
        assert!(rt.advance_behavior_clock().unwrap());
    }
    assert_eq!(rt.view().history_count(&rule, pos).unwrap(), 7); // first off was tick 2
    let before = rt.behavior_state().unwrap();
    let world = rt.view().world().clone();
    let mut restored = ElectricalPistonRuntime::from_behavior_state(&before).unwrap();
    assert_eq!(restored.view().history_count(&rule, pos).unwrap(), 7);
    assert!(rt.advance_behavior_clock().unwrap());
    assert!(restored.advance_behavior_clock().unwrap());
    assert_eq!(rt.view().history_count(&rule, pos).unwrap(), 6);
    assert_eq!(restored.view().history_count(&rule, pos).unwrap(), 6);
    assert_eq!(rt.view().world(), &world);
    assert_ne!(rt.behavior_state().unwrap(), before);
    assert_eq!(
        rt.behavior_state().unwrap(),
        restored.behavior_state().unwrap()
    );
}
