use dustroute_library::blueprint::BlueprintCatalog;
use dustroute_minecraft::{Pos, Region};
use dustroute_translate::minecraft_export::{JavaExportConfig, initial_java_block_state};
use dustroute_translate::piston_construction::{ElectricalConstruction, electrical_snapshot};
use dustroute_translate::snapshot::{MinecraftSnapshot, assembly_from_snapshot};

#[test]
fn passive_shape_construction_preserves_states_support_order_and_teardown() {
    for (state, device) in [
        (
            "top",
            serde_json::json!({"pos":{"x":0,"y":2,"z":0},"name":"minecraft:lever","properties":{"face":"floor","facing":"east","powered":"false"}}),
        ),
        (
            "bottom",
            serde_json::json!({"pos":{"x":0,"y":0,"z":0},"name":"minecraft:stone_button","properties":{"face":"ceiling","facing":"north","powered":"false"}}),
        ),
        (
            "double",
            serde_json::json!({"pos":{"x":1,"y":1,"z":0},"name":"minecraft:lever","properties":{"face":"wall","facing":"east","powered":"false"}}),
        ),
    ] {
        let snapshot: MinecraftSnapshot = serde_json::from_value(serde_json::json!({
            "min":{"x":-4,"y":-4,"z":-4},"max":{"x":4,"y":6,"z":4},
            "blocks":[{"pos":{"x":0,"y":1,"z":0},"name":"minecraft:smooth_stone_slab","properties":{"type":state,"waterlogged":"false"}},device]
        })).unwrap();
        let region = Region::new(snapshot.min, snapshot.max);
        let assembly =
            assembly_from_snapshot(&snapshot, "slab construction", vec![region]).unwrap();
        let assembly = serde_json::from_str::<dustroute_library::assembly::Assembly>(
            &serde_json::to_string(&assembly).unwrap(),
        )
        .unwrap();
        let world = assembly
            .inspect(&BlueprintCatalog::default())
            .unwrap()
            .proposed_world();
        assert!(world.support_issues().is_empty());
        let slab = world.get(Pos::new(0, 1, 0)).unwrap();
        assert_eq!(
            initial_java_block_state(slab, &JavaExportConfig::default()).unwrap(),
            format!("minecraft:smooth_stone_slab[type={state},waterlogged=false]")
        );
        let plan = ElectricalConstruction::new(&world, region, Default::default()).unwrap();
        assert_eq!(plan.build_steps()[0].position, Pos::new(0, 1, 0));
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
fn incomplete_and_waterlogged_slab_states_cannot_be_exported_or_planned() {
    for properties in [
        serde_json::json!({"type":"top"}),
        serde_json::json!({"type":"top","waterlogged":"true"}),
    ] {
        let snapshot: MinecraftSnapshot=serde_json::from_value(serde_json::json!({
            "min":{"x":-4,"y":-4,"z":-4},"max":{"x":4,"y":4,"z":4},
            "blocks":[{"pos":{"x":0,"y":0,"z":0},"name":"minecraft:stone_slab","properties":properties}]
        })).unwrap();
        let region = Region::new(snapshot.min, snapshot.max);
        let world = assembly_from_snapshot(&snapshot, "incomplete slab", vec![region])
            .unwrap()
            .inspect(&BlueprintCatalog::default())
            .unwrap()
            .proposed_world();
        assert!(
            initial_java_block_state(
                world.get(Pos::default()).unwrap(),
                &JavaExportConfig::default()
            )
            .is_err()
        );
        assert!(ElectricalConstruction::new(&world, region, Default::default()).is_err());
    }
}
