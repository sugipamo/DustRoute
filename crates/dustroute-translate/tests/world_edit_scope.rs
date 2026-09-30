use dustroute_library::world_edit::WorldEditScope;
use dustroute_minecraft::{Pos, Region};
use dustroute_translate::piston_construction::ElectricalModification;
use dustroute_translate::snapshot::MinecraftSnapshot;
use serde_json::json;

fn observer() -> MinecraftSnapshot {
    serde_json::from_value(json!({"min":{"x":-2,"y":-1,"z":-2},"max":{"x":3,"y":3,"z":2},
        "blocks":[{"pos":{"x":0,"y":1,"z":0},"name":"minecraft:observer","properties":{"facing":"east","powered":"false"}}]})).unwrap()
}
fn point(p: Pos) -> Region {
    Region::new(p, p)
}

#[test]
fn scope_refuses_a_transient_protected_observer_pulse_even_when_final_state_matches() {
    let before = observer();
    let mut after = before.clone();
    after.blocks.push(
        serde_json::from_value(json!({"pos":{"x":1,"y":1,"z":0},"name":"minecraft:stone"}))
            .unwrap(),
    );
    let unrestricted = ElectricalModification::new(&before, &after, Default::default()).unwrap();
    assert_eq!(unrestricted.after(), &after);
    let scope = WorldEditScope {
        editable: vec![point(Pos::new(1, 1, 0))],
        protected: vec![point(Pos::new(0, 1, 0))],
    };
    let error =
        ElectricalModification::new_scoped(&before, &after, scope, Default::default()).unwrap_err();
    assert!(
        error.contains("protected state changed at Pos { x: 0, y: 1, z: 0 }"),
        "{error}"
    );
    assert!(error.contains("runtime"));
}

#[test]
fn explicitly_admitting_motion_space_still_preserves_the_other_known_cells_on_undo() {
    let before = observer();
    let mut after = before.clone();
    after.blocks.push(
        serde_json::from_value(json!({"pos":{"x":1,"y":1,"z":0},"name":"minecraft:stone"}))
            .unwrap(),
    );
    let scope = WorldEditScope {
        editable: vec![Region::new(Pos::new(0, 1, 0), Pos::new(1, 1, 0))],
        protected: vec![point(Pos::new(2, 1, 0))],
    };
    let proof =
        ElectricalModification::new_scoped(&before, &after, scope.clone(), Default::default())
            .unwrap();
    assert_eq!(proof.scope(), &scope);
    assert_eq!(proof.steps(false).last().unwrap().expected, after);
    assert_eq!(proof.steps(true).last().unwrap().expected, before);
    assert!(
        !scope.allows_change(Pos::new(3, 2, 1)),
        "undeclared known space is protected too"
    );
}

#[test]
fn denied_writes_overlap_and_unknown_scope_are_refused_before_constructing_a_plan() {
    let before = observer();
    let mut after = before.clone();
    after.blocks.push(
        serde_json::from_value(json!({"pos":{"x":1,"y":1,"z":0},"name":"minecraft:glass"}))
            .unwrap(),
    );
    for (scope, reason) in [
        (
            WorldEditScope {
                editable: vec![point(Pos::new(2, 1, 0))],
                protected: vec![],
            },
            "declared write",
        ),
        (
            WorldEditScope {
                editable: vec![point(Pos::new(1, 1, 0))],
                protected: vec![point(Pos::new(1, 1, 0))],
            },
            "overlaps protected",
        ),
        (
            WorldEditScope {
                editable: vec![point(Pos::new(4, 1, 0))],
                protected: vec![],
            },
            "outside the complete observation",
        ),
    ] {
        let error = ElectricalModification::new_scoped(&before, &after, scope, Default::default())
            .unwrap_err();
        assert!(error.contains(reason), "{error}");
    }
}
