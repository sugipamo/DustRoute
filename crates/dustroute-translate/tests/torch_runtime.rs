use dustroute_library::blueprint::BlueprintCatalog;
use dustroute_minecraft::time::runtime::BlockIdentity;
use dustroute_minecraft::{Block, BlockKind, Facing, Pos, Region, World};
use dustroute_translate::piston_construction::{ElectricalConstruction, electrical_snapshot};
use dustroute_translate::snapshot::assembly_from_snapshot;
const T: Pos = Pos::new(0, 3, 0);
fn region() -> Region {
    Region::new(Pos::new(-6, -3, -6), Pos::new(6, 10, 6))
}
fn scene(support: Facing, lit: bool) -> World {
    let mut w = World::new();
    let d = support.offset();
    w.place(BlockKind::Solid, T.offset(d.x, d.y, d.z));
    let mut b = Block::new(BlockKind::RedstoneTorch);
    b.support_offset = Some(d);
    b.powered = Some(lit);
    w.set(T, b);
    w
}
#[test]
fn synthetic_and_observed_mounts_roundtrip_to_the_same_concrete_identity() {
    for support in [
        Facing::Down,
        Facing::North,
        Facing::East,
        Facing::South,
        Facing::West,
    ] {
        for lit in [false, true] {
            let w = scene(support, lit);
            let snapshot = electrical_snapshot(&w, region()).unwrap();
            let record = snapshot.blocks.iter().find(|b| b.pos == T).unwrap();
            assert_eq!(
                record.name,
                if support == Facing::Down {
                    "minecraft:redstone_torch"
                } else {
                    "minecraft:redstone_wall_torch"
                }
            );
            assert_eq!(record.properties["lit"], lit.to_string());
            assert_eq!(
                record.properties.contains_key("facing"),
                support != Facing::Down
            );
            let imported = assembly_from_snapshot(&snapshot, "torch", vec![region()])
                .unwrap()
                .inspect(&BlueprintCatalog::default())
                .unwrap()
                .proposed_world();
            assert_eq!(
                BlockIdentity::of(w.get(T).unwrap()),
                BlockIdentity::of(imported.get(T).unwrap())
            );
            assert_eq!(electrical_snapshot(&imported, region()).unwrap(), snapshot);
        }
    }
}
#[test]
fn command_construction_models_initial_relighting_and_removes_every_mount() {
    for support in [
        Facing::Down,
        Facing::North,
        Facing::East,
        Facing::South,
        Facing::West,
    ] {
        for lit in [false, true] {
            let w = scene(support, lit);
            let plan = ElectricalConstruction::new(&w, region(), Default::default()).unwrap();
            let step = plan.build_steps().iter().find(|s| s.position == T).unwrap();
            assert!(step.state.contains(&format!("lit={lit}")));
            let settled = plan.settled().blocks.iter().find(|b| b.pos == T).unwrap();
            assert_eq!(settled.properties["lit"], "true");
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
#[test]
fn unsupported_mounts_and_missing_or_extra_native_state_fail_closed() {
    assert!(electrical_snapshot(&scene(Facing::Up, true), region()).is_err());
    let w = scene(Facing::West, true);
    let snapshot = electrical_snapshot(&w, region()).unwrap();
    let mut imported = assembly_from_snapshot(&snapshot, "torch", vec![region()])
        .unwrap()
        .inspect(&BlueprintCatalog::default())
        .unwrap()
        .proposed_world();
    imported
        .get_mut(T)
        .unwrap()
        .observed_properties
        .insert("burnout".into(), "8".into());
    assert!(electrical_snapshot(&imported, region()).is_err());
    imported
        .get_mut(T)
        .unwrap()
        .observed_properties
        .remove("burnout");
    imported
        .get_mut(T)
        .unwrap()
        .observed_properties
        .remove("lit");
    assert!(electrical_snapshot(&imported, region()).is_err());
}
