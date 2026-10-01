use dustroute_translate::piston_construction::ElectricalModification;
use dustroute_translate::snapshot::{MinecraftSnapshot, MinecraftSnapshotBlock};
use dustroute_translate::world::Pos;
use serde_json::json;

fn machine() -> MinecraftSnapshot {
    serde_json::from_value(json!({
        "min":{"x":99,"y":99,"z":102}, "max":{"x":105,"y":104,"z":109},
        "blocks":[
            {"pos":{"x":101,"y":101,"z":104},"name":"minecraft:sticky_piston","properties":{"facing":"east","extended":"false"}},
            {"pos":{"x":101,"y":101,"z":105},"name":"minecraft:slime_block"},
            {"pos":{"x":101,"y":102,"z":105},"name":"minecraft:observer","properties":{"facing":"south","powered":"false"}},
            {"pos":{"x":102,"y":101,"z":104},"name":"minecraft:slime_block"},
            {"pos":{"x":102,"y":101,"z":105},"name":"minecraft:sticky_piston","properties":{"facing":"west","extended":"false"}},
            {"pos":{"x":102,"y":102,"z":104},"name":"minecraft:observer","properties":{"facing":"north","powered":"false"}}
        ]
    })).unwrap()
}

fn block(pos: Pos, name: &str) -> MinecraftSnapshotBlock {
    MinecraftSnapshotBlock {
        pos,
        name: name.into(),
        properties: Default::default(),
    }
}

#[test]
fn adds_and_removes_bar_without_rebuilding_the_existing_engine() {
    let before = machine();
    let mut after = before.clone();
    after.blocks.extend([
        block(Pos::new(101, 101, 106), "minecraft:slime_block"),
        block(Pos::new(101, 101, 107), "minecraft:glass"),
    ]);
    let plan = ElectricalModification::new(&before, &after, Default::default()).unwrap();
    assert_eq!(plan.steps(false).len(), 2);
    assert_eq!(plan.steps(true).len(), 2);
    for step in plan.steps(false).iter().chain(plan.steps(true)) {
        assert!([Pos::new(101, 101, 106), Pos::new(101, 101, 107)].contains(&step.position));
        for original in &before.blocks {
            assert!(step.expected.materialize().blocks.contains(original));
        }
    }
    assert_eq!(&plan.steps(false).last().unwrap().expected, plan.after());
    assert_eq!(&plan.steps(true).last().unwrap().expected, plan.before());
}

#[test]
fn refuses_unknown_space_duplicate_records_and_lossy_states() {
    let before = machine();
    let mut after = before.clone();
    after.max.z += 1;
    assert!(ElectricalModification::new(&before, &after, Default::default()).is_err());
    after = before.clone();
    after.blocks.push(after.blocks[0].clone());
    assert!(ElectricalModification::new(&before, &after, Default::default()).is_err());
    after = before.clone();
    after.blocks[0].properties.remove("extended");
    assert!(ElectricalModification::new(&before, &after, Default::default()).is_err());
}

#[test]
fn refuses_a_change_that_starts_the_machine_instead_of_matching_the_target() {
    let before = machine();
    let mut after = before.clone();
    // Observer on the first engine sees this cell. The declared target leaves
    // the engine in place; callback-induced flight cannot be silently accepted.
    after
        .blocks
        .push(block(Pos::new(101, 102, 106), "minecraft:stone"));
    assert!(ElectricalModification::new(&before, &after, Default::default()).is_err());
}
