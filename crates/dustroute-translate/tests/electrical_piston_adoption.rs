#[path = "support/runtime_blueprint.rs"]
mod fixture;

use dustroute_translate::blueprint_update::{BlueprintUpdates, UpdateStatus};
use dustroute_translate::promotion::CheckStatus;

#[test]
fn mixed_electrical_migration_is_explicit_and_revalidated_after_restart() {
    let f = fixture::electrical_fixture(false);
    let original = f.catalog.clone();
    let mut updates = BlueprintUpdates::new(f.catalog);
    updates.create(f.request.clone()).unwrap();
    assert_eq!(updates.catalog(), &original);
    let report = updates.validate(&f.request.id).unwrap();
    assert_eq!(report.status(), CheckStatus::Passed, "{report:?}");
    let mut restored = BlueprintUpdates::from_json(&updates.to_json().unwrap()).unwrap();
    restored.adopt(&f.request.id).unwrap();
    for revision in original.revisions() {
        assert_eq!(restored.catalog().revision(&revision.id), Some(revision));
    }
    assert_eq!(restored.catalog().assembly(&f.base.id), Some(&f.base));
    let restored = BlueprintUpdates::from_json(&restored.to_json().unwrap()).unwrap();
    assert_eq!(
        restored.proposal(&f.request.id).unwrap().status(),
        UpdateStatus::Adopted
    );
    assert_eq!(
        restored.review(&f.request.id).unwrap().status(),
        CheckStatus::Passed
    );
}

#[test]
fn a_passing_electrical_parent_does_not_erase_a_retained_child_requirement() {
    let f = fixture::electrical_fixture(true);
    let original = f.catalog.clone();
    let mut updates = BlueprintUpdates::new(f.catalog);
    updates.create(f.request.clone()).unwrap();
    let report = updates.validate(&f.request.id).unwrap();
    assert_eq!(
        report.behavior_status(),
        Some(CheckStatus::Passed),
        "{report:?}"
    );
    assert_eq!(report.status(), CheckStatus::Failed);
    let mut saved: serde_json::Value = serde_json::from_str(&updates.to_json().unwrap()).unwrap();
    let recorded = &mut saved["proposals"][0]["events"][0]["report"];
    recorded["occurrences"] = serde_json::json!([]);
    recorded["arrangement"] = serde_json::json!([]);
    recorded["behavior"] = serde_json::json!([]);
    let mut restored = BlueprintUpdates::from_json(&saved.to_string()).unwrap();
    assert!(restored.adopt(&f.request.id).is_err());
    assert_eq!(restored.catalog(), &original);
    assert_eq!(
        restored.proposal(&f.request.id).unwrap().status(),
        UpdateStatus::Open
    );
}
