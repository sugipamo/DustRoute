//! Versioned archive encoding and validated reconstruction. Saved claims are not proof.
use super::validation::invalid;
use super::{
    BlueprintCatalog, BlueprintError, BlueprintRevision, ClassificationRevision, TypeRevision,
};
pub const CATALOG_SCHEMA: &str = "dustroute.blueprint-catalog.v13";
use serde::{Deserialize, Serialize};

#[derive(Deserialize, Serialize)]
struct Archive {
    schema: String,
    types: Vec<TypeRevision>,
    classifications: Vec<ClassificationRevision>,
    revisions: Vec<BlueprintRevision>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    assemblies: Vec<crate::assembly::AssemblyRevision>,
}

impl BlueprintCatalog {
    pub fn to_json(&self) -> Result<String, BlueprintError> {
        serde_json::to_string_pretty(self)
            .map_err(|error| BlueprintError::Invalid(error.to_string()))
    }

    /// Archives are self-contained and may list children after parents. No
    /// filesystem paths, function names or network references are executed.
    pub fn from_json(input: &str) -> Result<Self, BlueprintError> {
        let archive = serde_json::from_str(input)
            .map_err(|error| BlueprintError::Invalid(error.to_string()))?;
        Self::from_archive(archive)
    }
    fn from_archive(archive: Archive) -> Result<Self, BlueprintError> {
        if archive.schema != CATALOG_SCHEMA {
            return invalid(
                "retired or unsupported blueprint archive schema; recreate the catalog with the current version (v13)",
            );
        }
        let mut catalog = Self::default();
        for definition in archive.types {
            catalog.insert_type(definition)?;
        }
        for definition in archive.classifications {
            catalog.insert_classification(definition)?;
        }
        for revision in archive.revisions {
            let id = revision.id.clone();
            if catalog.revisions.insert(id.clone(), revision).is_some() {
                return Err(BlueprintError::DuplicateRevision(id));
            }
        }
        for id in catalog.revisions.keys() {
            catalog.validate_revision(id)?;
        }
        for revision in archive.assemblies {
            let id = revision.id.clone();
            if catalog.assemblies.insert(id.clone(), revision).is_some() {
                return Err(BlueprintError::DuplicateAssembly(id));
            }
        }
        for id in catalog.assemblies.keys() {
            catalog.validate_assembly_revision(id)?;
        }
        Ok(catalog)
    }
}

impl Serialize for BlueprintCatalog {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        Archive {
            schema: CATALOG_SCHEMA.into(),
            types: self.types.values().cloned().collect(),
            classifications: self.classifications.values().cloned().collect(),
            revisions: self.revisions.values().cloned().collect(),
            assemblies: self.assemblies.values().cloned().collect(),
        }
        .serialize(serializer)
    }
}
impl<'de> Deserialize<'de> for BlueprintCatalog {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        Self::from_archive(Archive::deserialize(deserializer)?).map_err(serde::de::Error::custom)
    }
}
