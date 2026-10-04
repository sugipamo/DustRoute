//! MCP-only encoding adapters. Computation and history use native report types.
pub(in super::super) use crate::recorded_analysis::reports::{
    focused_component, focused_hierarchy,
};
use serde_json::{Value, json};

pub(in super::super) fn focused_explanation_json(
    analysis: &dustroute_translate::analysis::PhysicalAnalysis,
    target: dustroute_physical::Pos,
    analysis_complete: bool,
) -> Value {
    serde_json::to_value(dustroute_translate::analysis::explain_focused_component(analysis,target,analysis_complete))
        .unwrap_or_else(|error|json!({"position":target,"status":"unavailable","reason":format!("focused explanation serialization failed: {error}")}))
}
pub(in super::super) fn hierarchical_result_json(
    bounds: dustroute_translate::world_reverse::RegionBounds,
    hierarchy: &dustroute_ir::HierarchicalIr,
    focused: Option<crate::recorded_analysis::reports::FocusedHierarchy>,
    expansion: &super::super::circuit_capture::ExpansionEvidence,
    focus: Option<dustroute_physical::Pos>,
) -> Value {
    serde_json::to_value(crate::recorded_analysis::reports::hierarchical_report(
        bounds,
        hierarchy,
        focused,
        expansion.recorded(),
        focus,
    ))
    .expect("serializable hierarchical report")
}
pub(in super::super) fn circuit_identity_json(
    hierarchy: &dustroute_ir::HierarchicalIr,
    logical_role: Option<&dustroute_translate::analysis::LogicalRole>,
    analysis_complete: bool,
    repair_count: usize,
) -> Value {
    serde_json::to_value(crate::recorded_analysis::circuit_identity(
        hierarchy,
        logical_role,
        analysis_complete,
        repair_count,
    ))
    .expect("serializable circuit identity")
}
