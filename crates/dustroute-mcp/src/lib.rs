//! MCP-facing orchestration for a visible Minecraft bot.

pub mod api;
pub mod bridge;
pub mod config;
pub mod discovery;
pub mod operations;
pub mod policy;
pub mod selection;
pub mod service;
mod state;
pub mod transition;

pub use api::{
    DIAGNOSTIC_SCHEMA_V1, ERROR_SCHEMA_V1, ErrorResponse, McpErrorCode, PLACEMENT_SCHEMA_V1,
    REPAIR_SCHEMA_V1, TRANSITION_SCHEMA_V1, TransitionTraceResponse,
};
pub use bridge::{
    BlockUpdateEvent, BotBridge, BotBridgeError, BotBridgeMetrics, BotStatus, LeverActivation,
    ObservedBlock, ObservedBlockState, PlayerObservation, UpdateRecording, UpdateRecordingStarted,
    VisiblePlayer,
};
pub use config::{McpConfig, McpConfigError, McpTransport};
pub use discovery::{CircuitDiscovery, DiscoveryError, discover_connected_region};
pub use dustroute_app::{
    BlockChange, PlacementPlan, PlanningError, UndoPlan, plan_world_overlay, relocate_world,
};
pub use operations::{OperationKind, OperationRecord, OperationRegistry, OperationStatus};
pub use policy::{McpPolicy, PolicyError};
pub use selection::{RegionSelection, SelectionError, SelectionSession};
pub use service::{DustRouteMcp, ToolProfile};
pub use transition::{
    TransitionSafety, TransitionSafetyAssessment, TransitionSafetyReason, assess_transition_safety,
    behavior_trace_from_recording, scenario_trace_from_recording,
    scenario_trace_from_recording_with_initial,
};
