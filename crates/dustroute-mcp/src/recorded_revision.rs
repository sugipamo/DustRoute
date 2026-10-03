//! Diagnostics for hypothetical revisions, never live evidence or capabilities.
use crate::{failure::FailureCause, recorded_analysis::CircuitIdentity};
use dustroute_translate::{
    diagnostic::{DiagnosticCounts, DiagnosticHealth, report::Finding},
    world::WorldValidationIssue,
    world_reverse::TerminalConfidence,
};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(untagged)]
pub(crate) enum RevisionValidation {
    StateReview(Box<StateRevisionReview>),
    AdoptedReview { source: AdoptedReviewSource },
}
#[derive(Clone, Copy, Debug, Serialize, Deserialize)]
pub(crate) enum AdoptedReviewSource {
    #[serde(rename = "fresh adopted Assembly review")]
    FreshAdoptedAssemblyReview,
}
impl RevisionValidation {
    pub fn state_mut(&mut self) -> Option<&mut StateRevisionReview> {
        match self {
            Self::StateReview(review) => Some(review),
            Self::AdoptedReview { .. } => None,
        }
    }
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub(crate) struct StateRevisionReview {
    pub before: StateValidation,
    pub after: StateValidation,
    pub simulation_ticks: usize,
    pub scope: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub electrical_modification: Option<ElectricalValidation>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub assembly: Option<AssemblyValidation>,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(tag = "status", rename_all = "snake_case")]
pub(crate) enum StateValidation {
    Unavailable {
        error: String,
        simulation: SimulationValidation,
    },
    Incomplete(StateReport),
    StructurallyValid(StateReport),
    InvalidOrUnsupported(StateReport),
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub(crate) struct StateReport {
    pub placement_issues: Vec<WorldValidationIssue>,
    pub summary: VirtualAnalysisSummary,
    pub simulation: SimulationValidation,
    pub property_validation: String,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(tag = "status", rename_all = "snake_case")]
pub(crate) enum SimulationValidation {
    NotRun {
        #[serde(skip_serializing_if = "Option::is_none")]
        reason: Option<String>,
    },
    Simulated {
        result: TerminalSimulation,
    },
    Unavailable {
        error: String,
    },
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub(crate) struct TerminalSimulation {
    pub ticks: usize,
    pub inputs: Vec<TerminalSample>,
    pub outputs: Vec<TerminalSample>,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub(crate) struct TerminalSample {
    pub position: dustroute_physical::Pos,
    pub powered: bool,
    pub strength: u8,
    pub confidence: TerminalConfidence,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub(crate) struct VirtualAnalysisSummary {
    pub health: DiagnosticHealth,
    pub diagnostic_counts: DiagnosticCounts,
    pub source_counts: BTreeMap<String, usize>,
    pub probable_faults: Vec<Finding>,
    pub mixed_ir: MixedSummary,
    pub identity: CircuitIdentity,
    pub scope: String,
    pub functional_behavior_verified: bool,
}
#[derive(Clone, Copy, Debug, Eq, PartialEq, Ord, PartialOrd, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum RepresentationKind {
    LogicGate,
    TimedCell,
    PhysicalRegion,
    Boundary,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub(crate) struct MixedSummary {
    pub physical_components: usize,
    pub recognized_components: usize,
    pub unresolved_components: usize,
    pub nodes: usize,
    pub edges: usize,
    pub representations: BTreeMap<RepresentationKind, usize>,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(tag = "status", rename_all = "snake_case")]
pub(crate) enum ElectricalValidation {
    NotRun {
        reason: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        changed_positions: Option<usize>,
        #[serde(skip_serializing_if = "Option::is_none")]
        placement_authorized: Option<bool>,
    },
    Passed {
        forward_steps: usize,
        undo_steps: usize,
        execution_profile: String,
        scope: String,
        model_initial_queue: String,
        runtime_history_reconstructed: bool,
    },
    FailedOrUnsupported {
        error: String,
        cause: FailureCause,
        placement_authorized: bool,
    },
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(tag = "status", rename_all = "snake_case")]
pub(crate) enum AssemblyValidation {
    PlacementAndDeclaredConnectionsValid { scope: String },
    InvalidOrUnsupported { error: String },
    Unavailable { error: String },
}
