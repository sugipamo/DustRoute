//! MCP-facing orchestration for a visible Minecraft bot.

pub mod bridge;
pub mod discovery;
pub mod operations;
pub mod policy;
pub mod selection;
pub mod service;

pub use bridge::{BotBridge, BotBridgeError, BotStatus, PlayerObservation, VisiblePlayer};
pub use discovery::{CircuitDiscovery, DiscoveryError, discover_connected_region};
pub use dustroute_app::{
    BlockChange, PlacementPlan, PlanningError, UndoPlan, plan_world_overlay, relocate_world,
};
pub use operations::{OperationKind, OperationRecord, OperationRegistry, OperationStatus};
pub use policy::{McpPolicy, PolicyError};
pub use selection::{RegionSelection, SelectionError, SelectionSession};
pub use service::DustRouteMcp;
