//! Presentation-fixture decoding belongs to tests. Archive reconstruction
//! remains typed, with validation claims treated as historical diagnostics.
#![allow(dead_code)] // Individual test targets use different fixture helpers.
#[path = "../../../dustroute-library/tests/support/catalog_fixture.rs"]
mod catalog_fixture;
pub use catalog_fixture::FixtureJson;
use dustroute_translate::blueprint_update::{BlueprintUpdateError, BlueprintUpdates};

pub fn catalog(
    source: &str,
) -> Result<
    dustroute_library::blueprint::BlueprintCatalog,
    dustroute_library::blueprint::BlueprintError,
> {
    catalog_fixture::catalog(source)
}

impl FixtureJson for BlueprintUpdates {
    fn fixture_json(&self) -> Result<String, serde_json::Error> {
        serde_json::to_string_pretty(&self.archive())
    }
}

pub fn updates(source: &str) -> Result<BlueprintUpdates, BlueprintUpdateError> {
    let archive = serde_json::from_str(source)
        .map_err(|error| BlueprintUpdateError::Invalid(error.to_string()))?;
    BlueprintUpdates::from_archive(archive)
}
