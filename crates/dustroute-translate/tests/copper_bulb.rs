use dustroute_library::blueprint::BlueprintCatalog;
use dustroute_minecraft::{Block, BlockKind, Pos, Region, World};
use dustroute_translate::piston_construction::{ElectricalConstruction, electrical_snapshot};
use dustroute_translate::sim::RedstoneTickSimulator;
use dustroute_translate::snapshot::assembly_from_snapshot;

const B: Pos = Pos::new(0, 3, 0);
fn region() -> Region {
    Region::new(Pos::new(-5, -3, -5), Pos::new(5, 10, 5))
}
fn bulb(name: &str, lit: bool, powered: bool) -> Block {
    let mut block = Block::new(BlockKind::CopperBulb);
    block.observed_name = Some(format!("minecraft:{name}"));
    block.powered = Some(lit);
    block.observed_properties = [
        ("lit".into(), lit.to_string()),
        ("powered".into(), powered.to_string()),
    ]
    .into();
    block
}

#[test]
fn observed_waxed_states_roundtrip_and_compatibility_execution_rejects_them() {
    for name in [
        "waxed_copper_bulb",
        "waxed_exposed_copper_bulb",
        "waxed_weathered_copper_bulb",
        "waxed_oxidized_copper_bulb",
    ] {
        for lit in [false, true] {
            for powered in [false, true] {
                let mut world = World::new();
                world.set(B, bulb(name, lit, powered));
                let snapshot = electrical_snapshot(&world, region()).unwrap();
                let assembly = assembly_from_snapshot(&snapshot, "bulb", vec![region()]).unwrap();
                let restored = assembly
                    .inspect(&BlueprintCatalog::default())
                    .unwrap()
                    .proposed_world();
                let b = restored.get(B).unwrap();
                assert_eq!(b.kind, BlockKind::CopperBulb);
                assert_eq!(b.powered, Some(lit));
                assert_eq!(b.observed_properties["powered"], powered.to_string());
                assert_eq!(electrical_snapshot(&restored, region()).unwrap(), snapshot);
                let error = RedstoneTickSimulator::new(restored).err().unwrap();
                assert!(matches!(
                    error,
                    dustroute_translate::electrical::ElectricalSolveError::UnsupportedBlock {
                        kind: BlockKind::CopperBulb,
                        ..
                    }
                ));
            }
        }
    }
}

#[test]
fn powered_construction_and_teardown_use_exact_declared_bulb_properties() {
    for lit in [false, true] {
        let mut world = World::new();
        world.set(B, bulb("waxed_weathered_copper_bulb", lit, false));
        world.place(BlockKind::RedstoneBlock, B.offset(-1, 0, 0));
        let plan = ElectricalConstruction::new(&world, region(), Default::default()).unwrap();
        let installed = plan.build_steps().iter().find(|s| s.position == B).unwrap();
        assert!(
            installed
                .state
                .starts_with("minecraft:waxed_weathered_copper_bulb[")
        );
        assert!(installed.state.contains(&format!("lit={lit}")));
        assert!(installed.state.contains("powered=false"));
        let settled = plan.settled().blocks.iter().find(|b| b.pos == B).unwrap();
        assert_eq!(settled.properties["lit"], (!lit).to_string());
        assert_eq!(settled.properties["powered"], "true");
        assert!(
            plan.remove_steps()
                .last()
                .unwrap()
                .expected
                .blocks
                .is_empty()
        );
    }
}

#[test]
fn incomplete_or_extra_state_does_not_become_a_lossless_placement_snapshot() {
    let mut world = World::new();
    world.set(B, bulb("waxed_copper_bulb", false, false));
    world
        .get_mut(B)
        .unwrap()
        .observed_properties
        .insert("invented".into(), "true".into());
    assert!(electrical_snapshot(&world, region()).is_err());
    world
        .get_mut(B)
        .unwrap()
        .observed_properties
        .remove("invented");
    world
        .get_mut(B)
        .unwrap()
        .observed_properties
        .remove("powered");
    assert!(electrical_snapshot(&world, region()).is_err());
    assert!(!world.placement_issues().is_empty());
}
