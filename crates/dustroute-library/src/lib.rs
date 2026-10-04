//! Provenance-aware reusable circuit component catalog.
//!
//! External claims and layouts are catalog data, not Minecraft semantics. A
//! component becomes trusted for automatic replacement only after its logical
//! behavior and physical realization have accumulated the required evidence.

pub mod assembly;
pub mod behavior_context;
pub mod behavior_type;
pub mod blueprint;
pub mod building;
mod builtin;
pub mod builtin_blueprints;
mod builtin_definitions;
pub mod builtin_laws;
pub mod builtin_primitives;
mod catalog;
mod component;
pub mod execution_context;
pub mod flying_machine;
mod interfaces;
pub mod location_observation;
pub mod runtime_behavior;
mod verify;
pub mod world_edit;

pub use builtin::{
    DUSTROUTE_COMPACT_XOR_ID, DUSTROUTE_COMPILED_XOR_ID, REDSTONE_COMPILER_XOR_ID, builtin_catalog,
};
pub use catalog::{Catalog, CatalogError, ComponentQuery};
pub use component::{
    Compatibility, Component, ComponentId, ComponentKind, Evidence, EvidenceKind, LogicalSpec,
    PhysicalMetrics, Port, PortDirection, Provenance, VerificationLevel,
};
pub use verify::{LogicalVerification, VerificationError, verify_logical_spec};
