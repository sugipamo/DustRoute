//! Bounded display records; the executor retains the complete proof separately.
use super::{BatchSummary, StateSummary, batch_summary};
use crate::construction_jobs::JobStageBinding;
use crate::operations::mutation::Success;
use crate::placement_source::PlacementSource;
use dustroute_library::world_edit::WorldEditScope;
use dustroute_physical::Pos;
use dustroute_translate::diagnostic::difference::SnapshotDifference;
use dustroute_translate::piston_construction::{
    ElectricalConstructionStep, ElectricalModification,
};
use dustroute_translate::snapshot::{MinecraftSnapshot, MinecraftSnapshotBlock};
use dustroute_translate::world_reverse::RegionBounds;
use serde::Serialize;
use uuid::Uuid;

#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct ElectricalEditPreview {
    schema_version: &'static str,
    ok: Success,
    operation_id: Uuid,
    kind: &'static str,
    source: PlacementSource,
    revision_id: Uuid,
    read_only: bool,
    job_stage: Option<JobStageBinding>,
    bounds: RegionBounds,
    edit_scope: WorldEditScope,
    execution_batches: Vec<BatchSummary>,
    undo_execution_batches: Vec<BatchSummary>,
    conditions: EditConditions,
    validation_scope: &'static str,
    next_step: &'static str,
    #[serde(flatten)]
    states: ElectricalStates,
}
impl ElectricalEditPreview {
    pub(crate) fn new(
        operation_id: Uuid,
        source: PlacementSource,
        revision_id: Uuid,
        job_stage: Option<JobStageBinding>,
        read_only: bool,
        proof: &ElectricalModification,
    ) -> Self {
        Self {
            schema_version: "dustroute.electrical-edit-preview.v2",
            ok: Success,
            operation_id,
            kind: "electrical_revision_modification",
            source,
            revision_id,
            read_only,
            job_stage,
            bounds: RegionBounds::new(proof.before().min, proof.before().max),
            edit_scope: proof.scope().clone(),
            execution_batches: batch_summary(proof.steps(false)),
            undo_execution_batches: batch_summary(proof.steps(true)),
            conditions: EditConditions::declared(),
            validation_scope: "complete declared state and per-command physics; live readback at batch boundaries; no flying/harvest contract implied",
            next_step: "show_operation then confirm invoke_operation; no automatic retry/rollback",
            states: ElectricalStates::new(proof),
        }
    }
    pub(crate) fn operation_id(&self) -> Uuid {
        self.operation_id
    }
}
#[derive(Debug, Serialize)]
pub(crate) struct ShownElectricalEdit {
    #[serde(flatten)]
    pub response: ElectricalEditPreview,
    pub preview: crate::bridge::PreviewSubmission,
}

#[derive(Clone, Debug, PartialEq, Serialize)]
struct EditConditions {
    stationary_observation_required: bool,
    model_initial_queue: &'static str,
    runtime_history_reconstructed: bool,
    functional_behavior_verified: bool,
    protected_state_checked: &'static str,
    outside_editable: &'static str,
    fixed_environment: &'static str,
    natural_growth: &'static str,
    operator_requirement: &'static str,
}
impl EditConditions {
    fn declared() -> Self {
        Self {
            stationary_observation_required: true,
            model_initial_queue: "assumed_empty",
            runtime_history_reconstructed: false,
            functional_behavior_verified: false,
            protected_state_checked: "every modeled committed microstep, forward and undo; live full-region readback at batch boundaries",
            outside_editable: "all other observed cells are protected; no ownership inferred",
            fixed_environment: "enclosed source water only; source or containment changes are unsupported",
            natural_growth: "not modeled; live state drift stops execution",
            operator_requirement: "finish prior motion and keep external inputs/edits out of the work region",
        }
    }
}
#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(untagged)]
enum StateDisplay {
    Full(MinecraftSnapshot),
    Summary(StateSummary),
}
#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(untagged)]
enum StepsDisplay {
    Full(Vec<ElectricalConstructionStep>),
    Summary(Vec<StepSummary>),
}
#[derive(Clone, Debug, PartialEq, Serialize)]
struct StepSummary {
    position: Pos,
    state: String,
    wait_ticks: u64,
    expected_non_air_blocks: usize,
    complete_expected_state_retained_by_executor: bool,
}
#[derive(Clone, Debug, PartialEq, Serialize)]
struct ElectricalStates {
    step_states_expanded: bool,
    before: StateDisplay,
    after: StateDisplay,
    requested_changes: Vec<MinecraftSnapshotBlock>,
    steps: StepsDisplay,
    undo_steps: StepsDisplay,
    no_write_checkpoint: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    settled_differences_count: Option<usize>,
    #[serde(skip_serializing_if = "Option::is_none")]
    settled_differences_truncated: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    settled_differences: Option<Vec<SnapshotDifference>>,
}
impl ElectricalStates {
    fn new(proof: &ElectricalModification) -> Self {
        let expanded_records = proof.before().blocks.len()
            + proof.after().blocks.len()
            + proof.requests().len()
            + proof
                .steps(false)
                .iter()
                .chain(proof.steps(true))
                .map(|step| step.expected.len())
                .sum::<usize>();
        let expanded = expanded_records <= 8192
            && proof.before().blocks.len() + proof.after().blocks.len() <= 512;
        let state = |snapshot: &MinecraftSnapshot| {
            if expanded {
                StateDisplay::Full(snapshot.clone())
            } else {
                StateDisplay::Summary(StateSummary::new(snapshot))
            }
        };
        let steps = |undo| {
            if expanded {
                StepsDisplay::Full(proof.steps(undo).to_vec())
            } else {
                StepsDisplay::Summary(
                    proof
                        .steps(undo)
                        .iter()
                        .map(|step| StepSummary {
                            position: step.position,
                            state: step.state.clone(),
                            wait_ticks: step.wait_ticks,
                            expected_non_air_blocks: step.expected.len(),
                            complete_expected_state_retained_by_executor: true,
                        })
                        .collect(),
                )
            }
        };
        let differences = (!expanded).then(|| {
            dustroute_translate::diagnostic::difference::differences(proof.before(), proof.after())
                .expect("validated full-context proof")
        });
        Self {
            step_states_expanded: expanded,
            before: state(proof.before()),
            after: state(proof.after()),
            requested_changes: proof.requests().to_vec(),
            steps: steps(false),
            undo_steps: steps(true),
            no_write_checkpoint: proof.steps(false).is_empty(),
            settled_differences_count: differences.as_ref().map(Vec::len),
            settled_differences_truncated: differences.as_ref().map(|ds| ds.len() > 64),
            settled_differences: differences.map(|ds| ds.into_iter().take(64).collect()),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[tokio::test]
    async fn preview_and_discarded_stage_have_no_execution_facts_or_replay_authority() {
        use crate::operations::preview::DiscardedJobStage;
        use crate::operations::{
            OperationKind, OperationRegistry, OperationResult, OperationStatus,
        };
        let after = MinecraftSnapshot {
            min: Pos::new(-1, -1, -1),
            max: Pos::new(1, 1, 1),
            blocks: vec![MinecraftSnapshotBlock {
                pos: Pos::new(0, 0, 0),
                name: "minecraft:stone".into(),
                properties: Default::default(),
            }],
        };
        let before = MinecraftSnapshot {
            blocks: vec![],
            ..after.clone()
        };
        let proof = ElectricalModification::new(&before, &after, Default::default()).unwrap();
        let id = Uuid::from_u128(1);
        let revision_id = Uuid::from_u128(2);
        let preview = ElectricalEditPreview::new(
            id,
            PlacementSource::CircuitRevision { revision_id },
            revision_id,
            None,
            true,
            &proof,
        );
        let wire = serde_json::to_value(&preview).unwrap();
        assert_eq!(wire["step_states_expanded"], true);
        assert_eq!(wire["before"], json!(before));
        assert_eq!(wire["after"], json!(after));
        assert_eq!(wire["steps"], json!(proof.steps(false)));
        assert_eq!(wire["undo_steps"], json!(proof.steps(true)));
        assert!(wire.get("settled_differences").is_none());
        assert_eq!(wire["job_stage"], json!(null));
        let registry = OperationRegistry::default();
        registry
            .record_completed(id, OperationKind::PlacementPreview, preview.into())
            .await;
        let initial = registry.get(id).await.unwrap();
        assert_eq!(initial.status, OperationStatus::Completed);
        let result = initial.result.unwrap();
        assert!(matches!(result, OperationResult::ElectricalEditPreview(_)));
        assert!(result.progress().is_none());
        assert!(!result.consumed());
        registry
            .record_completed(
                id,
                OperationKind::PlacementPreview,
                DiscardedJobStage::superseded().into(),
            )
            .await;
        let discarded = registry.get(id).await.unwrap();
        assert_eq!(discarded.status, OperationStatus::Failed);
        assert_eq!(discarded.progress_percent, 0);
        let result = discarded.result.unwrap();
        assert!(result.progress().is_none());
        assert!(!result.consumed());
        let wire = serde_json::to_value(result).unwrap();
        assert_eq!(
            wire,
            json!({"ok":false,"status":"job_stage_capability_discarded","next_step":"freshly plan the job stage"})
        );
        let job_id = Uuid::from_u128(3);
        let discarded = serde_json::to_value(DiscardedJobStage::discarded(job_id)).unwrap();
        assert_eq!(discarded["job_id"], json!(job_id));
        assert!(discarded.get("execution_progress").is_none());
    }

    #[test]
    fn repeated_state_budget_can_require_summary_even_for_a_small_region() {
        let after = MinecraftSnapshot {
            min: Pos::new(-1, -1, -1),
            max: Pos::new(264, 1, 1),
            blocks: (0..264)
                .map(|x| MinecraftSnapshotBlock {
                    pos: Pos::new(x, 0, 0),
                    name: "minecraft:stone".into(),
                    properties: Default::default(),
                })
                .collect(),
        };
        let before = MinecraftSnapshot {
            blocks: after.blocks[..200].to_vec(),
            ..after.clone()
        };
        let proof = ElectricalModification::new(&before, &after, Default::default()).unwrap();
        let wire = serde_json::to_value(ElectricalStates::new(&proof)).unwrap();
        assert_eq!(wire["step_states_expanded"], false);
        assert_eq!(wire["settled_differences_count"], 64);
        assert_eq!(wire["settled_differences_truncated"], false);
        assert_eq!(wire["before"]["non_air_blocks"], 200);
        assert!(wire["steps"][0].get("expected").is_none());
        assert_eq!(proof.steps(false).last().unwrap().expected, after);
    }

    #[test]
    fn large_preview_keeps_all_command_values_but_not_repeated_worlds() {
        let after = MinecraftSnapshot {
            min: Pos::new(-1, -1, -1),
            max: Pos::new(513, 1, 1),
            blocks: (0..512)
                .map(|x| MinecraftSnapshotBlock {
                    pos: Pos::new(x, 0, 0),
                    name: "minecraft:stone".into(),
                    properties: Default::default(),
                })
                .collect(),
        };
        let before = MinecraftSnapshot {
            blocks: after.blocks[..448].to_vec(),
            ..after.clone()
        };
        let proof = ElectricalModification::new(&before, &after, Default::default()).unwrap();
        let preview = serde_json::to_value(ElectricalStates::new(&proof)).unwrap();
        assert_eq!(preview["step_states_expanded"], false);
        assert_eq!(preview["requested_changes"].as_array().unwrap().len(), 64);
        assert_eq!(preview["steps"].as_array().unwrap().len(), 64);
        assert!(preview["steps"][0].get("expected").is_none());
        assert_eq!(preview["after"]["non_air_blocks"], 512);
        assert!(serde_json::to_vec(&preview).unwrap().len() < 65536);
        assert_eq!(proof.steps(false).last().unwrap().expected, after);
    }
}
