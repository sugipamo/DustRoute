//! Synchronous analysis output. These projections never restore execution or
//! observation authority, and are encoded only by the public MCP endpoint.
use crate::recorded_analysis::{CircuitIdentity, reports::FocusedComponent};
use dustroute_physical::Pos;
use dustroute_translate::{
    analysis::FocusedExplanation, diagnostic::CircuitDiagnosticReport, world_reverse::RegionBounds,
};
use serde::Serialize;
use uuid::Uuid;

#[derive(Serialize)]
pub(in super::super) struct Conversion<R, E> {
    #[serde(flatten)]
    pub report: R,
    #[serde(flatten)]
    pub captured: Capture,
    #[serde(flatten)]
    pub detail: E,
}
#[derive(Serialize)]
pub(in super::super) struct Capture {
    pub mechanisms: Vec<crate::recorded_analysis::reports::ObservedMechanism>,
    pub circuit_id: Uuid,
}
#[derive(Serialize)]
pub(in super::super) struct GazeHierarchy {
    pub circuit_identity: CircuitIdentity,
    pub diagnostic: CircuitDiagnosticReport,
    pub next_tools: NextTools,
}
#[derive(Serialize)]
pub(in super::super) struct GazeFlat {
    pub circuit_identity: CircuitIdentity,
    pub diagnostic: CircuitDiagnosticReport,
    pub focused_component: Option<FocusedComponent>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub focused_explanation: Option<FocusedExplanation>,
    pub discovery: Discovery,
    pub analysis_complete: bool,
    pub macro_replacement_candidates: Option<super::macro_conversion::MacroProposals>,
    pub next_tools: NextTools,
    pub interpretation_guidance: &'static str,
}
#[derive(Serialize)]
pub(in super::super) struct Discovery {
    pub seed: Option<Pos>,
    pub bounds: RegionBounds,
}
#[derive(Serialize)]
pub(in super::super) struct Selected {}

#[derive(Serialize)]
pub(in super::super) struct NextTools {
    ir_detail: &'static str,
    repair_planning: &'static str,
    transition_planning: &'static str,
}
impl Default for NextTools {
    fn default() -> Self {
        Self {
            ir_detail: "get_circuit_ir",
            repair_planning: "new_repair",
            transition_planning: "new_transition_test",
        }
    }
}

#[derive(Serialize)]
pub(in super::super) struct FocusedDiagnostic {
    pub ok: crate::operations::mutation::Success,
    pub schema_version: &'static str,
    pub analysis_mode: &'static str,
    #[serde(flatten)]
    pub captured: Capture,
    pub content_id: crate::snapshot_content::ContentId,
    pub circuit_expires_in_seconds: u64,
    pub mutation_performed: bool,
    pub target: Option<Pos>,
    pub bounds: RegionBounds,
    pub expansion: crate::recorded_analysis::RecordedExpansion,
    pub diagnostic: CircuitDiagnosticReport,
    pub focused_explanation: Option<FocusedExplanation>,
    pub detail_tools: DetailTools,
}
#[derive(Serialize)]
pub(in super::super) struct DetailTools {
    full_conversion: &'static str,
    raw_observation: &'static str,
    repair_planning: &'static str,
}
impl Default for DetailTools {
    fn default() -> Self {
        Self {
            full_conversion: "convert_from_circuit",
            raw_observation: "get_world",
            repair_planning: "new_repair",
        }
    }
}
