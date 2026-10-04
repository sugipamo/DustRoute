//! Immutable blueprint records and overlapping physical interpretations.
//!
//! Records, append-only storage, structural validation, bounded expansion and
//! archive reconstruction have separate responsibilities. Catalog membership
//! remains a claim; neither expansion nor deserialization grants placement proof.
use std::collections::BTreeMap;

mod archive;
pub use archive::{BlueprintCatalogArchive, CATALOG_SCHEMA};
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

/// Authoring data shared by generators and public import. No saved pass is
/// included, and creating a catalog never adopts or authorizes placement.
#[derive(Clone, Debug, Default, serde::Deserialize, serde::Serialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct BlueprintRecords {
    #[serde(default)]
    pub types: Vec<TypeRevision>,
    #[serde(default)]
    pub classifications: Vec<ClassificationRevision>,
    #[serde(default)]
    pub revisions: Vec<BlueprintRevision>,
    #[serde(default)]
    pub assemblies: Vec<crate::assembly::AssemblyRevision>,
}

impl BlueprintRecords {
    pub fn catalog(&self) -> Result<BlueprintCatalog, BlueprintError> {
        let mut catalog = BlueprintCatalog::default();
        for definition in &self.types {
            catalog.insert_type(definition.clone())?;
        }
        for definition in &self.classifications {
            catalog.insert_classification(definition.clone())?;
        }
        catalog.insert_revisions(self.revisions.clone())?;
        let mut ids = std::collections::BTreeSet::new();
        for assembly in &self.assemblies {
            if !ids.insert(&assembly.id) {
                return Err(BlueprintError::DuplicateAssembly(assembly.id.clone()));
            }
        }
        let mut pending = self.assemblies.iter().collect::<Vec<_>>();
        while !pending.is_empty() {
            let previous = pending.len();
            let mut remaining = Vec::new();
            for assembly in pending {
                if assembly
                    .parents
                    .iter()
                    .all(|id| catalog.assembly(id).is_some())
                {
                    catalog.insert_assembly(assembly.clone())?;
                } else {
                    remaining.push(assembly);
                }
            }
            if remaining.len() == previous {
                let parent = remaining[0]
                    .parents
                    .iter()
                    .find(|id| catalog.assembly(id).is_none())
                    .expect("unresolved parent");
                return Err(BlueprintError::UnknownAssembly(parent.clone()));
            }
            pending = remaining;
        }
        Ok(catalog)
    }
}
