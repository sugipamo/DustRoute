//! Provenance-aware reusable circuit component catalog.
//!
//! External claims and layouts are catalog data, not Minecraft semantics. A
//! component becomes trusted for automatic replacement only after its logical
//! behavior and physical realization have accumulated the required evidence.

mod builtin;
mod catalog;
mod component;
mod verify;

pub use builtin::{
    DUSTROUTE_COMPACT_XOR_ID, DUSTROUTE_COMPILED_XOR_ID, REDSTONE_COMPILER_XOR_ID, builtin_catalog,
};
pub use catalog::{Catalog, CatalogError, ComponentQuery};
pub use component::{
    Compatibility, Component, ComponentId, ComponentKind, Evidence, EvidenceKind, LogicalSpec,
    PhysicalMetrics, Port, PortDirection, Provenance, VerificationLevel,
};
pub use verify::{LogicalVerification, VerificationError, verify_logical_spec};
