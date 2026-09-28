use dustroute_minecraft::physical::{self, passive};
use dustroute_minecraft::piston_electrical::{
    ElectricalWorld, HORIZONTAL, SIDES, conducts, full_face, validate_evidence,
};
use dustroute_minecraft::time::piston_runtime::{
    new_piston_runtime, schedule_electrical_input_after_tick,
};
use dustroute_minecraft::{
    Block, BlockKind, CapabilityLevel, Facing, ObservationClassification, Pos, Region,
    WireConnection, World,
};

fn observed(name: &str, slab: Option<&str>) -> Block {
    let (kind, classification) = physical::classify(name);
    let mut b = Block::new(kind);
    b.observed_name = Some(name.into());
    b.observation_classification = classification;
    if let Some(slab) = slab {
        b.observed_properties = [
            ("type".into(), slab.into()),
            ("waterlogged".into(), "false".into()),
        ]
        .into();
    }
    b
}

#[test]
fn slab_state_selects_support_faces_and_conduction_without_changing_kind() {
    for name in ["minecraft:stone_slab", "minecraft:smooth_stone_slab"] {
        for state in ["bottom", "top", "double"] {
            let b = observed(name, Some(state));
            validate_evidence(&b).unwrap();
            assert_eq!(b.kind, BlockKind::Transparent);
            assert_eq!(conducts(&b), state == "double");
            for side in SIDES {
                let supports = state == "double"
                    || (state == "top" && side == Facing::Up)
                    || (state == "bottom" && side == Facing::Down);
                assert_eq!(full_face(&b, side), supports);
                assert_eq!(
                    physical::of_block(&b).unwrap().center_face(&b, side),
                    supports
                );
                for component in [
                    BlockKind::Lever,
                    BlockKind::Button,
                    BlockKind::RedstoneTorch,
                ] {
                    assert_eq!(
                        physical::of_kind(component).supports_attachment(&b, side.opposite()),
                        supports
                    );
                }
            }
        }
    }
    for spec in passive::BUILTINS {
        if let passive::PassiveStates::Fixed(_) = spec.states {
            for name in spec.names {
                let b = observed(name, None);
                validate_evidence(&b).unwrap();
                assert_eq!(conducts(&b), spec.kind == BlockKind::Solid);
                assert!(SIDES.into_iter().all(|s| full_face(&b, s)));
            }
        }
    }
}

#[test]
fn incomplete_wet_unknown_and_misclassified_passives_never_gain_admission() {
    let valid = observed("minecraft:stone_slab", Some("top"));
    let mut invalid = Vec::new();
    for key in ["type", "waterlogged"] {
        let mut b = valid.clone();
        b.observed_properties.remove(key);
        invalid.push(b);
    }
    for (key, value) in [
        ("type", "upper"),
        ("waterlogged", "true"),
        ("extra", "false"),
    ] {
        let mut b = valid.clone();
        b.observed_properties.insert(key.into(), value.into());
        invalid.push(b);
    }
    for name in [
        "minecraft:oak_slab",
        "minecraft:glass_pane",
        "some_mod:stone_slab",
        "minecraft:stone_stairs",
    ] {
        let mut b = valid.clone();
        b.observed_name = Some(name.into());
        invalid.push(b);
    }
    let mut b = valid.clone();
    b.kind = BlockKind::Solid;
    invalid.push(b);
    let mut b = valid;
    b.observation_classification = ObservationClassification::Coarse;
    invalid.push(b);
    for b in invalid {
        assert!(physical::of_block(&b).is_none(), "{b:?}");
        assert!(validate_evidence(&b).is_err());
        assert!(!conducts(&b));
        assert!(SIDES.into_iter().all(|s| !full_face(&b, s)));
        if b.observed_name
            .as_deref()
            .is_some_and(|n| passive::named(n).is_some())
        {
            assert_eq!(b.capabilities().placement, CapabilityLevel::Unsupported);
        }
    }
}

#[test]
fn slab_top_support_does_not_imply_a_full_vertical_wire_rise_face() {
    for (name, slab, rise) in [
        ("minecraft:stone_slab", Some("top"), WireConnection::Side),
        ("minecraft:stone_slab", Some("double"), WireConnection::Up),
        ("minecraft:tinted_glass", None, WireConnection::Up),
    ] {
        let support = observed(name, slab);
        assert_eq!(
            physical::of_block(&support)
                .unwrap()
                .block_traits(&support)
                .wire_rise_connection,
            Some(rise)
        );
        let mut world = World::new();
        world.place(BlockKind::Solid, Pos::new(-1, 0, 0));
        world.set(Pos::new(0, 1, 0), observed(name, slab));
        for pos in [Pos::new(-1, 1, 0), Pos::new(0, 2, 0)] {
            let wire = world.place(BlockKind::RedstoneWire, pos);
            wire.support_offset = Some(Facing::Down.offset());
            wire.power_level = Some(0);
            wire.wire_connections = Some(HORIZONTAL.map(|d| (d, WireConnection::None)).into());
        }
        let query =
            ElectricalWorld::new(&world, Region::new(Pos::new(-4, -3, -3), Pos::new(4, 5, 3)))
                .unwrap();
        assert_eq!(
            query.wire_shape(Pos::new(-1, 1, 0)).unwrap()[&Facing::East],
            rise
        );
    }
}

#[test]
fn pushed_and_pulled_slabs_preserve_all_passive_properties() {
    use dustroute_minecraft::{PistonState, PistonVariant};
    for state in ["bottom", "top", "double"] {
        let payload = observed("minecraft:stone_slab", Some(state));
        let mut world = World::new();
        let body = world.place(BlockKind::Piston, Pos::default());
        body.facing = Some(Facing::East);
        body.piston_variant = Some(PistonVariant::Sticky);
        world.set(Pos::new(1, 0, 0), payload.clone());
        world.place(BlockKind::Solid, Pos::new(-2, 0, 0));
        let lever = world.place(BlockKind::Lever, Pos::new(-1, 0, 0));
        lever.powered = Some(false);
        lever.support_offset = Some(Facing::West.offset());
        let mut run = new_piston_runtime(
            world,
            Region::new(Pos::new(-5, -4, -4), Pos::new(6, 4, 4)),
            Default::default(),
        )
        .unwrap();
        schedule_electrical_input_after_tick(&mut run, 1, Pos::new(-1, 0, 0), true).unwrap();
        run.run_until_idle().unwrap();
        assert_eq!(run.view().block(Pos::new(2, 0, 0)).unwrap(), payload);
        let t = run.view().time().game_tick + 2;
        schedule_electrical_input_after_tick(&mut run, t, Pos::new(-1, 0, 0), false).unwrap();
        run.run_until_idle().unwrap();
        assert_eq!(run.view().block(Pos::new(1, 0, 0)).unwrap(), payload);
        assert_eq!(
            run.view().block(Pos::default()).unwrap().piston_state,
            Some(PistonState::Retracted)
        );
    }
}
