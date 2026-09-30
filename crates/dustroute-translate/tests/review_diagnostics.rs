#[path = "support/runtime_blueprint.rs"]
mod fixture;
use dustroute_library::blueprint::*;
use dustroute_minecraft::{BlockKind, Pos};
use dustroute_translate::behavior_type::BehaviorBudget;
use dustroute_translate::blueprint_update::{BlueprintUpdates, RecordedReview};
use dustroute_translate::building::generate_building;
use dustroute_translate::promotion::{CheckKind, CheckStatus, review_assembly_with_context};
use dustroute_translate::review_diagnostics::{CheckExpectation, ReviewObservation};
use serde_json::json;

#[test]
fn a_child_failure_during_motion_identifies_requirement_position_time_and_input() {
    let f = fixture::fixture(true, false);
    let mut updates = BlueprintUpdates::new(f.catalog);
    updates.create(f.request.clone()).unwrap();
    let report = updates.review(&f.request.id).unwrap();
    assert_eq!(report.status(), CheckStatus::Failed);
    let diagnostics = report.diagnostics(64);
    assert!(!diagnostics.live_world_verified);
    let finding = diagnostics
        .findings
        .iter()
        .find(|f| {
            f.status == CheckStatus::Failed
                && f.kind == CheckKind::StaticType
                && f.evidence
                    .as_ref()
                    .is_some_and(|e| e.position == Some(Pos::new(0, 1, 0)))
        })
        .unwrap();
    assert_eq!(
        finding.instance.as_ref().unwrap()[0],
        f.base.assembly.instances[0].instance
    );
    assert!(finding.revision.is_some());
    let evidence = finding.evidence.as_ref().unwrap();
    assert!(evidence.type_revision.is_some());
    assert!(evidence.port.is_some());
    assert!(matches!(
        &evidence.expected,
        Some(CheckExpectation::BlockKind {
            block_kind: BlockKind::Piston
        })
    ));
    assert!(
        evidence
            .actual
            .as_ref()
            .is_some_and(|b| b.kind == BlockKind::MovingPiston)
    );
    assert!(matches!(
        evidence.observation,
        Some(ReviewObservation::CommittedRuntimeState { .. })
    ));
    assert!(
        evidence
            .inputs
            .iter()
            .any(|i| i.position == Pos::new(-1, 1, 0) && i.powered.is_some())
    );
    let saved = serde_json::to_value(RecordedReview::from(&report)).unwrap();
    let restored: RecordedReview = serde_json::from_value(saved.clone()).unwrap();
    assert_eq!(serde_json::to_value(restored).unwrap(), saved);
    let mut archived = BlueprintUpdates::from_json(&updates.to_json().unwrap()).unwrap();
    assert!(
        archived.adopt(&f.request.id).is_err(),
        "saved evidence cannot authorize adoption"
    );
}

#[test]
fn native_behavior_counterexamples_are_not_hidden_in_detail_strings() {
    let mut f = fixture::fixture(false, false);
    let mut records = BlueprintRecords {
        types: f.catalog.type_revisions().cloned().collect(),
        revisions: f.catalog.revisions().cloned().collect(),
        assemblies: f.catalog.assemblies().cloned().collect(),
        ..Default::default()
    };
    let TypeContract::RepeatedSettling { relation } = &mut records
        .types
        .iter_mut()
        .find(|t| t.id.as_str() == "runtime-test.body-relation.v1")
        .unwrap()
        .contract
    else {
        panic!()
    };
    for row in &mut relation.rows {
        row.outputs[0] = !row.outputs[0];
    }
    f.catalog = records.catalog().unwrap();
    f.catalog
        .insert_revisions(f.request.revisions.clone())
        .unwrap();
    let report = review_assembly_with_context(
        &f.catalog,
        &f.request.candidate_state.assembly,
        Some(&f.context.into()),
        Default::default(),
    )
    .unwrap();
    let diagnostics = report.diagnostics(64);
    assert_eq!(diagnostics.status, CheckStatus::Failed);
    let e = diagnostics
        .findings
        .iter()
        .filter(|f| f.kind == CheckKind::Behavior)
        .find_map(|f| {
            f.evidence.as_ref().filter(|e| {
                e.behavior
                    .as_ref()
                    .is_some_and(|b| !b["counterexample"].is_null())
            })
        })
        .unwrap();
    let behavior = e.behavior.as_ref().unwrap();
    assert_eq!(behavior["status"], "failed");
    assert!(behavior["counterexample"]["held_inputs"].is_array());
    assert!(behavior["counterexample"]["prefix"].is_array());
    assert!(behavior["counterexample"]["cycle_outputs"].is_array());
    assert!(e.binding.is_some());
    assert!(e.type_revision.is_some());
}

#[test]
fn budget_exhaustion_stays_undetermined_and_omissions_are_explicit() {
    let generated = generate_building(
        serde_json::from_value(
            json!({"namespace":"test.diagnostic","width":5,"depth":5,"height":4}),
        )
        .unwrap(),
    )
    .unwrap();
    let mut catalog = generated.records.catalog().unwrap();
    catalog
        .insert_revisions(generated.request.revisions.clone())
        .unwrap();
    let report = review_assembly_with_context(
        &catalog,
        &generated.request.candidate_state.assembly,
        Some(&generated.context.into()),
        BehaviorBudget {
            max_steps: 0,
            ..Default::default()
        },
    )
    .unwrap();
    let diagnostics = report.diagnostics(3);
    assert_eq!(diagnostics.status, CheckStatus::Undetermined);
    assert_eq!(diagnostics.failed_checks, 0);
    assert_eq!(diagnostics.findings.len(), 3);
    assert!(diagnostics.omitted_findings > 0);
    assert_eq!(
        diagnostics.total_findings,
        diagnostics.findings.len() + diagnostics.omitted_findings
    );
    assert!(
        diagnostics
            .findings
            .iter()
            .all(|f| f.status == CheckStatus::Undetermined)
    );
    let zero = report.diagnostics(0);
    assert!(zero.findings.is_empty());
    assert_eq!(zero.omitted_findings, zero.total_findings);
}
