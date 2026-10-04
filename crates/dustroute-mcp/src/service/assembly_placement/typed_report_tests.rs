//! Offline diagnostic contract: projections cannot authorize or replay writes.
use super::*;
use crate::operations::preview::AssemblyReconstructionPreview;
use crate::recorded_instance::{
    AssemblyDiagnosis, DiagnosisOutcome, RecordedInstanceObservation, RecordedInstanceReport,
    RecordedObservationOutcome, RecordedRevalidation,
};

#[tokio::test]
async fn assembly_previews_keep_history_diagnosis_and_execution_authority_separate() {
    let mut f = super::super::blueprint_tests::runtime_fixture::electrical_fixture(false);
    f.catalog
        .insert_revisions(f.request.revisions.clone())
        .unwrap();
    let source = SourceIdentity {
        record: f.request.candidate_state,
        adopted_by: dustroute_library::blueprint::BlueprintUpdateId::new("preview.test").unwrap(),
        context: f.context,
        catalog: f.catalog,
    };
    let transform = AssemblyTransform {
        source_anchor: Pos::default(),
        target_anchor: Pos::new(-96, 180, 1000),
        rotation: dustroute_translate::cells::RotationY::R90,
    };
    let proof = ValidatedAssemblyPlacement::review_target(
        &source.catalog,
        &source.record.assembly,
        &source.context,
        transform,
    )
    .unwrap();
    // Compare native and recorded projections at the public wire boundary;
    // neither the projection nor decoding it can reconstruct the proof.
    assert_eq!(
        serde_json::to_value(proof.review()).unwrap(),
        serde_json::to_value(proof.review().recorded()).unwrap()
    );
    let diagnosis = AssemblyDiagnosis {
        diagnosis: dustroute_translate::diagnostic::report::Diagnosis::design_comparison(vec![]),
        details: DiagnosisOutcome::ObservationUnavailable {
            reason: "test".into(),
            cause: "test".into(),
        },
        observation_failure: None,
    };
    let report = RecordedInstanceReport {
        observation: RecordedInstanceObservation {
            outcome: RecordedObservationOutcome::TargetMismatch {
                reason: "test".into(),
            },
            observed_at_unix_ms: None,
            runtime_history_reconstructed: false,
        },
        revalidation: RecordedRevalidation::Failed {
            reason: "test".into(),
        },
        removal_eligible: false,
        diagnosis: Some(diagnosis.clone()),
    };
    let record = PlacedAssembly {
        schema: PlacedAssembly::schema(),
        instance_id: uuid::Uuid::new_v4(),
        revision: 1,
        player: "Tester".into(),
        assembly_id: source.record.id.clone(),
        source_identity: source.clone(),
        transform,
        target: TargetServer {
            host: "offline.test".into(),
            port: 25565,
            version: "1.21.11".into(),
            dimension: "minecraft:overworld".into(),
            enabled_features: vec![],
        },
        assembly: proof.assembly().clone(),
        context: proof.context().clone(),
        expected: proof.settled().clone(),
        state: InstanceState::NeedsInspection,
        attempts: vec![],
        last_observation: Some(report.clone()),
        updated_at_unix_ms: 1,
    };
    let service = DustRouteMcp::with_policy_and_player(McpPolicy::default(), "Tester");
    let operation_id = uuid::Uuid::new_v4();
    service
        .plans
        .table::<StoredAssemblyPlacement>()
        .lock()
        .await
        .insert(
            operation_id,
            StoredAssemblyPlacement {
                player: record.player.clone(),
                dimension: record.target.dimension.clone(),
                assembly_id: record.assembly_id.clone(),
                source_identity: source.clone(),
                proof: proof.clone(),
                previewed: false,
                state: PistonPlacementState::Planned,
                expires_at: Instant::now() - Duration::from_secs(1),
                transform,
                target: record.target.clone(),
                action: AssemblyPlacementAction::Construct,
            },
        );
    for state in [
        PistonPlacementState::Planned,
        PistonPlacementState::Applied,
        PistonPlacementState::Undone,
        PistonPlacementState::NeedsInspection,
    ] {
        service
            .plans
            .table::<StoredAssemblyPlacement>()
            .lock()
            .await
            .get_mut(&operation_id)
            .unwrap()
            .state = state;
        // Read-only details remain available for consumed/expired plans. This
        // display cannot certify readiness, restore a proof or create activity.
        let details = service
            .assembly_service()
            .get_assembly_construction(operation_id)
            .await
            .unwrap();
        let wire = serde_json::to_value(details).unwrap();
        assert_eq!(wire["state"], format!("{state:?}"));
        assert_eq!(wire["previewed"], false);
        assert_eq!(
            wire["steps"],
            serde_json::to_value(proof.steps(false)).unwrap()
        );
        assert!(
            wire.as_object()
                .unwrap()
                .contains_key("reconstruction_conditions")
        );
        assert!(wire["reconstruction_conditions"].is_null());
        assert_eq!(wire["stored_history_is_validation_proof"], false);
        assert!(service.operations.activity(operation_id).await.is_none());
    }
    let construction = AssemblyConstructionPreview::new(
        uuid::Uuid::new_v4(),
        record.assembly_id.clone(),
        &source,
        record.target.dimension.clone(),
        false,
        &proof,
    );
    let removal = AssemblyRemovalPreview::new(
        uuid::Uuid::new_v4(),
        &record,
        RemovalReference::Constructed,
        None,
        &proof,
        report.clone(),
        false,
    );
    let intention = crate::assembly_registry::ReconstructionAttempt {
        baseline: record.expected.clone(),
        steps: proof.steps(true).to_vec(),
    };
    let reconstruction = AssemblyReconstructionPreview::new(
        uuid::Uuid::new_v4(),
        &record,
        &proof,
        &intention,
        vec![],
        report,
        false,
    );
    let history = serde_json::to_value(&reconstruction).unwrap();
    let response = serde_json::to_value(AssemblyManagementReport::Reconstruction {
        plan: Box::new(reconstruction.clone()),
        diagnosis: Some(Box::new(diagnosis)),
    })
    .unwrap();
    assert!(history.get("diagnosis").is_none());
    assert_eq!(response["observation"], history["observation"]);
    assert_eq!(response["diagnosis"], history["observation"]["diagnosis"]);
    for preview in [
        AssemblyPreview::Construction(Box::new(construction)),
        AssemblyPreview::Removal(Box::new(removal)),
        AssemblyPreview::Reconstruction(Box::new(reconstruction)),
    ] {
        let result: crate::operations::OperationResult = preview.into();
        assert!(!result.failed());
        assert!(result.progress().is_none());
        assert!(!result.consumed());
        let wire = serde_json::to_value(result).unwrap();
        assert!(wire.get("execution_progress").is_none());
        assert!(wire.get("verified_steps").is_none());
    }
    let mut saved = record.clone();
    // Historic display preserves the stored bounds; it cannot repair or
    // certify an invalid stored region simply by normalizing it.
    saved.expected.min = Pos::new(5, 6, 7);
    saved.expected.max = Pos::new(-5, -6, -7);
    let summary = saved.summary();
    assert_eq!(summary.bounds.min, saved.expected.min);
    assert_eq!(summary.bounds.max, saved.expected.max);
    assert!(!summary.saved_record_is_validation_proof);
    assert_eq!(summary.last_observation, saved.last_observation);
}
