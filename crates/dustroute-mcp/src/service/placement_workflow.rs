//! One command-placement attempt, its context check and independent readback.
//! Player resolution and MCP dispatch have already happened at the facade.
use super::*;
pub(super) struct PlacementWorkflow<'a> {
    pub bridge: &'a BotBridge,
    pub policy: &'a McpPolicy,
    pub plans: &'a OperationPlans,
    pub operations: &'a OperationRegistry,
    pub mutation_lock: &'a Mutex<()>,
    pub actor: Result<String, FailureCause>,
}
impl PlacementWorkflow<'_> {
    fn world_editor(&self) -> world_editor::WorldEditor<'_> {
        world_editor::WorldEditor {
            bridge: self.bridge,
            policy: self.policy,
        }
    }
    pub(super) async fn mutate_placement(
        &self,
        params: ConfirmedOperationParams,
        undo: bool,
    ) -> Value {
        let mut progress = ExecutionProgress::default();
        let mut response = self.execute_placement(&params, undo, &mut progress).await;
        response["operation_id"] = json!(params.operation_id);
        if let Ok(id) = uuid::Uuid::parse_str(&params.operation_id) {
            self.operations
                .record_completed(
                    id,
                    if undo {
                        OperationKind::PlacementUndo
                    } else {
                        OperationKind::PlacementApply
                    },
                    response.clone(),
                )
                .await;
        }
        response
    }
    async fn execute_placement(
        &self,
        params: &ConfirmedOperationParams,
        undo: bool,
        progress: &mut ExecutionProgress,
    ) -> Value {
        if !params.confirm {
            return json!({
                "ok": false,
                "error": "confirm must be true because this operation changes the world"
            });
        }
        if let Err(error) = self.policy.authorize_mutation() {
            return progress.cause(error).response();
        }
        let queue = crate::performance::span(crate::performance::Phase::MutationQueue);
        let _mutation_guard = self.mutation_lock.lock().await;
        drop(queue);
        let operation_id = match uuid::Uuid::parse_str(&params.operation_id) {
            Ok(id) => id,
            Err(error) => {
                return workflow_error(McpErrorCode::InvalidArgument, error.to_string(), false);
            }
        };
        if let Some(context) = self.plans.placements().lock().await.revision(&operation_id) {
            let player = match self.actor.clone() {
                Ok(player) => player,
                Err(error) => return json!({"ok":false,"error":error}),
            };
            if let Err(error) = self.policy.authorize_player(&player) {
                return workflow_error(McpErrorCode::PermissionDenied, error.to_string(), false);
            }
            if context.player != player {
                return json!({"ok":false,"error":"placement belongs to another player"});
            }
        }
        let plan = match self
            .plans
            .placements()
            .lock()
            .await
            .get(&operation_id)
            .cloned()
        {
            Some(plan) => plan,
            None => return json!({ "ok": false, "error": "unknown operation ID" }),
        };
        let dimension = match self
            .plans
            .placements()
            .lock()
            .await
            .dimension(&operation_id)
            .cloned()
        {
            Some(dimension) => dimension,
            None => {
                return json!({
                    "ok": false,
                    "error": "placement plan has no captured dimension"
                });
            }
        };
        if let Err(error) = self.policy.authorize_dimension(&dimension) {
            return progress.cause(error).response();
        }
        let is_applied = self
            .plans
            .placements()
            .lock()
            .await
            .is_applied(&operation_id);
        if undo && !is_applied {
            return json!({ "ok": false, "error": "placement plan is not applied" });
        }
        if !undo && is_applied {
            return json!({ "ok": false, "error": "placement plan is already applied" });
        }
        if !undo
            && (self.policy.preview_required
                || self
                    .plans
                    .placements()
                    .lock()
                    .await
                    .has_revision(&operation_id))
            && !plan.previewed
        {
            return workflow_error(
                McpErrorCode::InvalidState,
                "placement must be shown with show_operation before invoke_operation",
                false,
            );
        }

        let source = if undo {
            &plan.undo.changes
        } else {
            &plan.changes
        };
        if let Err(error) = self.policy.validate_placement_size(source.len()) {
            return progress.cause(error).response();
        }
        progress.phase = FailurePhase::BeforeReadback;
        crate::performance::execution_progress(progress);
        progress.total_changes = Some(source.len());
        let (baseline_matches, baseline_mismatches) = match self
            .world_editor()
            .verify_placement_changes(source, &dimension, false)
            .await
        {
            Ok(result) => result,
            Err(error) => return progress.cause(error).response(),
        };
        if !baseline_matches {
            if let Some(stored) = self.plans.placements().lock().await.get_mut(&operation_id) {
                stored.previewed = false;
            }
            let mut response = progress
                .cause(FailureCause::mismatch(
                    "placement baseline is stale; preview again before changing the world",
                    &baseline_mismatches,
                ))
                .response();
            response["mismatches"] = json!(baseline_mismatches);
            return response;
        }
        // Serialized plans carry no validation proof. Recheck the current
        // placement context before forward writes. Exact undo restores captured
        // evidence and intentionally does not certify a new placement.
        let validated = if undo {
            None
        } else {
            match self
                .world_editor()
                .validate_live_changes(source, &dimension)
                .await
            {
                Ok(changes) => Some(changes),
                Err(error) => return progress.cause(error).response(),
            }
        };
        let source = validated
            .as_ref()
            .map_or(source.as_slice(), |v| v.changes());
        let mut changes = source.iter().collect::<Vec<_>>();
        changes.sort_by_key(|change| {
            let priority = match change.after.kind {
                BlockKind::Solid
                | BlockKind::Transparent
                | BlockKind::RedstoneBlock
                | BlockKind::Observer
                | BlockKind::Piston => 0,
                BlockKind::RedstoneTorch | BlockKind::Lever => 2,
                _ => 1,
            };
            (priority, change.pos.y, change.pos.x, change.pos.z)
        });
        let export = JavaExportConfig {
            relative: false,
            ..JavaExportConfig::default()
        };
        let writes = match changes
            .into_iter()
            .map(|change| {
                dustroute_translate::minecraft_export::native_block_state(
                    &change.after,
                    &export,
                    dustroute_translate::minecraft_export::ExportPurpose::InitialPlacement,
                )
                .map(|state| CommandWrite {
                    pos: change.pos,
                    state,
                })
                .map_err(|e| e.to_string())
            })
            .collect::<Result<Vec<_>, _>>()
        {
            Ok(writes) => writes,
            Err(error) => return json!({ "ok": false, "error": error.to_string() }),
        };
        if let Err(error) = self
            .check_revision_placement(operation_id, undo, false, true)
            .await
        {
            return progress.cause(error).response();
        }
        progress.operation_consumed = self
            .plans
            .placements()
            .lock()
            .await
            .revision(&operation_id)
            .is_some();
        if let Err(error) = self
            .plans
            .placements()
            .lock()
            .await
            .begin(&operation_id, undo)
        {
            return progress
                .cause(FailureCause::new(CauseKind::InvalidState, error))
                .response();
        }
        progress.operation_consumed = true;
        let previous_world = progress.world;
        progress.begin_submission();
        crate::performance::execution_progress(progress);
        let bridge_result = match self.bridge.write_blocks(&writes, &dimension).await {
            Ok(result) => {
                progress.submitted(result.submitted_changes);
                crate::performance::execution_progress(progress);
                result
            }
            Err(error) => return progress.submission_error(error, previous_world).response(),
        };
        progress.phase = FailurePhase::AfterReadback;
        crate::performance::execution_progress(progress);
        let (verified, verification_mismatches) = match self
            .world_editor()
            .verify_placement_changes_eventually(source, &dimension)
            .await
        {
            Ok(result) => result,
            Err(error) => return progress.cause(error).response(),
        };
        progress.phase = FailurePhase::Verification;
        crate::performance::execution_progress(progress);
        if !verified {
            let mut response = progress
                .cause(FailureCause::mismatch(
                    "placement verification failed",
                    &verification_mismatches,
                ))
                .response();
            response["mismatches"] = json!(verification_mismatches);
            response["bridge"] = json!(bridge_result);
            return response;
        }
        // The full revision boundary must also match before declaring a verified target.
        if let Err(error) = self
            .check_revision_placement(operation_id, undo, true, false)
            .await
        {
            return progress.cause(error).response();
        }
        progress.world = WorldOutcome::Verified;
        progress.verified_steps = source.len();
        self.plans
            .placements()
            .lock()
            .await
            .set_applied(&operation_id, !undo);
        if let Some(stored) = self.plans.placements().lock().await.get_mut(&operation_id) {
            stored.previewed = false;
        }
        json!({
            "schema_version": PLACEMENT_SCHEMA_V1,
            "execution_progress":progress,
            "ok": true,
            "operation_id": operation_id,
            "action": if undo { "undo" } else { "apply" },
            "changed_blocks": source.len(),
            "verified": verified,
            "dimension": dimension,
            "bridge": bridge_result,
        })
    }

    async fn check_revision_placement(
        &self,
        id: uuid::Uuid,
        undo: bool,
        after: bool,
        consume: bool,
    ) -> Result<(), FailureCause> {
        let Some(context) = self.plans.placements().lock().await.revision(&id).cloned() else {
            return Ok(());
        };
        let player = self.actor.clone()?;
        self.policy
            .authorize_player(&player)
            .map_err(FailureCause::from)?;
        if context.player != player {
            return Err("revision placement belongs to another player".into());
        }
        if !after
            && (!context.lifecycle.can_begin(undo)
                || (!undo && context.expires_at <= Instant::now()))
        {
            return Err(
                "revision placement attempt expired or consumed; inspect before recovery".into(),
            );
        }
        let expected = if undo == after {
            &context.before
        } else {
            &context.after
        };
        self.policy
            .validate_region(dustroute_translate::world_reverse::RegionBounds::new(
                expected.min,
                expected.max,
            ))
            .map_err(FailureCause::from)?;
        let status = self.bridge.status().await.map_err(FailureCause::from)?;
        if !status.connected
            || status.version != context.version
            || status.dimension.as_deref() != Some(context.dimension.as_str())
        {
            return Err("server version or dimension changed".into());
        }
        let actual = self
            .bridge
            .scan_region(expected.min, expected.max, &context.dimension)
            .await
            .map_err(FailureCause::from)?;
        if actual.min != expected.min
            || actual.max != expected.max
            || crate::revision::blocks(&actual)? != crate::revision::blocks(expected)?
        {
            return Err(
                "revision placement context changed or result differs; inspect current world"
                    .into(),
            );
        }
        if consume {
            let mut contexts = self.plans.placements().lock().await;
            let stored = contexts
                .revision_mut(&id)
                .ok_or("revision placement missing")?;
            if !undo && stored.expires_at <= Instant::now() {
                return Err("revision placement expired during scan".into());
            }
            stored.lifecycle.begin(undo)?;
        }
        if after {
            if let Some(stored) = self.plans.placements().lock().await.revision_mut(&id) {
                stored.lifecycle.confirm(undo);
            }
        }
        Ok(())
    }
}
