//! Verify the typed reconstruction boundary and the established non-JSON
//! storage encoding, independently of presentation fixtures.
use dustroute_codec::storage::{decode, encode};
use dustroute_library::blueprint::{
    BlueprintCatalog, BlueprintCatalogArchive, BlueprintError, BlueprintRevision,
    ClassificationRevision, TypeRevision,
};
use dustroute_library::builtin_blueprints::builtin_blueprints;
use dustroute_library::builtin_primitives::builtin_primitives;
use serde::Serialize;

// The original private record, including the serializer's type name. Changing
// that name would change canonical bytes even when the JSON fields look equal.
#[derive(Serialize)]
struct Archive {
    schema: String,
    types: Vec<TypeRevision>,
    classifications: Vec<ClassificationRevision>,
    revisions: Vec<BlueprintRevision>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    assemblies: Vec<dustroute_library::assembly::AssemblyRevision>,
}

#[test]
fn typed_archive_preserves_existing_storage_bytes_and_round_trips() {
    for catalog in [builtin_blueprints(), builtin_primitives()] {
        let current = catalog.archive();
        let previous = Archive {
            schema: current.schema.clone(),
            types: current.types.clone(),
            classifications: current.classifications.clone(),
            revisions: current.revisions.clone(),
            assemblies: current.assemblies.clone(),
        };
        let limit = 4 * 1024 * 1024;
        let old_bytes = encode("catalog-test.v1", &previous, limit).unwrap();
        assert_eq!(
            encode("catalog-test.v1", catalog, limit).unwrap(),
            old_bytes
        );
        assert_eq!(
            encode("catalog-test.v1", &current, limit).unwrap(),
            old_bytes
        );
        let restored: BlueprintCatalog = decode("catalog-test.v1", &old_bytes, limit).unwrap();
        assert_eq!(&restored, catalog);
        let records: BlueprintCatalogArchive =
            decode("catalog-test.v1", &old_bytes, limit).unwrap();
        assert_eq!(BlueprintCatalog::from_archive(records).unwrap(), *catalog);
    }
}

#[test]
fn typed_reconstruction_refuses_retired_schema_and_duplicate_records() {
    let mut archive = builtin_blueprints().archive();
    archive.schema = "dustroute.blueprint-catalog.v12".into();
    assert!(BlueprintCatalog::from_archive(archive).is_err());
    let mut archive = builtin_blueprints().archive();
    let duplicate = archive.revisions[0].clone();
    archive.revisions.push(duplicate.clone());
    assert_eq!(
        BlueprintCatalog::from_archive(archive),
        Err(BlueprintError::DuplicateRevision(duplicate.id))
    );
}
