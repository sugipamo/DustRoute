//! One command-placement attempt, its context check and independent readback.
//! Player resolution and MCP dispatch have already happened at the facade.
use super::*;
pub(super) struct PlacementWorkflow<'a> {
    pub bridge: &'a BotBridge,
    pub policy: &'a McpPolicy,
    pub plans: &'a OperationPlans,
    pub operations: &'a OperationRegistry,
    pub mutation_lock: &'a Mutex<()>,
    pub actor: Result<String, String>,
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
        if !params.confirm {
            return json!({
                "ok": false,
                "error": "confirm must be true because this operation changes the world"
            });
        }
        if let Err(error) = self.policy.authorize_mutation() {
            return json!({ "ok": false, "error": error.to_string() });
        }
        let _mutation_guard = self.mutation_lock.lock().await;
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
            return json!({ "ok": false, "error": error.to_string() });
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
            return json!({ "ok": false, "error": error.to_string() });
        }
        let (baseline_matches, baseline_mismatches) = match self
            .world_editor()
            .verify_placement_changes(source, &dimension, false)
            .await
        {
            Ok(result) => result,
            Err(error) => return json!({ "ok": false, "error": error }),
        };
        if !baseline_matches {
            if let Some(stored) = self.plans.placements().lock().await.get_mut(&operation_id) {
                stored.previewed = false;
            }
            return json!({
                "ok": false,
                "error": "placement baseline is stale; preview again before changing the world",
                "mismatches": baseline_mismatches
            });
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
                Err(error) => return json!({ "ok": false, "error": error }),
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
            return json!({"ok":false,"error":error});
        }
        if let Err(error) = self
            .plans
            .placements()
            .lock()
            .await
            .begin(&operation_id, undo)
        {
            return workflow_error(McpErrorCode::InvalidState, error, false);
        }
        let bridge_result = match self.bridge.write_blocks(&writes, &dimension).await {
            Ok(result) => result,
            Err(error) => return json!({ "ok": false, "error": error.to_string() }),
        };
        let (verified, verification_mismatches) = match self
            .world_editor()
            .verify_placement_changes_eventually(source, &dimension)
            .await
        {
            Ok(result) => result,
            Err(error) => return json!({ "ok": false, "error": error }),
        };
        if !verified {
            return json!({
                "ok": false,
                "error": format!("placement write completed but live verification failed at {:?}", verification_mismatches),
                "mismatches": verification_mismatches,
                "bridge": bridge_result
            });
        }
        if let Err(error) = self
            .check_revision_placement(operation_id, undo, true, false)
            .await
        {
            return json!({"ok":false,"error":error,"status":"needs_inspection","retry_allowed":false});
        }
        self.plans
            .placements()
            .lock()
            .await
            .set_applied(&operation_id, !undo);
        if let Some(stored) = self.plans.placements().lock().await.get_mut(&operation_id) {
            stored.previewed = false;
        }
        self.operations
            .record_completed(
                uuid::Uuid::new_v4(),
                if undo {
                    OperationKind::PlacementUndo
                } else {
                    OperationKind::PlacementApply
                },
                json!({
                    "source_operation_id": operation_id,
                    "changed_blocks": source.len(),
                    "dimension": dimension,
                }),
            )
            .await;
        json!({
            "schema_version": PLACEMENT_SCHEMA_V1,
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
    ) -> Result<(), String> {
        let Some(context) = self.plans.placements().lock().await.revision(&id).cloned() else {
            return Ok(());
        };
        let player = self.actor.clone()?;
        self.policy
            .authorize_player(&player)
            .map_err(|e| e.to_string())?;
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
            .map_err(|e| e.to_string())?;
        let status = self.bridge.status().await.map_err(|e| e.to_string())?;
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
            .map_err(|e| e.to_string())?;
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
