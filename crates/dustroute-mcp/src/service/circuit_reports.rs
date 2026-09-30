//! Stateless report projections. These functions take explicit model data;
//! none can read a bridge, mutate an operation, or load a persisted catalog.
use serde_json::{Value, json};
mod analysis;
mod inspection;
mod revision;
mod truth_table;
pub(super) use analysis::circuit_identity_json;
pub(super) use analysis::focused_explanation_json;
pub(super) use analysis::focused_hierarchy_role_json;
pub(super) use analysis::focused_role_json;
pub(super) use analysis::hierarchical_result_json;
pub(super) use analysis::mixed_ir_json;
pub(super) use inspection::raw_world_inspection;
pub(super) use revision::electrical_modification_validation;
pub(super) use revision::revision_json;
pub(super) use revision::revision_validation;
pub(super) use truth_table::reverse_result_json;

pub(super) fn bounds_json(bounds: dustroute_translate::world_reverse::RegionBounds) -> Value {
    json!({ "min": bounds.min, "max": bounds.max })
}

pub(super) fn truth_table_status(
    translated: &dustroute_translate::api::ReverseResult,
) -> &'static str {
    if translated.truth_table.is_some() {
        "computed"
    } else if matches!(
        translated.truth_table_error.as_ref(),
        Some(
            dustroute_translate::world_reverse::TruthTableError::BudgetExceeded { .. }
                | dustroute_translate::world_reverse::TruthTableError::RuntimeBudgetExceeded { .. }
                | dustroute_translate::world_reverse::TruthTableError::ElapsedBudgetExceeded { .. }
        )
    ) {
        "budget_exceeded"
    } else if translated.truth_table_error.is_some() {
        "unavailable"
    } else {
        "not_requested"
    }
}

pub(super) const MAX_FLAT_ANALYSIS_COMPONENTS: usize = 512;
