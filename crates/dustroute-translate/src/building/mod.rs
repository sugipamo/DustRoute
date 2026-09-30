//! Building authoring uses ordinary immutable Blueprints and shared physics.
mod component;
mod design;
mod design_geometry;
mod door;
mod door_interface;
mod geometry;
mod sources;
mod update;
pub use design::{
    AttachedDesignComponent, BuildingDesignError, GeneratedBuildingDesign, generate_building_design,
};
pub use door::{AttachedBuildingDoor, generate_building_with_door};
pub use update::{GeneratedBuildingDesignUpdate, generate_building_design_update};

use crate::blueprint_update::{BlueprintUpdateRequest, BlueprintUpdates, RecordedReview};
use crate::piston_construction::{ElectricalConstruction, construction_batches};
use crate::promotion::{CheckStatus, review_assembly_with_context};
use crate::snapshot::MinecraftSnapshot;
use dustroute_library::blueprint::BlueprintRecords;
use dustroute_library::building::{BuildingEntrance, BuildingRequest};
use dustroute_library::runtime_behavior::RuntimeBehaviorContext;
use serde::Serialize;
use std::collections::BTreeMap;

#[derive(Clone, Debug, Serialize)]
pub struct GeneratedBuilding {
    pub specification: BuildingRequest,
    pub entrance: BuildingEntrance,
    pub records: BlueprintRecords,
    pub request: BlueprintUpdateRequest,
    pub context: RuntimeBehaviorContext,
    pub expected: MinecraftSnapshot,
    pub parts: BTreeMap<String, usize>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub door: Option<AttachedBuildingDoor>,
    pub verification: BuildingVerification,
}

#[derive(Clone, Debug, Serialize)]
pub struct BuildingVerification {
    pub status: CheckStatus,
    pub review: RecordedReview,
    pub construction_steps: usize,
    pub construction_batches: usize,
    pub removal_steps: usize,
    pub removal_batches: usize,
    pub live_world_verified: bool,
}

/// Fresh structural and construction review, not adoption or site observation.
pub fn generate_building(
    specification: BuildingRequest,
) -> Result<GeneratedBuilding, BuildingDesignError> {
    let (geometry, entrance) = geometry::expand(&specification)?;
    let context = RuntimeBehaviorContext::fresh_pistons(geometry.region, vec![]);
    let (records, request) = sources::build(&specification, &geometry, &context)?;
    finish(
        specification,
        entrance,
        geometry,
        records,
        request,
        context,
        None,
    )
}

fn finish(
    specification: BuildingRequest,
    entrance: BuildingEntrance,
    geometry: geometry::Geometry,
    records: BlueprintRecords,
    request: BlueprintUpdateRequest,
    context: RuntimeBehaviorContext,
    door: Option<AttachedBuildingDoor>,
) -> Result<GeneratedBuilding, BuildingDesignError> {
    let verification = verify_candidate(&records, &request, &context, &geometry.initial)?;
    Ok(GeneratedBuilding {
        specification,
        entrance,
        records,
        request,
        context,
        expected: geometry.initial,
        door,
        parts: geometry
            .parts
            .into_iter()
            .map(|(name, blocks)| (name, blocks.len()))
            .collect(),
        verification,
    })
}

fn verify_candidate(
    records: &BlueprintRecords,
    request: &BlueprintUpdateRequest,
    context: &RuntimeBehaviorContext,
    expected: &MinecraftSnapshot,
) -> Result<BuildingVerification, BuildingDesignError> {
    if expected.blocks.len() > 256 {
        return Err("generated building exceeds the 256-block custom Assembly budget".into());
    }
    let mut updates = BlueprintUpdates::new(records.catalog().map_err(|e| e.to_string())?);
    updates.create(request.clone()).map_err(|e| e.to_string())?;
    let mut catalog = records.catalog().map_err(|e| e.to_string())?;
    catalog
        .insert_revisions(request.revisions.clone())
        .map_err(|e| e.to_string())?;
    let report = review_assembly_with_context(
        &catalog,
        &request.candidate_state.assembly,
        Some(&context.clone().into()),
        Default::default(),
    )
    .map_err(|e| e.to_string())?;
    if report.status() != CheckStatus::Passed {
        return Err(BuildingDesignError {
            code: "verification_not_established",
            item: None,
            position: None,
            detail: format!(
                "whole-Assembly review {:?}; inspect diagnostics.findings before changing the design",
                report.status()
            ),
            diagnostics: Some(Box::new(report.diagnostics(64))),
        });
    }
    let world = request
        .candidate_state
        .assembly
        .inspect(&catalog)
        .map_err(|e| e.to_string())?
        .proposed_world();
    let construction =
        ElectricalConstruction::new(&world, context.known_region, context.root_limits)
            .map_err(|e| BuildingDesignError::new("construction_not_established", e))?;
    if construction.settled() != expected
        || !construction
            .remove_steps()
            .last()
            .is_some_and(|s| s.expected.blocks.is_empty())
    {
        return Err(BuildingDesignError::new(
            "construction_mismatch",
            "building construction or teardown differs from declared geometry",
        ));
    }
    Ok(BuildingVerification {
        status: CheckStatus::Passed,
        review: RecordedReview::from(&report),
        construction_steps: construction.build_steps().len(),
        construction_batches: construction_batches(construction.build_steps()).count(),
        removal_steps: construction.remove_steps().len(),
        removal_batches: construction_batches(construction.remove_steps()).count(),
        live_world_verified: false,
    })
}
