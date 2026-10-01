use dustroute_library::blueprint::BlueprintCatalog;
use dustroute_minecraft::{Pos, Region, RotationY, World};
use dustroute_translate::minecraft_export::{JavaExportConfig, initial_java_block_state};
use dustroute_translate::piston_construction::{ElectricalConstruction, electrical_snapshot};
use dustroute_translate::snapshot::{MinecraftSnapshot, assembly_from_snapshot};

#[test]
fn adhesion_materials_roundtrip_and_construct_in_every_horizontal_rotation() {
    let snapshot: MinecraftSnapshot = serde_json::from_value(serde_json::json!({
        "min":{"x":-4,"y":-4,"z":-4},"max":{"x":5,"y":5,"z":4},"blocks":[
            {"pos":{"x":0,"y":0,"z":0},"name":"minecraft:sticky_piston","properties":{"facing":"east","extended":"false"}},
            {"pos":{"x":1,"y":0,"z":0},"name":"minecraft:slime_block"},
            {"pos":{"x":1,"y":1,"z":0},"name":"minecraft:slime_block"},
            {"pos":{"x":1,"y":0,"z":1},"name":"minecraft:honey_block"},
            {"pos":{"x":-2,"y":0,"z":0},"name":"minecraft:stone"},
            {"pos":{"x":-1,"y":0,"z":0},"name":"minecraft:lever","properties":{"face":"wall","facing":"east","powered":"false"}}
        ]
    })).unwrap();
    let known = Region::new(snapshot.min, snapshot.max);
    let assembly = assembly_from_snapshot(&snapshot, "adhesion", vec![known]).unwrap();
    let restored: dustroute_library::assembly::Assembly =
        serde_json::from_str(&serde_json::to_string(&assembly).unwrap()).unwrap();
    assert_eq!(restored, assembly);
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
        let mut w = World::new();
        for (p, b) in base.iter() {
            w.set(rotation.pos(*p), rotation.block(b));
        }
        let region = Region::new(Pos::new(-5, -4, -5), Pos::new(5, 5, 5));
        let plan = ElectricalConstruction::new(&w, region, Default::default()).unwrap();
        assert_eq!(plan.settled(), &electrical_snapshot(&w, region).unwrap());
        assert!(
            plan.remove_steps()
                .last()
                .unwrap()
                .expected
                .materialize()
                .blocks
                .is_empty()
        );
        for p in [Pos::new(1, 0, 0), Pos::new(1, 0, 1)] {
            let b = w.get(rotation.pos(p)).unwrap();
            assert_eq!(
                initial_java_block_state(b, &JavaExportConfig::default()).unwrap(),
                b.observed_name.clone().unwrap()
            );
        }
    }
}
