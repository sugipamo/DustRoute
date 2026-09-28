#[path = "support/flying_machine_blueprint.rs"]
mod fixture;

use dustroute_translate::blueprint_update::{BlueprintUpdates, UpdateStatus};
use dustroute_translate::promotion::CheckStatus;

#[test]
fn a_single_flight_is_freshly_verified_and_adopted_after_restart() {
    let f = fixture::fixture();
    let original = f.catalog.clone();
    let mut updates = BlueprintUpdates::new(f.catalog);
    updates.create(f.request.clone()).unwrap();
    let report = updates.validate(&f.request.id).unwrap();
    assert_eq!(report.status(), CheckStatus::Passed, "{report:?}");
    let mut restored = BlueprintUpdates::from_json(&updates.to_json().unwrap()).unwrap();
    restored.adopt(&f.request.id).unwrap();
    for revision in original.revisions() {
        assert_eq!(restored.catalog().revision(&revision.id), Some(revision));
    }
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
fn a_short_course_cannot_reuse_a_forged_successful_arrival_report() {
    let mut f = fixture::fixture();
    // Move the stopper into the route without changing the promised endpoint.
    let stopper = f
        .request
        .candidate_state
        .assembly
        .blocks
        .iter_mut()
        .find(|b| b.position.x == 12)
        .unwrap();
    stopper.position.x = 8;
    let mut updates = BlueprintUpdates::new(f.catalog);
    updates.create(f.request.clone()).unwrap();
    let report = updates.validate(&f.request.id).unwrap();
    assert_eq!(
        report.behavior_status(),
        Some(CheckStatus::Failed),
        "{report:?}"
    );
    let mut saved: serde_json::Value = serde_json::from_str(&updates.to_json().unwrap()).unwrap();
    let report = &mut saved["proposals"][0]["events"][0]["report"];
    for key in ["occurrences", "arrangement", "behavior"] {
        report[key] = serde_json::json!([]);
    }
    let mut restored = BlueprintUpdates::from_json(&saved.to_string()).unwrap();
    assert!(restored.adopt(&f.request.id).is_err());
}
