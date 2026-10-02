use dustroute_library::building::{BuildingDesignRequest, GroundedBuildingDesignRequest};
use dustroute_minecraft::{Block, BlockKind, Pos};
use dustroute_translate::blueprint_update::BlueprintUpdates;
use dustroute_translate::building::{generate_building_design, generate_grounded_building_design};
use dustroute_translate::promotion::{CheckStatus, review_assembly_with_context};
use serde_json::json;

fn design() -> GroundedBuildingDesignRequest {
    serde_json::from_value(json!({"ground_material":"stone", "design": {
        "namespace":"test.grounded-pavilion", "name":"Five-by-five roof on four columns",
        "known_region":{"min":{"x":-1,"y":-1,"z":-1},"max":{"x":5,"y":7,"z":5}},
        "parts":[
            {"name":"roof","shapes":[{"kind":"fill","material":"cobblestone","region":{
                "min":{"x":0,"y":6,"z":0},"max":{"x":4,"y":6,"z":4}}}]},
            {"name":"columns","shapes":[
                {"kind":"fill","material":"cobblestone","region":{"min":{"x":0,"y":0,"z":0},"max":{"x":0,"y":5,"z":0}}},
                {"kind":"fill","material":"cobblestone","region":{"min":{"x":4,"y":0,"z":0},"max":{"x":4,"y":5,"z":0}}},
                {"kind":"fill","material":"cobblestone","region":{"min":{"x":0,"y":0,"z":4},"max":{"x":0,"y":5,"z":4}}},
                {"kind":"fill","material":"cobblestone","region":{"min":{"x":4,"y":0,"z":4},"max":{"x":4,"y":5,"z":4}}}
            ]}
        ],
        "spaces":[{"name":"inside","region":{"min":{"x":1,"y":0,"z":1},"max":{"x":3,"y":5,"z":3}}}]
    }})).unwrap()
}

#[test]
fn grounded_adoption_survives_restart_and_retains_ground_and_air_requirements() {
    let g = generate_grounded_building_design(design()).unwrap();
    assert_eq!(g.baseline.blocks.len(), 49);
    assert_eq!(g.expected.blocks.len(), 98);
    assert_eq!(g.structure.len(), 49);
    assert_eq!(g.materials.len(), 1);
    assert_eq!(g.materials["minecraft:cobblestone"], 49);
    assert_eq!(g.verification.modeled_changes, 49);
    assert_eq!(g.verification.modeled_removals, 49);
    assert!(g.verification.baseline_restored);
    assert!(!g.verification.player_construction_verified);
    assert!(!g.verification.live_world_verified);
    let mut updates = BlueprintUpdates::new(g.records.catalog().unwrap());
    updates.create(g.request.clone()).unwrap();
    let mut updates = BlueprintUpdates::from_json(&updates.to_json().unwrap()).unwrap();
    updates.adopt(&g.request.id).unwrap();
    let updates = BlueprintUpdates::from_json(&updates.to_json().unwrap()).unwrap();
    let base = updates.catalog().assembly(&g.request.base_state).unwrap();
    assert_eq!(base.assembly.blocks.len(), 49);
    let assembly = &updates
        .catalog()
        .assembly(&g.request.candidate_state.id)
        .unwrap()
        .assembly;
    for bad_position in [Pos::new(2, -1, 2), Pos::new(2, 1, 2)] {
        let mut changed = assembly.clone();
        if bad_position.y == -1 {
            changed.blocks.retain(|b| b.position != bad_position);
        } else {
            changed
                .blocks
                .push(dustroute_library::blueprint::PositionedBlock {
                    position: bad_position,
                    block: Block::new(BlockKind::Solid),
                });
        }
        let review = review_assembly_with_context(
            updates.catalog(),
            &changed,
            Some(&g.context.clone().into()),
            Default::default(),
        )
        .unwrap();
        assert_eq!(review.status(), CheckStatus::Failed, "{bad_position:?}");
    }
}

#[test]
fn ground_cannot_silently_override_permanent_air_or_reinterpret_ordinary_design() {
    let request = design();
    let plain: BuildingDesignRequest = request.design.clone();
    let old = generate_building_design(plain, None).unwrap();
    assert_eq!(old.expected.blocks.len(), 49);
    assert!(old.expected.blocks.iter().all(|b| b.pos.y >= 0));
    let mut conflicting = request;
    conflicting.design.spaces[0].region.min.y = -1;
    assert_eq!(
        generate_grounded_building_design(conflicting)
            .unwrap_err()
            .code,
        "ground_conflicts_with_permanent_air"
    );
}
