use dustroute_library::blueprint::BlueprintCatalog;
use dustroute_library::runtime_behavior::RuntimeBehaviorContext;
use dustroute_minecraft::{Pos, Region, RotationY};
use dustroute_translate::assembly_transform::AssemblyTransform;
use dustroute_translate::piston_construction::{ElectricalConstruction, electrical_snapshot};
use dustroute_translate::snapshot::{MinecraftSnapshot, assembly_from_snapshot};

#[test]
fn flight_course_uses_shared_construction_operating_reference_and_reconstruction() {
    let initial: MinecraftSnapshot =
        serde_json::from_str(include_str!("fixtures/flying-machine-initial.json")).unwrap();
    let region = Region::new(initial.min, initial.max);
    let assembly = assembly_from_snapshot(&initial, "finite flight", vec![region]).unwrap();
    let context = RuntimeBehaviorContext::fresh_pistons(region, vec![Pos::new(0, 2, 0)]);
    for rotation in [
        RotationY::R0,
        RotationY::R90,
        RotationY::R180,
        RotationY::R270,
    ] {
        let transform = AssemblyTransform {
            source_anchor: Pos::default(),
            target_anchor: Pos::new(270_000, 180, 1000),
            rotation,
        };
        let (moved, context) = transform.apply(&assembly, &context).unwrap();
        let world = moved
            .inspect(&BlueprintCatalog::default())
            .unwrap()
            .proposed_world();
        let plan = ElectricalConstruction::new(&world, context.known_region, context.root_limits)
            .unwrap_or_else(|e| panic!("{rotation:?}: {e}"));
        assert_eq!(
            plan.settled(),
            &electrical_snapshot(&world, context.known_region).unwrap()
        );
        assert_eq!(plan.build_steps().len(), 9);
        assert!(
            plan.remove_steps()
                .last()
                .unwrap()
                .expected
                .blocks
                .is_empty()
        );
        let arrived = plan
            .operating_reference(&[(context.input_levers[0], true)], context.root_limits)
            .unwrap();
        for block in &initial.blocks {
            let mut local = block.pos;
            if local.y <= 1 && local.x <= 1 {
                local.x += 10;
            }
            let position = transform.position(local).unwrap();
            assert!(
                arrived
                    .blocks
                    .iter()
                    .any(|b| b.pos == position && b.name == block.name),
                "{rotation:?}: missing {} at {position:?}",
                block.name
            );
        }
        assert_eq!(arrived.blocks.len(), 9);
        let (baseline, removal) = plan
            .operating_removal(&[(context.input_levers[0], true)], context.root_limits)
            .unwrap();
        assert_eq!(baseline, arrived);
        assert!(removal.last().unwrap().expected.blocks.is_empty());
        let reconstruction = plan
            .reconstruction_steps(&arrived, context.root_limits)
            .unwrap();
        assert!(reconstruction.iter().any(|s| s.expected.blocks.is_empty()));
        assert_eq!(&reconstruction.last().unwrap().expected, plan.settled());
    }
}
