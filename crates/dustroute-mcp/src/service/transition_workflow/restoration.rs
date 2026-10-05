//! Restores an attempted scenario and independently verifies the saved snapshot.
//! Natural restoration is tried before the existing command-write fallback.
use super::{StoredTransitionPlan, TransitionSession};
use crate::OperationKind;
use crate::api::McpErrorCode;
use crate::bridge_protocol::{CommandSubmission, CommandWrite};
use crate::failure::{
    CauseKind, ExecutionProgress, FailureCause, FailurePhase, FailureReport, WorldOutcome,
};
use crate::operations::transition::{
    TransitionOutcome, TransitionRefusal, TransitionRestoreResult,
};
use crate::service::observed_java_block_state;
use uuid::Uuid;

impl TransitionSession<'_> {
    pub async fn restore(
        &self,
        operation_id: Uuid,
    ) -> Result<TransitionRestoreResult, TransitionRefusal> {
        let plan = self.plan(operation_id).await?;
        if !plan.lifecycle.attempted() {
            return Err(TransitionRefusal::coded(
                McpErrorCode::InvalidState,
                "scenario has not been attempted",
            ));
        }
        let current = self
            .bridge
            .get_block(plan.lever, &plan.dimension)
            .await
            .map_err(FailureCause::from)?;
        let powered = current
            .state
            .properties
            .get("powered")
            .and_then(|value| value.parse::<bool>().ok());
        let mut progress = ExecutionProgress {
            operation_consumed: true,
            ..Default::default()
        };
        progress.phase = FailurePhase::Restore;
        let mut failure = None;
        let activation_error = if powered != Some(plan.original_powered) {
            progress.begin_submission();
            match self
                .bridge
                .activate_lever(plan.lever, &plan.dimension)
                .await
            {
                Ok(_) => {
                    progress.submitted(1);
                    None
                }
                Err(error) => {
                    progress.submitted_changes = None;
                    FailureReport::append(
                        &progress,
                        &mut failure,
                        error.cause().at(FailurePhase::Restore),
                    );
                    Some(error.to_string())
                }
            }
        } else {
            None
        };
        progress.phase = FailurePhase::Restore;
        if let Err(error) = self
            .bridge
            .wait_ticks(plan.observation_ticks, &plan.dimension)
            .await
        {
            FailureReport::append(
                &progress,
                &mut failure,
                error.cause().at(FailurePhase::Restore),
            );
        }
        let block = self.bridge.get_block(plan.lever, &plan.dimension).await;
        let snapshot = self
            .bridge
            .scan_region(plan.bounds.min, plan.bounds.max, &plan.dimension)
            .await;
        let lever_restored = block.as_ref().is_ok_and(|block| {
            block.state.name == "minecraft:lever"
                && block
                    .state
                    .properties
                    .get("powered")
                    .and_then(|v| v.parse::<bool>().ok())
                    == Some(plan.original_powered)
        });
        let region_restored = snapshot
            .as_ref()
            .is_ok_and(|snapshot| snapshot == &plan.initial_snapshot);
        for error in [block.as_ref().err(), snapshot.as_ref().err()]
            .into_iter()
            .flatten()
        {
            FailureReport::append(
                &progress,
                &mut failure,
                error.cause().at(FailurePhase::Restore),
            );
        }
        let naturally_verified = activation_error.is_none() && lever_restored && region_restored;
        let mut forced_restore = None;
        let verified = if naturally_verified {
            true
        } else {
            let snapshot_restore = self
                .restore_snapshot(&plan, &mut progress, &mut failure)
                .await;
            forced_restore = Some(snapshot_restore.submission);
            snapshot_restore.verified
        };
        if verified {
            progress.world = WorldOutcome::Verified;
            progress.verified_steps = 1;
        }
        if let Some(stored) = self
            .plans
            .table::<StoredTransitionPlan>()
            .lock()
            .await
            .get_mut(&operation_id)
        {
            stored.lifecycle.confirm(verified);
        }
        let result = TransitionRestoreResult::completed(
            operation_id,
            verified,
            activation_error,
            naturally_verified,
            forced_restore,
            TransitionOutcome::from_attempt(progress.clone(), failure),
        );
        crate::performance::execution_progress(&progress);
        self.operations
            .record_completed(
                operation_id,
                OperationKind::TransitionRestore,
                result.clone().into(),
            )
            .await;
        Ok(result)
    }

    /// Submission evidence and final snapshot verification remain separate facts.
    async fn restore_snapshot(
        &self,
        plan: &StoredTransitionPlan,
        progress: &mut ExecutionProgress,
        failure: &mut Option<FailureReport>,
    ) -> SnapshotRestoration {
        let writes = plan
            .initial_snapshot
            .blocks
            .iter()
            .map(|block| {
                observed_java_block_state(block)
                    .parse()
                    .map(|state| CommandWrite {
                        pos: block.pos,
                        state,
                    })
            })
            .collect::<Result<Vec<_>, _>>();
        let submission = match self
            .policy
            .validate_placement_size(plan.initial_snapshot.blocks.len())
        {
            Ok(()) => match writes {
                Ok(writes) => {
                    let previous = progress.world;
                    progress.begin_submission();
                    match self.bridge.write_blocks(&writes, &plan.dimension).await {
                        Ok(receipt) => {
                            progress.submitted(receipt.submitted_changes);
                            Ok(receipt)
                        }
                        Err(error) => {
                            let report = progress.submission_error(error, previous);
                            FailureReport::append(
                                progress,
                                failure,
                                report.primary.clone().at(FailurePhase::Restore),
                            );
                            Err(report.to_string())
                        }
                    }
                }
                Err(error) => {
                    FailureReport::append(
                        progress,
                        failure,
                        FailureCause::new(CauseKind::InvalidInput, error.clone())
                            .at(FailurePhase::Restore),
                    );
                    Err(error)
                }
            },
            Err(error) => {
                let cause = FailureCause::from(error);
                let message = cause.to_string();
                FailureReport::append(progress, failure, cause.at(FailurePhase::Restore));
                Err(message)
            }
        };
        progress.phase = FailurePhase::Restore;
        if let Err(error) = self
            .bridge
            .wait_ticks(plan.observation_ticks, &plan.dimension)
            .await
        {
            FailureReport::append(progress, failure, error.cause().at(FailurePhase::Restore));
        }
        let verified = match self
            .bridge
            .scan_region(plan.bounds.min, plan.bounds.max, &plan.dimension)
            .await
        {
            Ok(snapshot) => {
                let matches = snapshot == plan.initial_snapshot;
                if !matches {
                    FailureReport::append(
                        progress,
                        failure,
                        FailureCause::new(
                            CauseKind::VerificationMismatch,
                            "snapshot restoration differs from original region",
                        )
                        .at(FailurePhase::Restore),
                    );
                }
                matches
            }
            Err(error) => {
                FailureReport::append(progress, failure, error.cause().at(FailurePhase::Restore));
                false
            }
        };
        SnapshotRestoration {
            verified,
            submission,
        }
    }
}

struct SnapshotRestoration {
    submission: Result<CommandSubmission, String>,
    verified: bool,
}
