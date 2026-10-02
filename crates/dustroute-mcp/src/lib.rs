//! MCP-facing orchestration for a visible Minecraft bot.

pub mod api;
mod assembly_registry;
mod blueprint_mcp;
pub mod bridge;
pub mod bridge_protocol;
pub mod config;
mod construction_jobs;
pub mod discovery;
mod edit_registry;
pub mod failure;
pub mod observation_evidence;
pub mod operations;
pub mod performance;
mod piston_assembly;
pub mod piston_door;
pub mod policy;
mod revision;
pub mod selection;
pub mod service;
pub mod snapshot_content;
mod source_identity;
mod state;
mod storage;
#[cfg(feature = "voxrig")]
pub mod survival_cleanup;
#[cfg(feature = "voxrig")]
pub mod survival_construction;
#[cfg(feature = "voxrig")]
pub mod survival_execution;
#[cfg(feature = "voxrig")]
pub mod survival_navigation;
pub mod transition;
#[cfg(feature = "voxrig")]
pub mod voxrig_bridge;

pub use api::{
    DIAGNOSTIC_SCHEMA_V1, ERROR_SCHEMA_V1, ErrorResponse, McpErrorCode, PLACEMENT_SCHEMA_V1,
    REPAIR_SCHEMA_V1, ScenarioTransitionResponse, TRANSITION_SCHEMA_V1, TransitionTraceResponse,
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
