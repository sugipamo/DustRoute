//! Durable provenance is typed data, never a reusable placement capability.
use dustroute_library::{
    assembly::AssemblyRevision,
    blueprint::{BlueprintCatalog, BlueprintUpdateId},
    runtime_behavior::RuntimeBehaviorContext,
};
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub(crate) struct SourceIdentity {
    pub record: AssemblyRevision,
    pub adopted_by: BlueprintUpdateId,
    pub context: RuntimeBehaviorContext,
    pub catalog: BlueprintCatalog,
}
impl SourceIdentity {
    /// Unrelated append-only catalog additions are allowed. Existing definitions
    /// must remain exact; source IDs alone do not establish their content.
    pub fn matches(&self, current: &Self) -> bool {
        self.record == current.record
            && self.adopted_by == current.adopted_by
            && self.context == current.context
            && self
                .catalog
                .revisions()
                .all(|r| current.catalog.revision(&r.id) == Some(r))
            && self
                .catalog
                .type_revisions()
                .all(|r| current.catalog.type_revision(&r.id) == Some(r))
            && self
                .catalog
                .classifications()
                .all(|r| current.catalog.classification(&r.id) == Some(r))
            && self
                .catalog
                .assemblies()
                .all(|r| current.catalog.assembly(&r.id) == Some(r))
    }
}
