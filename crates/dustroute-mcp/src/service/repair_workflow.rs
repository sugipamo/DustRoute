//! Repair application workflow. Stores and world effects have explicit owners;
//! routing, circuit capture and Blueprint admission are outside this service.
use super::{
    StoredRepairPlan, requests::ConfirmedOperationParams, workflow_error,
    world_editor::WorldEditor, world_from_snapshot_for_service,
};
use crate::api::{McpErrorCode, REPAIR_SCHEMA_V1};
use crate::state::{PlanRecordKind, PlanStateStore};
use crate::{BlockChange, BotBridge, McpPolicy, OperationKind, OperationRegistry};
use dustroute_app::DustRouteService;
use dustroute_translate::api::ReverseRequest;
use serde_json::{Value, json};
use tokio::sync::Mutex;

pub(super) struct RepairWorkflow<'a> {
    pub bridge: &'a BotBridge,
    pub policy: &'a McpPolicy,
    pub state_store: &'a PlanStateStore,
    pub app: &'a DustRouteService,
    pub operations: &'a OperationRegistry,
    pub mutation_lock: &'a Mutex<()>,
}
/// The expendable repair archive contract is shared by proposal, preview and
/// mutation. It is read from disk each time so expiry cannot resurrect in RAM.
pub(super) struct RepairPlans<'a>(pub &'a PlanStateStore);
impl RepairPlans<'_> {
    pub fn save(&self, id: uuid::Uuid, plan: &StoredRepairPlan) -> Result<(), String> {
        self.0.save(PlanRecordKind::Repairs, id, plan)
    }
    pub fn load(&self, id: uuid::Uuid) -> Result<Option<StoredRepairPlan>, String> {
        self.0.load(PlanRecordKind::Repairs, id).map_err(|e| format!("repair record is unavailable or uses a retired format; diagnose and create a new plan: {e}"))
    }
}

impl RepairWorkflow<'_> {
    fn world_editor(&self) -> WorldEditor<'_> {
        WorldEditor {
            bridge: self.bridge,
            policy: self.policy,
        }
    }
    async fn store_repair_plan(
        &self,
        id: uuid::Uuid,
        plan: StoredRepairPlan,
    ) -> Result<(), String> {
        RepairPlans(self.state_store).save(id, &plan)
    }
    async fn repair_plan(&self, id: uuid::Uuid) -> Result<Option<StoredRepairPlan>, String> {
        RepairPlans(self.state_store).load(id)
    }

    pub(super) async fn mutate_repair(
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
        let mut plan = match self.repair_plan(operation_id).await {
            Ok(Some(plan)) => plan,
            Ok(None) => {
                return json!({ "ok": false, "error": "unknown or expired repair ID" });
            }
            Err(error) => return json!({ "ok": false, "error": error }),
        };
        if !undo && !plan.contract_satisfied {
            return workflow_error(
                McpErrorCode::VerificationFailed,
                "the optimization preservation contract is not fully satisfied",
                false,
            );
        }
        if let Err(error) = plan.lifecycle.begin(undo, self.policy.preview_required) {
            return workflow_error(McpErrorCode::InvalidState, error, false);
        }
        let patch = if undo {
            plan.patch.inverse()
        } else {
            plan.patch.clone()
        };
        let bridge_result = if undo {
            if let Err(error) = self.store_repair_plan(operation_id, plan.clone()).await {
                return workflow_error(McpErrorCode::Internal, error, false);
            }
            self.world_editor()
                .write_physical_change_batch(&patch.changes, &plan.dimension)
                .await
        } else {
            let changes: Vec<_> = patch
                .changes
                .iter()
                .map(|c| BlockChange {
                    pos: c.pos,
                    before: c.before.clone(),
                    after: c.after.clone(),
                    collision: false,
                })
                .collect();
            let validated = match self
                .world_editor()
                .validate_live_changes(&changes, &plan.dimension)
                .await
            {
                Ok(value) => value,
                Err(error) => return json!({ "ok": false, "error": error }),
            };
            if let Err(error) = self.store_repair_plan(operation_id, plan.clone()).await {
                return workflow_error(McpErrorCode::Internal, error, false);
            }
            self.world_editor()
                .write_validated_physical_changes(&validated, &plan.dimension)
                .await
        };
        let bridge = match bridge_result {
            Ok(result) => result,
            Err(error) => return json!({ "ok": false, "error": error }),
        };
        let (verified, mismatches) = match self
            .world_editor()
            .verify_physical_changes(&patch.changes, &plan.dimension)
            .await
        {
            Ok(result) => result,
            Err(error) => return json!({ "ok": false, "error": error }),
        };
        let (boundary_verified, boundary_mismatches) = match self
            .world_editor()
            .verify_preserved_boundary(&plan.preserved_boundary, &plan.dimension)
            .await
        {
            Ok(result) => result,
            Err(error) => return json!({ "ok": false, "error": error }),
        };
        if (!verified || !boundary_verified) && !undo {
            let rollback = plan.patch.inverse();
            let rollback_result = self
                .world_editor()
                .write_physical_change_batch(&rollback.changes, &plan.dimension)
                .await;
            let rollback_verified = rollback_result.is_ok()
                && self
                    .world_editor()
                    .verify_physical_changes(&rollback.changes, &plan.dimension)
                    .await
                    .is_ok_and(|(verified, _)| verified)
                && self
                    .world_editor()
                    .verify_preserved_boundary(&plan.preserved_boundary, &plan.dimension)
                    .await
                    .is_ok_and(|(verified, _)| verified);
            if rollback_verified {
                let mut rolled_back = plan.clone();
                rolled_back.lifecycle.confirm(true);
                if let Err(error) = self.store_repair_plan(operation_id, rolled_back).await {
                    return workflow_error(
                        McpErrorCode::Internal,
                        format!("rollback verified but state could not be saved: {error}"),
                        false,
                    );
                }
            }
            return json!({
                "ok": false,
                "error": "repair verification failed; automatic rollback attempted",
                "mismatches": mismatches,
                "boundary_mismatches": boundary_mismatches,
                "rollback_submitted": rollback_result.is_ok(),
                "rollback_ok": rollback_verified,
            });
        }
        if !verified || !boundary_verified {
            return json!({"ok":false,"error":"repair undo requires inspection","verified":verified,"boundary_verified":boundary_verified,"mismatches":mismatches,"boundary_mismatches":boundary_mismatches});
        }
        let mut updated_plan = plan.clone();
        updated_plan.lifecycle.confirm(undo);
        if let Err(error) = self.store_repair_plan(operation_id, updated_plan).await {
            return json!({
                "ok": false,
                "error": format!("world changed but repair state could not be persisted: {error}"),
                "verification_ok": verified,
                "mismatches": mismatches
            });
        }
        let snapshot = match self
            .bridge
            .scan_region(
                plan.analysis_bounds.min,
                plan.analysis_bounds.max,
                &plan.dimension,
            )
            .await
        {
            Ok(snapshot) => snapshot,
            Err(error) => return json!({ "ok": false, "error": error.to_string() }),
        };
        let post_world = world_from_snapshot_for_service(&snapshot);
        let post_analysis = post_world.as_ref().ok().map(|world| {
            let request = if plan.baseline_truth_table.is_some() {
                ReverseRequest::new(plan.analysis_bounds).with_truth_table(8)
            } else {
                ReverseRequest::new(plan.analysis_bounds)
            };
            self.app.analyze_physical(world, request)
        });
        let fragments_after = post_analysis
            .as_ref()
            .map(|analysis| analysis.reverse.analysis.scene.fragments.len());
        let semantic_verification = match (
            plan.baseline_truth_table.as_ref(),
            post_analysis
                .as_ref()
                .and_then(|analysis| analysis.reverse.truth_table.as_ref()),
        ) {
            (Some(expected), Some(actual)) => {
                let comparison =
                    dustroute_translate::world_reverse::compare_truth_tables(expected, actual);
                json!({
                    "available": true,
                    "equivalent": comparison.comparable && comparison.fitness_penalty == 0,
                    "comparison": comparison
                })
            }
            _ => json!({
                "available": false,
                "reason": "the repair proposal did not include a baseline truth table; structural and block-state verification still ran"
            }),
        };
        let resulting_logic = post_analysis
            .as_ref()
            .map(|analysis| &analysis.logical_role);
        self.operations
            .record_completed(
                uuid::Uuid::new_v4(),
                if undo {
                    OperationKind::RepairUndo
                } else {
                    OperationKind::RepairApply
                },
                json!({ "source_operation_id": operation_id, "verified": verified }),
            )
            .await;
        json!({
            "schema_version": REPAIR_SCHEMA_V1,
            "ok": true,
            "operation_id": operation_id,
            "action": if undo { "undo" } else { "apply" },
            "verified": verified,
            "boundary_verified": boundary_verified,
            "boundary_mismatches": boundary_mismatches,
            "changed_blocks": patch.changes.len(),
            "fragments_before": plan.fragments_before,
            "fragments_after": fragments_after,
            "resulting_logic": resulting_logic,
            "semantic_verification": semantic_verification,
            "bridge": bridge,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::service::{DustRouteMcp, operation_lifecycle::RepairLifecycle};
    use dustroute_physical::{
        Block, BlockKind, PhysicalBlockChange, PhysicalPatch, PhysicalPatchReason, Pos,
    };
    use dustroute_translate::world_reverse::RegionBounds;
    use std::sync::{
        Arc,
        atomic::{AtomicUsize, Ordering},
    };
    use tokio::{
        io::{AsyncBufReadExt, AsyncWriteExt, BufReader},
        net::TcpListener,
    };

    #[tokio::test]
    async fn repair_intent_is_persisted_before_submission_and_lost_reply_cannot_be_replayed_after_restart()
     {
        let root = crate::service::test_support::temporary();
        let store = PlanStateStore::new(root.clone(), 3600);
        let id = uuid::Uuid::new_v4();
        let pos = Pos::new(0, 1, 0);
        let mut wire = Block::new(BlockKind::RedstoneWire);
        wire.support_offset = Some(Pos::new(0, -1, 0));
        RepairPlans(&store)
            .save(
                id,
                &StoredRepairPlan {
                    patch: PhysicalPatch {
                        reason: PhysicalPatchReason::ConnectMissingWire,
                        affected_fragments: vec![],
                        confidence_percent: 100,
                        explanation: "restore wire".into(),
                        changes: vec![PhysicalBlockChange {
                            pos,
                            before: Block::new(BlockKind::Air),
                            after: wire,
                        }],
                    },
                    dimension: "minecraft:overworld".into(),
                    analysis_bounds: RegionBounds::new(Pos::new(-1, 0, -1), Pos::new(1, 2, 1)),
                    fragments_before: 1,
                    baseline_truth_table: None,
                    lifecycle: RepairLifecycle::Previewed,
                    contract_satisfied: true,
                    preserved_boundary: vec![],
                },
            )
            .unwrap();
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap().to_string();
        let writes = Arc::new(AtomicUsize::new(0));
        let count = writes.clone();
        let saved = store.clone();
        let transport = tokio::spawn(async move {
            loop {
                let (mut stream, _) = listener.accept().await.unwrap();
                let mut line = String::new();
                BufReader::new(&mut stream)
                    .read_line(&mut line)
                    .await
                    .unwrap();
                let request: serde_json::Value = serde_json::from_str(&line).unwrap();
                let response = match request["method"].as_str().unwrap() {
                    "scan_region" => {
                        let snapshot = json!({"min":request["params"]["min"],"max":request["params"]["max"],"blocks":[{"pos":{"x":0,"y":0,"z":0},"name":"minecraft:stone","properties":{}}]});
                        json!({"id":request["id"],"result":crate::bridge::test_readback_response(&request,crate::bridge::test_scan_world(&request,snapshot))})
                    }
                    "submit_physical_batch" => {
                        assert_eq!(
                            RepairPlans(&saved).load(id).unwrap().unwrap().lifecycle,
                            RepairLifecycle::NeedsInspection
                        );
                        count.fetch_add(1, Ordering::SeqCst);
                        // A transport error makes effects uncertain; it is not an acknowledgement.
                        json!({"id":request["id"],"error":"reply unavailable after submission"})
                    }
                    method => panic!("unexpected bridge request {method}"),
                };
                stream
                    .write_all(format!("{response}\n").as_bytes())
                    .await
                    .unwrap();
            }
        });
        let mut service = DustRouteMcp::with_policy_and_player(
            address.clone(),
            McpPolicy {
                read_only: false,
                ..McpPolicy::default()
            },
            "builder",
        );
        service.state_store = store.clone();
        let result: serde_json::Value = serde_json::from_str(
            &service
                .mutate_repair(
                    ConfirmedOperationParams {
                        operation_id: id.to_string(),
                        confirm: true,
                    },
                    false,
                )
                .await,
        )
        .unwrap();
        assert_eq!(result["ok"], false, "{result}");
        assert_eq!(
            writes.load(Ordering::SeqCst),
            1,
            "the test must reach transport: {result}"
        );
        let mut restarted = DustRouteMcp::with_policy_and_player(
            address,
            McpPolicy {
                read_only: false,
                ..McpPolicy::default()
            },
            "builder",
        );
        restarted.state_store = store.clone();
        for undo in [false, true] {
            let result: serde_json::Value = serde_json::from_str(
                &restarted
                    .mutate_repair(
                        ConfirmedOperationParams {
                            operation_id: id.to_string(),
                            confirm: true,
                        },
                        undo,
                    )
                    .await,
            )
            .unwrap();
            assert_eq!(result["ok"], false, "{result}");
            assert_eq!(result["error_code"], "invalid_state", "{result}");
        }
        assert_eq!(writes.load(Ordering::SeqCst), 1);
        assert_eq!(
            RepairPlans(&store).load(id).unwrap().unwrap().lifecycle,
            RepairLifecycle::NeedsInspection
        );
        transport.abort();
        std::fs::remove_dir_all(root).unwrap();
    }
}
