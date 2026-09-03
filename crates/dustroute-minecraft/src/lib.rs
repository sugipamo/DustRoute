//! Minecraft-specific world state and block behavior foundations.
//!
//! This crate deliberately has no dependency on DustRoute physical or logical
//! IRs. Version-sensitive Minecraft behavior belongs here.

pub mod blocks;
mod delta;
pub mod time;
mod world;

pub use delta::{
    BlockChange, BlockMove, ChangeReason, DeltaCause, Region, RegionSet, Shape, ShapeId, StateId,
    WorldDelta, WorldDeltaError,
};

pub use blocks::{
    BlockBehaviorProfile, DEFAULT_PISTON_MOTION_PROFILE, PISTON_PUSH_LIMIT, PistonAction,
    PistonBlockMove, PistonError, PistonMotionProfile, PistonMotionProfileError, PistonPlan,
    PistonPlanningContext, UpdateModel, behavior_profile, observed_name_is_immovable, piston_state,
    piston_variant, plan_piston, plan_piston_in_region,
};
pub use world::{
    Block, BlockCapabilities, BlockKind, BlockProperties, BlockRedstoneTraits, CapabilityLevel,
    Facing, ObservationClassification, OccupiedShape, PistonState, PistonVariant, Pos,
    SupportError, WireConnection, World, observed_name_requires_live_observation,
};
