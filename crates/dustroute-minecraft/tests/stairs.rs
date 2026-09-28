use dustroute_minecraft::physical::{
    self,
    stairs::{self, StairShape},
};
use dustroute_minecraft::piston_electrical::{self, HORIZONTAL, SIDES};
use dustroute_minecraft::time::piston_runtime::{
    ElectricalPistonRuntime, new_piston_runtime, schedule_electrical_input_after_tick,
};
use dustroute_minecraft::{
    Block, BlockKind, Facing, ObservationClassification, Pos, Region, RotationY, ValidatedWorld,
    WireConnection, World,
};

fn stair(name: &str, top: bool, shape: StairShape, rotation: RotationY) -> Block {
    let mut b = Block::new(BlockKind::Transparent);
    b.observed_name = Some(format!("minecraft:{name}"));
    b.observation_classification = ObservationClassification::Exact;
    b.observed_properties = [
        ("facing".into(), "north".into()),
        ("half".into(), if top { "top" } else { "bottom" }.into()),
        ("shape".into(), shape.name().into()),
        ("waterlogged".into(), "false".into()),
    ]
    .into();
    rotation.block(&b)
}
fn region() -> Region {
    Region::new(Pos::new(-8, -5, -8), Pos::new(8, 8, 8))
}
const ROTATIONS: [RotationY; 4] = [
    RotationY::R0,
    RotationY::R90,
    RotationY::R180,
    RotationY::R270,
];

#[test]
fn all_stair_states_match_faces_of_the_source_cuboids_and_rotate_losslessly() {
    for name in [
        "stone_stairs",
        "cobblestone_stairs",
        "quartz_stairs",
        "smooth_quartz_stairs",
    ] {
        for top in [false, true] {
            for shape in StairShape::ALL {
                for rotation in ROTATIONS {
                    let b = stair(name, top, shape, rotation);
                    piston_electrical::validate_evidence(&b).unwrap();
                    let p = physical::of_block(&b).unwrap();
                    assert!(!p.conducts(&b));
                    assert_eq!(
                        rotation.inverse().block(&b),
                        stair(name, top, shape, RotationY::R0)
                    );
                    // Independent 2x2x2 occupancy from native cuboid unions, including
                    // the missing quadrant of inner/outer corners.
                    let mut occupied = Vec::new();
                    for x in 0..2 {
                        for y in 0..2 {
                            for z in 0..2 {
                                let high = match shape {
                                    StairShape::Straight => z == 0,
                                    StairShape::InnerLeft => z == 0 || x == 0,
                                    StairShape::InnerRight => z == 0 || x == 1,
                                    StairShape::OuterLeft => z == 0 && x == 0,
                                    StairShape::OuterRight => z == 0 && x == 1,
                                };
                                if y == i32::from(top) || high {
                                    occupied.push(Pos::new(x, y, z));
                                }
                            }
                        }
                    }
                    for side in SIDES {
                        let count = occupied
                            .iter()
                            .filter(|p| match side {
                                Facing::Up => p.y == 1,
                                Facing::Down => p.y == 0,
                                Facing::North => p.z == 0,
                                Facing::South => p.z == 1,
                                Facing::West => p.x == 0,
                                Facing::East => p.x == 1,
                            })
                            .count();
                        let face = rotation.facing(side);
                        assert_eq!(
                            p.full_face(&b, face),
                            count == 4,
                            "{name} {top} {shape:?} {rotation:?} {face:?}"
                        );
                        assert_eq!(p.center_face(&b, face), count == 4);
                    }
                }
            }
        }
    }
}

#[test]
fn incomplete_wet_or_unregistered_stairs_are_not_executable() {
    let valid = stair("stone_stairs", true, StairShape::Straight, RotationY::R0);
    let mut invalid = Vec::new();
    for key in ["facing", "half", "shape", "waterlogged"] {
        let mut b = valid.clone();
        b.observed_properties.remove(key);
        invalid.push(b);
    }
    for (key, value) in [
        ("facing", "up"),
        ("half", "double"),
        ("shape", "curved"),
        ("waterlogged", "true"),
        ("extra", "false"),
    ] {
        let mut b = valid.clone();
        b.observed_properties.insert(key.into(), value.into());
        invalid.push(b);
    }
    for name in ["minecraft:oak_stairs", "custom:stone_stairs"] {
        let mut b = valid.clone();
        b.observed_name = Some(name.into());
        invalid.push(b);
    }
    let mut b = valid.clone();
    b.facing = Some(Facing::South);
    invalid.push(b);
    let mut b = valid;
    b.observation_classification = ObservationClassification::Coarse;
    invalid.push(b);
    for b in invalid {
        assert!(physical::of_block(&b).is_none());
        assert!(piston_electrical::validate_evidence(&b).is_err());
    }
}

#[test]
fn native_corner_precedence_parallel_suppression_and_half_mismatch() {
    for rotation in ROTATIONS {
        for top in [false, true] {
            for left in [false, true] {
                let own =
                    stairs::state(&stair("stone_stairs", top, StairShape::Straight, rotation))
                        .unwrap();
                let turn = if left {
                    RotationY::R270
                } else {
                    RotationY::R90
                };
                let other = stairs::state(&stair(
                    "quartz_stairs",
                    top,
                    StairShape::Straight,
                    rotation.then(turn),
                ))
                .unwrap();
                let query = |pairs: &[(Facing, stairs::StairState)]| {
                    own.neighbor_shape::<()>(|f| {
                        Ok(pairs.iter().find(|(s, _)| *s == f).map(|(_, s)| *s))
                    })
                    .unwrap()
                };
                let outer = if left {
                    StairShape::OuterLeft
                } else {
                    StairShape::OuterRight
                };
                let inner = if left {
                    StairShape::InnerLeft
                } else {
                    StairShape::InnerRight
                };
                assert_eq!(
                    query(&[(own.facing, other), (own.facing.opposite(), other)]),
                    outer
                );
                assert_eq!(query(&[(own.facing.opposite(), other)]), inner);
                assert_eq!(
                    query(&[(own.facing, other), (other.facing.opposite(), own)]),
                    StairShape::Straight
                );
                assert_eq!(
                    query(&[(own.facing.opposite(), other), (other.facing, own)]),
                    StairShape::Straight
                );
                assert_eq!(
                    query(&[(own.facing, stairs::StairState { top: !top, ..other })]),
                    StairShape::Straight
                );
                assert!(
                    own.neighbor_shape(|_: Facing| Err::<Option<stairs::StairState>, _>(
                        "unobserved"
                    ))
                    .is_err()
                );
            }
        }
    }
}

#[test]
fn shape_loss_detaches_wall_lever_and_restores_pending_callbacks() {
    let mut w = World::new();
    w.set(
        Pos::default(),
        stair("stone_stairs", true, StairShape::InnerLeft, RotationY::R0),
    );
    w.set(
        Pos::new(0, 0, 1),
        stair("quartz_stairs", true, StairShape::Straight, RotationY::R270),
    );
    let lever = w.place(BlockKind::Lever, Pos::new(-1, 0, 0));
    lever.powered = Some(false);
    lever.support_offset = Some(Facing::East.offset());
    let mut run = new_piston_runtime(w, region(), Default::default()).unwrap();
    run.run_until_idle().unwrap();
    run.remove_now(Pos::new(0, 0, 1)).unwrap();
    let mut checkpoint = None;
    while run.microstep().unwrap().is_some() {
        if checkpoint.is_none()
            && run
                .view()
                .block(Pos::default())
                .unwrap()
                .observed_properties["shape"]
                == "straight"
            && run.view().block(Pos::new(-1, 0, 0)).unwrap().kind == BlockKind::Lever
        {
            checkpoint = Some(run.checkpoint());
        }
    }
    assert_eq!(
        run.view().block(Pos::new(-1, 0, 0)).unwrap().kind,
        BlockKind::Air
    );
    let mut restored = ElectricalPistonRuntime::from_checkpoint(
        &checkpoint.expect("shape committed before detach"),
    )
    .unwrap();
    restored.run_until_idle().unwrap();
    assert_eq!(restored.state_key(), run.state_key());
    let state = run.behavior_state().unwrap();
    assert_eq!(
        ElectricalPistonRuntime::from_behavior_state(&state)
            .unwrap()
            .behavior_state()
            .unwrap(),
        state
    );
}

#[test]
fn explicit_command_shape_is_preserved_until_a_horizontal_shape_update() {
    let mut run = new_piston_runtime(World::new(), region(), Default::default()).unwrap();
    run.run_until_idle().unwrap();
    let requested = stair("stone_stairs", true, StairShape::OuterLeft, RotationY::R0);
    run.install_now(Pos::default(), requested.clone()).unwrap();
    run.run_until_idle().unwrap();
    assert_eq!(run.view().block(Pos::default()).unwrap(), requested);
    run.install_now(Pos::new(0, 1, 0), Block::new(BlockKind::Solid))
        .unwrap();
    run.run_until_idle().unwrap();
    assert_eq!(run.view().block(Pos::default()).unwrap(), requested);
    run.install_now(Pos::new(1, 0, 0), Block::new(BlockKind::Solid))
        .unwrap();
    run.run_until_idle().unwrap();
    assert_eq!(
        run.view()
            .block(Pos::default())
            .unwrap()
            .observed_properties["shape"],
        "straight"
    );
}

#[test]
fn moving_stair_recomputes_its_corner_at_arrival() {
    for top in [false, true] {
        for turn in [RotationY::R90, RotationY::R270] {
            let mut w = World::new();
            w.place(BlockKind::Piston, Pos::default()).facing = Some(Facing::East);
            w.set(
                Pos::new(1, 0, 0),
                stair("stone_stairs", top, StairShape::Straight, RotationY::R0),
            );
            w.set(
                Pos::new(2, 0, -1),
                stair("quartz_stairs", top, StairShape::Straight, turn),
            );
            w.place(BlockKind::Solid, Pos::new(-2, 0, 0));
            let l = w.place(BlockKind::Lever, Pos::new(-1, 0, 0));
            l.powered = Some(false);
            l.support_offset = Some(Facing::West.offset());
            let mut run = new_piston_runtime(w, region(), Default::default()).unwrap();
            schedule_electrical_input_after_tick(&mut run, 1, Pos::new(-1, 0, 0), true).unwrap();
            run.run_until_idle().unwrap();
            let b = run.view().block(Pos::new(2, 0, 0)).unwrap();
            assert_eq!(b.observed_name.as_deref(), Some("minecraft:stone_stairs"));
            assert_eq!(
                b.observed_properties["shape"],
                if turn == RotationY::R90 {
                    "outer_right"
                } else {
                    "outer_left"
                }
            );
            assert_eq!(b.observed_properties["waterlogged"], "false");
        }
    }
}

#[test]
fn placement_and_electrical_wire_rises_use_the_same_directional_stair_face() {
    for shape in StairShape::ALL {
        for rotation in ROTATIONS {
            for from in HORIZONTAL {
                let support = stair("stone_stairs", true, shape, rotation);
                let side = from.opposite();
                let expected = physical::wire_rise_connection(&support, side).unwrap();
                let mut w = World::new();
                let lower = from.offset().offset(0, 1, 0);
                w.place(BlockKind::Solid, from.offset());
                w.set(Pos::new(0, 1, 0), support);
                for pos in [lower, Pos::new(0, 2, 0)] {
                    let wire = w.place(BlockKind::RedstoneWire, pos);
                    wire.support_offset = Some(Facing::Down.offset());
                    wire.power_level = Some(0);
                    wire.wire_connections =
                        Some(HORIZONTAL.map(|d| (d, WireConnection::None)).into());
                }
                let q = piston_electrical::ElectricalWorld::new(&w, region()).unwrap();
                let arms = q.wire_shape(lower).unwrap();
                assert_eq!(arms[&side], expected);
                w.get_mut(lower).unwrap().wire_connections = Some(arms);
                assert!(ValidatedWorld::try_from(w).is_ok());
            }
        }
    }
}
