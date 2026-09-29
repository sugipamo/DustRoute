use dustroute_library::blueprint::BlueprintCatalog;
use dustroute_minecraft::time::piston_runtime::{
    ElectricalPistonRuntime, new_piston_runtime, schedule_electrical_input_after_tick,
};
use dustroute_minecraft::{BlockKind, ObservationClassification, Pos, Region, World};
use dustroute_translate::piston_construction::{ElectricalConstruction, electrical_snapshot};
use dustroute_translate::snapshot::MinecraftSnapshot;
use dustroute_translate::snapshot::assembly_from_snapshot;

fn import(value: serde_json::Value) -> (World, Region) {
    let snapshot: MinecraftSnapshot = serde_json::from_value(value).unwrap();
    let region = Region::new(snapshot.min, snapshot.max);
    let world = assembly_from_snapshot(&snapshot, "literal reference materials", vec![region])
        .unwrap()
        .inspect(&BlueprintCatalog::default())
        .unwrap()
        .proposed_world();
    (world, region)
}

#[test]
fn material_identity_survives_conductor_power_motion_and_saved_execution() {
    for (payload, conductor) in [
        ("smooth_quartz", "cyan_wool"),
        ("cyan_wool", "smooth_quartz"),
    ] {
        let (world, region) = import(serde_json::json!({
            "min":{"x":-6,"y":-2,"z":-5}, "max":{"x":6,"y":10,"z":5},
            "blocks":[
                {"pos":{"x":0,"y":3,"z":0},"name":"minecraft:sticky_piston","properties":{"facing":"up","extended":"false"}},
                {"pos":{"x":0,"y":4,"z":0},"name":format!("minecraft:{payload}")},
                {"pos":{"x":-1,"y":3,"z":0},"name":format!("minecraft:{conductor}")},
                {"pos":{"x":-2,"y":3,"z":0},"name":"minecraft:lever","properties":{"face":"wall","facing":"west","powered":"false"}}
            ]
        }));
        let input = Pos::new(-2, 3, 0);
        let source = Pos::new(0, 4, 0);
        let destination = Pos::new(0, 5, 0);
        assert_eq!(
            world.get(source).unwrap().observation_classification,
            ObservationClassification::Exact
        );
        let plan = ElectricalConstruction::new(&world, region, Default::default()).unwrap();
        assert_eq!(
            plan.settled(),
            &electrical_snapshot(&world, region).unwrap()
        );
        assert!(
            plan.build_steps()
                .iter()
                .any(|step| step.state == format!("minecraft:{payload}"))
        );
        let mut runtime = new_piston_runtime(world.clone(), region, Default::default()).unwrap();
        schedule_electrical_input_after_tick(&mut runtime, 1, input, true).unwrap();
        let mut checkpoint = None;
        let mut behavior = None;
        while runtime.microstep().unwrap().is_some() {
            if runtime.view().block(destination).unwrap().kind == BlockKind::MovingPiston {
                if checkpoint.is_none() && !runtime.at_input_boundary() {
                    checkpoint = Some(runtime.checkpoint());
                }
                if behavior.is_none() && runtime.at_input_boundary() {
                    behavior = Some(runtime.behavior_state().unwrap());
                }
            }
        }
        assert_eq!(
            runtime.view().block(source).unwrap().kind,
            BlockKind::PistonHead
        );
        assert_eq!(
            runtime.view().block(destination).unwrap(),
            *world.get(source).unwrap()
        );
        let mut exact = ElectricalPistonRuntime::from_checkpoint(&checkpoint.unwrap()).unwrap();
        exact.run_until_idle().unwrap();
        assert_eq!(exact.state_key(), runtime.state_key());
        let mut representative =
            ElectricalPistonRuntime::from_behavior_state(&behavior.unwrap()).unwrap();
        representative.run_until_idle().unwrap();
        assert_eq!(
            representative.behavior_state().unwrap(),
            runtime.behavior_state().unwrap()
        );
        runtime.input_now(input, false).unwrap();
        runtime.run_until_idle().unwrap();
        assert_eq!(runtime.view().world(), &world);
        assert_eq!(
            electrical_snapshot(runtime.view().world(), region).unwrap(),
            electrical_snapshot(&world, region).unwrap()
        );
    }
}

#[test]
fn reference_materials_and_devices_preserve_the_declared_idle_initial_world() {
    let fixture: serde_json::Value =
        serde_json::from_str(include_str!("fixtures/reference-3x3-bobiloosky-v1.json")).unwrap();
    let (world, region) = import(fixture["initial"].clone());
    let materials: Vec<_> = world
        .iter()
        .filter(|(_, b)| {
            matches!(
                b.observed_name.as_deref(),
                Some("minecraft:smooth_quartz" | "minecraft:cyan_wool")
            )
        })
        .collect();
    assert_eq!(materials.len(), 18);
    assert!(materials.iter().all(|(_, b)| b.kind == BlockKind::Solid
        && b.observation_classification == ObservationClassification::Exact));
    let mut runtime = new_piston_runtime(world.clone(), region, Default::default()).unwrap();
    runtime.run_until_idle().unwrap();
    assert_eq!(runtime.view().world(), &world);
    for name in ["minecraft:magenta_wool", "minecraft:unknown_material"] {
        let (world, region) = import(serde_json::json!({
            "min":{"x":-4,"y":-4,"z":-4},"max":{"x":4,"y":4,"z":4},
            "blocks":[{"pos":{"x":0,"y":0,"z":0},"name":name}]
        }));
        assert_eq!(
            world
                .get(Pos::default())
                .unwrap()
                .observation_classification,
            ObservationClassification::Coarse
        );
        assert!(new_piston_runtime(world, region, Default::default()).is_err());
    }
}
