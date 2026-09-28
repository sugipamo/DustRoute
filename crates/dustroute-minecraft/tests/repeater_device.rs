use dustroute_minecraft::device_program::{self, Callback, Query, ResolvedEffect};
use dustroute_minecraft::piston_electrical::{ElectricalWorld, HORIZONTAL, SIDES};
use dustroute_minecraft::time::piston_runtime::*;
use dustroute_minecraft::time::runtime::*;
use dustroute_minecraft::{Block, BlockKind, Facing, Pos, Region, WireConnection, World};

const P: Pos = Pos::new(0, 4, 0);
fn region() -> Region {
    Region::new(Pos::new(-7, -2, -7), Pos::new(7, 12, 7))
}
fn offset(p: Pos, d: Facing, n: i32) -> Pos {
    let d = d.offset();
    p.offset(d.x * n, d.y * n, d.z * n)
}
fn repeater(facing: Facing, delay: u8) -> Block {
    let mut b = Block::new(BlockKind::Repeater);
    b.facing = Some(facing);
    b.support_offset = Some(Facing::Down.offset());
    b.powered = Some(false);
    b.delay = Some(delay);
    b
}
fn scene(facing: Facing, delay: u8) -> (World, Pos) {
    let mut w = World::new();
    w.place(BlockKind::Solid, P.offset(0, -1, 0));
    w.set(P, repeater(facing, delay));
    let input = offset(P, facing, -1);
    w.place(BlockKind::Solid, input.offset(0, -1, 0));
    let b = w.place(BlockKind::Lever, input);
    b.powered = Some(false);
    b.support_offset = Some(Facing::Down.offset());
    (w, input)
}

#[test]
fn all_horizontal_facings_and_delays_use_shared_ticks_and_directional_emission() {
    for facing in HORIZONTAL {
        for delay in 1..=4 {
            let (w, input) = scene(facing, delay);
            let mut rt = new_piston_runtime(w, region(), Default::default()).unwrap();
            schedule_electrical_input(&mut rt, 1, input, true).unwrap();
            schedule_electrical_input(&mut rt, 2, input, false).unwrap();
            let mut changes = Vec::new();
            while let Some(r) = rt.microstep().unwrap() {
                if r.invocation.kind == InvocationKind::ScheduledTick {
                    assert!(matches!(
                        r.invocation.call.payload,
                        PistonEvent::Device {
                            callback: Callback::Tick,
                            ..
                        }
                    ));
                }
                if r.delta
                    .as_ref()
                    .is_some_and(|d| d.changes.iter().any(|c| c.position == P))
                {
                    let on = rt.view().block(P).unwrap().powered.unwrap();
                    changes.push((r.invocation.time.game_tick, on));
                    let q = ElectricalWorld::new(rt.view().world(), region()).unwrap();
                    for side in SIDES {
                        let e = q.emission(P, side, true).unwrap();
                        let expected = if on && side == facing.opposite() {
                            15
                        } else {
                            0
                        };
                        assert_eq!((e.weak, e.strong), (expected, expected));
                    }
                }
            }
            assert_eq!(
                changes,
                [
                    (1 + u64::from(delay) * 2, true),
                    (1 + u64::from(delay) * 4, false)
                ]
            );
        }
    }
}

#[test]
fn dust_connects_to_both_gate_ends_but_only_to_an_observers_output() {
    for side in HORIZONTAL {
        for facing in HORIZONTAL {
            for kind in [BlockKind::Repeater, BlockKind::Observer] {
                let mut w = World::new();
                w.place(BlockKind::Solid, P.offset(0, -1, 0));
                let wire = w.place(BlockKind::RedstoneWire, P);
                wire.power_level = Some(0);
                wire.support_offset = Some(Facing::Down.offset());
                wire.wire_connections = Some(
                    HORIZONTAL
                        .into_iter()
                        .map(|d| (d, WireConnection::None))
                        .collect(),
                );
                let neighbor = offset(P, side, 1);
                w.place(BlockKind::Solid, neighbor.offset(0, -1, 0));
                let mut b = if kind == BlockKind::Repeater {
                    repeater(facing, 1)
                } else {
                    let mut b = Block::new(kind);
                    b.powered = Some(false);
                    b.facing = Some(facing);
                    b
                };
                b.powered = Some(false);
                w.set(neighbor, b);
                let shape = ElectricalWorld::new(&w, region())
                    .unwrap()
                    .wire_shape(P)
                    .unwrap();
                let expected =
                    facing == side.opposite() || kind == BlockKind::Repeater && facing == side;
                assert_eq!(
                    shape[&side] != WireConnection::None,
                    expected,
                    "{kind:?} {side:?} {facing:?}"
                );
            }
        }
    }
}

#[test]
fn output_then_shapes_then_followup_reservation_survive_every_microstep() {
    let (mut w, input) = scene(Facing::East, 1);
    let lamp = P.offset(1, 0, 0);
    w.place(BlockKind::RedstoneLamp, lamp).powered = Some(false);
    let observer = P.offset(0, 1, 0);
    let b = w.place(BlockKind::Observer, observer);
    b.powered = Some(false);
    b.facing = Some(Facing::Up); // Watches the repeater from above.
    let rep_id = BlockIdentity::of(w.get(P).unwrap());
    let observer_id = BlockIdentity::of(w.get(observer).unwrap());
    let mut rt = new_piston_runtime(w, region(), Default::default()).unwrap();
    schedule_electrical_input(&mut rt, 1, input, true).unwrap();
    schedule_electrical_input(&mut rt, 2, input, false).unwrap();
    let mut checkpoints = Vec::new();
    let mut boundaries = [false; 3];
    while let Some(r) = rt.microstep().unwrap() {
        if r.invocation.time.game_tick == 3 {
            if let Some(d) = &r.delta {
                if d.changes
                    .iter()
                    .any(|c| c.position == P && c.after.powered == Some(true))
                {
                    assert!(!rt.view().block_tick_queued(P, &rep_id));
                    assert_eq!(rt.view().block(lamp).unwrap().powered, Some(false));
                    boundaries[0] = true;
                }
                if d.changes
                    .iter()
                    .any(|c| c.position == lamp && c.after.powered == Some(true))
                {
                    assert!(!rt.view().block_tick_queued(observer, &observer_id));
                    assert!(!rt.view().block_tick_queued(P, &rep_id));
                    boundaries[1] = true;
                }
            }
            if rt.view().block_tick_queued(observer, &observer_id)
                && !rt.view().block_tick_queued(P, &rep_id)
            {
                boundaries[2] = true;
            }
            checkpoints.push(rt.checkpoint());
        }
    }
    assert_eq!(boundaries, [true; 3]);
    assert!(!checkpoints.is_empty());
    for checkpoint in checkpoints {
        let mut restored = ElectricalPistonRuntime::from_checkpoint(&checkpoint).unwrap();
        restored.run_until_idle().unwrap();
        assert_eq!(restored.state_key(), rt.state_key());
        assert_eq!(restored.trace(), rt.trace());
    }
}

#[test]
fn collected_tick_suppresses_requests_and_priorities_remain_data_driven() {
    let b = repeater(Facing::East, 2);
    let p = device_program::program(&b).unwrap();
    for (powered, misaligned, collected, priority) in [
        (false, false, false, Some(2)),
        (true, false, false, Some(1)),
        (false, true, false, Some(0)),
        (true, true, false, Some(0)),
        (false, true, true, None),
        (true, false, true, None),
    ] {
        let run = p
            .prepare(Callback::Neighbor, &b, |q| {
                Ok(match q {
                    Query::Constant { value } => *value,
                    Query::Powered => powered.into(),
                    Query::GateInputPowered => (!powered).into(),
                    Query::SideGatePowered => 0,
                    Query::OutputGateMisaligned => misaligned.into(),
                    Query::TickCollected => collected.into(),
                    Query::State { .. } => 2,
                    _ => panic!("unexpected query: {q:?}"),
                })
            })
            .unwrap()
            .unwrap();
        let expected: Vec<_> = priority
            .into_iter()
            .map(|priority| ResolvedEffect::Schedule { delay: 4, priority })
            .collect();
        assert_eq!(run.effects(), expected);
    }
}

#[test]
fn registered_gate_retains_strict_orientation_support_and_observed_state_admission() {
    let mut b = repeater(Facing::East, 2);
    b.observed_name = Some("minecraft:repeater".into());
    b.observed_properties = [
        ("facing", "west"),
        ("powered", "false"),
        ("locked", "false"),
        ("delay", "2"),
    ]
    .map(|(k, v)| (k.into(), v.into()))
    .into();
    dustroute_minecraft::piston_electrical::validate_evidence(&b).unwrap();
    for bad in 0..7 {
        let mut b = b.clone();
        match bad {
            0 => b.facing = Some(Facing::Up),
            1 => b.support_offset = Some(Facing::North.offset()),
            2 => b.delay = Some(0),
            3 => b.delay = Some(5),
            4 => {
                b.observed_properties.remove("locked");
            }
            5 => {
                b.observed_properties.insert("delay".into(), "1".into());
            }
            _ => b.observed_name = Some("minecraft:unregistered_repeater".into()),
        }
        assert!(
            dustroute_minecraft::piston_electrical::validate_evidence(&b).is_err(),
            "case {bad}"
        );
    }
}
