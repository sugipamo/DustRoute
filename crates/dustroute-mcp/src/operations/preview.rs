//! Diagnostic projections of intentions. These values cannot reconstruct a
//! fresh proof, executable plan or observation authority.
mod electrical;
mod job;
use super::mutation::Success;
use crate::placement_source::PlacementSource;
use dustroute_translate::piston_construction::{ElectricalConstructionStep, construction_batches};
use dustroute_translate::snapshot::MinecraftSnapshot;
use dustroute_translate::world_reverse::RegionBounds;
pub use electrical::ElectricalEditPreview;
pub(crate) use electrical::ShownElectricalEdit;
pub(crate) use job::{JobObservation, JobResponse, JobStagePreview, JobSummary};
use serde::Serialize;
use uuid::Uuid;

#[derive(Clone, Debug, PartialEq, Serialize)]
pub(crate) struct StateSummary {
    bounds: RegionBounds,
    non_air_blocks: usize,
    state_content_id: crate::snapshot_content::ContentId,
    role: &'static str,
}
impl StateSummary {
    pub fn new(snapshot: &MinecraftSnapshot) -> Self {
        Self {
            bounds: RegionBounds {
                min: snapshot.min,
                max: snapshot.max,
            },
            non_air_blocks: snapshot.blocks.len(),
            state_content_id: crate::snapshot_content::content_id(snapshot),
            role: "model intention, not live-world evidence",
        }
    }
}
#[derive(Clone, Debug, PartialEq, Serialize)]
pub(crate) struct BatchSummary {
    first_step: usize,
    last_step: usize,
    changed_blocks: usize,
    wait_ticks: u64,
    full_region_readback_before_and_after: bool,
}
pub(crate) fn batch_summary(steps: &[ElectricalConstructionStep]) -> Vec<BatchSummary> {
    construction_batches(steps)
        .map(|batch| BatchSummary {
            first_step: batch.first_step(),
            last_step: batch.last_step(),
            changed_blocks: batch.steps().len(),
            wait_ticks: batch.wait_ticks(),
            full_region_readback_before_and_after: true,
        })
        .collect()
}

#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct GroundedRevisionPreview {
    ok: Success,
    operation_id: Uuid,
    source: PlacementSource,
    revision_id: Uuid,
    base_observation_id: Uuid,
    bounds: RegionBounds,
    plan: dustroute_app::PlacementPlan,
    read_only: bool,
    next_step: &'static str,
    validation_scope: &'static str,
}
impl GroundedRevisionPreview {
    pub(crate) fn new(
        source: PlacementSource,
        revision_id: Uuid,
        base_observation_id: Uuid,
        bounds: RegionBounds,
        plan: dustroute_app::PlacementPlan,
        read_only: bool,
    ) -> Self {
        Self {
            ok: Success,
            operation_id: plan.operation_id,
            source,
            revision_id,
            base_observation_id,
            bounds,
            plan,
            read_only,
            next_step: "show_operation then invoke_operation(confirm=true)",
            validation_scope: "fresh model review when declared, modeled placement, exact live base and surrounding context; not general functional equivalence",
        }
    }
}
#[derive(Debug, Serialize)]
#[serde(untagged)]
pub(crate) enum GroundedPlacementPreview {
    Ordinary(Box<GroundedRevisionPreview>),
    Electrical(Box<ElectricalEditPreview>),
    Job(Box<JobStagePreview>),
}

#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct DiscardedJobStage {
    ok: bool,
    status: DiscardedStatus,
    #[serde(skip_serializing_if = "Option::is_none")]
    job_id: Option<Uuid>,
    next_step: &'static str,
}
#[derive(Clone, Copy, Debug, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
enum DiscardedStatus {
    JobStageCapabilityDiscarded,
}
impl DiscardedJobStage {
    pub(crate) fn superseded() -> Self {
        Self {
            ok: false,
            status: DiscardedStatus::JobStageCapabilityDiscarded,
            job_id: None,
            next_step: "freshly plan the job stage",
        }
    }
    pub(crate) fn discarded(job_id: Uuid) -> Self {
        Self {
            job_id: Some(job_id),
            next_step: "get_operation for durable edit history, or manage_construction_job",
            ..Self::superseded()
        }
    }
}

#[derive(Debug, Serialize)]
pub(crate) struct ElectricalEditHistory {
    ok: Success,
    operation_id: Uuid,
    kind: &'static str,
    record: crate::edit_registry::EditRecord,
    historical: bool,
    saved_record_is_validation_proof: bool,
    executable_plan_restored: bool,
}
impl ElectricalEditHistory {
    pub fn new(record: crate::edit_registry::EditRecord) -> Self {
        Self {
            ok: Success,
            operation_id: record.operation_id,
            kind: "electrical_revision_modification",
            record,
            historical: true,
            saved_record_is_validation_proof: false,
            executable_plan_restored: false,
        }
    }
}
