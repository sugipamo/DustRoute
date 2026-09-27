//! Minecraft-specific world state and block behavior foundations.
//!
//! This crate deliberately has no dependency on DustRoute physical or logical
//! IRs. Version-sensitive Minecraft behavior belongs here.

pub mod blocks;
pub mod comparator_law;
mod delta;
pub mod device_callback_law;
pub mod dust_law;
pub mod execution_context;
pub mod lamp_law;
pub mod law;
pub mod observer_law;
pub mod piston_electrical;
pub mod piston_electrical_law;
pub mod piston_law;
pub mod piston_motion_law;
pub mod repeater_law;
mod rotation;
pub mod spatial;
pub use rotation::RotationY;
pub mod time;
mod validation;
mod world;
pub use validation::{
    HistoricalPlacementV1, ValidatedWorld, WorldValidationError, WorldValidationIssue,
    wire_rise_issues,
};

pub use delta::{
    BlockChange, BlockMove, ChangeReason, DeltaCause, Region, RegionSet, Shape, ShapeId, StateId,
    WorldDelta, WorldDeltaError,
};

pub use blocks::{
    BlockBehaviorProfile, DEFAULT_PISTON_MOTION_PROFILE, PISTON_PUSH_LIMIT, PistonAction,
    PistonBlockMove, PistonError, PistonMotionProfile, PistonMotionProfileError, PistonPlan,
    PistonPlanningContext, RedstonePropagationError, UpdateModel, behavior_profile,
    observed_name_is_immovable, piston_input_connected, piston_state, piston_variant, plan_piston,
    plan_piston_in_region,
};

pub(crate) use blocks::{
    direct_piston_neighbors, external_world_delta, piston_input_powered_in_region,
    redstone_input_delta, redstone_lamp_delta, redstone_position_known,
    redstone_repeater_delay_game_ticks, redstone_repeater_delta, redstone_repeater_input_powered,
    redstone_repeater_output_position, redstone_repeater_powered, redstone_update_positions,
    redstone_wire_delta, redstone_wire_update_positions,
};
pub use world::{
    Block, BlockCapabilities, BlockKind, BlockProperties, BlockRedstoneTraits, CapabilityLevel,
    Facing, ObservationClassification, OccupiedShape, PistonBlockEntityState, PistonHeadState,
    PistonState, PistonVariant, Pos, SupportError, WireConnection, World,
    observed_name_requires_live_observation,
};
