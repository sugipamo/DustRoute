use dustroute_minecraft::device_program::{BoolProperty, Property, program};
use dustroute_minecraft::piston_electrical::{ElectricalWorld, SIDES, validate_evidence};
use dustroute_minecraft::time::piston_runtime::{ElectricalPistonRuntime, new_piston_runtime};
use dustroute_minecraft::{
    Block, BlockKind, Facing, HistoricalPlacementV1, Pos, Region, ValidatedWorld, World,
};

const B: Pos = Pos::new(0, 3, 0);
fn region() -> Region {
    Region::new(Pos::new(-5, -3, -5), Pos::new(5, 10, 5))
}
fn along(pos: Pos, side: Facing) -> Pos {
    let d = side.offset();
    pos.offset(d.x, d.y, d.z)
}
fn bulb(lit: bool, powered: bool, name: Option<&str>) -> Block {
    let mut b = Block::new(BlockKind::CopperBulb);
    b.powered = Some(lit);
    b.observed_properties
        .insert("powered".into(), powered.to_string());
    if let Some(name) = name {
        b.observed_name = Some(name.into());
        b.observed_properties.insert("lit".into(), lit.to_string());
    }
    b
}
fn state(b: &Block) -> (bool, bool) {
    let d = program(b).unwrap().definition();
    (
        d.state(b, Property::Bool(BoolProperty::Lit)).unwrap() != 0,
        d.state(b, Property::Bool(BoolProperty::Powered)).unwrap() != 0,
    )
}

#[test]
fn six_faces_toggle_only_on_rising_edges_and_lit_is_not_signal_emission() {
    for side in SIDES {
        for initial_lit in [false, true] {
            let mut world = World::new();
            world.set(B, bulb(initial_lit, false, None));
            let input = along(B, side);
            let lever = world.place(BlockKind::Lever, input);
            lever.powered = Some(false);
            lever.support_offset = Some(side.opposite().offset());
            let mut rt = new_piston_runtime(world, region(), Default::default()).unwrap();
            rt.run_until_idle().unwrap();
            let mut lit = initial_lit;
            let mut was_powered = false;
            for powered in [true, true, false, false, true, false] {
                if powered && !was_powered {
                    lit = !lit;
                }
                rt.input_now(input, powered).unwrap();
                rt.run_until_idle().unwrap();
                assert_eq!(state(&rt.view().block(B).unwrap()), (lit, powered));
                let world = ElectricalWorld::new(rt.view().world(), region()).unwrap();
                assert_eq!(
                    world.comparator_readout(B).unwrap(),
                    Some(if lit { 15 } else { 0 })
                );
                for face in SIDES {
                    let emitted = world.emission(B, face, true).unwrap();
                    assert_eq!((emitted.weak, emitted.strong), (0, 0));
                }
                assert_eq!(rt.pending_count(), 0);
                was_powered = powered;
            }
        }
    }
}

#[test]
fn all_initial_states_sample_power_without_inventing_an_edge() {
    for lit in [false, true] {
        for previous in [false, true] {
            for powered in [false, true] {
                let mut world = World::new();
                world.set(B, bulb(lit, previous, None));
                if powered {
                    world.place(BlockKind::RedstoneBlock, along(B, Facing::West));
                }
                let mut rt = new_piston_runtime(world, region(), Default::default()).unwrap();
                rt.run_until_idle().unwrap();
                assert_eq!(
                    state(&rt.view().block(B).unwrap()),
                    (lit ^ (powered && !previous), powered)
                );
            }
        }
    }
}

#[test]
fn installed_power_sampling_commits_both_bits_and_restores_every_microstep() {
    for name in [
        "waxed_copper_bulb",
        "waxed_exposed_copper_bulb",
        "waxed_weathered_copper_bulb",
        "waxed_oxidized_copper_bulb",
    ] {
        let mut world = World::new();
        world.place(BlockKind::RedstoneBlock, along(B, Facing::West));
        let mut rt = new_piston_runtime(world, region(), Default::default()).unwrap();
        rt.run_until_idle().unwrap();
        rt.install_now(B, bulb(false, false, Some(name))).unwrap();
        let mut checkpoints = vec![];
        while rt.microstep().unwrap().is_some() {
            checkpoints.push(rt.checkpoint());
        }
        assert_eq!(state(&rt.view().block(B).unwrap()), (true, true));
        let writes: Vec<_> = rt
            .trace()
            .iter()
            .flat_map(|r| r.delta.iter().flat_map(|d| &d.changes))
            .filter(|c| c.position == B && c.before.kind == BlockKind::CopperBulb)
            .collect();
        assert_eq!(writes.len(), 1);
        assert_eq!(state(&writes[0].before), (false, false));
        assert_eq!(state(&writes[0].after), (true, true));
        for checkpoint in checkpoints {
            let mut restored = ElectricalPistonRuntime::from_checkpoint(&checkpoint).unwrap();
            restored.run_until_idle().unwrap();
            assert_eq!(restored.state_key(), rt.state_key());
        }
        let mut restored =
            ElectricalPistonRuntime::from_behavior_state(&rt.behavior_state().unwrap()).unwrap();
        assert_eq!(state(&restored.view().block(B).unwrap()), (true, true));
        restored.remove_now(along(B, Facing::West)).unwrap();
        restored.run_until_idle().unwrap();
        assert_eq!(state(&restored.view().block(B).unwrap()), (true, false));
    }
}

#[test]
fn new_geometry_supports_placement_without_extending_historical_or_payload_laws() {
    let mut world = World::new();
    world.set(B, bulb(false, false, None));
    let lever = world.place(BlockKind::Lever, along(B, Facing::Up));
    lever.powered = Some(false);
    lever.support_offset = Some(Facing::Down.offset());
    ValidatedWorld::try_from(world.clone()).unwrap();
    assert!(HistoricalPlacementV1::try_from(world.clone()).is_err());
    let b = world.get(B).unwrap();
    assert_eq!(
        dustroute_minecraft::piston_law::builtin_piston_laws()
            .payload_rejection(b)
            .unwrap(),
        1
    );
    assert_eq!(
        dustroute_minecraft::piston_law::electrical_payload_laws()
            .payload_rejection(b)
            .unwrap(),
        1
    );
    assert!(
        dustroute_minecraft::piston_law::builtin_piston_laws()
            .input_connected(b, Facing::East)
            .is_err()
    );
    let mut engine = dustroute_minecraft::time::PhysicsEngine::new_diagnostic(world, 32);
    assert!(engine.run_redstone_propagation().is_err());
    let mut engine = dustroute_minecraft::time::PhysicsEngine::new_diagnostic(World::new(), 32);
    engine.schedule_world_change(1, B, bulb(false, false, None));
    assert!(engine.run_redstone_propagation().is_err());
    assert!(
        engine.world().get(B).is_none(),
        "unsupported insertion must not write first"
    );
    for name in [
        "copper_bulb",
        "exposed_copper_bulb",
        "another_mod:waxed_copper_bulb",
    ] {
        assert!(validate_evidence(&bulb(false, false, Some(name))).is_err());
    }
    let mut incomplete = bulb(false, false, Some("waxed_copper_bulb"));
    incomplete.observed_properties.remove("powered");
    assert!(validate_evidence(&incomplete).is_err());
}
