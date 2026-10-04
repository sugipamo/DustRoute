//! MCP output projections and codecs for native report records. These functions
//! cannot read a bridge, mutate an operation, or load a persisted catalog.
//! Synchronous conversion and operation history compose native reports directly;
//! output serializers never reread an encoded payload to make a decision.
pub(super) use crate::recorded_analysis::reports::{focused_component, focused_hierarchy};
pub(super) use conversion::{
    Capture, Conversion, DetailTools, Discovery, FocusedDiagnostic, GazeFlat, GazeHierarchy,
    NextTools, Selected,
};
pub(super) use macro_conversion::{MacroPlanReport, MacroProposals};
use serde_json::{Value, json};
mod conversion;
mod inspection;
mod macro_conversion;
#[cfg(test)]
mod macro_conversion_tests;
mod mixed;
mod revision;
#[cfg(test)]
mod tests;
#[cfg(test)]
mod truth_table;
pub(super) use inspection::{
    CapturedWorldInspection, GazeExpansion, GazeWorldInspection, InspectionBoundary,
    InspectionFailure, raw_world_inspection,
};
pub(super) use mixed::{CircuitIrResponse, IrIdentityMismatch, IrNodeUnavailable, mixed_ir_report};
pub(super) use revision::electrical_modification_validation;
pub(super) use revision::revision_validation;
pub(super) use revision::{RevisionDisplay, revision_display};
#[cfg(test)]
pub(super) use truth_table::reverse_result_json;

pub(super) fn bounds_json(bounds: dustroute_translate::world_reverse::RegionBounds) -> Value {
    json!({ "min": bounds.min, "max": bounds.max })
}

pub(super) use crate::recorded_analysis::reports::MAX_FLAT_ANALYSIS_COMPONENTS;
