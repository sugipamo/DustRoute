//! Fixed-door candidates are recorded intentions, never executable capabilities.
use crate::bridge::PreviewSubmission;
use crate::bridge_protocol::CommandWrite;
use crate::operations::mutation::{Success, UnrecordedFailure};
use crate::piston_door::{DoorState, ValidatedDoorPlacement, VerifiedDoor};
use dustroute_physical::Pos;
use dustroute_translate::native_state::NativeBlockState;
use dustroute_translate::piston_observation::PistonObservation;
use dustroute_translate::world_reverse::RegionBounds;
use serde::Serialize;
use std::collections::BTreeMap;
use uuid::Uuid;

/// Literal intended writes without a decoder or conversion into a native plan.
#[derive(Clone, Debug, PartialEq, Serialize)]
struct PistonCommand {
    pos: Pos,
    state: NativeBlockState,
}
fn commands(writes: Vec<CommandWrite>) -> Vec<PistonCommand> {
    writes
        .into_iter()
        .map(|write| PistonCommand {
            pos: write.pos,
            state: write.state,
        })
        .collect()
}

#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct PistonPlacementPreview {
    ok: Success,
    operation_id: Uuid,
    circuit: &'static str,
    origin: Pos,
    anchor: Pos,
    bounds: RegionBounds,
    initial_state: DoorState,
    materials: BTreeMap<String, usize>,
    changed_blocks: usize,
    collision_count: usize,
    undo_change_count: usize,
    read_only: bool,
    changes: Vec<PistonCommand>,
    undo_changes: Vec<PistonCommand>,
    next_step: &'static str,
    placement_note: &'static str,
}
impl PistonPlacementPreview {
    pub(crate) fn new(
        operation_id: Uuid,
        anchor: Pos,
        proof: &ValidatedDoorPlacement,
        materials: BTreeMap<String, usize>,
        read_only: bool,
    ) -> Self {
        Self {
            ok: Success,
            operation_id,
            circuit: "piston-door-1x2",
            origin: proof.origin(),
            anchor,
            bounds: proof.bounds(),
            initial_state: DoorState::Open,
            materials,
            changed_blocks: proof.initial().blocks.len(),
            collision_count: 0,
            undo_change_count: proof.initial().blocks.len(),
            read_only,
            changes: commands(proof.writes(false)),
            undo_changes: commands(proof.writes(true)),
            next_step: "show_operation, then invoke_operation(confirm=true)",
            placement_note: "origin is three blocks above the gaze target to preserve an empty guard above the ground",
        }
    }
}

/// Preserve the historical PascalCase lifecycle display, without parsing Debug text.
#[derive(Clone, Copy, Debug, PartialEq, Serialize)]
pub(crate) enum PistonPlanState {
    Planned,
    Applied,
    Undone,
    NeedsInspection,
}
#[derive(Debug, Serialize)]
pub(crate) struct PistonPlanDisplay {
    ok: Success,
    read_only: bool,
    plan: PistonPlanDetails,
}
#[derive(Debug, Serialize)]
struct PistonPlanDetails {
    operation_id: Uuid,
    origin: Pos,
    bounds: RegionBounds,
    changes: Vec<PistonCommand>,
    undo_changes: Vec<PistonCommand>,
    previewed: bool,
    state: PistonPlanState,
}
impl PistonPlanDisplay {
    pub(crate) fn new(
        operation_id: Uuid,
        proof: &ValidatedDoorPlacement,
        previewed: bool,
        state: PistonPlanState,
        read_only: bool,
    ) -> Self {
        Self {
            ok: Success,
            read_only,
            plan: PistonPlanDetails {
                operation_id,
                origin: proof.origin(),
                bounds: proof.bounds(),
                changes: commands(proof.writes(false)),
                undo_changes: commands(proof.writes(true)),
                previewed,
                state,
            },
        }
    }
}
#[derive(Debug, Serialize)]
pub(crate) struct ShownPistonPlacement {
    ok: Success,
    operation_id: Uuid,
    preview: PreviewSubmission,
    bounds: RegionBounds,
    changes: Vec<PistonCommand>,
    undo_changes: Vec<PistonCommand>,
    initial_state: DoorState,
}
impl ShownPistonPlacement {
    pub(crate) fn new(
        operation_id: Uuid,
        proof: &ValidatedDoorPlacement,
        preview: PreviewSubmission,
    ) -> Self {
        Self {
            ok: Success,
            operation_id,
            preview,
            bounds: proof.bounds(),
            changes: commands(proof.writes(false)),
            undo_changes: commands(proof.writes(true)),
            initial_state: DoorState::Open,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct DoorProposal {
    ok: Success,
    operation_id: Uuid,
    contract: &'static str,
    observation: PistonObservation,
    state: DoorState,
    target: DoorState,
    bounds: RegionBounds,
    lever: Pos,
    expires_in_seconds: u64,
    next_step: &'static str,
}
impl DoorProposal {
    pub(crate) fn new(
        operation_id: Uuid,
        observation: PistonObservation,
        door: &VerifiedDoor,
        target: DoorState,
    ) -> Self {
        Self {
            ok: Success,
            operation_id,
            contract: "piston_door_v1",
            observation,
            state: door.state(),
            target,
            bounds: door.bounds(),
            lever: door.lever(),
            expires_in_seconds: 300,
            next_step: "show_operation, then invoke_operation(confirm=true) after confirmation",
        }
    }
}
#[derive(Debug, Serialize)]
#[serde(untagged)]
pub(crate) enum DoorPlanningFailure {
    Refused(UnrecordedFailure),
    Incomplete {
        ok: bool,
        error: &'static str,
        observation: PistonObservation,
    },
}
impl DoorPlanningFailure {
    pub(crate) fn incomplete(observation: PistonObservation) -> Self {
        Self::Incomplete {
            ok: false,
            error: "door is not a completely observed stable configuration",
            observation,
        }
    }
}
impl From<String> for DoorPlanningFailure {
    fn from(value: String) -> Self {
        Self::Refused(value.into())
    }
}
impl From<&str> for DoorPlanningFailure {
    fn from(value: &str) -> Self {
        Self::Refused(value.into())
    }
}
impl From<crate::failure::FailureCause> for DoorPlanningFailure {
    fn from(value: crate::failure::FailureCause) -> Self {
        Self::Refused(value.into())
    }
}
#[derive(Debug, Serialize)]
pub(crate) struct ShownDoor {
    ok: Success,
    operation_id: Uuid,
    state: DoorState,
    target: DoorState,
    preview: PreviewSubmission,
    warning: &'static str,
}
impl ShownDoor {
    pub(crate) fn new(
        operation_id: Uuid,
        state: DoorState,
        target: DoorState,
        preview: PreviewSubmission,
    ) -> Self {
        Self {
            ok: Success,
            operation_id,
            state,
            target,
            preview,
            warning: "Normal lever activation changes this door and leaves it in the requested state. Failure requires fresh inspection; no automatic retry or rollback.",
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::operations::OperationResult;
    use crate::piston_door::{inspect, sample, verify};
    use dustroute_translate::piston_observation::PistonObservationState;
    use serde_json::json;

    #[test]
    fn placement_history_preserves_full_intent_without_execution_facts() {
        let mut baseline = sample(DoorState::Open);
        baseline.blocks.clear();
        let proof = ValidatedDoorPlacement::new(Pos::default(), &baseline, "1.21.11").unwrap();
        let mut materials = BTreeMap::<String, usize>::new();
        for block in &proof.initial().blocks {
            *materials.entry(block.name.clone()).or_default() += 1;
        }
        let id = Uuid::new_v4();
        let anchor = Pos::new(0, -3, 0);
        let candidate = PistonPlacementPreview::new(id, anchor, &proof, materials.clone(), true);
        let expected = json!({
            "ok":true,"operation_id":id,"circuit":"piston-door-1x2",
            "origin":proof.origin(),"anchor":anchor,"bounds":proof.bounds(),
            "initial_state":"open","materials":materials,
            "changed_blocks":proof.initial().blocks.len(),"collision_count":0,
            "undo_change_count":proof.initial().blocks.len(),"read_only":true,
            "changes":proof.writes(false),"undo_changes":proof.writes(true),
            "next_step":"show_operation, then invoke_operation(confirm=true)",
            "placement_note":"origin is three blocks above the gaze target to preserve an empty guard above the ground"
        });
        let history = OperationResult::from(candidate);
        assert_eq!(serde_json::to_value(&history).unwrap(), expected);
        assert!(!history.failed());
        assert!(history.progress().is_none());
        assert!(!history.consumed());
        for (state, label) in [
            (PistonPlanState::Planned, "Planned"),
            (PistonPlanState::Applied, "Applied"),
            (PistonPlanState::Undone, "Undone"),
            (PistonPlanState::NeedsInspection, "NeedsInspection"),
        ] {
            let display = PistonPlanDisplay::new(id, &proof, true, state, false);
            let wire = serde_json::to_value(display).unwrap();
            assert_eq!(wire["plan"]["state"], label);
            assert_eq!(wire["plan"]["changes"], expected["changes"]);
            assert_eq!(wire["plan"]["undo_changes"], expected["undo_changes"]);
            assert_eq!(wire["plan"]["previewed"], true);
            assert!(wire.get("execution_progress").is_none());
        }
    }

    #[test]
    fn stable_or_unresolved_observation_never_becomes_a_recorded_execution() {
        let snapshot = sample(DoorState::Open);
        let door = verify(&snapshot, "1.21.11").unwrap();
        let observation = inspect(&snapshot, "1.21.11", true, None);
        let id = Uuid::new_v4();
        let expected = json!({
            "ok":true,"operation_id":id,"contract":"piston_door_v1",
            "observation":observation,"state":"open","target":"closed",
            "bounds":door.bounds(),"lever":door.lever(),"expires_in_seconds":300,
            "next_step":"show_operation, then invoke_operation(confirm=true) after confirmation"
        });
        let history =
            OperationResult::from(DoorProposal::new(id, observation, &door, DoorState::Closed));
        assert_eq!(serde_json::to_value(&history).unwrap(), expected);
        assert!(!history.failed());
        assert!(history.progress().is_none());
        assert!(!history.consumed());
        for state in [
            PistonObservationState::Moving,
            PistonObservationState::Indeterminate,
            PistonObservationState::ObservationIncomplete,
            PistonObservationState::UnsupportedVersion,
        ] {
            let observation = PistonObservation::unresolved(state, "unresolved", "rescan required");
            let expected = json!({
                "ok":false,"error":"door is not a completely observed stable configuration",
                "observation":observation
            });
            let wire = serde_json::to_value(DoorPlanningFailure::incomplete(observation)).unwrap();
            assert_eq!(wire, expected);
            assert!(wire.get("operation_id").is_none());
            assert!(wire.get("execution_progress").is_none());
            assert!(wire.get("failure").is_none());
        }
    }
}
