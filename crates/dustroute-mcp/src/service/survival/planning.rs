//! Read-only construction generation and publication of process-local plans.
use super::*;

impl DustRouteMcp {
    pub(super) async fn plan_survival(
        &self,
        owner: &str,
        input: PlanningInput,
        bot: &voxrig::Client,
    ) -> Value {
        let PlanningInput {
            source,
            site,
            supplied,
            temporary_material,
            limits,
            specification,
        } = input;
        let scope = site.scope().clone();
        if let Err(e) =
            self.policy
                .validate_region(dustroute_translate::world_reverse::RegionBounds::new(
                    scope.observed.min,
                    scope.observed.max,
                ))
        {
            return failure("permission_denied", e);
        }
        let ops = match bot.survival() {
            Ok(o) => o,
            Err(e) => return failure("native_refused", e),
        };
        let scene = match ops.capture_survival_scene(region(scope.observed)).await {
            Ok(s) => s,
            Err(e) => return failure("observation_unavailable", e),
        };
        if let Err(e) = policy_scope(&self.policy, &scope, &scene.source().dimension) {
            return failure("permission_denied", e);
        }
        let result = generate_construction_plan_async(
            scene,
            site,
            supplied,
            temporary_material.clone(),
            limits,
        )
        .await;
        let generated = match result {
            Ok(r) => r,
            Err(e) => {
                return json!({"ok":false,"error":{"code":"generation_refused","cause":e},"writes_minecraft":false});
            }
        };
        self.publish_survival_plan(
            owner,
            source,
            generated,
            ConstructionSpecification {
                specification,
                scope,
                temporary_material,
            },
            None,
        )
        .await
    }

    pub(super) async fn publish_survival_plan(
        &self,
        owner: &str,
        source: SourceIdentity,
        generated: crate::survival_construction::generation::GeneratedConstructionPlan,
        construction: ConstructionSpecification,
        parent: Option<ContinuationParent>,
    ) -> Value {
        if self.survival.entries.lock().await.len() >= 32 {
            return failure("job_capacity", "at most 32 process-local jobs");
        }
        if let Err(e) = self
            .policy
            .validate_placement_size(generated.plan.steps().len())
        {
            return failure("permission_denied", e);
        }
        let id = uuid::Uuid::new_v4();
        let path = self.state_store.survival_job_root().join(id.to_string());
        if let Err(e) = std::fs::create_dir_all(&path) {
            return failure("journal_io", e);
        }
        let manifest = JobManifest {
            schema: JobSchema::V1,
            job_id: id,
            owner: owner.into(),
            source: Some(source.clone()),
            preview: Some(Box::new((&generated).into())),
            construction: Some(construction),
            parent_job_id: parent.as_ref().map(|p| p.id),
            execution_authority_restorable: crate::survival_execution::diagnostic::DiagnosticOnly,
        };
        if let Err(e) = save(&path.join("manifest.json"), &manifest) {
            return failure("journal_io", e);
        }
        let response = json!({"ok":true,"schema_version":"dustroute.survival-job.v1","job_id":id,
            "state":"planned","preview":generated,"writes_minecraft":false,"inventory_receipt":false,
            "expires_after_seconds":900,"next_step":"review plan, then action=start with confirmed=true; current inventory/site/source will be rechecked"});
        self.survival.entries.lock().await.insert(
            id,
            Entry::planned(
                owner.into(),
                source,
                generated.plan,
                Instant::now() + Duration::from_secs(900),
                parent,
            ),
        );
        response
    }
}
