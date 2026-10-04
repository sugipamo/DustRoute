//! MCP-only encoding of native reverse reports. No calculation or history uses Value.
use serde_json::Value;
pub(in super::super) fn reverse_result_json(
    bounds: dustroute_translate::world_reverse::RegionBounds,
    translated: &dustroute_translate::api::ReverseResult,
) -> Value {
    serde_json::to_value(crate::recorded_analysis::reports::reverse_report(
        bounds, translated,
    ))
    .expect("serializable reverse report")
}
