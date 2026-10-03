//! Historical placement provenance, never a passing admission or live plan.
use crate::recorded_review::RecordedReviewResponse;
use dustroute_library::blueprint::{AssemblyRevisionId, BlueprintUpdateId};
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub(crate) enum PlacementSource {
    CircuitRevision {
        revision_id: uuid::Uuid,
    },
    AdoptedAssemblyRevision {
        assembly_revision_id: AssemblyRevisionId,
        adopted_by: BlueprintUpdateId,
        grounding_assembly_revision_id: AssemblyRevisionId,
        fresh_review: Box<RecordedReviewResponse>,
        literal_observation: LiteralObservation,
        candidate_interpretation: CandidateInterpretation,
    },
}
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub(crate) enum LiteralObservation {
    #[serde(rename = "grounding.base_snapshot")]
    GroundingBaseSnapshot,
}
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub(crate) enum CandidateInterpretation {
    #[serde(rename = "record.assembly")]
    RecordAssembly,
}
