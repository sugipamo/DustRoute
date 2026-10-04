#[path = "support/catalog_fixture.rs"]
mod catalog_fixture;
use dustroute_library::builtin_laws::builtin_laws;

#[test]
fn typed_law_metadata_and_programs_equal_the_frozen_catalog_records() {
    for frozen in [
        include_str!("fixtures/builtin-laws-v1/dust-blueprint-v1.json"),
        include_str!("fixtures/builtin-laws-v1/torch-law-v1.json"),
    ] {
        let old = catalog_fixture::catalog(frozen).unwrap();
        for revision in old.revisions() {
            assert_eq!(builtin_laws().revision(&revision.id), Some(revision));
        }
    }
}
