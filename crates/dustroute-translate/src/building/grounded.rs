//! Site-bound passive geometry, shared Blueprint review and protected diff proof.
//! No player reach/access or live-site/adoption authority is implied.
use super::{BuildingDesignError, design_geometry, sources};
use crate::blueprint_update::{BlueprintUpdates, RecordedReview};
use crate::piston_construction::ElectricalModification;
use crate::promotion::{CheckStatus, review_assembly_with_context};
use crate::snapshot::{MinecraftSnapshot, MinecraftSnapshotBlock, assembly_from_snapshot};
use dustroute_library::blueprint::BlueprintRecords;
use dustroute_library::building::GroundedBuildingDesignRequest;
use dustroute_library::runtime_behavior::RuntimeBehaviorContext;
use dustroute_library::world_edit::WorldEditScope;
use dustroute_minecraft::{Pos, Region};
use serde::Serialize;
use std::collections::BTreeMap;

#[derive(Clone, Debug, Serialize)]
pub struct GeneratedGroundedBuildingDesign {
    pub specification: GroundedBuildingDesignRequest,
    pub records: BlueprintRecords,
    pub request: crate::blueprint_update::BlueprintUpdateRequest,
    pub context: RuntimeBehaviorContext,
    /// Complete declared starting site. Must be independently observed live.
    pub baseline: MinecraftSnapshot,
    /// Final structure plus unchanged ground, including exact empty cells.
    pub expected: MinecraftSnapshot,
    pub protected_ground: Region,
    /// Permanent material counts exclude the already existing ground.
    pub materials: BTreeMap<String, usize>,
    /// Exact structural requests, not commands or an executable player schedule.
    pub structure: Vec<MinecraftSnapshotBlock>,
    pub verification: GroundedBuildingVerification,
}

#[derive(Clone, Debug, Serialize)]
pub struct GroundedBuildingVerification {
    pub review: RecordedReview,
    pub modeled_changes: usize,
    pub modeled_removals: usize,
    pub baseline_restored: bool,
    pub live_world_verified: bool,
    pub player_construction_verified: bool,
}

/// Author a new grounded contract, never reinterpret an old air-guard pass.
/// The current bounded stage uses the shared 64-change modification budget.
pub fn generate_grounded_building_design(
    specification: GroundedBuildingDesignRequest,
) -> Result<GeneratedGroundedBuildingDesign, BuildingDesignError> {
    let fail = |code: &'static str, detail: &str| BuildingDesignError::new(code, detail);
    let design = &specification.design;
    if design.component.is_some() {
        return Err(fail(
            "unsupported_grounded_component",
            "grounded survival authoring currently admits passive structures only",
        ));
    }
    let mut geometry = design_geometry::expand(design)?;
    let region = geometry.region;
    let ground = Region::new(
        region.min,
        Pos::new(region.max.x, region.min.y, region.max.z),
    );
    if design
        .spaces
        .iter()
        .any(|s| design_geometry::intersects(s.region, ground))
    {
        return Err(fail(
            "ground_conflicts_with_permanent_air",
            "declared permanent air intersects protected ground",
        ));
    }
    let structure = geometry.initial.blocks.clone();
    let mut materials = BTreeMap::new();
    for b in &structure {
        *materials.entry(b.name.clone()).or_default() += 1;
    }
    let baseline = MinecraftSnapshot {
        min: region.min,
        max: region.max,
        blocks: (ground.min.x..=ground.max.x)
            .flat_map(|x| {
                (ground.min.z..=ground.max.z).map(move |z| MinecraftSnapshotBlock {
                    pos: Pos::new(x, ground.min.y, z),
                    name: specification.ground_material.native_name().into(),
                    properties: Default::default(),
                })
            })
            .collect(),
    };
    geometry.initial.blocks.extend(baseline.blocks.clone());
    geometry.initial.blocks.sort_by_key(|b| b.pos);
    let expected = geometry.initial.clone();
    let context = RuntimeBehaviorContext::fresh_pistons(region, vec![]);
    let (mut records, mut request) = sources::build_layout(
        &design.namespace,
        &design.name,
        &geometry,
        &design.spaces,
        &context,
    )
    .map_err(|e| fail("invalid_sources", &e))?;
    // The parent site's predecessor already contains the ground. It is never
    // introduced as a building part, resource request or editable diff.
    let base = records
        .assemblies
        .iter_mut()
        .find(|a| a.id == request.base_state)
        .ok_or_else(|| fail("invalid_sources", "missing grounded predecessor"))?;
    base.assembly.blocks =
        assembly_from_snapshot(&baseline, "Protected existing ground", vec![region])
            .map_err(|e| fail("invalid_sources", &e.to_string()))?
            .blocks;
    base.assembly.name = "Declared unbuilt grounded site".into();
    request.description = "New exact structure with an immutable protected ground contract. Ground is already present in the declared predecessor. No live observation, adoption, player access or execution permission is granted.".into();
    let mut updates = BlueprintUpdates::new(
        records
            .catalog()
            .map_err(|e| fail("invalid_sources", &e.to_string()))?,
    );
    updates
        .create(request.clone())
        .map_err(|e| fail("invalid_sources", &e.to_string()))?;
    let mut catalog = records
        .catalog()
        .map_err(|e| fail("invalid_sources", &e.to_string()))?;
    catalog
        .insert_revisions(request.revisions.clone())
        .map_err(|e| fail("invalid_sources", &e.to_string()))?;
    let report = review_assembly_with_context(
        &catalog,
        &request.candidate_state.assembly,
        Some(&context.clone().into()),
        Default::default(),
    )
    .map_err(|e| fail("verification_not_established", &e.to_string()))?;
    if report.status() != CheckStatus::Passed {
        return Err(BuildingDesignError {
            code: "verification_not_established",
            item: None,
            position: None,
            detail: "grounded whole-Assembly review failed".into(),
            diagnostics: Some(Box::new(report.diagnostics(64))),
        });
    }
    let scope = WorldEditScope {
        editable: vec![Region::new(
            Pos::new(region.min.x, region.min.y + 1, region.min.z),
            region.max,
        )],
        protected: vec![ground],
    };
    let proof =
        ElectricalModification::new_scoped(&baseline, &expected, scope, context.root_limits)
            .map_err(|e| fail("grounded_construction_not_established", &e.to_string()))?;
    if proof
        .steps(true)
        .last()
        .is_none_or(|s| s.expected.materialize() != baseline)
    {
        return Err(fail(
            "grounded_restoration_not_established",
            "modeled removal must restore existing ground and all declared air",
        ));
    }
    let verification = GroundedBuildingVerification {
        review: RecordedReview::from(&report),
        modeled_changes: proof.steps(false).len(),
        modeled_removals: proof.steps(true).len(),
        baseline_restored: true,
        live_world_verified: false,
        player_construction_verified: false,
    };
    Ok(GeneratedGroundedBuildingDesign {
        specification,
        records,
        request,
        context,
        baseline,
        expected,
        protected_ground: ground,
        materials,
        structure,
        verification,
    })
}
