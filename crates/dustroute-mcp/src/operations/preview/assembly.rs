//! Assembly intentions and historical displays are data, never fresh proofs.
//! None of these projections can restore an executable plan or live baseline.
use super::{BatchSummary, batch_summary};
use crate::assembly_registry::{
    InstanceSummary, OperatingRemoval, PlacedAssembly, ReconstructionAttempt,
};
use crate::operations::construction::{AssemblyConstructionKind, ReconstructionConditions};
use crate::operations::mutation::Success;
use crate::piston_assembly::ValidatedAssemblyPlacement;
use crate::recorded_instance::{
    AssemblyDiagnosis, RecordedInstanceReport, RecordedPlacementReview,
};
use crate::source_identity::SourceIdentity;
use dustroute_library::assembly::Assembly;
use dustroute_library::blueprint::{AssemblyRevisionId, BlueprintUpdateId};
use dustroute_library::runtime_behavior::RuntimeBehaviorContext;
use dustroute_translate::diagnostic::difference::SnapshotDifference;
use dustroute_translate::piston_construction::ElectricalConstructionStep;
use dustroute_translate::snapshot::MinecraftSnapshot;
use dustroute_translate::world_reverse::RegionBounds;
use rmcp::schemars;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(
    Clone, Copy, Debug, Default, Deserialize, Serialize, schemars::JsonSchema, PartialEq, Eq,
)]
#[serde(rename_all = "snake_case")]
pub(crate) enum RemovalReference {
    #[default]
    Constructed,
    ObservedInputs,
}

#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(untagged)]
pub enum AssemblyPreview {
    Construction(Box<AssemblyConstructionPreview>),
    Removal(Box<AssemblyRemovalPreview>),
    Reconstruction(Box<AssemblyReconstructionPreview>),
}

#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct AssemblyConstructionPreview {
    ok: Success,
    operation_id: Uuid,
    kind: AssemblyConstructionKind,
    assembly_revision_id: AssemblyRevisionId,
    adopted_by: BlueprintUpdateId,
    bounds: RegionBounds,
    dimension: String,
    proposed_assembly: Assembly,
    execution_context: RuntimeBehaviorContext,
    fresh_target_review: RecordedPlacementReview,
    construction_steps: Vec<ElectricalConstructionStep>,
    undo_steps: Vec<ElectricalConstructionStep>,
    live_world_verified: bool,
    execution_batches: Vec<BatchSummary>,
    undo_execution_batches: Vec<BatchSummary>,
    read_only: bool,
    next_step: &'static str,
    undo_requirement: &'static str,
}
impl AssemblyConstructionPreview {
    pub(crate) fn new(
        operation_id: Uuid,
        assembly_revision_id: AssemblyRevisionId,
        source: &SourceIdentity,
        dimension: String,
        read_only: bool,
        proof: &ValidatedAssemblyPlacement,
    ) -> Self {
        Self {
            ok: Success,
            operation_id,
            kind: AssemblyConstructionKind::Construct,
            assembly_revision_id,
            adopted_by: source.adopted_by.clone(),
            bounds: proof.bounds(),
            dimension,
            proposed_assembly: proof.assembly().clone(),
            execution_context: proof.context().clone(),
            fresh_target_review: proof.review().recorded(),
            construction_steps: proof.steps(false).to_vec(),
            undo_steps: proof.steps(true).to_vec(),
            live_world_verified: false,
            execution_batches: batch_summary(proof.steps(false)),
            undo_execution_batches: batch_summary(proof.steps(true)),
            read_only,
            next_step: "show_operation, then invoke_operation(confirm=true)",
            undo_requirement: "exact constructed settled state and unchanged entire observation region",
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct AssemblyRemovalPreview {
    ok: Success,
    kind: AssemblyConstructionKind,
    operation_id: Uuid,
    instance_id: Uuid,
    record_revision: u64,
    bounds: RegionBounds,
    dimension: String,
    read_only: bool,
    removal_steps: Vec<ElectricalConstructionStep>,
    removal_reference: RemovalReference,
    operating_removal: Option<OperatingRemoval>,
    execution_batches: Vec<BatchSummary>,
    server_readiness_proven: bool,
    runtime_history_reconstructed: bool,
    operator_requirement: &'static str,
    fresh_target_review: RecordedPlacementReview,
    observation: RecordedInstanceReport,
    next_step: &'static str,
}
impl AssemblyRemovalPreview {
    pub(crate) fn new(
        operation_id: Uuid,
        record: &PlacedAssembly,
        removal_reference: RemovalReference,
        operating: Option<&OperatingRemoval>,
        proof: &ValidatedAssemblyPlacement,
        observation: RecordedInstanceReport,
        read_only: bool,
    ) -> Self {
        let steps = operating.map_or_else(|| proof.steps(true), |p| p.steps.as_slice());
        Self {
            ok: Success,
            kind: AssemblyConstructionKind::Remove,
            operation_id,
            instance_id: record.instance_id,
            record_revision: record.revision,
            bounds: proof.bounds(),
            dimension: record.target.dimension.clone(),
            read_only,
            removal_steps: steps.to_vec(),
            removal_reference,
            operating_removal: operating.cloned(),
            execution_batches: batch_summary(steps),
            server_readiness_proven: false,
            runtime_history_reconstructed: false,
            operator_requirement: "let prior operations finish and keep external inputs and edits out of the region during removal",
            fresh_target_review: proof.review().recorded(),
            observation,
            next_step: "show_operation, then invoke_operation(confirm=true)",
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct AssemblyReconstructionPreview {
    ok: Success,
    kind: AssemblyConstructionKind,
    operation_id: Uuid,
    instance_id: Uuid,
    record_revision: u64,
    bounds: RegionBounds,
    dimension: String,
    read_only: bool,
    differences: Vec<SnapshotDifference>,
    reconstruction: ReconstructionAttempt,
    reconstruction_conditions: ReconstructionConditions,
    execution_batches: Vec<BatchSummary>,
    fresh_target_review: RecordedPlacementReview,
    observation: RecordedInstanceReport,
    next_step: &'static str,
}
impl AssemblyReconstructionPreview {
    pub(crate) fn new(
        operation_id: Uuid,
        record: &PlacedAssembly,
        proof: &ValidatedAssemblyPlacement,
        reconstruction: &ReconstructionAttempt,
        differences: Vec<SnapshotDifference>,
        observation: RecordedInstanceReport,
        read_only: bool,
    ) -> Self {
        Self {
            ok: Success,
            kind: AssemblyConstructionKind::Reconstruct,
            operation_id,
            instance_id: record.instance_id,
            record_revision: record.revision,
            bounds: proof.bounds(),
            dimension: record.target.dimension.clone(),
            read_only,
            differences,
            reconstruction: reconstruction.clone(),
            reconstruction_conditions: ReconstructionConditions::declared(),
            execution_batches: batch_summary(&reconstruction.steps),
            fresh_target_review: proof.review().recorded(),
            observation,
            next_step: "show_operation; confirm all observed blocks may be removed and rebuilt, then invoke_operation(confirm=true)",
        }
    }
}

#[derive(Debug, Serialize)]
#[serde(untagged)]
pub(crate) enum AssemblyManagementReport {
    List {
        ok: Success,
        instances: Vec<InstanceSummary>,
        fresh_observation: bool,
    },
    Get(Box<InstanceDetails>),
    Observed {
        ok: bool,
        instance: Box<InstanceSummary>,
        observation: Box<RecordedInstanceReport>,
        error: Option<&'static str>,
    },
    Diagnosed {
        ok: Success,
        instance: Box<InstanceSummary>,
        diagnosis: Option<Box<AssemblyDiagnosis>>,
        observation: Box<RecordedInstanceReport>,
    },
    Removal(Box<AssemblyRemovalPreview>),
    Reconstruction {
        #[serde(flatten)]
        plan: Box<AssemblyReconstructionPreview>,
        diagnosis: Option<Box<AssemblyDiagnosis>>,
    },
    ReconstructionRefused {
        ok: bool,
        error: String,
        diagnosis: Option<Box<AssemblyDiagnosis>>,
    },
}
#[derive(Debug, Serialize)]
pub(crate) struct InstanceDetails {
    ok: Success,
    instance: InstanceSummary,
    assembly: Assembly,
    execution_context: RuntimeBehaviorContext,
    expected_snapshot: MinecraftSnapshot,
    pinned_source: SourceIdentity,
    fresh_observation: bool,
}
impl InstanceDetails {
    pub(crate) fn new(record: PlacedAssembly) -> Self {
        Self {
            ok: Success,
            instance: record.summary(),
            assembly: record.assembly,
            execution_context: record.context,
            expected_snapshot: record.expected,
            pinned_source: record.source_identity,
            fresh_observation: false,
        }
    }
}

/// Public spelling of the existing in-memory state, without text formatting.
#[derive(Clone, Copy, Debug, Serialize)]
pub(crate) enum AssemblyPlanState {
    Planned,
    Applied,
    Undone,
    NeedsInspection,
}

#[derive(Debug, Serialize)]
pub(crate) struct AssemblyPlanSteps {
    steps: Vec<ElectricalConstructionStep>,
    construction_steps: Vec<ElectricalConstructionStep>,
    undo_steps: Vec<ElectricalConstructionStep>,
    execution_batches: Vec<BatchSummary>,
    undo_execution_batches: Vec<BatchSummary>,
    operating_removal: Option<OperatingRemoval>,
    reconstruction: Option<ReconstructionAttempt>,
    reconstruction_conditions: Option<ReconstructionConditions>,
}
impl AssemblyPlanSteps {
    pub(crate) fn new(
        proof: &ValidatedAssemblyPlacement,
        steps: &[ElectricalConstructionStep],
        undo_steps: &[ElectricalConstructionStep],
        operating_removal: Option<&OperatingRemoval>,
        reconstruction: Option<&ReconstructionAttempt>,
    ) -> Self {
        Self {
            steps: steps.to_vec(),
            construction_steps: proof.steps(false).to_vec(),
            undo_steps: proof.steps(true).to_vec(),
            execution_batches: batch_summary(steps),
            undo_execution_batches: batch_summary(undo_steps),
            operating_removal: operating_removal.cloned(),
            reconstruction: reconstruction.cloned(),
            reconstruction_conditions: reconstruction.map(|_| ReconstructionConditions::declared()),
        }
    }
}
#[derive(Debug, Serialize)]
pub(crate) struct AssemblyPlanDetails {
    pub ok: Success,
    pub operation_id: Uuid,
    pub kind: AssemblyConstructionKind,
    pub assembly_revision_id: AssemblyRevisionId,
    pub bounds: RegionBounds,
    pub proposed_assembly: Assembly,
    #[serde(flatten)]
    pub steps: AssemblyPlanSteps,
    pub previewed: bool,
    pub state: AssemblyPlanState,
    pub stored_history_is_validation_proof: bool,
}
#[derive(Debug, Serialize)]
pub(crate) struct ShownAssemblyPlan {
    pub ok: Success,
    pub operation_id: Uuid,
    pub preview: crate::bridge::PreviewSubmission,
    pub bounds: RegionBounds,
    pub kind: AssemblyConstructionKind,
    #[serde(flatten)]
    pub steps: AssemblyPlanSteps,
}
