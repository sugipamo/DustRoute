//! Historical instance observations and diagnosis. None is a live baseline,
//! target proof or operation plan, and no conversion back to those exists.
use crate::recorded_review::RecordedReviewResponse;
use crate::{failure::FailureCause, observation_evidence::ObservationEvidence};
use dustroute_translate::{
    diagnostic::{difference::DifferenceKind, report::Diagnosis},
    snapshot::MinecraftSnapshot,
};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

#[derive(Clone, Debug, Serialize, Deserialize)]
pub(crate) struct RecordedPlacementReview {
    #[serde(flatten)]
    pub review: RecordedReviewResponse,
    #[serde(skip_serializing_if = "Vec::is_empty", default)]
    pub device_initial_conditions: Vec<RecordedDeviceInitialCondition>,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub(crate) struct RecordedDeviceInitialCondition {
    pub position: dustroute_physical::Pos,
    pub block: Option<String>,
    pub initial_output_signal: Option<u8>,
    pub position_history: Option<RecordedPositionHistory>,
    pub runtime_state_reconstructed_from_snapshot: bool,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub(crate) struct RecordedPositionHistory {
    pub assumed: String,
    pub observed: bool,
    pub survives_block_removal: bool,
    pub window_game_ticks: u16,
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum SampleClock {
    Client,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub(crate) struct RecordedSampleEvidence {
    pub readbacks: [ObservationEvidence; 2],
    pub sample_interval_ticks: u16,
    pub sample_interval_clock: SampleClock,
    pub observed_server_tick_interval: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub observed_client_tick_interval: Option<u64>,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub(crate) struct RecordedStableSamples {
    pub snapshot: MinecraftSnapshot,
    #[serde(flatten)]
    pub evidence: RecordedSampleEvidence,
    pub matching_samples: usize,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(tag = "status", rename_all = "snake_case")]
pub(crate) enum RecordedObservationOutcome {
    Matches {
        reason: Option<String>,
        #[serde(flatten)]
        samples: RecordedStableSamples,
    },
    Changed {
        reason: String,
        #[serde(flatten)]
        samples: RecordedStableSamples,
    },
    HistoryUnavailable {
        reason: String,
        samples: [MinecraftSnapshot; 2],
        #[serde(flatten)]
        evidence: RecordedSampleEvidence,
    },
    TargetMismatch {
        reason: String,
    },
    ObservationIncomplete {
        reason: String,
        cause: FailureCause,
    },
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub(crate) struct RecordedInstanceObservation {
    #[serde(flatten)]
    pub outcome: RecordedObservationOutcome,
    pub observed_at_unix_ms: Option<u64>,
    pub runtime_history_reconstructed: bool,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(tag = "status", rename_all = "snake_case")]
pub(crate) enum RecordedRevalidation {
    Passed {
        fresh_target_review: Box<RecordedPlacementReview>,
    },
    Failed {
        reason: String,
    },
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub(crate) struct RecordedInstanceReport {
    #[serde(flatten)]
    pub observation: RecordedInstanceObservation,
    pub revalidation: RecordedRevalidation,
    /// Eligibility at recording time; never consulted to authorize a new action.
    pub removal_eligible: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub diagnosis: Option<AssemblyDiagnosis>,
}

// Diagnosis is pure data and can be shared by generation and historical reads.
// It contains instructions/counts, not an executable reconstruction or proof.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub(crate) struct AssemblyDiagnosis {
    #[serde(flatten)]
    pub diagnosis: Diagnosis,
    #[serde(flatten)]
    pub details: DiagnosisOutcome,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub observation_failure: Option<FailureCause>,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(tag = "status", rename_all = "snake_case")]
pub(crate) enum DiagnosisOutcome {
    ObservationUnavailable { reason: String, cause: String },
    ReferenceUnverified(ComparisonDetails),
    MatchesReference(ComparisonDetails),
    DifferencesFound(ComparisonDetails),
}
impl DiagnosisOutcome {
    pub fn comparison_mut(&mut self) -> Option<&mut ComparisonDetails> {
        match self {
            Self::ObservationUnavailable { .. } => None,
            Self::ReferenceUnverified(details)
            | Self::MatchesReference(details)
            | Self::DifferencesFound(details) => Some(details),
        }
    }
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(tag = "mode", rename_all = "snake_case")]
pub(crate) enum ComparisonReference {
    RemovedInstance,
    ObservedInputs {
        input_order: Vec<dustroute_physical::Pos>,
        limit: String,
    },
    DeclaredInitial {
        reason: String,
        limit: String,
    },
    SavedInitialUnverified {
        reason: String,
        limit: String,
    },
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub(crate) struct DifferenceSummary {
    pub differing_positions: usize,
    pub by_kind: BTreeMap<DifferenceKind, usize>,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub(crate) struct ReconstructionOffer {
    pub steps: usize,
    pub target: String,
    pub affected_blocks: usize,
    pub next_action: String,
    pub review_scope: String,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub(crate) struct ComparisonDetails {
    pub reference: ComparisonReference,
    pub reference_snapshot: MinecraftSnapshot,
    pub summary: DifferenceSummary,
    pub repair_details: Option<ReconstructionOffer>,
    pub cause: String,
    pub ownership: String,
    pub server_readiness_proven: bool,
    pub interpretation: String,
}
