use dustroute_minecraft::piston_electrical::{ElectricalWorld, HORIZONTAL};
use dustroute_minecraft::time::piston_runtime::{ElectricalPistonRuntime, new_piston_runtime};
use dustroute_minecraft::{Block, BlockKind, Facing, Pos, Region, WireConnection, World};

const C: Pos = Pos::new(0, 3, 0);
fn region() -> Region {
    Region::new(Pos::new(-12, -3, -12), Pos::new(12, 12, 12))
}
fn along(p: Pos, d: Facing, n: i32) -> Pos {
    let d = d.offset();
    p.offset(d.x * n, d.y * n, d.z * n)
}
fn comparator(output: Facing, subtract: bool) -> Block {
    let mut b = Block::new(BlockKind::Comparator);
    b.powered = Some(false);
    b.facing = Some(output);
    b.support_offset = Some(Facing::Down.offset());
    b.observed_properties.insert(
        "mode".into(),
        if subtract { "subtract" } else { "compare" }.into(),
    );
    b
}
fn supported(w: &mut World, p: Pos, b: Block) {
    w.place(BlockKind::Solid, along(p, Facing::Down, 1));
    w.set(p, b);
}
fn lever(w: &mut World, p: Pos) {
    let mut b = Block::new(BlockKind::Lever);
    b.powered = Some(false);
    b.support_offset = Some(Facing::Down.offset());
    supported(w, p, b);
}
fn line(w: &mut World, direction: Facing, length: i32) {
    w.place(BlockKind::RedstoneBlock, along(C, direction, length + 1));
    for n in 1..=length {
        let mut b = Block::new(BlockKind::RedstoneWire);
        b.power_level = Some(0);
        b.support_offset = Some(Facing::Down.offset());
        b.wire_connections = Some(HORIZONTAL.map(|d| (d, WireConnection::Side)).into());
        supported(w, along(C, direction, n), b);
    }
}
fn shape(w: &mut World) {
    let positions: Vec<_> = w
        .iter()
        .filter(|(_, b)| b.kind == BlockKind::RedstoneWire)
        .map(|(p, _)| *p)
        .collect();
    for pos in positions {
        let arms = ElectricalWorld::new(w, region())
            .unwrap()
            .wire_shape(pos)
            .unwrap();
        w.get_mut(pos).unwrap().wire_connections = Some(arms);
    }
}

#[test]
fn java_signal_domain_and_analog_circuit_inputs() {
    for rear in 0u8..=15 {
        for side in 0u8..=15 {
            for subtract in [false, true] {
                let expected = if subtract {
                    rear.saturating_sub(side)
                } else if rear >= side {
                    rear
                } else {
                    0
                };
                assert_eq!(
                    dustroute_minecraft::device_callback_law::comparator_signal(
                        rear, side, subtract
                    ),
                    Some(expected)
                );
            }
        }
    }
    assert_eq!(
        dustroute_minecraft::device_callback_law::comparator_signal(16, 0, false),
        None
    );
    for subtract in [false, true] {
        let mut w = World::new();
        supported(&mut w, C, comparator(Facing::East, subtract));
        line(&mut w, Facing::West, 3);
        line(&mut w, Facing::South, 5);
        w.place(BlockKind::RedstoneLamp, along(C, Facing::East, 1))
            .powered = Some(false);
        shape(&mut w);
        let mut rt = new_piston_runtime(w, region(), Default::default()).unwrap();
        rt.run_until_idle().unwrap();
        assert_eq!(
            rt.view()
                .block(along(C, Facing::West, 1))
                .unwrap()
                .power_level,
            Some(13)
        );
        assert_eq!(
            rt.view()
                .block(along(C, Facing::South, 1))
                .unwrap()
                .power_level,
            Some(11)
        );
        assert_eq!(
            rt.view().stored_output(C).unwrap(),
            if subtract { 2 } else { 13 }
        );
        assert_eq!(
            rt.view().block(along(C, Facing::East, 1)).unwrap().powered,
            Some(true)
        );
    }
}

#[test]
fn four_output_directions_compare_subtract_and_no_snapshot_inference() {
    for direction in HORIZONTAL {
        for subtract in [false, true] {
            for side_power in [false, true] {
                let mut w = World::new();
                supported(&mut w, C, comparator(direction, subtract));
                w.place(BlockKind::RedstoneBlock, along(C, direction, -1));
                if side_power {
                    let side = HORIZONTAL
                        .into_iter()
                        .find(|d| *d != direction && *d != direction.opposite())
                        .unwrap();
                    w.place(BlockKind::RedstoneBlock, along(C, side, 1));
                }
                let lamp = along(C, direction, 1);
                w.place(BlockKind::RedstoneLamp, lamp).powered = Some(false);
                assert!(
                    ElectricalWorld::new(&w, region())
                        .unwrap()
                        .emission(C, direction.opposite(), true)
                        .is_err()
                );
                let mut rt = new_piston_runtime(w, region(), Default::default()).unwrap();
                assert_eq!(rt.view().stored_output(C).unwrap(), 0);
                rt.run_until_idle().unwrap();
                let expected = if subtract && side_power { 0 } else { 15 };
                assert_eq!(rt.view().stored_output(C).unwrap(), expected);
                assert_eq!(rt.view().block(lamp).unwrap().powered, Some(expected > 0));
                let before = rt.view().block(C).unwrap();
                rt.use_now(C).unwrap();
                assert!(
                    rt.run_until_idle().is_err(),
                    "mode use is unsupported by this profile"
                );
                assert_eq!(rt.view().block(C).unwrap(), before);
            }
        }
    }
}

#[test]
fn bulb_readout_updates_through_one_conductor_and_survives_checkpoint_boundaries() {
    for gap in [0, 1] {
        let mut w = World::new();
        supported(&mut w, C, comparator(Facing::East, false));
        let bpos = along(C, Facing::West, 1 + gap);
        let mut b = Block::new(BlockKind::CopperBulb);
        b.powered = Some(false);
        b.observed_properties
            .insert("powered".into(), "false".into());
        w.set(bpos, b);
        if gap == 1 {
            w.place(BlockKind::Solid, along(C, Facing::West, 1));
        }
        let input = along(bpos, Facing::West, 1);
        lever(&mut w, input);
        let lamp = along(C, Facing::East, 1);
        w.place(BlockKind::RedstoneLamp, lamp).powered = Some(false);
        let mut rt = new_piston_runtime(w, region(), Default::default()).unwrap();
        rt.run_until_idle().unwrap();
        for (powered, expected) in [(true, 15), (false, 15), (true, 0), (false, 0)] {
            rt.input_now(input, powered).unwrap();
            let mut checkpoints = vec![];
            while rt.microstep().unwrap().is_some() {
                checkpoints.push(rt.checkpoint());
            }
            assert_eq!(rt.view().stored_output(C).unwrap(), expected);
            assert_eq!(rt.view().block(lamp).unwrap().powered, Some(expected != 0));
            for checkpoint in checkpoints {
                let mut restored = ElectricalPistonRuntime::from_checkpoint(&checkpoint).unwrap();
                restored.run_until_idle().unwrap();
                assert_eq!(restored.state_key(), rt.state_key());
            }
            let restored =
                ElectricalPistonRuntime::from_behavior_state(&rt.behavior_state().unwrap())
                    .unwrap();
            assert_eq!(restored.view().stored_output(C).unwrap(), expected);
            assert_eq!(
                restored.behavior_state().unwrap(),
                rt.behavior_state().unwrap()
            );
        }
    }
}

#[test]
fn internal_output_changes_precede_visible_power_and_replacement_clears_the_register() {
    let mut w = World::new();
    supported(&mut w, C, comparator(Facing::East, false));
    let input = along(C, Facing::West, 1);
    lever(&mut w, input);
    let mut rt = new_piston_runtime(w, region(), Default::default()).unwrap();
    rt.run_until_idle().unwrap();
    rt.input_now(input, true).unwrap();
    rt.run_until_idle().unwrap();
    let internal = rt
        .trace()
        .iter()
        .position(|r| r.output_changes.contains(&(C, 0, 15)))
        .unwrap();
    let visible = rt
        .trace()
        .iter()
        .position(|r| {
            r.delta.as_ref().is_some_and(|d| {
                d.changes.iter().any(|c| {
                    c.position == C
                        && c.before.powered == Some(false)
                        && c.after.powered == Some(true)
                })
            })
        })
        .unwrap();
    assert!(internal < visible);
    assert_eq!(rt.trace()[internal].invocation.time.game_tick, 2);
    rt.remove_now(C).unwrap();
    rt.run_until_idle().unwrap();
    assert_eq!(rt.view().stored_output(C).unwrap(), 0);
    rt.input_now(input, false).unwrap();
    rt.run_until_idle().unwrap();
    rt.install_now(C, comparator(Facing::East, true)).unwrap();
    rt.run_until_idle().unwrap();
    assert_eq!(rt.view().stored_output(C).unwrap(), 0);
}

#[test]
fn fresh_powered_flag_does_not_invent_internal_signal_and_bad_observations_fail() {
    // Java subtraction skips a POWERED repair if its stored output stays zero.
    // It is observable after /setblock ...[powered=true,mode=subtract].
    for subtract in [false, true] {
        let mut w = World::new();
        let mut b = comparator(Facing::East, subtract);
        b.powered = Some(true);
        supported(&mut w, C, b);
        let mut rt = new_piston_runtime(w, region(), Default::default()).unwrap();
        rt.run_until_idle().unwrap();
        assert_eq!(rt.view().stored_output(C).unwrap(), 0);
        assert_eq!(rt.view().block(C).unwrap().powered, Some(subtract));
    }
    for change in 0..4 {
        let mut b = comparator(Facing::East, false);
        match change {
            0 => {
                b.observed_properties.remove("mode");
            }
            1 => {
                b.observed_properties
                    .insert("mode".into(), "invalid".into());
            }
            2 => b.power_level = Some(7),
            _ => b.observed_name = Some("mod:comparator".into()),
        }
        let mut w = World::new();
        supported(&mut w, C, b);
        assert!(new_piston_runtime(w, region(), Default::default()).is_err());
    }
    assert_ne!(
        dustroute_minecraft::piston_law::electrical_payload_laws()
            .payload_rejection(&comparator(Facing::East, false))
            .unwrap(),
        0
    );
}

#[test]
fn stored_comparator_output_locks_a_repeater_from_the_side() {
    let mut w = World::new();
    supported(&mut w, C, comparator(Facing::East, false));
    w.place(BlockKind::RedstoneBlock, along(C, Facing::West, 1));
    let r = along(C, Facing::East, 1);
    let mut repeater = Block::new(BlockKind::Repeater);
    repeater.facing = Some(Facing::North);
    repeater.delay = Some(1);
    repeater.powered = Some(false);
    repeater.support_offset = Some(Facing::Down.offset());
    supported(&mut w, r, repeater);
    let mut rt = new_piston_runtime(w, region(), Default::default()).unwrap();
    rt.run_until_idle().unwrap();
    assert_eq!(
        rt.view().block(r).unwrap().observed_properties["locked"],
        "true"
    );
    rt.remove_now(along(C, Facing::West, 1)).unwrap();
    rt.run_until_idle().unwrap();
    assert_eq!(
        rt.view().block(r).unwrap().observed_properties["locked"],
        "false"
    );
}

#[test]
fn comparator_readout_overrides_direct_power_but_not_saturated_power_through_a_conductor() {
    for direct in [false, true] {
        let mut w = World::new();
        supported(&mut w, C, comparator(Facing::East, false));
        let rear = along(C, Facing::West, 1);
        let bpos = along(C, Facing::West, if direct { 1 } else { 2 });
        let mut b = Block::new(BlockKind::CopperBulb);
        b.powered = Some(false);
        b.observed_properties
            .insert("powered".into(), direct.to_string());
        w.set(bpos, b);
        if !direct {
            w.place(BlockKind::Solid, rear);
        }
        // Lever strongly powers the rear cube. Direct readout 0 wins; behind
        // a conductor, Java's already-saturated input skips the distant readout.
        let input = along(rear, Facing::Up, 1);
        let mut lever = Block::new(BlockKind::Lever);
        lever.powered = Some(true);
        lever.support_offset = Some(Facing::Down.offset());
        w.set(input, lever);
        let mut rt = new_piston_runtime(w, region(), Default::default()).unwrap();
        rt.run_until_idle().unwrap();
        assert_eq!(
            rt.view().stored_output(C).unwrap(),
            if direct { 0 } else { 15 }
        );
    }
}

#[test]
fn gate_tick_requests_respect_collected_batch_and_target_alignment() {
    use dustroute_minecraft::device_program::{Callback, Query, ResolvedEffect, program};
    let b = comparator(Facing::East, false);
    for collected in [false, true] {
        for misaligned in [false, true] {
            let run = program(&b)
                .unwrap()
                .prepare(Callback::Neighbor, &b, |q| {
                    Ok(match q {
                        Query::Constant { value } => *value,
                        Query::GateOutputLevel => 9,
                        Query::GateOutputChanged => 1,
                        Query::Powered | Query::State { .. } => 0,
                        Query::TickCollected => collected.into(),
                        Query::OutputGateMisaligned => misaligned.into(),
                        _ => panic!("unexpected query"),
                    })
                })
                .unwrap()
                .unwrap();
            let expected = if collected {
                vec![]
            } else {
                vec![ResolvedEffect::Schedule {
                    delay: 2,
                    priority: if misaligned { 2 } else { 3 },
                }]
            };
            assert_eq!(run.effects(), expected);
        }
    }
}

#[test]
fn powered_write_notifies_output_both_from_added_and_after_the_shape_pass() {
    use dustroute_minecraft::device_program::Callback;
    use dustroute_minecraft::time::piston_runtime::PistonEvent;
    let mut w = World::new();
    supported(&mut w, C, comparator(Facing::East, false));
    w.place(BlockKind::RedstoneBlock, along(C, Facing::West, 1));
    let lamp = along(C, Facing::East, 1);
    w.place(BlockKind::RedstoneLamp, lamp).powered = Some(false);
    let mut rt = new_piston_runtime(w, region(), Default::default()).unwrap();
    rt.run_until_idle().unwrap();
    let deliveries: Vec<_> = rt
        .trace()
        .iter()
        .filter(|r| {
            r.invocation.time.game_tick == 2
                && r.invocation.call.target == lamp
                && matches!(
                    r.invocation.call.payload,
                    PistonEvent::Device {
                        callback: Callback::Neighbor,
                        ..
                    }
                )
        })
        .collect();
    assert_eq!(deliveries.len(), 2);
    let lit_change = rt
        .trace()
        .iter()
        .find(|r| {
            r.delta.as_ref().is_some_and(|d| {
                d.changes
                    .iter()
                    .any(|c| c.position == lamp && c.after.powered == Some(true))
            })
        })
        .unwrap();
    assert_eq!(deliveries[0].invocation.id, lit_change.invocation.id);
    assert!(lit_change.invocation.id < deliveries[1].invocation.id);
}
