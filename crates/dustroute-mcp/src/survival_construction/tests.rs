//! Contract/resource tests only; native collision/hit behavior is tested in Voxrig.
use super::*;
use dustroute_translate::building::generate_grounded_building_design;
use serde_json::json;

pub(crate) fn design() -> GeneratedGroundedBuildingDesign {
    generate_grounded_building_design(serde_json::from_value(json!({
        "ground_material":"stone", "design": {
            "namespace":"test.survival-plan", "name":"Elevated roof",
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
        }
    })).unwrap()).unwrap()
}
fn region(a: [i32; 3], b: [i32; 3]) -> Region {
    Region::new(pos(a), pos(b))
}
pub(crate) fn scope() -> ConstructionScope {
    ConstructionScope {
        observed: region([-8, -2, -8], [12, 10, 12]),
        edits: WorldEditScope {
            editable: vec![region([-8, 0, -8], [12, 9, 12])],
            protected: vec![region([-8, -1, -8], [12, -1, 12])],
        },
        temporary: vec![region([-4, 0, 0], [-2, 5, 4])],
        travel: TravelBounds {
            min: [-7.0, 0.0, -7.0],
            max: [12.0, 10.0, 12.0],
        },
        retreat: TravelBounds {
            min: [-2.0, 0.0, -2.0],
            max: [-1.0, 0.0, -1.0],
        },
    }
}
fn block(name: &str) -> NativeBlockState {
    NativeBlockState {
        name: format!("minecraft:{name}"),
        properties: Default::default(),
    }
}
fn edit(
    position: [i32; 3],
    before: NativeBlockState,
    after: NativeBlockState,
) -> HypotheticalBlockEdit {
    HypotheticalBlockEdit {
        position,
        before,
        after,
    }
}
pub(super) fn permanent(ledger: &mut Ledger<'_>) {
    for (&position, after) in &ledger.site.structure.clone() {
        ledger
            .place(
                PlacementPurpose::Permanent,
                &edit(position, air(), after.clone()),
            )
            .unwrap();
    }
}

#[test]
fn roof_contract_preserves_exact_materials_and_requires_complete_sequence() {
    let g = design();
    let site = ConstructionSite::from_grounded(&g, scope()).unwrap();
    assert_eq!(site.structure.len(), 49);
    assert_eq!(
        Ledger::new(&site)
            .finish(&BTreeMap::new())
            .unwrap_err()
            .code,
        SurvivalErrorCode::IncompleteSequence
    );
    let mut ledger = Ledger::new(&site);
    permanent(&mut ledger);
    let budget = BTreeMap::from([("minecraft:cobblestone".into(), 49)]);
    let resources = ledger.finish(&budget).unwrap();
    assert_eq!(resources.permanent, budget);
    assert_eq!(resources.required_supplied, budget);
    assert!(resources.temporary_without_recovery.is_empty());
    assert_eq!(g.baseline.blocks.len(), 49);
    assert_eq!(g.expected.blocks.len(), 98);
}

#[test]
fn temporary_reuse_never_credits_unobserved_item_recovery() {
    let site = ConstructionSite::from_grounded(&design(), scope()).unwrap();
    let mut ledger = Ledger::new(&site);
    permanent(&mut ledger);
    let put = edit([-3, 0, 2], air(), block("dirt"));
    let remove = edit(put.position, put.after.clone(), air());
    for _ in 0..2 {
        ledger.place(PlacementPurpose::Temporary, &put).unwrap();
        ledger.remove(&remove).unwrap();
    }
    assert_eq!(
        ledger.materials.peak_temporary_in_world["minecraft:dirt"],
        1
    );
    assert_eq!(
        ledger.materials.temporary_without_recovery["minecraft:dirt"],
        2
    );
    let budget = BTreeMap::from([
        ("minecraft:cobblestone".into(), 49),
        ("minecraft:dirt".into(), 1),
    ]);
    let err = ledger.finish(&budget).unwrap_err();
    assert_eq!(err.code, SurvivalErrorCode::InsufficientSuppliedMaterials);
    assert!(err.detail.contains("minecraft:dirt"));
}

#[test]
fn temporary_only_site_requires_owned_cleanup_and_exact_initial_restoration() {
    let g = design();
    let site = ConstructionSite::temporary_work(&g.baseline, scope()).unwrap();
    assert!(site.structure.is_empty());
    let mut ledger = Ledger::new(&site);
    let put = edit([-3, 0, 2], air(), block("dirt"));
    assert!(ledger.place(PlacementPurpose::Permanent, &put).is_err());
    let remove = edit(put.position, put.after.clone(), air());
    assert!(ledger.remove(&remove).is_err());
    ledger.place(PlacementPurpose::Temporary, &put).unwrap();
    assert!(
        ledger
            .finish(&BTreeMap::from([("minecraft:dirt".into(), 1)]))
            .is_err()
    );
    let mut ledger = Ledger::new(&site);
    ledger.place(PlacementPurpose::Temporary, &put).unwrap();
    ledger.remove(&remove).unwrap();
    assert!(
        ledger
            .finish(&BTreeMap::from([("minecraft:dirt".into(), 1)]))
            .is_ok()
    );
    let mut s = scope();
    s.temporary = vec![region([0, -1, 0], [0, -1, 0])];
    assert!(ConstructionSite::temporary_work(&g.baseline, s).is_err());
}

#[test]
fn temporary_scope_cannot_expand_into_air_obligations_structure_or_ground() {
    let g = design();
    for forbidden in [[2, 1, 2], [0, 6, 0], [2, -1, 2]] {
        let mut s = scope();
        s.temporary = vec![region(forbidden, forbidden)];
        assert_eq!(
            ConstructionSite::from_grounded(&g, s).unwrap_err().code,
            SurvivalErrorCode::InvalidSiteContract
        );
    }
    let mut s = scope();
    s.edits.protected.clear();
    s.edits.editable = vec![s.observed];
    assert!(
        ConstructionSite::from_grounded(&g, s)
            .unwrap_err()
            .detail
            .contains("ground")
    );
}

#[test]
fn only_exact_own_temporary_blocks_can_be_removed_and_all_must_be_removed() {
    let site = ConstructionSite::from_grounded(&design(), scope()).unwrap();
    let mut ledger = Ledger::new(&site);
    permanent(&mut ledger);
    for (p, name) in [
        ([0, 0, 0], "cobblestone"),
        ([2, -1, 2], "stone"),
        ([-3, 0, 2], "dirt"),
    ] {
        assert_eq!(
            ledger
                .remove(&edit(p, block(name), air()))
                .unwrap_err()
                .code,
            SurvivalErrorCode::RemovalOutsideTemporaryWorks
        );
    }
    let put = edit([-3, 0, 2], air(), block("dirt"));
    ledger.place(PlacementPurpose::Temporary, &put).unwrap();
    assert!(
        ledger
            .remove(&edit(put.position, block("stone"), air()))
            .is_err()
    );
    assert_eq!(ledger.temporary.len(), 1);
    assert_eq!(
        ledger.finish(&BTreeMap::new()).unwrap_err().code,
        SurvivalErrorCode::IncompleteSequence
    );
}

#[test]
fn wrong_duplicate_and_out_of_scope_placements_fail_without_material_credit() {
    let site = ConstructionSite::from_grounded(&design(), scope()).unwrap();
    let mut ledger = Ledger::new(&site);
    assert!(
        ledger
            .place(
                PlacementPurpose::Permanent,
                &edit([0, 0, 0], air(), block("dirt"))
            )
            .is_err()
    );
    assert!(
        ledger
            .place(
                PlacementPurpose::Temporary,
                &edit([2, 1, 2], air(), block("dirt"))
            )
            .is_err()
    );
    assert!(ledger.materials.required_supplied.is_empty());
    let put = edit([0, 0, 0], air(), block("cobblestone"));
    ledger.place(PlacementPurpose::Permanent, &put).unwrap();
    assert!(ledger.place(PlacementPurpose::Permanent, &put).is_err());
    assert_eq!(
        ledger.materials.required_supplied["minecraft:cobblestone"],
        1
    );
}

#[test]
fn malformed_public_contracts_and_excessive_scopes_refuse_before_iteration() {
    let mut g = design();
    let mut s = scope();
    s.observed.max.x = i32::MAX;
    assert!(ConstructionSite::from_grounded(&g, s).is_err());
    let mut s = scope();
    s.retreat.min[0] = f64::NAN;
    assert!(ConstructionSite::from_grounded(&g, s).is_err());
    g.protected_ground.max.x = i32::MAX;
    assert!(ConstructionSite::from_grounded(&g, scope()).is_err());
    g.protected_ground = region([-1, -1, -1], [5, -1, 5]);
    g.expected.blocks.push(g.expected.blocks[0].clone());
    assert!(ConstructionSite::from_grounded(&g, scope()).is_err());
}

/// Serialization-only fixture. It never supplies a live client or execution authority.
pub(crate) fn generated_wire_fixture() -> generation::GeneratedConstructionPlan {
    let source = voxrig::checked_survival::StandingContext {
        connection_id: 0,
        receive_sequence: 0,
        client_tick: 0,
        world_revision: 0,
        dimension: "minecraft:overworld".into(),
        position_basis: voxrig::checked_survival::StandingPositionBasis::Received {
            receive_sequence: 0,
        },
        position: [0.5, 1.0, 0.5],
        eye_position: [0.5, 2.62, 0.5],
        bounds: [0.2, 1.0, 0.2, 0.8, 2.8, 0.8],
        on_ground: true,
        support: vec![[0, 0, 0]],
        submerged: false,
        player: Default::default(),
    };
    let snapshot = MinecraftSnapshot {
        min: pos([-1, -1, -1]),
        max: pos([1, 2, 1]),
        blocks: vec![],
    };
    generation::GeneratedConstructionPlan {
        plan: HypotheticalConstructionPlan {
            motion_contract: SurvivalMotionContract::Predicted,
            source,
            scope: scope(),
            baseline: snapshot.clone(),
            expected: snapshot,
            steps: vec![],
            materials: Default::default(),
            final_position: [0.5, 1.0, 0.5],
            initial_temporary: vec![],
        },
        search: Default::default(),
    }
}
