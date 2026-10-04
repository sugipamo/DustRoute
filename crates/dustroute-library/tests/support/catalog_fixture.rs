//! Independent presentation fixtures for schema and tampering tests. Runtime
//! catalog APIs accept typed records and never select a JSON codec.
#![allow(dead_code)] // Individual test targets use different fixture helpers.
use dustroute_library::blueprint::{BlueprintCatalog, BlueprintCatalogArchive, BlueprintError};

pub trait FixtureJson {
    fn fixture_json(&self) -> Result<String, serde_json::Error>;
}
impl FixtureJson for BlueprintCatalog {
    fn fixture_json(&self) -> Result<String, serde_json::Error> {
        serde_json::to_string_pretty(self)
    }
}

pub fn catalog(source: &str) -> Result<BlueprintCatalog, BlueprintError> {
    let archive: BlueprintCatalogArchive =
        serde_json::from_str(source).map_err(|error| BlueprintError::Invalid(error.to_string()))?;
    BlueprintCatalog::from_archive(archive)
}
