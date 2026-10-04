//! The final MCP codec. JSON values are created here for wire encoding only;
//! no application authorization, plan, observation or progress is read from JSON.
use crate::api::{ErrorResponse, McpErrorCode};
use crate::failure::{CauseKind, FailureCause, FailurePhase};
use rmcp::model::{CallToolResult, ContentBlock};
use serde::Serialize;
use serde_json::{Value, json};

impl From<serde_json::Error> for FailureCause {
    fn from(error: serde_json::Error) -> Self {
        Self::new(CauseKind::Serialization, error.to_string())
    }
}
fn cause_value(cause: &FailureCause) -> Value {
    serde_json::to_value(cause.as_response()).expect("failure facts are serializable")
}
pub(super) fn typed_reply(value: impl Serialize) -> CallToolResult {
    match serde_json::to_value(value) {
        Ok(value) => json_reply(value),
        Err(error) => json_reply(cause_value(&FailureCause::from(error))),
    }
}

pub(super) fn json_reply(value: Value) -> CallToolResult {
    let response = encode_response(value);
    let content = vec![ContentBlock::text(response.text)];
    if response.failed {
        CallToolResult::error(content)
    } else {
        CallToolResult::success(content)
    }
}

/// Outcome stays separate from its final MCP text representation.
struct EncodedResponse {
    text: String,
    failed: bool,
}

fn encode_response(mut value: Value) -> EncodedResponse {
    let measurement = crate::performance::span(crate::performance::Phase::ResponseEncode);
    if value.get("ok") == Some(&Value::Bool(false))
        && let Some(object) = value.as_object_mut()
    {
        object
            .entry("schema_version")
            .or_insert_with(|| Value::String(crate::api::ERROR_SCHEMA_V1.to_owned()));
        object
            .entry("error_code")
            .or_insert_with(|| Value::String("internal".to_owned()));
        object.entry("retryable").or_insert(Value::Bool(false));
        // Legacy String-only boundaries have no trustworthy execution facts.
        // Preserve that absence rather than manufacturing zero writes or a phase.
        if !object.contains_key("failure") {
            let kind = match object.get("error_code").and_then(Value::as_str) {
                Some("invalid_argument") => CauseKind::InvalidInput,
                Some("invalid_state") => CauseKind::InvalidState,
                Some("not_found") => CauseKind::NotFound,
                Some("permission_denied") => CauseKind::PermissionDenied,
                Some("observation_unavailable") => CauseKind::ObservationUnavailable,
                Some("verification_failed") => CauseKind::VerificationMismatch,
                Some("serialization_failed") => CauseKind::Serialization,
                Some("unsupported") => CauseKind::Unsupported,
                Some("resource_limit") => CauseKind::ResourceLimit,
                Some("persistence_failed") => CauseKind::Persistence,
                _ => CauseKind::Unknown,
            };
            let message = object
                .get("error")
                .and_then(Value::as_str)
                .unwrap_or("operation failed without a diagnostic");
            object.insert("failure".into(),json!({"primary":FailureCause::new(kind,message).at(FailurePhase::Unknown),"secondary":[],"progress":null}));
            object.insert("recovery".into(),json!({"reobserve_required":null,"replan_required":null,"inspect_saved_record":null,"same_operation_replay_allowed":false}));
        }
    }
    let failed = value.get("ok") == Some(&Value::Bool(false));
    let (text, failed) = match serde_json::to_string_pretty(&value) {
        Ok(text) => (text, failed),
        Err(error) => (cause_value(&FailureCause::from(error)).to_string(), true),
    };
    drop(measurement.bytes(text.len()));
    EncodedResponse { text, failed }
}

// Independent compatibility adapter for the former final MCP promotion. It is
// used only by boundary regressions; production accepts native cause responses.
#[cfg(test)]
pub(super) fn legacy_cause_reply(cause: FailureCause) -> CallToolResult {
    let mut value = json!({"ok":false,"error":cause});
    if let Some(object) = value.as_object_mut() {
        // Only a typed cause is promoted; arbitrary strings remain unknown.
        if !object.contains_key("failure")
            && let Some(cause) = object
                .get("error")
                .cloned()
                .and_then(|value| serde_json::from_value::<FailureCause>(value).ok())
        {
            object.extend(
                cause_value(&cause)
                    .as_object()
                    .expect("diagnostic object")
                    .clone(),
            );
        }
    }
    json_reply(value)
}

pub(super) fn error_reply(
    code: McpErrorCode,
    message: impl Into<String>,
    retryable: bool,
) -> CallToolResult {
    typed_reply(ErrorResponse::new(code, message, retryable))
}
