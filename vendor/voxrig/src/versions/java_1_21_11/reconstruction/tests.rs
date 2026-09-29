use super::*;

fn world() -> World {
    let mut w = World::default();
    w.select_dimension(
        "minecraft:overworld".into(),
        super::super::world::Dimension::new(-64, 384).unwrap(),
    );
    for x in -1..=1 {
        for z in -1..=1 {
            w.seed_replay_cell([x * 16, 80, z * 16], 0);
        }
    }
    w
}
fn put(w: &mut World, p: Pos, s: NativeBlockState) {
    w.seed_replay_cell(p, super::super::state_id(&s).unwrap());
}
fn body(sticky: bool, dir: Direction) -> NativeBlockState {
    state(
        if sticky { "sticky_piston" } else { "piston" },
        &[("facing", dir.name()), ("extended", "false")],
    )
}
fn name(r: &Reconstruction, w: &World, p: Pos) -> String {
    r.cell(w, p).state.unwrap().name
}

fn wire(power: &str, sides: [&str; 4]) -> NativeBlockState {
    state(
        "redstone_wire",
        &[
            ("power", power),
            ("north", sides[0]),
            ("east", sides[1]),
            ("south", sides[2]),
            ("west", sides[3]),
        ],
    )
}

#[test]
fn moving_support_removes_wire_gates_and_buttons_without_running_server_timers() {
    let devices = [
        wire("13", ["side"; 4]),
        state(
            "repeater",
            &[
                ("facing", "north"),
                ("delay", "4"),
                ("locked", "true"),
                ("powered", "true"),
            ],
        ),
        state(
            "comparator",
            &[
                ("facing", "north"),
                ("mode", "subtract"),
                ("powered", "true"),
            ],
        ),
        state(
            "oak_button",
            &[("face", "floor"), ("facing", "north"), ("powered", "true")],
        ),
    ];
    for device in devices {
        let mut w = world();
        put(&mut w, [0, 80, 0], body(false, Direction::East));
        put(&mut w, [1, 80, 0], state("cyan_wool", &[]));
        put(&mut w, [1, 81, 0], device.clone());
        let mut r = Reconstruction::default();
        // A supported component keeps every server-owned property.
        r.update_neighbor(&w, [1, 81, 0], Direction::Down, 1.into(), 0)
            .unwrap();
        assert_eq!(r.cell(&w, [1, 81, 0]).state, Some(device));
        r.action(
            &w,
            [0, 80, 0],
            Action::Extend,
            Direction::East,
            "minecraft:piston",
            2,
        );
        assert!(r.issue.is_none(), "{:?}", r.issue);
        assert_eq!(name(&r, &w, [1, 81, 0]), "minecraft:air");
        r.advance(&w, 8);
        assert_eq!(name(&r, &w, [2, 80, 0]), "minecraft:cyan_wool");
        assert_eq!(name(&r, &w, [1, 81, 0]), "minecraft:air");
    }
}

#[test]
fn wire_layout_preserves_power_and_dot_but_rebuilds_horizontal_connections() {
    let p = [0, 80, 0];
    let mut w = world();
    put(&mut w, [0, 79, 0], state("stone", &[]));
    let dot = wire("9", ["none"; 4]);
    put(&mut w, p, dot.clone());
    let mut r = Reconstruction::default();
    r.update_neighbor(&w, p, Direction::Up, 1.into(), 0)
        .unwrap();
    assert_eq!(r.cell(&w, p).state, Some(dot));
    put(
        &mut w,
        [1, 80, 0],
        state("observer", &[("facing", "east"), ("powered", "false")]),
    );
    r.update_neighbor(&w, p, Direction::East, 2.into(), 0)
        .unwrap();
    assert_eq!(
        r.cell(&w, p).state,
        Some(wire("9", ["none", "side", "none", "side"]))
    );
    // An observer's detecting face and the side of a repeater do not connect.
    for block in [
        state("observer", &[("facing", "west"), ("powered", "false")]),
        state(
            "repeater",
            &[
                ("facing", "north"),
                ("delay", "4"),
                ("locked", "false"),
                ("powered", "false"),
            ],
        ),
    ] {
        put(&mut w, [1, 80, 0], block);
        let after = r
            .wire_update(&w, p, wire("9", ["side"; 4]), Direction::East)
            .unwrap();
        assert_eq!(after, wire("9", ["side"; 4]));
    }
    put(
        &mut w,
        [1, 80, 0],
        state(
            "repeater",
            &[
                ("facing", "west"),
                ("delay", "4"),
                ("locked", "false"),
                ("powered", "true"),
            ],
        ),
    );
    assert_eq!(
        r.wire_update(&w, p, wire("9", ["side"; 4]), Direction::East)
            .unwrap(),
        wire("9", ["none", "side", "none", "side"])
    );
}

#[test]
fn wire_slopes_distinguish_support_from_conduction_and_prepare_diagonal_neighbors() {
    let p = [0, 80, 0];
    let mut w = world();
    put(&mut w, [0, 79, 0], state("stone", &[]));
    put(&mut w, [1, 80, 0], state("glass", &[]));
    put(
        &mut w,
        [1, 81, 0],
        wire("7", ["none", "side", "none", "side"]),
    );
    put(&mut w, p, wire("7", ["none", "up", "none", "side"]));
    let mut r = Reconstruction::default();
    assert_eq!(
        r.wire_update(&w, p, wire("7", ["side"; 4]), Direction::Up)
            .unwrap(),
        wire("7", ["none", "up", "none", "side"])
    );
    // Glass has a full supporting face, but permits a downward connection.
    let upper = [1, 81, 0];
    put(&mut w, [0, 81, 0], state("glass", &[]));
    assert_eq!(
        r.wire_update(&w, upper, wire("7", ["side"; 4]), Direction::West)
            .unwrap(),
        wire("7", ["none", "side", "none", "side"])
    );
    put(&mut w, [0, 81, 0], state("stone", &[]));
    assert_eq!(
        r.wire_update(&w, upper, wire("7", ["side"; 4]), Direction::West)
            .unwrap(),
        wire("7", ["side"; 4])
    );
    put(&mut w, [0, 81, 0], state("air", &[]));
    // Removing the lower wire runs its old prepare callback even though the
    // upper wire is diagonal and ordinary six-neighbor updates cannot reach it.
    r.put(&w, p, state("air", &[]), 3, true, 0).unwrap();
    assert_eq!(r.cell(&w, upper).state, Some(wire("7", ["side"; 4])));
}

#[test]
fn normal_and_sticky_carriers_complete_in_all_six_directions() {
    let p = [0, 80, 0];
    for dir in Direction::ALL {
        for sticky in [false, true] {
            let mut w = world();
            put(&mut w, p, body(sticky, dir));
            put(&mut w, dir.offset(p, 1), state("stone", &[]));
            let mut r = Reconstruction::default();
            let piston = if sticky {
                "minecraft:sticky_piston"
            } else {
                "minecraft:piston"
            };
            r.action(&w, p, Action::Extend, dir, piston, 5);
            assert!(r.issue.is_none(), "{:?}", r.issue);
            assert_eq!(r.moving.len(), 2);
            assert_eq!(r.moving[&dir.offset(p, 2)].role, CarrierRole::Payload);
            r.advance(&w, 1);
            assert_eq!(r.moving[&dir.offset(p, 2)].progress, MotionProgress::Half);
            assert_eq!(
                r.moving[&dir.offset(p, 2)].last_progress,
                MotionProgress::Start
            );
            r.advance(&w, 2);
            assert_eq!(r.moving[&dir.offset(p, 2)].progress, MotionProgress::Full);
            r.advance(&w, 7);
            assert_eq!(r.moving.len(), 2);
            r.advance(&w, 8);
            assert!(r.moving.is_empty());
            assert_eq!(name(&r, &w, dir.offset(p, 2)), "minecraft:stone");
            r.action(&w, p, Action::Retract, dir, piston, 6);
            assert!(r.issue.is_none(), "{:?}", r.issue);
            assert_eq!(r.moving[&p].role, CarrierRole::Body);
            assert_eq!(r.moving.len(), if sticky { 2 } else { 1 });
            r.advance(&w, 16);
            assert_eq!(name(&r, &w, p), piston);
            assert_eq!(
                name(&r, &w, dir.offset(p, 1)),
                if sticky {
                    "minecraft:stone"
                } else {
                    "minecraft:air"
                }
            );
            assert_eq!(
                name(&r, &w, dir.offset(p, 2)),
                if sticky {
                    "minecraft:air"
                } else {
                    "minecraft:stone"
                }
            );
            // Received cache is never overwritten by predicted motion.
            assert_eq!(
                super::super::native_state(w.block(dir.offset(p, 1)).unwrap())
                    .unwrap()
                    .name,
                "minecraft:stone"
            );
        }
    }
}

#[test]
fn retracting_piston_back_keeps_mounted_component_supported_in_all_directions() {
    let p = [0, 80, 0];
    for dir in Direction::ALL {
        let back = dir.opposite();
        let lever = state(
            "lever",
            &[
                (
                    "face",
                    match back {
                        Direction::Up => "floor",
                        Direction::Down => "ceiling",
                        _ => "wall",
                    },
                ),
                (
                    "facing",
                    if back.horizontal() {
                        back.name()
                    } else {
                        "north"
                    },
                ),
                ("powered", "false"),
            ],
        );
        let attachment = back.offset(p, 1);
        let mut w = world();
        put(&mut w, p, body(true, dir));
        put(&mut w, attachment, lever.clone());
        let mut r = Reconstruction::default();
        r.action(&w, p, Action::Extend, dir, "minecraft:sticky_piston", 1);
        r.advance(&w, 8);
        r.action(&w, p, Action::Retract, dir, "minecraft:sticky_piston", 2);
        for tick in 8..=16 {
            r.advance(&w, tick);
            assert!(r.issue.is_none(), "{dir:?} {:?}", r.issue);
            assert_eq!(
                r.cell(&w, attachment).state,
                Some(lever.clone()),
                "{dir:?} at frame {tick}"
            );
        }
    }
}

#[test]
fn forced_completion_and_drop_retraction_leave_payload_behind() {
    let p = [0, 80, 0];
    let dir = Direction::East;
    for ticks in [0, 1, 2, 7, 8] {
        for action in [Action::Retract, Action::Drop] {
            let mut w = world();
            put(&mut w, p, body(true, dir));
            put(&mut w, [1, 80, 0], state("stone", &[]));
            let mut r = Reconstruction::default();
            r.action(&w, p, Action::Extend, dir, "minecraft:sticky_piston", 1);
            r.advance(&w, ticks);
            r.action(&w, p, action, dir, "minecraft:sticky_piston", 2);
            assert!(r.issue.is_none(), "{:?}", r.issue);
            r.advance(&w, ticks + 8);
            let pulls = ticks == 8 && action == Action::Retract;
            assert_eq!(
                name(&r, &w, [1, 80, 0]),
                if pulls {
                    "minecraft:stone"
                } else {
                    "minecraft:air"
                }
            );
            assert_eq!(
                name(&r, &w, [2, 80, 0]),
                if pulls {
                    "minecraft:air"
                } else {
                    "minecraft:stone"
                }
            );
        }
    }
}

#[test]
fn native_update_cancels_carrier_so_late_completion_cannot_restore_it() {
    let p = [0, 80, 0];
    let mut w = world();
    put(&mut w, p, body(false, Direction::East));
    put(&mut w, [1, 80, 0], state("stone", &[]));
    let mut r = Reconstruction::default();
    r.action(
        &w,
        p,
        Action::Extend,
        Direction::East,
        "minecraft:piston",
        1,
    );
    put(&mut w, [2, 80, 0], state("air", &[]));
    r.received(&[([2, 80, 0], 0)]);
    r.advance(&w, 100);
    assert_eq!(name(&r, &w, [2, 80, 0]), "minecraft:air");
    assert!(!r.moving.contains_key(&[2, 80, 0]));
}

#[test]
fn unsupported_and_missing_dependencies_never_publish_partial_motion() {
    let p = [0, 80, 0];
    let mut w = world();
    put(&mut w, p, body(false, Direction::East));
    put(&mut w, [1, 80, 0], state("sculk", &[]));
    let mut r = Reconstruction::default();
    r.action(
        &w,
        p,
        Action::Extend,
        Direction::East,
        "minecraft:piston",
        1,
    );
    assert!(matches!(
        r.issue,
        Some(ReconstructionIssue::UnsupportedBlock { .. })
    ));
    assert!(r.overlay.is_empty());
    assert!(r.cell(&w, p).state.is_none());
    let mut r = Reconstruction::default();
    r.action(
        &w,
        [1000, 80, 1000],
        Action::Extend,
        Direction::East,
        "minecraft:piston",
        1,
    );
    assert!(matches!(
        r.issue,
        Some(ReconstructionIssue::MissingBlock { .. })
    ));
}

#[test]
fn chunk_invalidation_and_unknown_carriers_are_explicit() {
    let p = [0, 80, 0];
    let mut w = world();
    put(&mut w, p, body(false, Direction::East));
    let mut r = Reconstruction::default();
    r.action(
        &w,
        p,
        Action::Extend,
        Direction::East,
        "minecraft:piston",
        1,
    );
    r.chunk_replaced([0, 0]);
    assert!(matches!(
        r.issue,
        Some(ReconstructionIssue::ChunkInvalidated { .. })
    ));
    let mut r = Reconstruction::default();
    let moving = super::super::state_id(&state(
        "moving_piston",
        &[("facing", "east"), ("type", "normal")],
    ))
    .unwrap();
    r.received(&[(p, moving)]);
    assert!(matches!(
        r.issue,
        Some(ReconstructionIssue::MissingCarrier { .. })
    ));
}

#[test]
fn known_carrier_can_finish_after_receiving_a_moving_state_packet() {
    let p = [0, 80, 0];
    let dest = [2, 80, 0];
    let mut w = world();
    put(&mut w, p, body(false, Direction::East));
    put(&mut w, [1, 80, 0], state("stone", &[]));
    let mut r = Reconstruction::default();
    r.action(
        &w,
        p,
        Action::Extend,
        Direction::East,
        "minecraft:piston",
        1,
    );
    let moving = state("moving_piston", &[("facing", "east"), ("type", "normal")]);
    let id = super::super::state_id(&moving).unwrap();
    put(&mut w, dest, moving);
    r.received(&[(dest, id)]);
    r.advance(&w, 8);
    assert_eq!(name(&r, &w, dest), "minecraft:stone");
    assert!(r.issue.is_none());
}

#[test]
fn shape_rules_preserve_all_native_stair_properties() {
    let mut w = world();
    let target = [2, 80, -1];
    put(&mut w, [0, 80, 0], body(false, Direction::East));
    put(&mut w, [1, 80, 0], state("stone", &[]));
    put(
        &mut w,
        [2, 80, 0],
        state(
            "cobblestone_stairs",
            &[
                ("facing", "west"),
                ("half", "top"),
                ("shape", "straight"),
                ("waterlogged", "false"),
            ],
        ),
    );
    put(
        &mut w,
        target,
        state(
            "quartz_stairs",
            &[
                ("facing", "north"),
                ("half", "top"),
                ("shape", "inner_left"),
                ("waterlogged", "false"),
            ],
        ),
    );
    let mut r = Reconstruction::default();
    r.action(
        &w,
        [0, 80, 0],
        Action::Extend,
        Direction::East,
        "minecraft:piston",
        42,
    );
    assert!(r.issue.is_none(), "{:?}", r.issue);
    let cell = r.cell(&w, target);
    assert_eq!(cell.state.unwrap().properties["shape"], "straight");
    assert!(matches!(
        cell.origin,
        StateOrigin::ClientUpdate {
            action_sequence: 42
        }
    ));
}

#[test]
fn complete_state_codec_roundtrips_every_bundled_state_and_rejects_missing_properties() {
    let definitions: serde_json::Value =
        serde_json::from_str(include_str!("../../../../data/java_1_21_11/blocks.json")).unwrap();
    let max = definitions.as_array().unwrap().last().unwrap()["maxStateId"]
        .as_i64()
        .unwrap() as i32;
    for id in 0..=max {
        let s = super::super::native_state(id).unwrap();
        assert_eq!(super::super::state_id(&s).unwrap(), id);
    }
    assert!(super::super::state_id(&state("piston", &[("facing", "east")])).is_err());
}

#[test]
fn slime_and_honey_pull_attached_payloads_in_six_directions_but_do_not_bond_to_each_other() {
    let p = [0, 80, 0];
    for dir in Direction::ALL {
        let side = if dir.horizontal() {
            Direction::Up
        } else {
            Direction::East
        };
        for (material, other) in [
            ("slime_block", "honey_block"),
            ("honey_block", "slime_block"),
        ] {
            let mut w = world();
            let root = dir.offset(p, 1);
            let stone = side.offset(root, 1);
            let separate = side.opposite().offset(root, 1);
            put(&mut w, p, body(true, dir));
            put(&mut w, root, state(material, &[]));
            put(&mut w, stone, state("stone", &[]));
            put(&mut w, separate, state(other, &[]));
            let mut r = Reconstruction::default();
            r.action(&w, p, Action::Extend, dir, "minecraft:sticky_piston", 1);
            assert!(r.issue.is_none(), "{:?}", r.issue);
            assert_eq!(r.moving.len(), 3);
            r.advance(&w, 8);
            assert_eq!(
                name(&r, &w, dir.offset(root, 1)),
                format!("minecraft:{material}")
            );
            assert_eq!(name(&r, &w, dir.offset(stone, 1)), "minecraft:stone");
            assert_eq!(name(&r, &w, separate), format!("minecraft:{other}"));
            r.action(&w, p, Action::Retract, dir, "minecraft:sticky_piston", 2);
            assert!(r.issue.is_none(), "{:?}", r.issue);
            r.advance(&w, 16);
            assert_eq!(name(&r, &w, root), format!("minecraft:{material}"));
            assert_eq!(name(&r, &w, stone), "minecraft:stone");
            assert_eq!(name(&r, &w, separate), format!("minecraft:{other}"));
        }
    }
}

#[test]
fn adhesive_branch_limit_obstructions_and_reverse_line_membership() {
    let p = [0, 80, 0];
    for count in [12, 13] {
        let mut w = world();
        put(&mut w, p, body(false, Direction::East));
        for dy in 0..count {
            put(&mut w, [1, 80 + dy, 0], state("slime_block", &[]));
        }
        let mut r = Reconstruction::default();
        let plan =
            adhesion::MovementPlan::calculate(&mut r, &w, p, [1, 80, 0], Direction::East, true)
                .unwrap();
        assert_eq!(plan.is_some(), count == 12);
        if let Some(plan) = plan {
            assert_eq!(plan.moved.len(), 12);
        }
    }
    let mut w = world();
    put(&mut w, p, body(false, Direction::East));
    for q in [[1, 80, 0], [1, 81, 0]] {
        put(&mut w, q, state("slime_block", &[]));
    }
    put(&mut w, [0, 81, 0], state("stone", &[]));
    put(&mut w, [1, 79, 0], state("obsidian", &[]));
    let mut r = Reconstruction::default();
    let plan = adhesion::MovementPlan::calculate(&mut r, &w, p, [1, 80, 0], Direction::East, true)
        .unwrap()
        .unwrap();
    assert!(plan.moved.contains(&[0, 81, 0]));
    assert!(!plan.moved.contains(&[1, 79, 0]));
    put(&mut w, [2, 81, 0], state("obsidian", &[]));
    assert!(
        adhesion::MovementPlan::calculate(&mut r, &w, p, [1, 80, 0], Direction::East, true)
            .unwrap()
            .is_none()
    );
}

#[test]
fn connected_slime_shapes_move_each_block_once_even_when_branches_collide() {
    let root = [1, 80, 0];
    let positions: Vec<_> = (1..=2)
        .flat_map(|x| (80..=81).flat_map(move |y| (0..=1).map(move |z| [x, y, z])))
        .collect();
    for mask in (1u16..256).filter(|v| v & 1 != 0) {
        let occupied: std::collections::BTreeSet<_> = positions
            .iter()
            .enumerate()
            .filter(|(i, _)| mask & (1 << i) != 0)
            .map(|(_, p)| *p)
            .collect();
        let mut connected = std::collections::BTreeSet::from([root]);
        let mut queue = vec![root];
        while let Some(p) = queue.pop() {
            for dir in Direction::ALL {
                let q = dir.offset(p, 1);
                if occupied.contains(&q) && connected.insert(q) {
                    queue.push(q);
                }
            }
        }
        let mut w = world();
        put(&mut w, [0, 80, 0], body(false, Direction::East));
        for p in &occupied {
            put(&mut w, *p, state("slime_block", &[]));
        }
        let mut r = Reconstruction::default();
        r.action(
            &w,
            [0, 80, 0],
            Action::Extend,
            Direction::East,
            "minecraft:piston",
            1,
        );
        assert!(r.issue.is_none(), "mask={mask}: {:?}", r.issue);
        assert_eq!(r.moving.len(), connected.len() + 1, "mask={mask}");
        r.advance(&w, 8);
        for p in connected {
            assert_eq!(
                name(&r, &w, Direction::East.offset(p, 1)),
                "minecraft:slime_block",
                "mask={mask}"
            );
        }
    }
}
