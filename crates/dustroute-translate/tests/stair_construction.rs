use dustroute_library::blueprint::BlueprintCatalog;
use dustroute_minecraft::{Facing, Pos, Region, RotationY, World};
use dustroute_translate::minecraft_export::{JavaExportConfig, initial_java_block_state};
use dustroute_translate::piston_construction::{ElectricalConstruction, electrical_snapshot};
use dustroute_translate::snapshot::{MinecraftSnapshot, assembly_from_snapshot};

#[test]
fn stair_corner_construction_rotation_save_reload_and_removal_preserve_literal_states() {
    for top in [false, true] {
        let snapshot: MinecraftSnapshot = serde_json::from_value(serde_json::json!({
            "min":{"x":-4,"y":-4,"z":-4},"max":{"x":4,"y":4,"z":4},
            "blocks":[
                {"pos":{"x":0,"y":0,"z":0},"name":"minecraft:quartz_stairs","properties":{"facing":"north","half":if top {"top"}else{"bottom"},"shape":"inner_left","waterlogged":"false"}},
                {"pos":{"x":0,"y":0,"z":1},"name":"minecraft:cobblestone_stairs","properties":{"facing":"west","half":if top {"top"}else{"bottom"},"shape":"straight","waterlogged":"false"}},
                {"pos":{"x":-1,"y":0,"z":0},"name":"minecraft:lever","properties":{"face":"wall","facing":"west","powered":"false"}}
            ]
        })).unwrap();
        let region = Region::new(snapshot.min, snapshot.max);
        let assembly = assembly_from_snapshot(&snapshot, "stair corner", vec![region]).unwrap();
        let restored: dustroute_library::assembly::Assembly =
            serde_json::from_str(&serde_json::to_string(&assembly).unwrap()).unwrap();
        assert_eq!(assembly, restored);
        let base = restored
            .inspect(&BlueprintCatalog::default())
            .unwrap()
            .proposed_world();
        for rotation in [
            RotationY::R0,
            RotationY::R90,
            RotationY::R180,
            RotationY::R270,
        ] {
            let mut world = World::new();
            for (pos, block) in base.iter() {
                world.set(rotation.pos(*pos), rotation.block(block));
            }
            assert!(world.support_issues().is_empty());
            let plan = ElectricalConstruction::new(&world, region, Default::default()).unwrap();
            assert_eq!(
                plan.settled(),
                &electrical_snapshot(&world, region).unwrap()
            );
            let lever = rotation.pos(Pos::new(-1, 0, 0));
            let install = plan
                .build_steps()
                .iter()
                .position(|s| s.position == lever)
                .unwrap();
            assert!(
                plan.build_steps()[..install]
                    .iter()
                    .any(|s| s.position == Pos::default())
            );
            assert!(
                plan.build_steps()[..install]
                    .iter()
                    .any(|s| s.position == rotation.pos(Pos::new(0, 0, 1)))
            );
            let literal = initial_java_block_state(
                world.get(Pos::default()).unwrap(),
                &JavaExportConfig::default(),
            )
            .unwrap();
            assert!(literal.contains("shape=inner_left"));
            assert!(literal.contains(match rotation.facing(Facing::North) {
                Facing::North => "facing=north",
                Facing::East => "facing=east",
                Facing::South => "facing=south",
                Facing::West => "facing=west",
                _ => unreachable!(),
            }));
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
}
