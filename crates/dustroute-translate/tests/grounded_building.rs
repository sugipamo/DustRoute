#[path = "support/archive_fixture.rs"]
mod archive_fixture;
use archive_fixture::FixtureJson;
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
    let mut updates = archive_fixture::updates(&updates.fixture_json().unwrap()).unwrap();
    updates.adopt(&g.request.id).unwrap();
    let updates = archive_fixture::updates(&updates.fixture_json().unwrap()).unwrap();
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

#[test]
fn translated_sites_keep_exact_absolute_obligations_without_requiring_world_origin() {
    use dustroute_library::blueprint::TypeContract;
    use dustroute_library::building::BuildingDesignShape;
    use dustroute_minecraft::Region;
    use std::collections::BTreeMap;

    let original = generate_grounded_building_design(design()).unwrap();
    for offset in [
        Pos::new(0, -60, 0),
        Pos::new(32, 20, 48),
        Pos::new(-32, -60, -48),
    ] {
        let shift = |p: Pos| Pos::new(p.x + offset.x, p.y + offset.y, p.z + offset.z);
        let shift_region = |r: &mut Region| {
            r.min = shift(r.min);
            r.max = shift(r.max);
        };
        let mut request = design();
        shift_region(&mut request.design.known_region);
        for part in &mut request.design.parts {
            for shape in &mut part.shapes {
                match shape {
                    BuildingDesignShape::Fill { region, .. }
                    | BuildingDesignShape::Shell { region, .. } => shift_region(region),
                    BuildingDesignShape::Blocks { positions, .. } => {
                        positions.iter_mut().for_each(|p| *p = shift(*p))
                    }
                }
            }
            part.cutouts.iter_mut().for_each(shift_region);
        }
        for space in &mut request.design.spaces {
            shift_region(&mut space.region);
        }
        let region = request.design.known_region;
        assert!(!region.contains(Pos::default()));
        // Both common-source callers must work at translated sites.
        generate_building_design(request.design.clone(), None).unwrap();
        let g = generate_grounded_building_design(request).unwrap();
        assert_eq!((g.expected.min, g.expected.max), (region.min, region.max));
        assert_eq!(g.materials, original.materials);
        let expected: BTreeMap<_, _> = original
            .expected
            .blocks
            .iter()
            .map(|b| (shift(b.pos), (&b.name, &b.properties)))
            .collect();
        let actual: BTreeMap<_, _> = g
            .expected
            .blocks
            .iter()
            .map(|b| (b.pos, (&b.name, &b.properties)))
            .collect();
        assert_eq!(actual, expected);
        let mut updates = BlueprintUpdates::new(g.records.catalog().unwrap());
        updates.create(g.request.clone()).unwrap();
        updates.adopt(&g.request.id).unwrap();
        let updates = archive_fixture::updates(&updates.fixture_json().unwrap()).unwrap();
        let catalog = updates.catalog();
        let body = g
            .request
            .revisions
            .iter()
            .find(|r| r.id.as_str().ends_with(".building.v2"))
            .unwrap();
        let anchor = body
            .ports
            .iter()
            .find(|p| p.name == "layout")
            .unwrap()
            .position;
        assert!(region.contains(anchor));
        let ty = g
            .records
            .types
            .iter()
            .find(|t| t.id.as_str().ends_with(".clearance.pattern.v1"))
            .unwrap();
        let TypeContract::BlockPattern { blocks } = &ty.contract else {
            panic!("expected exact pattern")
        };
        let obligations: BTreeMap<_, _> = blocks
            .iter()
            .map(|b| {
                (
                    Pos::new(
                        anchor.x + b.position.x,
                        anchor.y + b.position.y,
                        anchor.z + b.position.z,
                    ),
                    b.block.clone(),
                )
            })
            .collect();
        let assembly = &g.request.candidate_state.assembly;
        let world = assembly.inspect(catalog).unwrap().proposed_world();
        let mut count = 0;
        for x in region.min.x..=region.max.x {
            for y in region.min.y..=region.max.y {
                for z in region.min.z..=region.max.z {
                    let p = Pos::new(x, y, z);
                    assert_eq!(
                        obligations.get(&p),
                        Some(
                            &world
                                .get(p)
                                .cloned()
                                .unwrap_or_else(|| Block::new(BlockKind::Air))
                        )
                    );
                    count += 1;
                }
            }
        }
        assert_eq!(obligations.len(), count);
        // Ground removal and filling protected interior air must still fail
        // after translation, serialization and adoption.
        for (local, remove) in [(Pos::new(2, -1, 2), true), (Pos::new(2, 1, 2), false)] {
            let mut changed = assembly.clone();
            let p = shift(local);
            if remove {
                changed.blocks.retain(|b| b.position != p);
            } else {
                changed
                    .blocks
                    .push(dustroute_library::blueprint::PositionedBlock {
                        position: p,
                        block: Block::new(BlockKind::Solid),
                    });
            }
            let review = review_assembly_with_context(
                catalog,
                &changed,
                Some(&g.context.clone().into()),
                Default::default(),
            )
            .unwrap();
            assert_eq!(
                review.status(),
                CheckStatus::Failed,
                "offset={offset:?}, cell={p:?}"
            );
        }
    }
}
