#[path = "support/runtime_blueprint.rs"]
mod fixture;

use dustroute_library::assembly::BlueprintGrouping;
use dustroute_library::behavior_context::BehaviorReviewContext;
use dustroute_library::blueprint::*;
use dustroute_translate::behavior_type::BehaviorBudget;
use dustroute_translate::blueprint_update::{BlueprintUpdateError, BlueprintUpdates, UpdateStatus};
use dustroute_translate::promotion::{
    CheckStatus, PromotionCandidate, PromotionError, review_assembly_with_context,
};

#[test]
fn native_proposal_relocates_ports_and_adopts_only_after_fresh_reload_review() {
    let f = fixture::fixture(false, true);
    let original = f.catalog.clone();
    let mut updates = BlueprintUpdates::new(f.catalog);
    updates.create(f.request.clone()).unwrap();
    assert_eq!(updates.catalog(), &original);
    let report = updates.validate(&f.request.id).unwrap();
    assert_eq!(report.status(), CheckStatus::Passed, "{report:?}");
    assert_ne!(
        report.placement_validation_profile(),
        dustroute_translate::ValidatedWorld::PROFILE
    );
    assert_eq!(updates.catalog(), &original);
    let saved = updates.to_json().unwrap();
    assert!(saved.contains("dustroute.blueprint-updates.v5"));
    for version in 1..=4 {
        assert!(
            BlueprintUpdates::from_json(&saved.replace(
                "dustroute.blueprint-updates.v5",
                &format!("dustroute.blueprint-updates.v{version}")
            ))
            .is_err()
        );
    }
    let mut restored = BlueprintUpdates::from_json(&saved).unwrap();
    restored.adopt(&f.request.id).unwrap();
    assert_eq!(
        restored.proposal(&f.request.id).unwrap().status(),
        UpdateStatus::Adopted
    );
    assert_eq!(
        restored.catalog().assembly(&f.request.candidate_state.id),
        Some(&f.request.candidate_state)
    );
    assert_eq!(restored.catalog().assembly(&f.base.id), Some(&f.base));
    for source in original.revisions() {
        assert_eq!(restored.catalog().revision(&source.id), Some(source));
    }
    let reloaded = BlueprintUpdates::from_json(&restored.to_json().unwrap()).unwrap();
    assert_eq!(
        reloaded.review(&f.request.id).unwrap().status(),
        CheckStatus::Passed
    );
}

#[test]
fn a_forged_saved_pass_cannot_adopt_a_child_broken_only_during_motion() {
    let f = fixture::fixture(true, false);
    let original = f.catalog.clone();
    let mut updates = BlueprintUpdates::new(f.catalog);
    updates.create(f.request.clone()).unwrap();
    let report = updates.validate(&f.request.id).unwrap();
    assert_eq!(report.status(), CheckStatus::Failed);
    assert_eq!(report.behavior_status(), Some(CheckStatus::Passed));
    let mut saved: serde_json::Value = serde_json::from_str(&updates.to_json().unwrap()).unwrap();
    let recorded = &mut saved["proposals"][0]["events"][0]["report"];
    recorded["occurrences"] = serde_json::json!([]);
    recorded["arrangement"] = serde_json::json!([]);
    recorded["behavior"] = serde_json::json!([]);
    let mut restored = BlueprintUpdates::from_json(&saved.to_string()).unwrap();
    assert!(matches!(
        restored.adopt(&f.request.id),
        Err(BlueprintUpdateError::Validation(_))
    ));
    assert_eq!(restored.catalog(), &original);
    assert_eq!(
        restored.proposal(&f.request.id).unwrap().status(),
        UpdateStatus::Open
    );
    assert!(
        restored
            .review(&f.request.id)
            .unwrap()
            .occurrences
            .values()
            .flat_map(|r| &r.checks)
            .any(|c| c.status == CheckStatus::Failed && c.detail.contains("MovingPiston"))
    );
}

#[test]
fn promotion_pins_runtime_laws_and_keeps_legacy_world_proofs_separate() {
    let f = fixture::fixture(false, false);
    let grouping = BlueprintGrouping {
        id: BlueprintRevisionId::new("runtime-test.group.v1").unwrap(),
        name: "Native grouped body".into(),
        classifications: vec![],
        provenance: f
            .catalog
            .revision(&f.request.base_parent)
            .unwrap()
            .provenance
            .clone(),
    };
    let candidate = PromotionCandidate::prepare(&f.catalog, &f.base.assembly, grouping).unwrap();
    assert!(
        dustroute_translate::assembly::validate_assembly(&f.catalog, &f.base.assembly).is_err()
    );
    let review = candidate
        .validate_in_context(&f.catalog, f.context.clone(), BehaviorBudget::default())
        .unwrap();
    assert_eq!(
        review.report().status(),
        CheckStatus::Passed,
        "{:?}",
        review.report()
    );
    let mut adopted = f.catalog.clone();
    review.adopt(&mut adopted).unwrap();
    assert!(adopted.revision(&candidate.blueprint().id).is_some());
    // A same-ID executable-law edit is a changed dependency, even if it still
    // looks like a complete catalog after deserialization.
    let mut raw: serde_json::Value = serde_json::from_str(&f.catalog.to_json().unwrap()).unwrap();
    // Mutate a selected law's ordinary metadata without changing its valid program.
    fn change(value: &mut serde_json::Value) -> bool {
        match value {
            serde_json::Value::Object(map) => {
                if map.get("id").and_then(|v| v.as_str())
                    == Some("dustroute.law.piston.control.java-1-21-11.v1")
                {
                    map.insert("name".into(), serde_json::json!("Changed selected law"));
                    return true;
                }
                map.values_mut().any(change)
            }
            serde_json::Value::Array(values) => values.iter_mut().any(change),
            _ => false,
        }
    }
    assert!(change(&mut raw));
    let mut changed = BlueprintCatalog::from_json(&raw.to_string()).unwrap();
    assert!(matches!(
        review.adopt(&mut changed),
        Err(PromotionError::ChangedDependency(_))
    ));
}

#[test]
fn contexts_reject_mixed_shapes_and_history_alone_requires_the_new_schema() {
    let f = fixture::fixture(false, false);
    let mut mixed = serde_json::to_value(&f.context).unwrap();
    mixed["dust_law"] = serde_json::json!("dustroute.law.dust.v1");
    assert!(serde_json::from_value::<BehaviorReviewContext>(mixed).is_err());
    let context: BehaviorReviewContext = f.context.clone().into();
    assert_eq!(
        serde_json::to_value(&context).unwrap(),
        serde_json::to_value(&f.context).unwrap()
    );
    let report = review_assembly_with_context(
        &f.catalog,
        &f.base.assembly,
        Some(&context),
        BehaviorBudget::default(),
    )
    .unwrap();
    assert_eq!(report.status(), CheckStatus::Passed);
    let mut updates = BlueprintUpdates::new(f.catalog);
    updates.create(f.request.clone()).unwrap();
    updates.validate(&f.request.id).unwrap();
    let mut saved: serde_json::Value = serde_json::from_str(&updates.to_json().unwrap()).unwrap();
    saved["proposals"][0]["request"]
        .as_object_mut()
        .unwrap()
        .remove("behavior_context");
    saved["schema"] = serde_json::json!("dustroute.blueprint-updates.v4");
    assert!(BlueprintUpdates::from_json(&saved.to_string()).is_err());
}

#[test]
fn an_unfinished_native_execution_cannot_publish_candidate_revisions() {
    let mut f = fixture::fixture(false, false);
    f.context.root_limits.max_microsteps = 1;
    f.request.behavior_context = Some(f.context.into());
    let original = f.catalog.clone();
    let mut updates = BlueprintUpdates::new(f.catalog);
    updates.create(f.request.clone()).unwrap();
    assert_eq!(
        updates.review(&f.request.id).unwrap().status(),
        CheckStatus::Undetermined
    );
    let mut restored = BlueprintUpdates::from_json(&updates.to_json().unwrap()).unwrap();
    assert!(matches!(
        restored.adopt(&f.request.id),
        Err(BlueprintUpdateError::Validation(_))
    ));
    assert_eq!(restored.catalog(), &original);
}
