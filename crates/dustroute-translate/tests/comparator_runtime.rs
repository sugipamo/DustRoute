use dustroute_library::blueprint::BlueprintCatalog;
use dustroute_minecraft::time::piston_runtime::new_piston_runtime;
use dustroute_minecraft::{Block, BlockKind, Facing, Pos, Region, World};
use dustroute_translate::piston_construction::{ElectricalConstruction, electrical_snapshot};
use dustroute_translate::snapshot::assembly_from_snapshot;

const C: Pos = Pos::new(0, 3, 0);
fn region() -> Region {
    Region::new(Pos::new(-6, -2, -6), Pos::new(6, 10, 6))
}
fn comparator(mode: &str, powered: bool) -> Block {
    let mut b = Block::new(BlockKind::Comparator);
    b.facing = Some(Facing::East);
    b.support_offset = Some(Facing::Down.offset());
    b.powered = Some(powered);
    b.observed_name = Some("minecraft:comparator".into());
    b.observed_properties = [
        ("facing".into(), "west".into()),
        ("powered".into(), powered.to_string()),
        ("mode".into(), mode.into()),
    ]
    .into();
    b
}
#[test]
fn snapshot_roundtrip_preserves_mode_without_fabricating_block_entity_output() {
    for mode in ["compare", "subtract"] {
        for powered in [false, true] {
            let mut w = World::new();
            w.place(BlockKind::Solid, C.offset(0, -1, 0));
            w.set(C, comparator(mode, powered));
            let snapshot = electrical_snapshot(&w, region()).unwrap();
            let assembly = assembly_from_snapshot(&snapshot, "comparator", vec![region()]).unwrap();
            let restored = assembly
                .inspect(&BlueprintCatalog::default())
                .unwrap()
                .proposed_world();
            assert_eq!(electrical_snapshot(&restored, region()).unwrap(), snapshot);
            assert_eq!(restored.get(C).unwrap().power_level, None);
            let fresh = new_piston_runtime(restored, region(), Default::default()).unwrap();
            assert_eq!(fresh.view().stored_output(C).unwrap(), 0);
        }
    }
}
#[test]
fn construction_and_teardown_use_declared_modes_and_do_not_export_output_as_a_property() {
    for mode in ["compare", "subtract"] {
        let mut w = World::new();
        w.place(BlockKind::Solid, C.offset(0, -1, 0));
        w.set(C, comparator(mode, false));
        let plan = ElectricalConstruction::new(&w, region(), Default::default()).unwrap();
        let step = plan.build_steps().iter().find(|s| s.position == C).unwrap();
        assert!(step.state.contains(&format!("mode={mode}")));
        assert!(!step.state.contains("power="));
        assert!(
            plan.remove_steps()
                .last()
                .unwrap()
                .expected
                .materialize()
                .blocks
                .is_empty()
        );
        w.get_mut(C)
            .unwrap()
            .observed_properties
            .insert("outputSignal".into(), "8".into());
        assert!(electrical_snapshot(&w, region()).is_err());
    }
}
