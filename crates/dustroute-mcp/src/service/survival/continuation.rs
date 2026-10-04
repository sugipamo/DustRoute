//! Reobserve a sealed idle boundary and create a new process-local plan.
use super::*;
use crate::survival_construction::continuation::{assess, rebase, scene_snapshot};
use crate::survival_execution::{checkpoint, received_materials};

struct Preparation {
    owner: String,
    source: SourceIdentity,
    original: ConstructionSite,
    construction: ConstructionSpecification,
    boundary: checkpoint::SafeCheckpoint,
}
fn preparation(service: &DustRouteMcp, id: uuid::Uuid) -> Result<Preparation, Refusal> {
    let directory = service.state_store.survival_job_root().join(id.to_string());
    let manifest: JobManifest = load(&directory.join("manifest.store"))
        .map_err(|e| Refusal::new(ServiceCode::JobUnavailable, e))?;
    manifest
        .validate_identity(id)
        .map_err(|e| Refusal::new(ServiceCode::InvalidRecord, e))?;
    let owner = manifest.owner;
    service
        .policy
        .authorize_player(&owner)
        .map_err(|e| Refusal::new(ServiceCode::PermissionDenied, e))?;
    let boundary = checkpoint::read(&directory.join("execution")).map_err(|e| {
        let mut response = Refusal::new(e.code, &e.detail);
        if e.code == SurvivalErrorCode::CheckpointConsumed {
            if let Ok(claim) = load::<crate::survival_execution::diagnostic::CheckpointClaim>(
                &directory.join("execution/continuation-claim.store"),
            ) {
                response.linked_job(claim.new_job);
            }
        }
        if let Ok(diagnosis) = crate::survival_execution::diagnose(&directory.join("execution")) {
            response.recorded_diagnosis(diagnosis.record);
        }
        response
    })?;
    let saved = manifest
        .source
        .ok_or_else(|| Refusal::new(ServiceCode::InvalidRecord, "missing adopted source"))?;
    let current =
        crate::blueprint_mcp::construction_source(&service.state_store, &owner, &saved.record.id)
            .map_err(|e| Refusal::new(ServiceCode::SourceChanged, e))?;
    if !saved.matches(&current) {
        return Err(Refusal::new(
            ServiceCode::SourceChanged,
            "adopted source differs from checkpoint job",
        ));
    }
    let construction = manifest.construction.ok_or_else(|| {
        Refusal::new(
            ServiceCode::ContinuationSpecificationMissing,
            "missing construction specification",
        )
    })?;
    let specification = construction.specification.clone();
    let design = dustroute_translate::building::generate_grounded_building_design(specification)
        .map_err(|e| Refusal::new(ServiceCode::SpecificationInvalid, e))?;
    validate_design(&current, &design)
        .map_err(|e| Refusal::new(ServiceCode::SourceNotAdoptedOrMismatched, e))?;
    let scope = construction.scope.clone();
    if scope != boundary.scope {
        return Err(Refusal::new(
            ServiceCode::InvalidCheckpoint,
            "stored scope differs from checkpoint",
        ));
    }
    let site = ConstructionSite::from_grounded(&design, scope)
        .map_err(|e| Refusal::new(e.code, e.detail))?;
    policy_scope(&service.policy, site.scope(), &boundary.dimension)
        .map_err(|e| Refusal::new(ServiceCode::PermissionDenied, e))?;
    Ok(Preparation {
        owner,
        source: current,
        original: site,
        construction,
        boundary,
    })
}

pub(super) fn validate_design(
    source: &SourceIdentity,
    design: &dustroute_translate::building::GeneratedGroundedBuildingDesign,
) -> Result<(), String> {
    if source.record != design.request.candidate_state || source.context != design.context {
        return Err("specification does not match the exact adopted Assembly and context".into());
    }
    let catalog = design.records.catalog().map_err(|e| e.to_string())?;
    if !catalog
        .revisions()
        .chain(design.request.revisions.iter())
        .all(|r| source.catalog.revision(&r.id) == Some(r))
        || !catalog
            .type_revisions()
            .all(|r| source.catalog.type_revision(&r.id) == Some(r))
        || !catalog
            .classifications()
            .all(|r| source.catalog.classification(&r.id) == Some(r))
        || !catalog
            .assemblies()
            .all(|r| source.catalog.assembly(&r.id) == Some(r))
    {
        return Err("adopted definitions differ from the grounded specification".into());
    }
    Ok(())
}

impl DustRouteMcp {
    pub(super) async fn continue_survival(&self, id: uuid::Uuid, limits: SearchLimits) -> Reply {
        if self.survival.entries.lock().await.len() >= 32 {
            return failure(ServiceCode::JobCapacity, "at most 32 process-local jobs");
        }
        let service = self.clone();
        let prepared = tokio::task::spawn_blocking(move || preparation(&service, id)).await;
        let prepared = match prepared {
            Ok(Ok(v)) => v,
            Ok(Err(e)) => return e.into(),
            Err(e) => return failure(ServiceCode::ContinuationPreparationFailed, e),
        };
        let native = match self.bridge.survival_bridge() {
            Ok(n) => n,
            Err(e) => return failure(ServiceCode::BackendUnavailable, e),
        };
        let lease = match native.lease_survival() {
            Ok(l) => l,
            Err(e) => return failure(ServiceCode::SourceBusy, e),
        };
        let bot = lease.source();
        if prepared.boundary.endpoint != checkpoint::endpoint(&lease.reconnect()) {
            lease.release(bot);
            return failure(
                ServiceCode::CheckpointEndpointChanged,
                "continuation requires the original endpoint and builder profile",
            );
        }
        let result = self.continue_with_scene(prepared, id, limits, &bot).await;
        lease.release(bot);
        result.unwrap_or_else(|e| e)
    }
    async fn continue_with_scene(
        &self,
        prepared: Preparation,
        parent: uuid::Uuid,
        limits: SearchLimits,
        bot: &voxrig::Client,
    ) -> Result<Reply, Reply> {
        let Preparation {
            owner,
            source,
            original,
            construction,
            boundary,
        } = prepared;
        let ops = bot
            .survival()
            .map_err(|e| failure(ServiceCode::NativeRefused, e))?;
        let scene = ops
            .capture_survival_scene(region(original.scope().observed))
            .await
            .map_err(|e| failure(ServiceCode::ObservationUnavailable, e))?;
        if scene.source().dimension != boundary.dimension {
            return Err(failure(
                ServiceCode::CheckpointDimensionChanged,
                "builder is in a different dimension",
            ));
        }
        let fresh = scene_snapshot(&scene).map_err(|e| failure(e.code, e.detail))?;
        let diagnosis = assess(&original, &boundary.snapshot, &boundary.temporary, &fresh)
            .map_err(|e| failure(e.code, e.detail))?;
        if !diagnosis.conflicts.is_empty() {
            return Err(Reply::site_changed(diagnosis));
        }
        let (site, diagnosis) = rebase(original, &boundary.snapshot, &boundary.temporary, fresh)
            .map_err(|e| failure(e.code, e.detail))?;
        let supplied = received_materials(
            &ops.player_state()
                .await
                .map_err(|e| failure(ServiceCode::InventoryUnavailable, e))?,
        )
        .map_err(|e| failure(e.code, e.detail))?;
        let temporary_material = construction.temporary_material.clone();
        let generated = match generate_construction_plan_async(
            scene,
            site,
            supplied.clone(),
            temporary_material,
            limits,
        )
        .await
        {
            Ok(generated) => generated,
            Err(error) => {
                return Err(Reply::generation(
                    error,
                    Some(replies::GenerationContinuation {
                        diagnosis,
                        received_materials: supplied,
                    }),
                ));
            }
        };
        let publication = self
            .publish_survival_plan(
                &owner,
                source,
                generated,
                construction,
                Some(ContinuationParent {
                    id: parent,
                    checkpoint: boundary,
                }),
            )
            .await;
        Ok(Reply::continued(publication, parent, diagnosis, supplied))
    }
}
