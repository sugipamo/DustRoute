//! Repair candidates and competing hypotheses are diagnostic data, never plans.
use crate::api::{REPAIR_CONTEXT_SCHEMA_V1, REPAIR_SCHEMA_V1};
use crate::operations::mutation::Success;
use dustroute_physical::Pos;
use dustroute_physical::{
    BlockKind, ComponentId, Confidence, Facing, GapCandidate, GapEvidence, PhysicalPatch,
    PhysicalPatchReason, RepairImpact, RepairProposal, SupportRelation, TemporalAssessment,
    TransferKind,
};
use dustroute_translate::diagnostic::CircuitDiagnosticReport;
use dustroute_translate::world_reverse::RegionBounds;
use serde::Serialize;
use uuid::Uuid;

/// Exactly the historical proposal payload: it has no execution facts.
#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(transparent)]
pub struct RepairCandidate(RepairProposal);
impl RepairCandidate {
    pub(crate) fn new(proposal: RepairProposal) -> Self {
        Self(proposal)
    }
}
#[derive(Debug, Serialize)]
pub(crate) struct RepairCandidateEntry {
    pub operation_id: Uuid,
    pub diagnostic_finding_ids: Vec<String>,
    #[serde(flatten)]
    pub candidate: RepairCandidate,
}
#[derive(Debug, Serialize)]
pub(crate) struct RepairCandidates {
    schema_version: &'static str,
    ok: Success,
    circuit_id: Uuid,
    bounds: RegionBounds,
    fragments: usize,
    diagnostic: CircuitDiagnosticReport,
    proposal_count: usize,
    proposals: Vec<RepairCandidateEntry>,
    next_step: &'static str,
}
impl RepairCandidates {
    pub(crate) fn new(
        circuit_id: Uuid,
        bounds: RegionBounds,
        fragments: usize,
        diagnostic: CircuitDiagnosticReport,
        proposals: Vec<RepairCandidateEntry>,
    ) -> Self {
        Self {
            schema_version: REPAIR_SCHEMA_V1,
            ok: Success,
            circuit_id,
            bounds,
            fragments,
            diagnostic,
            proposal_count: proposals.len(),
            proposals,
            next_step: "review a proposal, call show_operation, ask for explicit confirmation, then call invoke_operation with confirm=true",
        }
    }
}
#[derive(Debug, Serialize)]
pub(crate) struct RemovalCandidate {
    schema_version: &'static str,
    ok: Success,
    operation_id: Uuid,
    proposal: RepairProposal,
    warning: &'static str,
    next_step: &'static str,
}
impl RemovalCandidate {
    pub(crate) fn new(operation_id: Uuid, proposal: RepairProposal) -> Self {
        Self {
            schema_version: REPAIR_SCHEMA_V1,
            ok: Success,
            operation_id,
            proposal,
            warning: "removal intent cannot be inferred from geometry alone; preview and explicit confirmation are required",
            next_step: "call show_operation, then invoke_operation with confirm=true only after confirmation",
        }
    }
}
#[derive(Debug, Serialize)]
pub(crate) struct ShownRepair {
    schema_version: &'static str,
    ok: Success,
    operation_id: Uuid,
    bounds: RegionBounds,
    patch: PhysicalPatch,
    preview: crate::bridge::PreviewSubmission,
    next_step: &'static str,
}
impl ShownRepair {
    pub(crate) fn new(
        operation_id: Uuid,
        bounds: RegionBounds,
        patch: PhysicalPatch,
        preview: crate::bridge::PreviewSubmission,
    ) -> Self {
        Self {
            schema_version: REPAIR_SCHEMA_V1,
            ok: Success,
            operation_id,
            bounds,
            patch,
            preview,
            next_step: "obtain explicit player confirmation before invoke_operation",
        }
    }
}

#[derive(Debug, Serialize)]
pub(crate) struct RelatedConnection {
    pub component: ComponentId,
    pub transfer: TransferKind,
    pub confidence: Confidence,
}
#[derive(Debug, Serialize)]
pub(crate) struct RelatedComponent {
    pub id: ComponentId,
    pub position: Pos,
    pub block: BlockKind,
    pub facing: Option<Facing>,
    pub support: Option<SupportRelation>,
    pub incoming: Vec<RelatedConnection>,
    pub outgoing: Vec<RelatedConnection>,
}
#[derive(Debug, Serialize)]
#[serde(untagged)]
pub(crate) enum RepairHypothesis {
    Repair {
        kind: PhysicalPatchReason,
        confidence_percent: u8,
        supporting_evidence: Vec<String>,
        contradictions: Vec<String>,
        diagnostic_finding_ids: Vec<String>,
        physical_evidence: Vec<GapEvidence>,
        counterfactual_impact: Option<RepairImpact>,
        operation_id: Option<Uuid>,
    },
    ExternalInputs {
        kind: ExternalInputHypothesis,
        confidence: HypothesisConfidence,
        supporting_evidence: Vec<&'static str>,
        contradictions: Vec<String>,
    },
}
#[derive(Debug, Serialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum ExternalInputHypothesis {
    IntentionalExternalInputs,
}
#[derive(Debug, Serialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum HypothesisConfidence {
    Plausible,
}
#[derive(Debug, Serialize)]
pub(crate) struct RepairContextFacts {
    pub observation_complete: bool,
    pub components: usize,
    pub fragments: usize,
    pub gap_candidates: Vec<GapCandidate>,
    pub gap_candidates_truncated: bool,
    pub diagnostic: CircuitDiagnosticReport,
    pub temporal: TemporalAssessment,
}
#[derive(Debug, Serialize)]
pub(crate) struct RepairContextReport {
    schema_version: &'static str,
    ok: Success,
    circuit_id: Uuid,
    operation_id: Option<Uuid>,
    summary: String,
    facts: RepairContextFacts,
    hypotheses: Vec<RepairHypothesis>,
    related_components: Vec<RelatedComponent>,
    questions: Vec<String>,
    next_step: &'static str,
}
impl RepairContextReport {
    pub(crate) fn new(
        circuit_id: Uuid,
        operation_id: Option<Uuid>,
        gap_count: usize,
        facts: RepairContextFacts,
        hypotheses: Vec<RepairHypothesis>,
        related_components: Vec<RelatedComponent>,
        questions: Vec<String>,
    ) -> Self {
        Self {
            schema_version: REPAIR_CONTEXT_SCHEMA_V1,
            ok: Success,
            circuit_id,
            operation_id,
            summary: format!(
                "{} physical component(s), {} traversal fragment(s), {} nearby gap candidate(s), {} repair hypothesis/hypotheses",
                facts.components,
                facts.fragments,
                gap_count,
                hypotheses.len()
            ),
            facts,
            hypotheses,
            related_components,
            questions,
            next_step: "compare the hypotheses with the player's intent; use show_operation only after choosing a repair hypothesis",
        }
    }
}
