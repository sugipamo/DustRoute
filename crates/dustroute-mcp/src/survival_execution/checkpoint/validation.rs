//! Read-only validation of persisted idle boundaries and confirmed temporary ownership.
use super::*;

#[derive(Deserialize)]
struct SavedEdit {
    position: [i32; 3],
    before: NativeBlockState,
    after: NativeBlockState,
}

pub(super) fn checked(directory: &Path) -> Result<SafeCheckpoint> {
    if directory.join("continuation-claim.json").try_exists()? {
        return Err(ExecutionError::new(
            "checkpoint_consumed",
            "another new job already claimed this checkpoint; diagnose that job",
        ));
    }
    let record = diagnose(directory)?.record;
    let last = record.events.last().ok_or_else(|| {
        ExecutionError::new(
            "safe_checkpoint_missing",
            "no settled idle checkpoint; lost handles cannot be restored",
        )
    })?;
    if record.continuation != Continuation::Checkpoint
        || record.outcome != OperationOutcome::Observed
        || last.phase != "idle_checkpoint"
        || last.continuation != Continuation::Checkpoint
        || last.outcome != OperationOutcome::Observed
        || last.step != record.completed_steps
    {
        return Err(ExecutionError::new(
            "safe_checkpoint_missing",
            "last durable event is not an idle checkpoint; unresolved effects require inspection",
        ));
    }
    let checkpoint: SafeCheckpoint = serde_json::from_value(last.evidence.clone())?;
    if checkpoint.schema != "dustroute.survival-checkpoint.v2"
        || checkpoint.execution_id != record.id
        || checkpoint.completed_steps != record.completed_steps
        || json!(checkpoint.scope) != record.plan["plan"]["scope"]
        || checkpoint.endpoint != record.plan["declared_reconnect"]
    {
        return Err(ExecutionError::new(
            "invalid_checkpoint",
            "checkpoint provenance differs from execution",
        ));
    }
    let owned = confirmed_temporary_ownership(&record)?;
    let recorded: BTreeMap<_, _> = checkpoint
        .temporary
        .iter()
        .map(|t| (t.position, t.state.clone()))
        .collect();
    if recorded.len() != checkpoint.temporary.len() || recorded != owned {
        return Err(ExecutionError::new(
            "invalid_checkpoint",
            "checkpoint ownership differs from confirmed prefix",
        ));
    }
    Ok(checkpoint)
}

/// Rebuild ownership only from the confirmed, evidence-typed prefix; JSON never
/// becomes a process-local executor or an independent ownership assertion.
fn confirmed_temporary_ownership(
    record: &ExecutionRecord,
) -> Result<BTreeMap<[i32; 3], NativeBlockState>> {
    // Ownership is rebuilt from confirmed prefix edits, never from a caller list.
    let plan = &record.plan["plan"];
    let completions: Vec<_> = record
        .events
        .iter()
        .filter(|e| e.phase == "step_completed")
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
            "invalid_checkpoint",
            "completed prefix lacks sequential observed events",
        ));
    }
    let mut owned = BTreeMap::new();
    let initial: Vec<TemporaryBlock> = serde_json::from_value(plan["initial_temporary"].clone())?;
    for block in initial {
        if owned.insert(block.position, block.state).is_some() {
            return Err(ExecutionError::new(
                "invalid_checkpoint",
                "duplicate initial ownership",
            ));
        }
    }
    let steps = plan["steps"]
        .as_array()
        .ok_or_else(|| ExecutionError::new("invalid_checkpoint", "missing diagnostic steps"))?;
    if record.completed_steps > steps.len() {
        return Err(ExecutionError::new(
            "invalid_checkpoint",
            "completed prefix exceeds plan",
        ));
    }
    for (index, step) in steps[..record.completed_steps].iter().enumerate() {
        let expected_outcome = if step["kind"] == "move" {
            OperationOutcome::Predicted
        } else {
            OperationOutcome::Observed
        };
        if completions[index].outcome != expected_outcome {
            return Err(ExecutionError::new(
                "invalid_checkpoint",
                "step outcome differs from its evidence contract",
            ));
        }
        match step["kind"].as_str() {
            Some("place") if step["purpose"] == "temporary" => {
                let edit: SavedEdit = serde_json::from_value(step["placement"]["edit"].clone())?;
                if owned.insert(edit.position, edit.after).is_some() {
                    return Err(ExecutionError::new(
                        "invalid_checkpoint",
                        "duplicate temporary placement",
                    ));
                }
            }
            Some("remove_temporary") => {
                let edit: SavedEdit = serde_json::from_value(step["edit"].clone())?;
                if owned.remove(&edit.position).as_ref() != Some(&edit.before) {
                    return Err(ExecutionError::new(
                        "invalid_checkpoint",
                        "removal lacks historical ownership",
                    ));
                }
            }
            Some("place" | "move") => {}
            _ => {
                return Err(ExecutionError::new(
                    "invalid_checkpoint",
                    "unknown historical step",
                ));
            }
        }
    }
    Ok(owned)
}
