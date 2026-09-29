//! Immutable blueprint records and overlapping physical interpretations.
//!
//! Records, append-only storage, structural validation, bounded expansion and
//! archive reconstruction have separate responsibilities. Catalog membership
//! remains a claim; neither expansion nor deserialization grants placement proof.
use std::collections::BTreeMap;

mod archive;
pub use archive::CATALOG_SCHEMA;
mod catalog;
mod expansion;
mod records;
mod validation;

pub(crate) use expansion::{rotated_block, transformed};
pub use records::{
    AssemblyRevisionId, BehaviorBinding, BlueprintConnection, BlueprintError, BlueprintInclusion,
    BlueprintLayout, BlueprintOccurrence, BlueprintPort, BlueprintPortBinding, BlueprintPortKind,
    BlueprintPortRef, BlueprintRevision, BlueprintRevisionId, BlueprintUpdateId,
    ClassificationRevision, ClassificationRevisionId, ExpandedBlueprint, ExpansionLimits,
    InstanceId, InstancePath, PositionedBlock, StaticTypeBinding, TypeContract, TypeRevision,
    TypeRevisionId,
};
pub(crate) use validation::{invalid, unique, validate_blocks};

/// Stored definitions can only be read or appended; IDs can never be rebound.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct BlueprintCatalog {
    types: BTreeMap<TypeRevisionId, TypeRevision>,
    classifications: BTreeMap<ClassificationRevisionId, ClassificationRevision>,
    revisions: BTreeMap<BlueprintRevisionId, BlueprintRevision>,
    assemblies: BTreeMap<AssemblyRevisionId, crate::assembly::AssemblyRevision>,
}
