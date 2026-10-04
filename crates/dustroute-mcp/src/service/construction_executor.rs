//! One shared live executor for full construction and differential edits.
//! Callers persist intent and consume the capability before entering here.
use crate::assembly_registry::TargetServer;
use crate::bridge_protocol::CommandWrite;
use crate::failure::{
    CauseKind, ExecutionProgress, FailureCause, FailurePhase, FailureReport, PersistenceOutcome,
    WorldOutcome,
};
use crate::observation_evidence::ObservationEvidence;
use crate::piston_assembly::ValidatedAssemblyPlacement;
use crate::{BotBridge, McpPolicy};
use dustroute_translate::piston_construction::{ElectricalConstructionStep, construction_batches};
use dustroute_translate::snapshot::MinecraftSnapshot;
use dustroute_translate::world_reverse::RegionBounds;

#[cfg(test)]
#[path = "construction_executor_tests.rs"]
mod tests;

pub(super) enum StageProgress {
    Readback(Box<ObservationEvidence>),
    Verified(usize),
    WriteIntent(ExecutionProgress),
}

pub(super) fn batch_summary(
    steps: &[ElectricalConstructionStep],
) -> Vec<crate::operations::preview::BatchSummary> {
    crate::operations::preview::batch_summary(steps)
}

pub(super) struct ConstructionExecutor<'a> {
    pub bridge: &'a BotBridge,
    pub policy: &'a McpPolicy,
    pub target: &'a TargetServer,
}
impl ConstructionExecutor<'_> {
    pub async fn execute(
        &self,
        baseline: &MinecraftSnapshot,
        steps: &[ElectricalConstructionStep],
        mut checkpoint: impl FnMut(StageProgress) -> Result<(), FailureCause>,
    ) -> Result<ExecutionProgress, FailureReport> {
        let mut progress = ExecutionProgress {
            total_changes: Some(steps.len()),
            operation_consumed: true,
            persistence: PersistenceOutcome::IntentSaved,
            durable_verified_steps: Some(0),
            ..Default::default()
        };
        crate::performance::execution_progress(&progress);
        let bounds = RegionBounds::new(baseline.min, baseline.max);
        self.policy
            .authorize_mutation()
            .map_err(|e| progress.cause(e))?;
        self.policy
            .authorize_dimension(&self.target.dimension)
            .map_err(|e| progress.cause(e))?;
        self.policy
            .validate_region(bounds)
            .map_err(|e| progress.cause(e))?;
        self.policy
            .validate_placement_size(steps.len())
            .map_err(|e| progress.cause(e))?;
        let mut expected = std::borrow::Cow::Borrowed(baseline);
        for batch in construction_batches(steps) {
            let batch_expected = batch.expected();
            progress.first_step = Some(batch.first_step());
            progress.last_step = Some(batch.last_step());
            let result: Result<(), FailureReport> = async {
                let writes = batch
                    .steps()
                    .iter()
                    .map(|step| {
                        if !bounds.contains(step.position)
                            || step.expected.bounds().min != bounds.min
                            || step.expected.bounds().max != bounds.max
                        {
                            return Err("construction step escapes the reviewed region".into());
                        }
                        Ok(CommandWrite {
                            pos: step.position,
                            state: step.state.parse()?,
                        })
                    })
                    .collect::<Result<Vec<_>, String>>()
                    .map_err(|e| progress.cause(FailureCause::new(CauseKind::InvalidInput, e)))?;
                progress.phase = FailurePhase::BeforeReadback;
                crate::performance::execution_progress(&progress);
                let status = self.bridge.status().await.map_err(|e| progress.cause(e))?;
                super::assembly_placement::server_contract(&status, &self.target.dimension)
                    .map_err(|e| progress.cause(FailureCause::new(CauseKind::Unsupported, e)))?;
                self.target
                    .check(&status)
                    .map_err(|e| progress.cause(FailureCause::new(CauseKind::InvalidState, e)))?;
                let before = self
                    .bridge
                    .scan_region_fresh(bounds.min, bounds.max, &self.target.dimension)
                    .await
                    .map_err(|e| progress.cause(e))?
                    .into_stationary_record()
                    .map_err(|e| progress.cause(e))?;
                ValidatedAssemblyPlacement::matches(&before.snapshot, &expected, &status.version)
                    .map_err(|e| {
                    progress.cause(FailureCause::new(CauseKind::VerificationMismatch, e))
                })?;
                {
                    let _measurement =
                        crate::performance::span(crate::performance::Phase::Checkpoint);
                    progress.phase = FailurePhase::CheckpointSave;
                    crate::performance::execution_progress(&progress);
                    if let Err(cause) =
                        checkpoint(StageProgress::Readback(Box::new(before.readback)))
                    {
                        progress.persistence = PersistenceOutcome::Uncertain;
                        return Err(progress.cause(cause));
                    }
                }
                progress.phase = FailurePhase::IntentSave;
                crate::performance::execution_progress(&progress);
                let mut intent = progress.clone();
                intent.phase = FailurePhase::Submission;
                intent.world = WorldOutcome::Unknown;
                {
                    let _measurement =
                        crate::performance::span(crate::performance::Phase::Checkpoint);
                    if let Err(cause) = checkpoint(StageProgress::WriteIntent(intent)) {
                        progress.persistence = PersistenceOutcome::Uncertain;
                        return Err(progress.cause(cause));
                    }
                }
                let previous_world = progress.world;
                progress.begin_submission();
                crate::performance::execution_progress(&progress);
                let receipt = self
                    .bridge
                    .write_blocks(&writes, &self.target.dimension)
                    .await
                    .map_err(|e| progress.submission_error(e, previous_world))?;
                progress.submitted(receipt.submitted_changes);
                crate::performance::execution_progress(&progress);
                progress.phase = FailurePhase::Wait;
                crate::performance::execution_progress(&progress);
                let mut remaining = batch.wait_ticks();
                while remaining > 0 {
                    let ticks = remaining.min(200) as u16;
                    self.bridge
                        .wait_ticks(ticks, &self.target.dimension)
                        .await
                        .map_err(|e| progress.cause(e))?;
                    remaining -= u64::from(ticks);
                }
                progress.phase = FailurePhase::AfterReadback;
                crate::performance::execution_progress(&progress);
                let after = self
                    .bridge
                    .scan_region_fresh(bounds.min, bounds.max, &self.target.dimension)
                    .await
                    .map_err(|e| progress.cause(e))?
                    .into_stationary_record()
                    .map_err(|e| progress.cause(e))?;
                {
                    let _measurement =
                        crate::performance::span(crate::performance::Phase::Checkpoint);
                    progress.phase = FailurePhase::CheckpointSave;
                    crate::performance::execution_progress(&progress);
                    if let Err(cause) =
                        checkpoint(StageProgress::Readback(Box::new(after.readback)))
                    {
                        progress.persistence = PersistenceOutcome::Uncertain;
                        return Err(progress.cause(cause));
                    }
                }
                let verification =
                    crate::performance::span(crate::performance::Phase::Verification);
                progress.phase = FailurePhase::Verification;
                crate::performance::execution_progress(&progress);
                let status = self.bridge.status().await.map_err(|e| progress.cause(e))?;
                self.target
                    .check(&status)
                    .map_err(|e| progress.cause(FailureCause::new(CauseKind::InvalidState, e)))?;
                ValidatedAssemblyPlacement::matches(
                    &after.snapshot,
                    &batch_expected,
                    &status.version,
                )
                .map_err(|e| {
                    progress.cause(FailureCause::new(CauseKind::VerificationMismatch, e))
                })?;
                drop(verification);
                progress.verified_steps = batch.last_step();
                crate::performance::execution_progress(&progress);
                if progress.verified_steps == steps.len() {
                    progress.world = WorldOutcome::Verified;
                }
                {
                    let _measurement =
                        crate::performance::span(crate::performance::Phase::Checkpoint);
                    progress.phase = FailurePhase::CheckpointSave;
                    crate::performance::execution_progress(&progress);
                    if let Err(cause) = checkpoint(StageProgress::Verified(batch.last_step())) {
                        progress.persistence = PersistenceOutcome::Uncertain;
                        return Err(progress.cause(cause));
                    }
                    progress.durable_verified_steps = Some(batch.last_step());
                    progress.persistence = PersistenceOutcome::CheckpointSaved;
                    crate::performance::execution_progress(&progress);
                }
                Ok(())
            }
            .await;
            result.map_err(|mut error| {
                error.primary.message = format!(
                    "construction batch steps {}..{}: {}",
                    batch.first_step(),
                    batch.last_step(),
                    error.primary.message
                );
                error
            })?;
            expected = std::borrow::Cow::Owned(batch_expected);
        }
        if steps.is_empty() {
            progress.world = WorldOutcome::Verified;
        }
        crate::performance::execution_progress(&progress);
        Ok(progress)
    }
}
