//! Settled idle boundaries, not recovery of lost native operation handles.
use super::diagnostic::{CheckpointSchema, RecordedEndpoint};
use super::*;
use crate::survival_construction::PlacementPurpose;
use crate::survival_construction::{
    ConstructionScope, TemporaryBlock, continuation::scene_snapshot,
};
use dustroute_translate::snapshot::MinecraftSnapshot;
use fs2::FileExt;
use validation::checked;

mod validation;
use serde::Deserialize;
use std::fs::{File, OpenOptions};

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SafeCheckpoint {
    pub schema: CheckpointSchema,
    pub execution_id: uuid::Uuid,
    pub completed_steps: usize,
    pub dimension: String,
    pub scope: ConstructionScope,
    pub snapshot: MinecraftSnapshot,
    pub temporary: Vec<TemporaryBlock>,
    pub endpoint: RecordedEndpoint,
    /// Diagnostic provenance only; never used to restore native standing authority.
    pub standing: Box<voxrig::checked_survival::diagnostic::RecordedStandingContext>,
}
pub(crate) fn endpoint(config: &ConnectionConfig) -> RecordedEndpoint {
    config.into()
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
                SurvivalErrorCode::CheckpointNotIdle,
                "outstanding or interrupted operation; no idle checkpoint",
            ));
        }
        self.call_active = true;
        let result = self.checkpoint_inner().await;
        if let Err(error) = &result {
            let _ = self.journal.event(
                ExecutionPhase::CheckpointRefused,
                self.journal.record.outcome.clone(),
                Continuation::NeedsInspection,
                ExecutionEvidence::CheckpointRefused {
                    error: error.clone(),
                },
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
                SurvivalErrorCode::CheckpointNativeUnsettled,
                "native history retains unresolved effects",
            ));
        }
        native::received_materials(&self.bot.survival()?.player_state().await?)?;
        // A second native capture rechecks current received geometry and standing.
        // Prediction remains prediction; this is not independent corroboration.
        let fresh = self.current_scene().await?;
        let checkpoint = SafeCheckpoint {
            schema: CheckpointSchema::V2,
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
            standing: Box::new(fresh.source().into()),
        };
        self.journal.event(
            ExecutionPhase::IdleCheckpoint,
            OperationOutcome::Observed,
            Continuation::Checkpoint,
            checkpoint.clone(),
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
        .map_err(|e| ExecutionError::new(SurvivalErrorCode::CheckpointWriterLive, e))?;
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
    if &now != expected {
        return Err(ExecutionError::new(
            SurvivalErrorCode::CheckpointChanged,
            "checkpoint differs from preview",
        ));
    }
    crate::storage::replace(
        &directory.join("continuation-claim.json"),
        &serde_json::to_vec(&super::diagnostic::CheckpointClaim {
            new_job,
            execution_id: now.execution_id,
            automatic_replay: super::diagnostic::DiagnosticOnly,
        })?,
        crate::storage::Durability::FileAndDirectory,
    )?;
    Ok(lock)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::survival_execution::diagnostic::{RecordedConstructionPlan, RecordedStep};
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
        let native_endpoint = endpoint(&voxrig::ConnectionConfig::offline(
            voxrig::Server::new("127.0.0.1", 25572),
            "NatMineBot",
            voxrig::MinecraftVersion::Java1_21_11,
        ));
        let mut journal = Journal::create(
            &d.0,
            RecordedExecutionPlan {
                plan: Some(RecordedConstructionPlan {
                    scope: scope.clone(),
                    initial_temporary: vec![],
                    steps: vec![],
                    motion_contract: None,
                    source: None,
                    baseline: None,
                    expected: None,
                    materials: None,
                    final_position: None,
                }),
                capabilities: None,
                declared_reconnect: Some(native_endpoint.clone()),
            },
        )
        .unwrap();
        let checkpoint = SafeCheckpoint {
            schema: CheckpointSchema::V2,
            execution_id: journal.record.id,
            completed_steps: 0,
            dimension: "minecraft:overworld".into(),
            scope,
            snapshot: crate::survival_construction::tests::design().baseline,
            temporary: vec![],
            endpoint: native_endpoint,
            standing: Box::new(
                voxrig::checked_survival::diagnostic::RecordedStandingContext {
                    connection_id: 1,
                    receive_sequence: 1,
                    client_tick: 0,
                    world_revision: 0,
                    dimension: "minecraft:overworld".into(),
                    position_basis:
                        voxrig::checked_survival::diagnostic::StandingPositionBasis::Received {
                            receive_sequence: 1,
                        },
                    position: [0.5, 0.0, 0.5],
                    eye_position: [0.5, 1.62, 0.5],
                    bounds: [0.2, 0.0, 0.2, 0.8, 1.8, 0.8],
                    on_ground: true,
                    support: vec![[0, -1, 0]],
                    submerged: false,
                    player: voxrig::checked_survival::diagnostic::LocalPlayerState::default(),
                },
            ),
        };
        journal
            .event(
                ExecutionPhase::IdleCheckpoint,
                OperationOutcome::Observed,
                Continuation::Checkpoint,
                checkpoint.clone(),
            )
            .unwrap();
        (d, journal, checkpoint)
    }
    #[test]
    fn only_settled_last_event_and_released_writer_can_supply_new_plan_facts() {
        let (d, mut journal, checkpoint) = fixture();
        assert_eq!(
            read(&d.0).unwrap_err().code,
            SurvivalErrorCode::CheckpointWriterLive
        );
        journal
            .intend(
                ExecutionPhase::MiningStartSend,
                super::evidence::mining_start_fixture(),
            )
            .unwrap();
        drop(journal);
        assert_eq!(
            read(&d.0).unwrap_err().code,
            SurvivalErrorCode::SafeCheckpointMissing
        );
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
        assert_eq!(
            read(&d.0).unwrap_err().code,
            SurvivalErrorCode::CheckpointConsumed
        );
        assert_eq!(
            diagnose(&d.0).unwrap().record.events.last().unwrap().phase,
            ExecutionPhase::IdleCheckpoint
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
                ExecutionPhase::IdleCheckpoint,
                OperationOutcome::Observed,
                Continuation::Checkpoint,
                checkpoint.clone(),
            )
            .unwrap();
        drop(journal);
        assert_eq!(
            read(&d.0).unwrap_err().code,
            SurvivalErrorCode::InvalidCheckpoint
        );
    }
    #[test]
    fn persisted_motion_prediction_cannot_certify_a_world_edit() {
        for (step, outcome, accepted) in [
            (
                RecordedStep::Move { prediction: None },
                OperationOutcome::Predicted,
                true,
            ),
            (
                RecordedStep::Move { prediction: None },
                OperationOutcome::Observed,
                false,
            ),
            (
                RecordedStep::Place {
                    purpose: PlacementPurpose::Permanent,
                    placement: None,
                },
                OperationOutcome::Predicted,
                false,
            ),
        ] {
            let (d, mut journal, mut checkpoint) = fixture();
            journal.record.plan.plan.as_mut().unwrap().steps = vec![step];
            journal.record.completed_steps = 1;
            checkpoint.completed_steps = 1;
            journal
                .event(
                    ExecutionPhase::StepCompleted,
                    outcome,
                    Continuation::Revalidate,
                    ExecutionEvidence::StepCompleted {
                        remaining_owned_temporary: vec![],
                    },
                )
                .unwrap();
            journal
                .event(
                    ExecutionPhase::IdleCheckpoint,
                    OperationOutcome::Observed,
                    Continuation::Checkpoint,
                    checkpoint.clone(),
                )
                .unwrap();
            drop(journal);
            let result = read(&d.0);
            if accepted {
                result.unwrap();
            } else {
                assert_eq!(
                    result.unwrap_err().code,
                    SurvivalErrorCode::InvalidCheckpoint
                );
            }
        }
    }

    #[test]
    fn unknown_step_and_missing_temporary_edit_never_create_ownership() {
        for step in [
            RecordedStep::Unknown,
            RecordedStep::Place {
                purpose: PlacementPurpose::Temporary,
                placement: None,
            },
        ] {
            let (d, mut journal, mut checkpoint) = fixture();
            journal.record.plan.plan.as_mut().unwrap().steps = vec![step];
            journal.record.completed_steps = 1;
            checkpoint.completed_steps = 1;
            journal
                .event(
                    ExecutionPhase::StepCompleted,
                    OperationOutcome::Observed,
                    Continuation::Revalidate,
                    ExecutionEvidence::StepCompleted {
                        remaining_owned_temporary: vec![],
                    },
                )
                .unwrap();
            journal
                .event(
                    ExecutionPhase::IdleCheckpoint,
                    OperationOutcome::Observed,
                    Continuation::Checkpoint,
                    checkpoint,
                )
                .unwrap();
            drop(journal);
            assert_eq!(
                read(&d.0).unwrap_err().code,
                SurvivalErrorCode::InvalidCheckpoint
            );
        }
    }
}
