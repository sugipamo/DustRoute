//! Minecraft-specific world state and block behavior foundations.
//!
//! This crate deliberately has no dependency on DustRoute physical or logical
//! IRs. Version-sensitive Minecraft behavior belongs here.

pub mod blocks;
pub mod time;
mod world;

pub use blocks::{BlockBehaviorProfile, UpdateModel, behavior_profile};
pub use world::{
    Block, BlockCapabilities, BlockKind, BlockProperties, BlockRedstoneTraits, CapabilityLevel,
    Facing, ObservationClassification, OccupiedShape, Pos, SupportError, WireConnection, World,
};
