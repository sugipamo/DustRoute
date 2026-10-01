#[path = "support/runtime_blueprint.rs"]
mod fixture;

use dustroute_library::blueprint::BlueprintCatalog;
use dustroute_minecraft::{BlockKind, Facing, Pos, Region, World};
use dustroute_translate::piston_construction::{ElectricalConstruction, electrical_snapshot};
use dustroute_translate::snapshot::MinecraftSnapshot;
use dustroute_translate::snapshot::assembly_from_snapshot;

#[test]
fn mixed_custom_worlds_have_explicit_construction_and_complete_teardown_sequences() {
    for json in [
        include_str!("fixtures/mixed-electrical-observed-b-v1.json"),
        include_str!("fixtures/mixed-electrical-observed-e-v1.json"),
    ] {
        let data: serde_json::Value = serde_json::from_str(json).unwrap();
        let initial: MinecraftSnapshot = serde_json::from_value(data["initial"].clone()).unwrap();
        let region = Region::new(initial.min, initial.max);
        let world = assembly_from_snapshot(&initial, "construction fixture", vec![region])
            .unwrap()
            .inspect(&BlueprintCatalog::default())
            .unwrap()
            .proposed_world();
        let plan = ElectricalConstruction::new(&world, region, Default::default()).unwrap();
        assert_eq!(plan.initial().blocks.len(), initial.blocks.len());
        assert_eq!(
            plan.settled(),
            &electrical_snapshot(&world, region).unwrap()
        );
        assert!(plan.build_steps().iter().all(|s| s.wait_ticks >= 4));
        assert!(
            plan.remove_steps()
                .last()
                .unwrap()
                .expected
                .materialize()
                .blocks
                .is_empty()
        );
    }
}

#[test]
fn constant_sources_are_installed_after_mechanics_and_undo_tracks_the_moved_payload() {
    let f = fixture::electrical_fixture(false);
    assert!(f.catalog.assembly(&f.base.id).is_some());
    let mut world = dustroute_minecraft::World::new();
    for b in &f.request.candidate_state.assembly.blocks {
        world.set(b.position, b.block.clone());
    }
    let plan =
        ElectricalConstruction::new(&world, f.context.known_region, Default::default()).unwrap();
    assert!(
        plan.settled()
            .blocks
            .iter()
            .any(|b| b.name == "minecraft:piston_head")
    );
    assert!(
        plan.remove_steps()
            .last()
            .unwrap()
            .expected
            .materialize()
            .blocks
            .is_empty()
    );
    assert!(
        plan.build_steps()
            .iter()
            .rev()
            .take(2)
            .all(|s| s.state == "minecraft:redstone_block")
    );
}

#[test]
fn export_preserves_observed_material_and_rejects_unknown_properties() {
    let mut world = dustroute_minecraft::World::new();
    let pos = Pos::default();
    let block = world.place(BlockKind::Solid, pos);
    block.observed_name = Some("minecraft:obsidian".into());
    let region = Region::new(pos, pos);
    assert_eq!(
        electrical_snapshot(&world, region).unwrap().blocks[0].name,
        "minecraft:obsidian"
    );
    world
        .get_mut(pos)
        .unwrap()
        .observed_properties
        .insert("unsupported".into(), "value".into());
    assert!(electrical_snapshot(&world, region).is_err());
}

#[test]
fn observer_chains_construct_without_placement_pulses_in_all_six_directions() {
    let region = Region::new(Pos::new(-5, -5, -5), Pos::new(5, 5, 5));
    for watched in [
        Facing::Down,
        Facing::Up,
        Facing::North,
        Facing::South,
        Facing::West,
        Facing::East,
    ] {
        let step = watched.offset();
        let first = Pos::default();
        let second = first.offset(step.x, step.y, step.z);
        let lamp = second.offset(step.x, step.y, step.z);
        let mut world = World::new();
        for pos in [first, second] {
            let observer = world.place(BlockKind::Observer, pos);
            observer.facing = Some(watched.opposite());
            observer.powered = Some(false);
        }
        world.place(BlockKind::RedstoneLamp, lamp).powered = Some(false);
        let plan = ElectricalConstruction::new(&world, region, Default::default()).unwrap();
        assert_eq!(
            plan.build_steps()
                .iter()
                .map(|s| s.position)
                .collect::<Vec<_>>(),
            [lamp, second, first]
        );
        // The plan exposes powered command initialization; Java resets each
        // observer OFF without a pulse. Settling must not hide a pulse.
        assert!(
            plan.build_steps()
                .iter()
                .filter(|s| s.state.starts_with("minecraft:observer["))
                .all(|s| s.state.contains("powered=true"))
        );
        assert!(
            plan.build_steps().iter().all(|s| s.wait_ticks == 4),
            "{watched:?}"
        );
        assert_eq!(
            plan.settled(),
            &electrical_snapshot(&world, region).unwrap()
        );
        assert!(
            plan.remove_steps()
                .last()
                .unwrap()
                .expected
                .materialize()
                .blocks
                .is_empty()
        );
    }
}

#[test]
fn observing_declared_air_does_not_require_a_synthetic_support_or_front_block() {
    let region = Region::new(Pos::new(-4, -4, -4), Pos::new(4, 4, 4));
    for watched in [
        Facing::Down,
        Facing::Up,
        Facing::North,
        Facing::South,
        Facing::West,
        Facing::East,
    ] {
        let mut world = World::new();
        let observer = world.place(BlockKind::Observer, Pos::default());
        observer.facing = Some(watched.opposite());
        observer.powered = Some(false);
        let d = watched.offset();
        world.place(BlockKind::Air, d);
        let plan = ElectricalConstruction::new(&world, region, Default::default()).unwrap();
        assert_eq!(plan.build_steps().len(), 1);
        assert_eq!(plan.build_steps()[0].wait_ticks, 4);
        assert_eq!(plan.settled().blocks.len(), 1);
        assert!(
            plan.remove_steps()
                .last()
                .unwrap()
                .expected
                .materialize()
                .blocks
                .is_empty()
        );
    }
}

#[test]
fn observer_front_cycles_and_support_front_cycles_are_rejected_without_a_fallback() {
    let region = Region::new(Pos::new(-4, -4, -4), Pos::new(4, 5, 4));
    let mut facing_each_other = World::new();
    for (pos, output) in [
        (Pos::default(), Facing::West),
        (Pos::new(1, 0, 0), Facing::East),
    ] {
        let observer = facing_each_other.place(BlockKind::Observer, pos);
        observer.facing = Some(output);
        observer.powered = Some(false);
    }
    let mut support_cycle = World::new();
    let observer = support_cycle.place(BlockKind::Observer, Pos::default());
    observer.facing = Some(Facing::Down); // Watches the repeater it supports.
    observer.powered = Some(false);
    let repeater = support_cycle.place(BlockKind::Repeater, Pos::new(0, 1, 0));
    repeater.support_offset = Some(Facing::Down.offset());
    repeater.facing = Some(Facing::East);
    repeater.powered = Some(false);
    repeater.delay = Some(1);
    for world in [facing_each_other, support_cycle] {
        let error = ElectricalConstruction::new(&world, region, Default::default()).unwrap_err();
        assert_eq!(
            error,
            "construction support and observer-front dependencies cannot be ordered"
        );
    }
}
