//! Owned historical review data. No inverse conversion to a fresh review.
use dustroute_library::blueprint::{BlueprintRevisionId, InstancePath};
use dustroute_translate::{
    blueprint_update::RecordedReview,
    promotion::{CheckKind, CheckStatus},
    review_diagnostics::{CheckEvidence, ReviewDiagnostics, ReviewFinding},
};
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub(crate) struct RecordedReviewFinding {
    #[serde(skip_serializing_if = "Option::is_none")]
    instance: Option<InstancePath>,
    #[serde(skip_serializing_if = "Option::is_none")]
    revision: Option<BlueprintRevisionId>,
    kind: CheckKind,
    status: CheckStatus,
    detail: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    evidence: Option<Box<CheckEvidence>>,
}
impl From<&ReviewFinding> for RecordedReviewFinding {
    fn from(value: &ReviewFinding) -> Self {
        let ReviewFinding {
            instance,
            revision,
            kind,
            status,
            detail,
            evidence,
        } = value;
        Self {
            instance: instance.clone(),
            revision: revision.clone(),
            kind: *kind,
            status: *status,
            detail: detail.clone(),
            evidence: evidence.clone(),
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub(crate) struct RecordedReviewDiagnostics {
    schema_version: String,
    status: CheckStatus,
    failed_checks: usize,
    undetermined_checks: usize,
    total_findings: usize,
    omitted_findings: usize,
    findings: Vec<RecordedReviewFinding>,
    live_world_verified: bool,
}
impl From<&ReviewDiagnostics> for RecordedReviewDiagnostics {
    fn from(value: &ReviewDiagnostics) -> Self {
        let ReviewDiagnostics {
            schema_version,
            status,
            failed_checks,
            undetermined_checks,
            total_findings,
            omitted_findings,
            findings,
            live_world_verified,
        } = value;
        Self {
            schema_version: (*schema_version).into(),
            status: *status,
            failed_checks: *failed_checks,
            undetermined_checks: *undetermined_checks,
            total_findings: *total_findings,
            omitted_findings: *omitted_findings,
            findings: findings.iter().map(RecordedReviewFinding::from).collect(),
            live_world_verified: *live_world_verified,
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub(crate) struct RecordedReviewResponse {
    status: CheckStatus,
    /// True means fresh when recorded; it never certifies freshness on reopen.
    #[serde(rename = "fresh")]
    fresh_at_recording: bool,
    scope: String,
    diagnostics: RecordedReviewDiagnostics,
    placement_validation_profile: String,
    initial_state_checks_passed: bool,
    checks: RecordedReview,
    behavior_verified: bool,
    declared_behavior_verified: bool,
    behavior_scope: String,
    behavior_status: Option<CheckStatus>,
    live_world_verified: bool,
}
impl From<&crate::blueprint_mcp::ReviewResponse> for RecordedReviewResponse {
    fn from(value: &crate::blueprint_mcp::ReviewResponse) -> Self {
        let crate::blueprint_mcp::ReviewResponse {
            status,
            fresh,
            scope,
            diagnostics,
            placement_validation_profile,
            initial_state_checks_passed,
            checks,
            behavior_verified,
            declared_behavior_verified,
            behavior_scope,
            behavior_status,
            live_world_verified,
        } = value;
        Self {
            status: *status,
            fresh_at_recording: *fresh,
            scope: (*scope).into(),
            diagnostics: diagnostics.into(),
            placement_validation_profile: (*placement_validation_profile).into(),
            initial_state_checks_passed: *initial_state_checks_passed,
            checks: checks.clone(),
            behavior_verified: *behavior_verified,
            declared_behavior_verified: *declared_behavior_verified,
            behavior_scope: (*behavior_scope).into(),
            behavior_status: *behavior_status,
            live_world_verified: *live_world_verified,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use dustroute_translate::promotion::{CheckResult, PromotionReport};
    use std::collections::BTreeMap;

    #[test]
    fn historical_projection_keeps_failed_review_and_public_fields_after_reopen() {
        let report = PromotionReport {
            occurrences: BTreeMap::new(),
            arrangement: vec![CheckResult {
                kind: CheckKind::Placement,
                status: CheckStatus::Failed,
                detail: "missing support at the target".into(),
                evidence: Some(Box::new(CheckEvidence {
                    position: Some(dustroute_physical::Pos::new(1, 2, 3)),
                    ..Default::default()
                })),
            }],
            behavior_context: None,
            behavior: vec![],
        };
        let fresh = crate::blueprint_mcp::review_response(
            &report,
            dustroute_library::builtin_blueprints::builtin_blueprints(),
        );
        let record = RecordedReviewResponse::from(&fresh);
        assert_eq!(record.status, CheckStatus::Failed);
        let bytes = dustroute_codec::storage::encode("review.history.v1", &record, 65536).unwrap();
        let reopened: RecordedReviewResponse =
            dustroute_codec::storage::decode("review.history.v1", &bytes, 65536).unwrap();
        assert_eq!(reopened, record);
        assert_eq!(
            serde_json::to_value(&reopened).unwrap(),
            serde_json::to_value(&fresh).unwrap()
        );
        assert_eq!(
            reopened.diagnostics.findings[0]
                .evidence
                .as_ref()
                .unwrap()
                .position,
            Some(dustroute_physical::Pos::new(1, 2, 3))
        );
        // Recorded freshness is a historical field, not a PromotionReport.
        assert!(reopened.fresh_at_recording);
        let source = crate::placement_source::PlacementSource::AdoptedAssemblyRevision {
            assembly_revision_id: dustroute_library::blueprint::AssemblyRevisionId::new("assembly")
                .unwrap(),
            adopted_by: dustroute_library::blueprint::BlueprintUpdateId::new("adoption").unwrap(),
            grounding_assembly_revision_id: dustroute_library::blueprint::AssemblyRevisionId::new(
                "grounding",
            )
            .unwrap(),
            fresh_review: Box::new(reopened),
            literal_observation: crate::placement_source::LiteralObservation::GroundingBaseSnapshot,
            candidate_interpretation:
                crate::placement_source::CandidateInterpretation::RecordAssembly,
        };
        let bytes = dustroute_codec::storage::encode("source.history.v1", &source, 65536).unwrap();
        let reopened: crate::placement_source::PlacementSource =
            dustroute_codec::storage::decode("source.history.v1", &bytes, 65536).unwrap();
        assert_eq!(reopened, source);
        let public = serde_json::to_value(&reopened).unwrap();
        assert_eq!(public["kind"], "adopted_assembly_revision");
        assert_eq!(public["literal_observation"], "grounding.base_snapshot");
        assert_eq!(public["candidate_interpretation"], "record.assembly");
        assert_eq!(public["fresh_review"]["status"], "failed");
        assert_eq!(public["fresh_review"]["fresh"], true);
    }

    #[test]
    fn recorded_reviews_cannot_be_deserialized_into_a_fresh_review() {
        trait AmbiguousIfRestorable<A> {
            fn marker() {}
        }
        impl<T> AmbiguousIfRestorable<()> for T {}
        impl<T: serde::Deserialize<'static>> AmbiguousIfRestorable<u8> for T {}
        impl<T: From<RecordedReviewResponse>> AmbiguousIfRestorable<u16> for T {}
        impl<T: From<crate::recorded_instance::RecordedPlacementReview>> AmbiguousIfRestorable<u32> for T {}
        let _ = <crate::blueprint_mcp::ReviewResponse as AmbiguousIfRestorable<_>>::marker;
        let _ = <crate::piston_assembly::PlacementReview as AmbiguousIfRestorable<_>>::marker;
        let _ = <crate::piston_assembly::ValidatedAssemblyPlacement as AmbiguousIfRestorable<_>>::marker;
    }
}
