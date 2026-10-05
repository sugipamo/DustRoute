//! Live effects in order: approach, record, consume, activate, observe, restore.
//! Every cleanup error is retained alongside the original failure.
use super::analysis::{ObservedTransition, assess_observation};
use super::{PreparedRun, StoredTransitionPlan};
use crate::OperationKind;
use crate::api::McpErrorCode;
use crate::failure::{
    CauseKind, ExecutionProgress, FailureCause, FailurePhase, FailureReport, WorldOutcome,
};
use crate::operations::transition::{RestorationSummary, TransitionRefusal, TransitionRunResult};
use dustroute_ir::SignalIntent;
use dustroute_physical::ComponentId;
use std::collections::BTreeMap;

impl PreparedRun<'_> {
    pub async fn run(
        self,
        contracts: BTreeMap<ComponentId, SignalIntent>,
    ) -> Result<TransitionRunResult, TransitionRefusal> {
        let PreparedRun {
            session,
            operation_id,
            plan,
            scene,
        } = self;
        let approach = session
            .bridge
            .approach_lever(plan.lever, &plan.dimension)
            .await
            .map_err(FailureCause::from)?;
        let started = session
            .bridge
            .start_update_recording(
                plan.bounds.min,
                plan.bounds.max,
                &plan.dimension,
                plan.max_events,
            )
            .await
            .map_err(FailureCause::from)?;
        {
            let mut plans = session.plans.table::<StoredTransitionPlan>().lock().await;
            let Some(stored) = plans.get_mut(&operation_id) else {
                return Err(TransitionRefusal::coded(
                    McpErrorCode::NotFound,
                    "scenario missing",
                ));
            };
            if let Err(error) = stored.lifecycle.begin(session.policy.preview_required) {
                return Err(TransitionRefusal::coded(McpErrorCode::InvalidState, error));
            }
        }
        let mut progress = ExecutionProgress {
            operation_consumed: true,
            total_changes: Some(2),
            ..Default::default()
        };
        progress.begin_submission();
        let mut failure = None;
        let mut activation = match session
            .bridge
            .activate_lever(plan.lever, &plan.dimension)
            .await
        {
            Ok(activation) => activation,
            Err(error) => {
                progress.submitted_changes = None;
                let mut report = progress.cause(error.cause());
                if let Err(error) = session
                    .bridge
                    .stop_update_recording(&started.recording_id, &plan.dimension)
                    .await
                {
                    report.secondary(error.cause().at(FailurePhase::AfterReadback));
                }
                crate::performance::execution_progress(&report.progress);
                let response = TransitionRunResult::failed_attempt(report, None);
                session
                    .operations
                    .record_completed(
                        operation_id,
                        OperationKind::TransitionRun,
                        response.clone().into(),
                    )
                    .await;
                return Ok(response);
            }
        };
        progress.submitted(1);
        activation.bot_approached |= approach.moved;
        progress.phase = FailurePhase::Wait;
        let wait_error = session
            .bridge
            .wait_ticks(plan.observation_ticks, &plan.dimension)
            .await
            .err()
            .map(|error| {
                let cause = error.cause().at(FailurePhase::Wait);
                FailureReport::append(&progress, &mut failure, cause);
                error.to_string()
            });
        progress.phase = FailurePhase::AfterReadback;
        let recording_result = session
            .bridge
            .stop_update_recording(&started.recording_id, &plan.dimension)
            .await;
        if let Err(error) = &recording_result {
            FailureReport::append(
                &progress,
                &mut failure,
                error.cause().at(FailurePhase::AfterReadback),
            );
        }

        progress.phase = FailurePhase::Restore;
        let restore_error = session
            .bridge
            .activate_lever(plan.lever, &plan.dimension)
            .await
            .err()
            .map(|error| {
                progress.submitted_changes = None;
                FailureReport::append(
                    &progress,
                    &mut failure,
                    error.cause().at(FailurePhase::Restore),
                );
                error.to_string()
            });
        if restore_error.is_none() {
            progress.submitted(1);
        }
        let restore_wait_error = session
            .bridge
            .wait_ticks(plan.observation_ticks, &plan.dimension)
            .await
            .err();
        if let Some(error) = &restore_wait_error {
            FailureReport::append(
                &progress,
                &mut failure,
                error.cause().at(FailurePhase::Restore),
            );
        }
        let restored_block = session.bridge.get_block(plan.lever, &plan.dimension).await;
        let restored_snapshot = session
            .bridge
            .scan_region(plan.bounds.min, plan.bounds.max, &plan.dimension)
            .await;
        let lever_restored = restored_block.as_ref().ok().is_some_and(|block| {
            block.state.name == "minecraft:lever"
                && block
                    .state
                    .properties
                    .get("powered")
                    .and_then(|value| value.parse::<bool>().ok())
                    == Some(plan.original_powered)
        });
        let region_restored = restored_snapshot
            .as_ref()
            .is_ok_and(|snapshot| snapshot == &plan.initial_snapshot);
        let restoration_verified = restore_error.is_none()
            && restore_wait_error.is_none()
            && lever_restored
            && region_restored;
        for error in [
            restored_block.as_ref().err(),
            restored_snapshot.as_ref().err(),
        ]
        .into_iter()
        .flatten()
        {
            FailureReport::append(
                &progress,
                &mut failure,
                error.cause().at(FailurePhase::Restore),
            );
        }
        if restored_block.is_ok()
            && restored_snapshot.is_ok()
            && (!lever_restored || !region_restored)
        {
            FailureReport::append(
                &progress,
                &mut failure,
                FailureCause::new(
                    CauseKind::VerificationMismatch,
                    "restored lever or region differs from initial state",
                )
                .at(FailurePhase::Restore),
            );
        }
        if lever_restored && region_restored {
            progress.world = WorldOutcome::Verified;
            progress.verified_steps = 2;
        }

        if let Some(stored) = session
            .plans
            .table::<StoredTransitionPlan>()
            .lock()
            .await
            .get_mut(&operation_id)
        {
            stored.lifecycle.confirm(restoration_verified);
        }
        let recording = match recording_result {
            Ok(recording) => recording,
            Err(_) => {
                let mut report = failure.expect("recording error inserted");
                *report.progress = progress.clone();
                crate::performance::execution_progress(&report.progress);
                let response =
                    TransitionRunResult::failed_attempt(report, Some(restoration_verified));
                session
                    .operations
                    .record_completed(
                        operation_id,
                        OperationKind::TransitionRun,
                        response.clone().into(),
                    )
                    .await;
                return Ok(response);
            }
        };
        let observation = ObservedTransition {
            activation,
            recording,
            restoration: RestorationSummary {
                lever_restored,
                region_restored,
                verified: restoration_verified,
                activation_error: restore_error,
                wait_error: restore_wait_error.as_ref().map(ToString::to_string),
                block_read_error: restored_block.as_ref().err().map(ToString::to_string),
                region_read_error: restored_snapshot.as_ref().err().map(ToString::to_string),
            },
            wait_error,
            progress,
            failure,
        };
        let result = assess_observation(operation_id, &plan, &scene, &contracts, observation);
        crate::performance::execution_progress(result.progress());
        session
            .operations
            .record_completed(
                operation_id,
                OperationKind::TransitionRun,
                result.clone().into(),
            )
            .await;
        Ok(result)
    }
}
