//! Read-only validation of persisted idle boundaries and confirmed temporary ownership.
use super::*;
use crate::survival_execution::diagnostic::{RecordedConstructionPlan, RecordedStep};

pub(super) fn checked(directory: &Path) -> Result<SafeCheckpoint> {
    if directory.join("continuation-claim.json").try_exists()? {
        return Err(ExecutionError::new(
            SurvivalErrorCode::CheckpointConsumed,
            "another new job already claimed this checkpoint; diagnose that job",
        ));
    }
    let record = diagnose(directory)?.record;
    let last = record.events.last().ok_or_else(|| {
        ExecutionError::new(
            SurvivalErrorCode::SafeCheckpointMissing,
            "no settled idle checkpoint; lost handles cannot be restored",
        )
    })?;
    if record.continuation != Continuation::Checkpoint
        || record.outcome != OperationOutcome::Observed
        || last.phase != ExecutionPhase::IdleCheckpoint
        || last.continuation != Continuation::Checkpoint
        || last.outcome != OperationOutcome::Observed
        || last.step != record.completed_steps
    {
        return Err(ExecutionError::new(
            SurvivalErrorCode::SafeCheckpointMissing,
            "last durable event is not an idle checkpoint; unresolved effects require inspection",
        ));
    }
    let ExecutionEvidence::Checkpoint(checkpoint) = &last.evidence else {
        return Err(ExecutionError::new(
            SurvivalErrorCode::InvalidCheckpoint,
            "missing typed checkpoint evidence",
        ));
    };
    let plan = record.plan.plan.as_ref().ok_or_else(|| {
        ExecutionError::new(
            SurvivalErrorCode::InvalidCheckpoint,
            "missing diagnostic construction plan",
        )
    })?;
    if checkpoint.schema != CheckpointSchema::V2
        || checkpoint.execution_id != record.id
        || checkpoint.completed_steps != record.completed_steps
        || checkpoint.scope != plan.scope
        || record.plan.declared_reconnect.as_ref() != Some(&checkpoint.endpoint)
    {
        return Err(ExecutionError::new(
            SurvivalErrorCode::InvalidCheckpoint,
            "checkpoint provenance differs from execution",
        ));
    }
    let owned = confirmed_temporary_ownership(&record, plan)?;
    let recorded: BTreeMap<_, _> = checkpoint
        .temporary
        .iter()
        .map(|t| (t.position, t.state.clone()))
        .collect();
    if recorded.len() != checkpoint.temporary.len() || recorded != owned {
        return Err(ExecutionError::new(
            SurvivalErrorCode::InvalidCheckpoint,
            "checkpoint ownership differs from confirmed prefix",
        ));
    }
    Ok((**checkpoint).clone())
}

/// Only typed confirmed edits contribute ownership; saved native previews cannot run.
fn confirmed_temporary_ownership(
    record: &ExecutionRecord,
    plan: &RecordedConstructionPlan,
) -> Result<BTreeMap<[i32; 3], NativeBlockState>> {
    let completions: Vec<_> = record
        .events
        .iter()
        .filter(|e| e.phase == ExecutionPhase::StepCompleted)
        .collect();
    if completions.len() != record.completed_steps
        || completions.iter().enumerate().any(|(i, e)| {
            e.step != i + 1
                || !matches!(
                    e.outcome,
                    OperationOutcome::Observed | OperationOutcome::Predicted
                )
                || e.continuation != Continuation::Revalidate
        })
    {
        return Err(ExecutionError::new(
            SurvivalErrorCode::InvalidCheckpoint,
            "completed prefix lacks sequential observed events",
        ));
    }
    let mut owned = BTreeMap::new();
    for block in &plan.initial_temporary {
        if owned.insert(block.position, block.state.clone()).is_some() {
            return Err(ExecutionError::new(
                SurvivalErrorCode::InvalidCheckpoint,
                "duplicate initial ownership",
            ));
        }
    }
    if record.completed_steps > plan.steps.len() {
        return Err(ExecutionError::new(
            SurvivalErrorCode::InvalidCheckpoint,
            "completed prefix exceeds plan",
        ));
    }
    for (index, step) in plan.steps[..record.completed_steps].iter().enumerate() {
        if completions[index].outcome != step.expected_outcome()? {
            return Err(ExecutionError::new(
                SurvivalErrorCode::InvalidCheckpoint,
                "step outcome differs from its evidence contract",
            ));
        }
        match step {
            RecordedStep::Place {
                purpose: PlacementPurpose::Temporary,
                placement,
            } => {
                let placement = placement.as_ref().ok_or_else(|| {
                    ExecutionError::new(
                        SurvivalErrorCode::InvalidCheckpoint,
                        "missing historical placement edit",
                    )
                })?;
                if owned
                    .insert(placement.edit.position, placement.edit.after.clone())
                    .is_some()
                {
                    return Err(ExecutionError::new(
                        SurvivalErrorCode::InvalidCheckpoint,
                        "duplicate temporary placement",
                    ));
                }
            }
            RecordedStep::RemoveTemporary { edit, .. } => {
                if owned.remove(&edit.position).as_ref() != Some(&edit.before) {
                    return Err(ExecutionError::new(
                        SurvivalErrorCode::InvalidCheckpoint,
                        "removal lacks historical ownership",
                    ));
                }
            }
            RecordedStep::Place { .. } | RecordedStep::Move { .. } => {}
            RecordedStep::Unknown => unreachable!("expected_outcome refuses unknown steps"),
        }
    }
    Ok(owned)
}
