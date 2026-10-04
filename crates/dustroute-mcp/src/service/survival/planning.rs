//! Read-only construction generation and publication of process-local plans.
use super::*;

impl DustRouteMcp {
    pub(super) async fn plan_survival(
        &self,
        owner: &str,
        input: PlanningInput,
        bot: &voxrig::Client,
    ) -> Reply {
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
            return failure(ServiceCode::PermissionDenied, e);
        }
        let ops = match bot.survival() {
            Ok(o) => o,
            Err(e) => return failure(ServiceCode::NativeRefused, e),
        };
        let scene = match ops.capture_survival_scene(region(scope.observed)).await {
            Ok(s) => s,
            Err(e) => return failure(ServiceCode::ObservationUnavailable, e),
        };
        if let Err(e) = policy_scope(&self.policy, &scope, &scene.source().dimension) {
            return failure(ServiceCode::PermissionDenied, e);
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
                return Reply::generation(e, None);
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
        .into()
    }

    pub(super) async fn publish_survival_plan(
        &self,
        owner: &str,
        source: SourceIdentity,
        generated: crate::survival_construction::generation::GeneratedConstructionPlan,
        construction: ConstructionSpecification,
        parent: Option<ContinuationParent>,
    ) -> Publication {
        if self.survival.entries.lock().await.len() >= 32 {
            return Refusal::new(ServiceCode::JobCapacity, "at most 32 process-local jobs").into();
        }
        if let Err(e) = self
            .policy
            .validate_placement_size(generated.plan.steps().len())
        {
            return Refusal::new(ServiceCode::PermissionDenied, e).into();
        }
        let id = uuid::Uuid::new_v4();
        let path = self.state_store.survival_job_root().join(id.to_string());
        if let Err(e) = std::fs::create_dir_all(&path) {
            return Refusal::new(ServiceCode::JournalIo, e).into();
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
        if let Err(e) = save(&path.join("manifest.store"), &manifest) {
            return Refusal::new(ServiceCode::JournalIo, e).into();
        }
        let response =
            Publication::Published(Box::new(PublishedPlan::new(id, (&generated).into())));
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
