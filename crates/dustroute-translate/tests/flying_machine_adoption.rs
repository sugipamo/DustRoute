#[path = "support/archive_fixture.rs"]
mod archive_fixture;
use archive_fixture::FixtureJson;
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
    let mut restored = archive_fixture::updates(&updates.fixture_json().unwrap()).unwrap();
    restored.adopt(&f.request.id).unwrap();
    for revision in original.revisions() {
        assert_eq!(restored.catalog().revision(&revision.id), Some(revision));
    }
    let restored = archive_fixture::updates(&restored.fixture_json().unwrap()).unwrap();
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
    let mut saved: serde_json::Value =
        serde_json::from_str(&updates.fixture_json().unwrap()).unwrap();
    let report = &mut saved["proposals"][0]["events"][0]["report"];
    for key in ["occurrences", "arrangement", "behavior"] {
        report[key] = serde_json::json!([]);
    }
    let mut restored = archive_fixture::updates(&saved.to_string()).unwrap();
    assert!(restored.adopt(&f.request.id).is_err());
}

#[test]
fn observed_engine_cane_obligations_are_freshly_adopted_without_a_generator_preset() {
    let f = fixture::observed_cane_fixture();
    let original = f.catalog.clone();
    let mut updates = BlueprintUpdates::new(f.catalog);
    updates.create(f.request.clone()).unwrap();
    let report = updates.validate(&f.request.id).unwrap();
    assert_eq!(report.status(), CheckStatus::Passed, "{report:?}");
    let mut restored = archive_fixture::updates(&updates.fixture_json().unwrap()).unwrap();
    restored.adopt(&f.request.id).unwrap();
    assert_eq!(
        restored.proposal(&f.request.id).unwrap().status(),
        UpdateStatus::Adopted
    );
    for revision in original.revisions() {
        assert_eq!(restored.catalog().revision(&revision.id), Some(revision));
    }
}

#[test]
fn observed_engine_cannot_adopt_a_short_course_or_a_changed_promised_root() {
    for defect in 0..3 {
        let mut f = fixture::observed_cane_fixture();
        let blocks = &mut f.request.candidate_state.assembly.blocks;
        if defect == 1 {
            blocks
                .iter_mut()
                .find(|b| b.block.observed_name.as_deref() == Some("minecraft:obsidian"))
                .unwrap()
                .position
                .x = 4;
        } else if defect == 0 {
            blocks.retain(|b| b.position != dustroute_translate::world::Pos::new(2, -1, 3));
        } else {
            blocks
                .iter_mut()
                .find(|b| b.position == dustroute_translate::world::Pos::new(2, -1, 3))
                .unwrap()
                .block
                .observed_properties
                .insert("age".into(), "8".into());
        }
        let mut updates = BlueprintUpdates::new(f.catalog);
        updates.create(f.request.clone()).unwrap();
        let report = updates.validate(&f.request.id).unwrap();
        assert_ne!(report.status(), CheckStatus::Passed, "{report:?}");
        let mut restored = archive_fixture::updates(&updates.fixture_json().unwrap()).unwrap();
        assert!(restored.adopt(&f.request.id).is_err());
    }
}
