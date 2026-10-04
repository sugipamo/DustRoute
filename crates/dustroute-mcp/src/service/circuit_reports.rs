//! MCP output projections and codecs for native report records. These functions
//! cannot read a bridge, mutate an operation, or load a persisted catalog.
//! JSON adapters are only for synchronous public response composition; analysis
//! computation and operation history use `recorded_analysis::reports` directly.
use serde_json::{Value, json};
mod analysis;
mod inspection;
mod mixed;
mod revision;
#[cfg(test)]
mod tests;
mod truth_table;
pub(super) use analysis::circuit_identity_json;
pub(super) use analysis::focused_component;
pub(super) use analysis::focused_explanation_json;
pub(super) use analysis::focused_hierarchy;
pub(super) use analysis::hierarchical_result_json;
pub(super) use inspection::{
    CapturedWorldInspection, GazeExpansion, GazeWorldInspection, InspectionBoundary,
    InspectionFailure, raw_world_inspection,
};
pub(super) use mixed::{CircuitIrResponse, IrIdentityMismatch, IrNodeUnavailable, mixed_ir_report};
pub(super) use revision::electrical_modification_validation;
pub(super) use revision::revision_validation;
pub(super) use revision::{RevisionDisplay, revision_display};
pub(super) use truth_table::reverse_result_json;

pub(super) fn bounds_json(bounds: dustroute_translate::world_reverse::RegionBounds) -> Value {
    json!({ "min": bounds.min, "max": bounds.max })
}

pub(super) use crate::recorded_analysis::reports::MAX_FLAT_ANALYSIS_COMPONENTS;
