//! Versioned archive encoding and validated reconstruction. Saved claims are not proof.
use super::validation::invalid;
use super::{
    BlueprintCatalog, BlueprintError, BlueprintRevision, ClassificationRevision, TypeRevision,
};
pub const CATALOG_SCHEMA: &str = "dustroute.blueprint-catalog.v13";
use serde::{Deserialize, Serialize};

/// Historical definitions, without executable validation or placement proof.
/// Codecs are chosen at an application boundary; reconstruction validates the
/// schema and all references independently of the codec.
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename = "Archive")]
pub struct BlueprintCatalogArchive {
    pub schema: String,
    pub types: Vec<TypeRevision>,
    pub classifications: Vec<ClassificationRevision>,
    pub revisions: Vec<BlueprintRevision>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub assemblies: Vec<crate::assembly::AssemblyRevision>,
}

impl BlueprintCatalog {
    /// Archives are self-contained and may list children after parents. No
    /// filesystem paths, function names or network references are executed.
    pub fn from_archive(archive: BlueprintCatalogArchive) -> Result<Self, BlueprintError> {
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

    #[must_use]
    pub fn archive(&self) -> BlueprintCatalogArchive {
        BlueprintCatalogArchive {
            schema: CATALOG_SCHEMA.into(),
            types: self.types.values().cloned().collect(),
            classifications: self.classifications.values().cloned().collect(),
            revisions: self.revisions.values().cloned().collect(),
            assemblies: self.assemblies.values().cloned().collect(),
        }
    }
}

impl Serialize for BlueprintCatalog {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        self.archive().serialize(serializer)
    }
}
impl<'de> Deserialize<'de> for BlueprintCatalog {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        Self::from_archive(BlueprintCatalogArchive::deserialize(deserializer)?)
            .map_err(serde::de::Error::custom)
    }
}
