//! Stable JSON-facing MCP response contracts.

use dustroute_physical::Pos;
use dustroute_translate::{ScenarioEvent, ScenarioTrace};
use serde::Serialize;

pub const ERROR_SCHEMA_V1: &str = "dustroute.error.v1";
pub const DIAGNOSTIC_SCHEMA_V1: &str = "dustroute.diagnostic.v1";
pub const PLACEMENT_SCHEMA_V1: &str = "dustroute.placement.v1";
pub const REPAIR_SCHEMA_V1: &str = "dustroute.repair.v1";
pub const REPAIR_CONTEXT_SCHEMA_V1: &str = "dustroute.repair-context.v1";
pub const TRANSITION_SCHEMA_V1: &str = "dustroute.transition.v1";

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum McpErrorCode {
    InvalidArgument,
    InvalidState,
    NotFound,
    PermissionDenied,
    ObservationUnavailable,
    BridgeUnavailable,
    SerializationFailed,
    VerificationFailed,
    Internal,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct ErrorResponse {
    pub ok: bool,
    pub schema_version: &'static str,
    pub error: String,
    pub error_code: McpErrorCode,
    pub retryable: bool,
}

impl ErrorResponse {
    #[must_use]
    pub fn new(code: McpErrorCode, message: impl Into<String>, retryable: bool) -> Self {
        Self {
            ok: false,
            schema_version: ERROR_SCHEMA_V1,
            error: message.into(),
            error_code: code,
            retryable,
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
pub struct PositionStrength {
    pub position: Pos,
    pub strength: u8,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
pub struct PositionPowered {
    pub position: Pos,
    pub powered: bool,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct TransitionTraceResponse {
    pub duration_redstone_ticks: u64,
    pub events: Vec<ScenarioEvent>,
    pub final_strengths: Vec<PositionStrength>,
    pub final_powered: Vec<PositionPowered>,
}

impl From<&ScenarioTrace> for TransitionTraceResponse {
    fn from(trace: &ScenarioTrace) -> Self {
        Self {
            duration_redstone_ticks: trace.duration_redstone_ticks,
            events: trace.events.clone(),
            final_strengths: trace
                .final_strengths
                .iter()
                .map(|(position, strength)| PositionStrength {
                    position: *position,
                    strength: *strength,
                })
                .collect(),
            final_powered: trace
                .final_powered
                .iter()
                .map(|(position, powered)| PositionPowered {
                    position: *position,
                    powered: *powered,
                })
                .collect(),
        }
    }
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use serde_json::json;

    use super::*;

    #[test]
    fn error_contract_keeps_the_legacy_message_and_adds_machine_fields() {
        let value = serde_json::to_value(ErrorResponse::new(
            McpErrorCode::NotFound,
            "unknown operation ID",
            false,
        ))
        .unwrap();
        assert_eq!(value["ok"], false);
        assert_eq!(value["schema_version"], ERROR_SCHEMA_V1);
        assert_eq!(value["error"], "unknown operation ID");
        assert_eq!(value["error_code"], "not_found");
        assert_eq!(value["retryable"], false);
    }

    #[test]
    fn transition_trace_contract_serializes_non_empty_coordinate_state() {
        let position = Pos::new(1, 64, -2);
        let trace = ScenarioTrace {
            duration_redstone_ticks: 2,
            events: Vec::new(),
            final_strengths: BTreeMap::from([(position, 15)]),
            final_powered: BTreeMap::from([(position, true)]),
        };
        let value = serde_json::to_value(TransitionTraceResponse::from(&trace)).unwrap();
        assert_eq!(
            value["final_strengths"][0],
            json!({
                "position": position,
                "strength": 15
            })
        );
        assert_eq!(value["final_powered"][0]["powered"], true);
    }
}
