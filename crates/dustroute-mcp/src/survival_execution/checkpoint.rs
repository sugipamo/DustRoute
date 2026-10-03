//! Settled idle boundaries, not recovery of lost native operation handles.
use super::*;
use crate::survival_construction::{
    ConstructionScope, TemporaryBlock, continuation::scene_snapshot,
};
use dustroute_translate::snapshot::MinecraftSnapshot;
use fs2::FileExt;
use validation::checked;

mod validation;
use serde::Deserialize;
use std::fs::{File, OpenOptions};

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct SafeCheckpoint {
    pub schema: String,
    pub execution_id: uuid::Uuid,
    pub completed_steps: usize,
    pub dimension: String,
    pub scope: ConstructionScope,
    pub snapshot: MinecraftSnapshot,
    pub temporary: Vec<TemporaryBlock>,
    pub endpoint: serde_json::Value,
    /// Diagnostic provenance only; never used to restore native standing authority.
    pub standing: serde_json::Value,
}
pub(crate) fn endpoint(config: &ConnectionConfig) -> serde_json::Value {
    json!({"host":config.server.host,"port":config.server.port,"username":config.username,"version":config.version})
}

impl SurvivalExecutor {
    pub(crate) fn pending_mining(&self) -> bool {
        self.pending.is_some()
    }

    /// Only a settled caller-owned executor can checkpoint. A request made during
    /// mining must first collect the normal result and native recovery boundary.
    pub(crate) async fn checkpoint(&mut self) -> Result<SafeCheckpoint> {
        if self.call_active
            || self.pending.is_some()
            || !matches!(
                self.record().continuation,
                Continuation::Revalidate | Continuation::Replan
            )
        {
            return Err(ExecutionError::new(
                "checkpoint_not_idle",
                "outstanding or interrupted operation; no idle checkpoint",
            ));
        }
        self.call_active = true;
        let result = self.checkpoint_inner().await;
        if let Err(error) = &result {
            let _ = self.journal.event(
                "checkpoint_refused",
                self.journal.record.outcome.clone(),
                Continuation::NeedsInspection,
                json!({"error":error}),
            );
        }
        // Success also seals this executor. All continuation uses a fresh plan.
        result
    }
    async fn checkpoint_inner(&mut self) -> Result<SafeCheckpoint> {
        self.current_scene().await?;
        let history = self.bot.survival()?.operation_history().await;
        if history.connection_closed
            || history.interrupted_packet_id.is_some()
            || history.receive_failure.is_some()
            || history.pending_inventory_swap.is_some()
            || !history.pending_creative_slots.is_empty()
            || history.mining.is_some()
            || history.placement.as_ref().is_some_and(|p| {
                !p.dispatched || p.observation.is_none() || p.requires_inspection.is_some()
            })
            || history
                .survival_motion
                .as_ref()
                .is_some_and(|m| !m.status.is_continuation_candidate())
            || history
                .selected_hotbar
                .as_ref()
                .is_some_and(|s| !s.dispatched)
        {
            return Err(ExecutionError::new(
                "checkpoint_native_unsettled",
                "native history retains unresolved effects",
            ));
        }
        native::received_materials(&self.bot.survival()?.player_state().await?)?;
        // A second native capture rechecks current received geometry and standing.
        // Prediction remains prediction; this is not independent corroboration.
        let fresh = self.current_scene().await?;
        let checkpoint = SafeCheckpoint {
            schema: "dustroute.survival-checkpoint.v2".into(),
            execution_id: self.record().id,
            completed_steps: self.record().completed_steps,
            dimension: fresh.source().dimension.clone(),
            scope: self.plan.scope().clone(),
            snapshot: scene_snapshot(&fresh).map_err(|e| ExecutionError::new(e.code, e.detail))?,
            temporary: self
                .temporary
                .iter()
                .map(|(&position, state)| TemporaryBlock {
                    position,
                    state: state.clone(),
                })
                .collect(),
            endpoint: endpoint(&self.reconnect),
            standing: json!(fresh.source()),
        };
        self.journal.event(
            "idle_checkpoint",
            OperationOutcome::Observed,
            Continuation::Checkpoint,
            json!(checkpoint),
        )?;
        Ok(checkpoint)
    }
}

/// Read under the old executor lock. A live writer, missing checkpoint, later
/// intent or already-consumed boundary cannot become a continuation source.
pub(crate) fn read(directory: &Path) -> Result<SafeCheckpoint> {
    let _lock = lock(directory)?;
    checked(directory)
}
fn lock(directory: &Path) -> Result<File> {
    let lock = OpenOptions::new()
        .read(true)
        .write(true)
        .open(directory.join("executor.lock"))?;
    lock.try_lock_exclusive()
        .map_err(|e| ExecutionError::new("checkpoint_writer_live", e))?;
    Ok(lock)
}

/// The durable claim is single-use even after controller loss. On admission
/// failure it remains a diagnostic stop, never permission to replay implicitly.
pub(crate) fn claim(
    directory: &Path,
    expected: &SafeCheckpoint,
    new_job: uuid::Uuid,
) -> Result<File> {
    let lock = lock(directory)?;
    let now = checked(directory)?;
    if json!(now) != json!(expected) {
        return Err(ExecutionError::new(
            "checkpoint_changed",
            "checkpoint differs from preview",
        ));
    }
    crate::storage::replace(
        &directory.join("continuation-claim.json"),
        &serde_json::to_vec(
            &json!({"new_job":new_job,"execution_id":now.execution_id,"automatic_replay":false}),
        )?,
        crate::storage::Durability::FileAndDirectory,
    )?;
    Ok(lock)
}

#[cfg(test)]
mod tests {
    use super::*;
    struct Directory(std::path::PathBuf);
    impl Drop for Directory {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }
    fn fixture() -> (Directory, Journal, SafeCheckpoint) {
        let d = Directory(
            std::env::temp_dir().join(format!("dustroute-checkpoint-{}", uuid::Uuid::new_v4())),
        );
        let scope = crate::survival_construction::tests::scope();
        let native_endpoint = json!({"host":"127.0.0.1","port":25572,"username":"NatMineBot","version":"Java1_21_11"});
        let mut journal = Journal::create(
            &d.0,
            json!({"plan":{"scope":scope,"initial_temporary":[],"steps":[]},"declared_reconnect":native_endpoint}),
        )
        .unwrap();
        let checkpoint = SafeCheckpoint {
            schema: "dustroute.survival-checkpoint.v2".into(),
            execution_id: journal.record.id,
            completed_steps: 0,
            dimension: "minecraft:overworld".into(),
            scope,
            snapshot: crate::survival_construction::tests::design().baseline,
            temporary: vec![],
            endpoint: native_endpoint,
            standing: json!({"position_basis":{"kind":"received","receive_sequence":1}}),
        };
        journal
            .event(
                "idle_checkpoint",
                OperationOutcome::Observed,
                Continuation::Checkpoint,
                json!(checkpoint),
            )
            .unwrap();
        (d, journal, checkpoint)
    }
    #[test]
    fn only_settled_last_event_and_released_writer_can_supply_new_plan_facts() {
        let (d, mut journal, checkpoint) = fixture();
        assert_eq!(read(&d.0).unwrap_err().code, "checkpoint_writer_live");
        journal
            .intend("mining_start_send", json!({"target":[1,2,3]}))
            .unwrap();
        drop(journal);
        assert_eq!(read(&d.0).unwrap_err().code, "safe_checkpoint_missing");
        let (d, journal, _) = fixture();
        drop(journal);
        assert_eq!(read(&d.0).unwrap().schema, checkpoint.schema);
        assert_eq!(
            diagnose(&d.0).unwrap().continuation,
            Continuation::NeedsInspection
        );
    }
    #[test]
    fn claim_survives_process_local_handle_loss_and_refuses_other_branches() {
        let (d, journal, checkpoint) = fixture();
        drop(journal);
        let child = uuid::Uuid::new_v4();
        let guard = claim(&d.0, &checkpoint, child).unwrap();
        assert!(claim(&d.0, &checkpoint, uuid::Uuid::new_v4()).is_err());
        drop(guard);
        assert_eq!(read(&d.0).unwrap_err().code, "checkpoint_consumed");
        assert_eq!(
            diagnose(&d.0).unwrap().record.events.last().unwrap().phase,
            "idle_checkpoint"
        );
    }
    #[test]
    fn ownership_is_derived_from_confirmed_prefix_not_a_saved_claim() {
        let (d, mut journal, mut checkpoint) = fixture();
        checkpoint.temporary.push(TemporaryBlock {
            position: [-3, 0, 2],
            state: NativeBlockState {
                name: "minecraft:dirt".into(),
                properties: Default::default(),
            },
        });
        journal
            .event(
                "idle_checkpoint",
                OperationOutcome::Observed,
                Continuation::Checkpoint,
                json!(checkpoint),
            )
            .unwrap();
        drop(journal);
        assert_eq!(read(&d.0).unwrap_err().code, "invalid_checkpoint");
    }
    #[test]
    fn persisted_motion_prediction_cannot_certify_a_world_edit() {
        for (kind, outcome, accepted) in [
            ("move", OperationOutcome::Predicted, true),
            ("move", OperationOutcome::Observed, false),
            ("place", OperationOutcome::Predicted, false),
        ] {
            let (d, mut journal, mut checkpoint) = fixture();
            journal.record.plan["plan"]["steps"] = json!([{"kind":kind,"purpose":"permanent"}]);
            journal.record.completed_steps = 1;
            checkpoint.completed_steps = 1;
            journal
                .event(
                    "step_completed",
                    outcome,
                    Continuation::Revalidate,
                    json!({}),
                )
                .unwrap();
            journal
                .event(
                    "idle_checkpoint",
                    OperationOutcome::Observed,
                    Continuation::Checkpoint,
                    json!(checkpoint),
                )
                .unwrap();
            drop(journal);
            let result = read(&d.0);
            if accepted {
                result.unwrap();
            } else {
                assert_eq!(result.unwrap_err().code, "invalid_checkpoint");
            }
        }
    }
}
