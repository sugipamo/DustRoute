//! A newly reviewed reconstruction, never continuation of an uncertain attempt.
use super::*;
use crate::assembly_registry::ReconstructionAttempt;

pub(super) fn conditions() -> Value {
    json!({
        "strategy":"teardown_observed_layout_then_rebuild_declared_initial_state",
        "server_readiness_proven":false,
        "runtime_history_reconstructed":false,
        "automatic_retry":false,
        "operator_requirement":"review all affected blocks; let previous commands finish and keep other inputs/edits out of the region during reconstruction",
        "limitation":"matching client samples cannot prove empty server queues, ownership of identical material, or atomic check-and-write"
    })
}

impl DustRouteMcp {
    pub(super) async fn plan_assembly_reconstruction(
        &self,
        record: PlacedAssembly,
        proof: ValidatedAssemblyPlacement,
        observation: observation::InstanceObservation,
        report: Value,
    ) -> Result<Value, String> {
        if record.state == InstanceState::Removed {
            return Err("removed instances need a new placement plan".into());
        }
        let baseline = observation::stable_baseline(&observation)?;
        let model = proof.clone();
        let initial = baseline.clone();
        let steps = tokio::task::spawn_blocking(move || model.reconstruction_steps(&initial))
            .await
            .map_err(|e| e.to_string())??;
        self.policy
            .validate_placement_size(steps.len())
            .map_err(|e| e.to_string())?;
        let differences =
            dustroute_translate::diagnostic::difference::differences(&baseline, proof.settled())?;
        let reconstruction = ReconstructionAttempt { baseline, steps };
        let operation_id = uuid::Uuid::new_v4();
        let response = json!({"ok":true,"kind":"placed_assembly_reconstruction",
            "operation_id":operation_id,"instance_id":record.instance_id,"record_revision":record.revision,
            "bounds":bounds_json(proof.bounds()),"dimension":record.target.dimension,
            "read_only":self.policy.read_only,"differences":differences,
            "reconstruction":reconstruction,"reconstruction_conditions":conditions(),
            "fresh_target_review":proof.review(),"observation":report,
            "next_step":"show_operation; confirm all observed blocks may be removed and rebuilt, then invoke_operation(confirm=true)"});
        let mut plans = self.assembly_placements.lock().await;
        plans.retain(|_, p| {
            p.state != PistonPlacementState::Planned || p.expires_at > Instant::now()
        });
        if plans.len() >= 256 {
            return Err("too many retained Assembly construction plans".into());
        }
        plans.insert(
            operation_id,
            StoredAssemblyPlacement {
                player: record.player,
                dimension: record.target.dimension.clone(),
                assembly_id: record.assembly_id,
                source_identity: record.source_identity,
                proof,
                previewed: false,
                state: PistonPlacementState::Planned,
                expires_at: Instant::now() + Duration::from_secs(300),
                transform: record.transform,
                target: record.target,
                action: AssemblyPlacementAction::Reconstruct {
                    instance_id: record.instance_id,
                    revision: record.revision,
                    plan: Box::new(reconstruction),
                },
            },
        );
        drop(plans);
        self.operations
            .record_completed(
                operation_id,
                OperationKind::PlacementPreview,
                response.clone(),
            )
            .await;
        Ok(response)
    }
}
