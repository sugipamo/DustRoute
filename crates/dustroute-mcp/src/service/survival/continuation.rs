//! Reobserve a sealed idle boundary and create a new process-local plan.
use super::*;
use crate::survival_construction::continuation::{assess, rebase, scene_snapshot};
use crate::survival_execution::{checkpoint, received_materials};

struct Preparation {
    owner: String,
    source: SourceIdentity,
    original: ConstructionSite,
    construction: Value,
    boundary: checkpoint::SafeCheckpoint,
}
fn preparation(service: &DustRouteMcp, id: uuid::Uuid) -> Result<Preparation, Value> {
    let directory = service.state_store.survival_job_root().join(id.to_string());
    let manifest =
        load(&directory.join("manifest.json")).map_err(|e| failure("job_unavailable", e))?;
    if manifest["schema"] != "dustroute.survival-job.v1"
        || manifest["job_id"] != json!(id)
        || manifest["execution_authority_restorable"] != false
    {
        return Err(failure(
            "invalid_record",
            "unknown schema or mismatched job identity",
        ));
    }
    let owner = manifest["owner"]
        .as_str()
        .ok_or_else(|| failure("invalid_record", "missing job owner"))?
        .to_owned();
    service
        .policy
        .authorize_player(&owner)
        .map_err(|e| failure("permission_denied", e))?;
    let boundary = checkpoint::read(&directory.join("execution")).map_err(|e| {
        let mut response = failure(e.code, &e.detail);
        if e.code == "checkpoint_consumed" {
            if let Ok(claim) = load(&directory.join("execution/continuation-claim.json")) {
                response["continuation_job_id"] = claim["new_job"].clone();
                response["next_step"] = json!("get the linked new job; do not replay the old checkpoint");
            }
        }
        if let Ok(diagnosis) = crate::survival_execution::diagnose(&directory.join("execution")) {
            let record = diagnosis.record;
            response["historical_diagnosis"] = json!({"execution_id":record.id,"completed_steps":record.completed_steps,"outcome":record.outcome,"recorded_continuation":record.continuation,"last_event":record.events.last(),"execution_authority_restored":false});
        }
        response
    })?;
    let saved: SourceIdentity = serde_json::from_value(manifest["source"].clone())
        .map_err(|e| failure("invalid_record", e))?;
    let current =
        crate::blueprint_mcp::construction_source(&service.state_store, &owner, &saved.record.id)
            .map_err(|e| failure("source_changed", e))?;
    if !saved.matches(&current) {
        return Err(failure(
            "source_changed",
            "adopted source differs from checkpoint job",
        ));
    }
    let construction = manifest["construction"].clone();
    let specification: GroundedBuildingDesignRequest =
        serde_json::from_value(construction["specification"].clone())
            .map_err(|e| failure("continuation_specification_missing", e))?;
    let design = dustroute_translate::building::generate_grounded_building_design(specification)
        .map_err(|e| failure("specification_invalid", e))?;
    validate_design(&current, &design)
        .map_err(|e| failure("source_not_adopted_or_mismatched", e))?;
    let scope: ConstructionScope = serde_json::from_value(construction["scope"].clone())
        .map_err(|e| failure("invalid_record", e))?;
    if json!(scope) != json!(boundary.scope) {
        return Err(failure(
            "invalid_checkpoint",
            "stored scope differs from checkpoint",
        ));
    }
    let site =
        ConstructionSite::from_grounded(&design, scope).map_err(|e| failure(e.code, e.detail))?;
    policy_scope(&service.policy, site.scope(), &boundary.dimension)
        .map_err(|e| failure("permission_denied", e))?;
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
    pub(super) async fn continue_survival(&self, id: uuid::Uuid, limits: SearchLimits) -> Value {
        if self.survival.entries.lock().await.len() >= 32 {
            return failure("job_capacity", "at most 32 process-local jobs");
        }
        let service = self.clone();
        let prepared = tokio::task::spawn_blocking(move || preparation(&service, id)).await;
        let prepared = match prepared {
            Ok(Ok(v)) => v,
            Ok(Err(e)) => return e,
            Err(e) => return failure("continuation_preparation_failed", e),
        };
        let Some(observer) = self.survival_observer.as_ref() else {
            return failure(
                "observer_not_configured",
                "fresh independent scene required",
            );
        };
        let native = match self.bridge.survival_bridge() {
            Ok(n) => n,
            Err(e) => return failure("backend_unavailable", e),
        };
        let lease = match native.lease_survival() {
            Ok(l) => l,
            Err(e) => return failure("source_busy", e),
        };
        let bot = lease.source();
        if prepared.boundary.endpoint != checkpoint::endpoint(&lease.reconnect()) {
            lease.release(bot);
            return failure(
                "checkpoint_endpoint_changed",
                "continuation requires the original endpoint and builder profile",
            );
        }
        let result = self
            .continue_with_scene(prepared, id, limits, &bot, observer)
            .await;
        lease.release(bot);
        result.unwrap_or_else(|e| e)
    }
    async fn continue_with_scene(
        &self,
        prepared: Preparation,
        parent: uuid::Uuid,
        limits: SearchLimits,
        bot: &voxrig::Client,
        observer: &voxrig::Client,
    ) -> Result<Value, Value> {
        let Preparation {
            owner,
            source,
            original,
            construction,
            boundary,
        } = prepared;
        let ops = bot.survival().map_err(|e| failure("native_refused", e))?;
        let scene = ops
            .capture_survival_scene(region(original.scope().observed))
            .await
            .map_err(|e| failure("observation_unavailable", e))?;
        if scene.source().dimension != boundary.dimension {
            return Err(failure(
                "checkpoint_dimension_changed",
                "builder is in a different dimension",
            ));
        }
        let obs_ops = observer
            .java_1_21_11_operations()
            .map_err(|e| failure("observer_unavailable", e))?;
        let before = obs_ops
            .player_state()
            .await
            .map_err(|e| failure("observer_unavailable", e))?;
        let observed = observer
            .observe_region(scene.region())
            .await
            .map_err(|e| failure("observer_unavailable", e))?;
        let after = obs_ops
            .player_state()
            .await
            .map_err(|e| failure("observer_unavailable", e))?;
        if before.dimension.as_deref() != Some(boundary.dimension.as_str())
            || after.dimension != before.dimension
            || after.connection_id != before.connection_id
            || observed.connection_id != before.connection_id
            || observed.connection_id == scene.source().connection_id
            || observed.region != scene.region()
            || observed.version != voxrig::MinecraftVersion::Java1_21_11
            || observed.blocks.len()
                != scene
                    .region()
                    .volume()
                    .map_err(|e| failure("invalid_observation", e))?
            || observed
                .blocks
                .iter()
                .map(|b| b.position)
                .collect::<std::collections::BTreeSet<_>>()
                .len()
                != observed.blocks.len()
            || observed.blocks.iter().any(|b| {
                b.state.is_none()
                    || b.state.as_ref() != scene.scenario().block(b.position).ok().as_ref()
            })
        {
            return Err(failure(
                "observer_mismatch",
                "fresh independent site differs from builder",
            ));
        }
        let fresh = scene_snapshot(&scene).map_err(|e| failure(e.code, e.detail))?;
        let diagnosis = assess(&original, &boundary.snapshot, &boundary.temporary, &fresh)
            .map_err(|e| failure(e.code, e.detail))?;
        if !diagnosis.conflicts.is_empty() {
            return Err(
                json!({"ok":false,"error":{"code":"checkpoint_site_changed","detail":"external differences require inspection; no automatic removal"},"diagnosis":diagnosis,"writes_minecraft":false}),
            );
        }
        let (site, diagnosis) = rebase(original, &boundary.snapshot, &boundary.temporary, fresh)
            .map_err(|e| failure(e.code, e.detail))?;
        let supplied = received_materials(
            &ops.player_state()
                .await
                .map_err(|e| failure("inventory_unavailable", e))?,
        )
        .map_err(|e| failure(e.code, e.detail))?;
        let temporary_material = construction["temporary_material"]
            .as_str()
            .ok_or_else(|| failure("invalid_record", "missing temporary material"))?
            .to_owned();
        let generated = generate_construction_plan_async(scene,site,supplied.clone(),temporary_material,limits).await
                .map_err(|e|json!({"ok":false,"error":{"code":"generation_refused","cause":e},"diagnosis":diagnosis,"received_materials":supplied,"writes_minecraft":false}))?;
        let mut response = self
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
        response["parent_job_id"] = json!(parent);
        response["diagnosis"] = json!(diagnosis);
        response["received_materials"] = json!(supplied);
        response["execution_authority_restored"] = json!(false);
        Ok(response)
    }
}
