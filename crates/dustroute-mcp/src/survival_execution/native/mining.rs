//! Split-phase owned temporary cleanup: collect outcome, recover source, reconcile.
use super::*;
use crate::survival_cleanup::{CleanupReconciliation, CleanupRecoveryPlan};
use voxrig::checked_survival::{MiningStatus, Operations, RecoveredSurvivalClient};

impl SurvivalExecutor {
    pub(in crate::survival_execution) async fn start_mining(
        &mut self,
        scene: &CapturedSurvivalScene,
        edit: HypotheticalBlockEdit,
        face_id: u8,
        rotation: [f32; 2],
    ) -> Result<ExecutionProgress> {
        let current = self.bot.survival()?;
        let fresh =
            scene
                .scenario()
                .preview_cube_removal(edit.position, face(face_id)?, rotation)?;
        if !same_edit(&fresh, &edit) {
            return Err(ExecutionError::new(
                SurvivalErrorCode::RemovalPlanChanged,
                "fresh removal differs from owned temporary edit",
            ));
        }
        let player = current.player_state().await?;
        let empty = choose_empty_hand(&player)?;
        let attempt = if let Some((previous, attempt)) = self.retry.take() {
            if previous != empty {
                return Err(ExecutionError::new(
                    SurvivalErrorCode::RetryPrerequisiteChanged,
                    "the selected improvement is no longer available",
                ));
            }
            attempt
        } else {
            1
        };
        self.journal.intend(
            ExecutionPhase::MiningSelectEmpty,
            ExecutionEvidence::MiningSelectEmpty {
                hotbar: empty,
                attempt,
                edit: (&edit).into(),
            },
        )?;
        current.select_hotbar(empty).await?;
        self.journal.intend(
            ExecutionPhase::MiningAim,
            ExecutionEvidence::MiningAim {
                rotation,
                edit: (&edit).into(),
            },
        )?;
        current.look(rotation).await?;
        self.journal.intend(
            ExecutionPhase::MiningStartSend,
            ExecutionEvidence::MiningStartSend {
                edit: (&edit).into(),
                face_id,
                attempt,
                hotbar: empty,
            },
        )?;
        let intent = current
            .start_survival_mining(edit.position, face(face_id)?)
            .await?;
        if intent.target != edit.position || intent.baseline != edit.before {
            return Err(ExecutionError::new(
                SurvivalErrorCode::MiningIntentChanged,
                "START differs from owned predecessor; no FINISH may follow",
            ));
        }
        self.journal.event(
            ExecutionPhase::MiningStarted,
            OperationOutcome::Pending,
            Continuation::AwaitMining,
            ExecutionEvidence::MiningStarted {
                intent: Box::new((&intent).into()),
                attempt,
                selected_from: Box::new((&player).into()),
            },
        )?;
        self.pending = Some(PendingMining {
            intent,
            edit: edit.clone(),
            slot: empty,
            attempt,
        });
        Ok(ExecutionProgress::MiningStarted {
            target: edit.position,
            selected_hotbar: empty,
            attempt,
        })
    }
    pub(in crate::survival_execution) async fn finish_mining(
        &mut self,
        pending: PendingMining,
    ) -> Result<ExecutionProgress> {
        let PendingMining {
            intent,
            edit,
            slot,
            attempt,
        } = pending;
        let current = self.bot.survival()?;
        let plan = self
            .collect_mining_outcome(&current, &intent, &edit, attempt)
            .await?;
        let recovered = self
            .recover_mining_connection(&current, &intent, &plan)
            .await?;
        self.bot = recovered.client;
        self.journal.record.reconnects += 1;
        let fresh = recovered
            .operations
            .capture_survival_scene(region(self.plan.scope().observed))
            .await?;
        let decision = plan.reconcile_fresh(&recovered.evidence, &fresh)?;
        let mut expected_site = self.expected.clone();
        if decision == CleanupReconciliation::AlreadyAbsent {
            expected_site.insert(edit.position, edit.after.clone());
        }
        if scene_blocks(&fresh)? != expected_site {
            return Err(ExecutionError::new(
                SurvivalErrorCode::RecoverySiteChanged,
                "fresh world differs outside the reconciled target",
            ));
        }
        self.journal.event(
            ExecutionPhase::Recovered,
            OperationOutcome::Observed,
            Continuation::NeedsInspection,
            ExecutionEvidence::Recovered {
                evidence: Box::new((&recovered.evidence).into()),
                decision,
                attempt,
            },
        )?;
        match decision {
            CleanupReconciliation::AlreadyAbsent => {
                let HypotheticalConstructionStep::RemoveTemporary { reconnect, .. } =
                    &self.plan.steps()[self.record().completed_steps]
                else {
                    return Err(ExecutionError::new(
                        SurvivalErrorCode::RecoveryPlanChanged,
                        "expected removal boundary",
                    ));
                };
                reconnect.validate_received_start(&fresh, intent.connection_id)?;
                self.journal.event(
                    ExecutionPhase::ReconnectBoundaryVerified,
                    OperationOutcome::Observed,
                    Continuation::NeedsInspection,
                    ExecutionEvidence::ReconnectBoundaryVerified {
                        requirement: reconnect.into(),
                        retired_connection_id: intent.connection_id,
                        received_start: Box::new(fresh.source().into()),
                    },
                )?;
                self.expected = expected_site;
                self.temporary.remove(&edit.position);
                self.complete_step(OperationOutcome::Observed)
            }
            CleanupReconciliation::NeedsNewPlan => {
                let player = recovered.operations.player_state().await?;
                let next = choose_empty_hand(&player)?;
                retry_improved(slot, next, attempt)?;
                self.retry = Some((next, attempt + 1));
                self.journal.event(
                    ExecutionPhase::RetryImprovement,
                    OperationOutcome::NotStarted,
                    Continuation::Replan,
                    ExecutionEvidence::RetryImprovement {
                        reason: RetryReason::DifferentReceivedEmptyHotbarAfterRetirement,
                        previous_hotbar: slot,
                        next_hotbar: next,
                        inventory_sequence: player.inventory.receive_sequence,
                        attempt: attempt + 1,
                        owned_target: (&edit).into(),
                    },
                )?;
                Ok(ExecutionProgress::Replanned {
                    previous_hotbar: slot,
                    next_hotbar: next,
                    attempt: attempt + 1,
                })
            }
        }
    }

    /// Record the existing attempt's outcome before retiring its source.
    async fn collect_mining_outcome(
        &mut self,
        current: &Operations,
        intent: &MiningIntent,
        edit: &HypotheticalBlockEdit,
        attempt: usize,
    ) -> Result<CleanupRecoveryPlan> {
        let status = current
            .wait_survival_mining(intent, Duration::from_millis(intent.estimated_wait_ms))
            .await?;
        if matches!(status, MiningStatus::Mining { .. }) {
            self.journal.intend(
                ExecutionPhase::MiningFinishSend,
                ExecutionEvidence::MiningFinishSend(Box::new(intent.into())),
            )?;
            if let Err(error) = current.finish_survival_mining(intent).await {
                if error.kind() != voxrig::ErrorKind::State
                    || !matches!(
                        current.observe_survival_mining(intent).await,
                        Ok(MiningStatus::RequiresInspection { .. })
                    )
                {
                    return Err(error.into());
                }
            }
            current.wait_survival_mining(intent, WAIT).await?;
        }
        let history = current.operation_history().await;
        let record = history.mining.as_ref().ok_or_else(|| {
            ExecutionError::new(
                SurvivalErrorCode::MiningHistoryMissing,
                "native history unavailable",
            )
        })?;
        let plan = CleanupRecoveryPlan::for_record(edit, record)?;
        let outcome = if record.removal.is_some() {
            OperationOutcome::Observed
        } else {
            OperationOutcome::Pending
        };
        self.journal.event(
            ExecutionPhase::MiningOutcome,
            outcome,
            Continuation::NeedsInspection,
            ExecutionEvidence::MiningOutcome {
                history: Box::new((&history).into()),
                recovery_plan: Box::new(plan.diagnostic()),
                attempt,
            },
        )?;
        Ok(plan)
    }

    /// Persist both lifecycle intents before their respective native operations.
    async fn recover_mining_connection(
        &mut self,
        current: &Operations,
        intent: &MiningIntent,
        plan: &CleanupRecoveryPlan,
    ) -> Result<RecoveredSurvivalClient> {
        let recovery = current.prepare_mining_profile_recovery(intent).await?;
        self.journal.intend(
            ExecutionPhase::RetireSource,
            ExecutionEvidence::RetireSource(Box::new(intent.into())),
        )?;
        recovery.close_source().await?;
        let target_condition = plan.target_condition();
        self.journal.intend(
            ExecutionPhase::FreshReconnect,
            ExecutionEvidence::FreshReconnect {
                method:
                    voxrig::checked_survival::diagnostic::MiningRecoveryMethod::SameProfileLogin,
                target_condition: target_condition.clone(),
            },
        )?;
        recovery
            .reconnect(self.reconnect.clone(), target_condition)
            .await
            .map_err(Into::into)
    }
}

fn retry_improved(previous: u8, next: u8, attempt: usize) -> Result<()> {
    if previous == next {
        return Err(ExecutionError::new(
            SurvivalErrorCode::RetryWithoutImprovement,
            "no different received empty slot; unchanged conditions cannot justify retry",
        ));
    }
    if attempt >= 3 {
        return Err(ExecutionError::new(
            SurvivalErrorCode::RetryLimit,
            "bounded recovery attempts exhausted",
        ));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn retry_requires_both_an_improvement_and_remaining_budget() {
        assert_eq!(
            retry_improved(0, 0, 1).unwrap_err().code,
            SurvivalErrorCode::RetryWithoutImprovement
        );
        assert!(retry_improved(0, 1, 1).is_ok());
        assert_eq!(
            retry_improved(0, 1, 3).unwrap_err().code,
            SurvivalErrorCode::RetryLimit
        );
    }
}
