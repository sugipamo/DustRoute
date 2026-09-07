//! Read-only recognition of an observed, two-sided 3x3 piston-door shape.
//!
//! This module deliberately sits between a Minecraft snapshot and the
//! scenario executor.  It reports what the snapshot supports without turning
//! an observed layout into a runnable scenario or inferring unobserved
//! redstone/piston behavior.

use std::collections::{BTreeMap, BTreeSet};
use std::fmt::{Display, Formatter};

use serde::{Deserialize, Serialize};

use crate::connectivity::EdgeKind;
use crate::world_reverse::{RegionBounds, analyze_world_region};
use crate::{
    Block, BlockKind, Facing, MinecraftSnapshot, MinecraftSnapshotBlock, PistonVariant, Pos, World,
    world_from_snapshot,
};

/// Versioned JSON contract for observed 3x3 piston-door recognition.
pub const OBSERVED_PISTON_DOOR_SCHEMA: &str = "dustroute.observed-3x3-piston-door.v1";

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ObservedRecognitionStatus {
    Recognized,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, Ord, PartialEq, PartialOrd, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ObservedPistonDoorState {
    Closed,
    Open,
    Transition,
    Unknown,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, Ord, PartialEq, PartialOrd, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ObservedPistonCellState {
    Closed,
    Open,
    Transition,
    Unknown,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ObservedPistonRole {
    OpenPusher,
    ClosePusher,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ObservedPistonDoorOrientationStatus {
    Certain,
    Ambiguous,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ObservedPistonState {
    Retracted,
    Extended,
    Moving,
    Unknown,
}

impl ObservedPistonState {
    #[must_use]
    pub const fn is_stable(self) -> bool {
        matches!(self, Self::Retracted | Self::Extended)
    }
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct ObservedBlockEvidence {
    pub position: Pos,
    pub kind: BlockKind,
    pub observed_name: String,
    pub properties: BTreeMap<String, String>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct ObservedPiston {
    pub position: Pos,
    pub role: ObservedPistonRole,
    pub facing: Facing,
    pub variant: PistonVariant,
    pub state: ObservedPistonState,
    pub observed_name: String,
    pub properties: BTreeMap<String, String>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct ObservedPistonDoorCell {
    pub index: usize,
    pub grid_x: u8,
    pub grid_y: u8,
    pub closed_position: Pos,
    pub open_position: Pos,
    pub current_position: Option<Pos>,
    pub current_block: Option<ObservedBlockEvidence>,
    pub state: ObservedPistonCellState,
    pub open_piston: Pos,
    pub close_piston: Pos,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct ObservedPistonDoorOrientation {
    pub status: ObservedPistonDoorOrientationStatus,
    pub open_direction: Facing,
    pub close_direction: Facing,
    pub width_axis: Facing,
    pub height_axis: Facing,
    pub alternatives: Vec<ObservedPistonDoorCandidate>,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct ObservedPistonDoorGeometry {
    pub open_piston_origin: Pos,
    pub closed_plane_origin: Pos,
    pub open_plane_origin: Pos,
    pub close_piston_origin: Pos,
    pub width: u8,
    pub height: u8,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct ObservedPistonInputEdge {
    pub source: Pos,
    pub piston: Pos,
    pub kind: String,
}

#[derive(Clone, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
pub struct ObservedPistonDoorControl {
    pub input_positions: Vec<Pos>,
    pub wire_positions: Vec<Pos>,
    pub repeater_positions: Vec<Pos>,
    pub piston_input_edges: Vec<ObservedPistonInputEdge>,
    pub reachable_pistons: Vec<Pos>,
    pub unreachable_pistons: Vec<Pos>,
    pub graph_node_count: usize,
    pub graph_edge_count: usize,
    pub boundary_touched: bool,
    pub complete: bool,
    pub unresolved: Vec<String>,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct ObservedPistonDoorObservation {
    pub snapshot_bounds: RegionBounds,
    pub required_bounds: RegionBounds,
    pub required_region_observed: bool,
    pub piston_state_properties_complete: bool,
    pub boundary_touched: bool,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct ObservedPistonDoor {
    pub schema_version: String,
    pub status: ObservedRecognitionStatus,
    pub kind: String,
    pub size: [u8; 2],
    pub orientation: ObservedPistonDoorOrientation,
    pub geometry: ObservedPistonDoorGeometry,
    pub state: ObservedPistonDoorState,
    pub state_evidence: Vec<String>,
    pub cells: Vec<ObservedPistonDoorCell>,
    pub pistons: Vec<ObservedPiston>,
    pub control: ObservedPistonDoorControl,
    pub observation: ObservedPistonDoorObservation,
    pub unresolved: Vec<String>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct ObservedPistonDoorCandidate {
    pub open_piston_origin: Pos,
    pub open_direction: Facing,
    pub close_piston_origin: Pos,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ObservedPistonDoorRecognitionError {
    InvalidSnapshot(String),
    DuplicatePosition(Pos),
    IncompleteObservation {
        required_positions: Vec<Pos>,
    },
    NoThreeByThreeCandidate,
    Ambiguous {
        candidates: Vec<ObservedPistonDoorCandidate>,
    },
}

impl Display for ObservedPistonDoorRecognitionError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::InvalidSnapshot(message) => {
                write!(
                    formatter,
                    "invalid observed piston-door snapshot: {message}"
                )
            }
            Self::DuplicatePosition(position) => {
                write!(
                    formatter,
                    "snapshot contains duplicate position {position:?}"
                )
            }
            Self::IncompleteObservation { required_positions } => write!(
                formatter,
                "observed piston-door candidate extends beyond the snapshot boundary at {} position(s)",
                required_positions.len()
            ),
            Self::NoThreeByThreeCandidate => {
                formatter.write_str("no unambiguous 3x3 piston-door candidate was observed")
            }
            Self::Ambiguous { candidates } => write!(
                formatter,
                "snapshot contains {} competing 3x3 piston-door candidates",
                candidates.len()
            ),
        }
    }
}

impl std::error::Error for ObservedPistonDoorRecognitionError {}

#[derive(Clone, Copy, Debug)]
struct PlaneAxes {
    width: Facing,
    height: Facing,
}

struct SnapshotIndex<'a> {
    snapshot: &'a MinecraftSnapshot,
    world: World,
    records: BTreeMap<Pos, &'a MinecraftSnapshotBlock>,
    bounds: RegionBounds,
}

impl<'a> SnapshotIndex<'a> {
    fn new(snapshot: &'a MinecraftSnapshot) -> Result<Self, ObservedPistonDoorRecognitionError> {
        if snapshot.min.x > snapshot.max.x
            || snapshot.min.y > snapshot.max.y
            || snapshot.min.z > snapshot.max.z
        {
            return Err(ObservedPistonDoorRecognitionError::InvalidSnapshot(
                "snapshot min must not exceed max".to_owned(),
            ));
        }
        let bounds = RegionBounds::new(snapshot.min, snapshot.max);
        let mut records = BTreeMap::new();
        for record in &snapshot.blocks {
            if !bounds.contains(record.pos) {
                return Err(ObservedPistonDoorRecognitionError::InvalidSnapshot(
                    format!(
                        "block at {:?} lies outside the declared snapshot bounds",
                        record.pos
                    ),
                ));
            }
            if records.insert(record.pos, record).is_some() {
                return Err(ObservedPistonDoorRecognitionError::DuplicatePosition(
                    record.pos,
                ));
            }
        }
        let world = world_from_snapshot(snapshot).map_err(|error| {
            ObservedPistonDoorRecognitionError::InvalidSnapshot(error.to_string())
        })?;
        Ok(Self {
            snapshot,
            world,
            records,
            bounds,
        })
    }

    fn contains(&self, position: Pos) -> bool {
        self.bounds.contains(position)
    }

    fn block(&self, position: Pos) -> Option<&Block> {
        self.world.get(position)
    }

    fn record(&self, position: Pos) -> Option<&MinecraftSnapshotBlock> {
        self.records.get(&position).copied()
    }

    fn kind_at(&self, position: Pos) -> BlockKind {
        self.world.kind_at(position)
    }

    fn is_boundary(&self, position: Pos) -> bool {
        position.x == self.snapshot.min.x
            || position.x == self.snapshot.max.x
            || position.y == self.snapshot.min.y
            || position.y == self.snapshot.max.y
            || position.z == self.snapshot.min.z
            || position.z == self.snapshot.max.z
    }

    fn block_evidence(&self, position: Pos) -> Option<ObservedBlockEvidence> {
        let block = self.block(position)?;
        let record = self.record(position)?;
        Some(ObservedBlockEvidence {
            position,
            kind: block.kind,
            observed_name: record.name.clone(),
            properties: record.properties.clone(),
        })
    }
}

/// Recognize a complete, two-sided 3x3 piston-door mechanical shape from an
/// observed snapshot.  The result is structural only; it never schedules a
/// piston or claims that the observed wiring is runnable.
pub fn recognize_observed_piston_door(
    snapshot: &MinecraftSnapshot,
) -> Result<ObservedPistonDoor, ObservedPistonDoorRecognitionError> {
    let index = SnapshotIndex::new(snapshot)?;
    // A two-sided door is intentionally represented by two directional
    // interpretations of the same mechanical shape.  Group by the complete
    // set of required positions so that this exact reverse pair is exposed as
    // role ambiguity instead of being mistaken for two competing doors.
    let mut candidate_groups = BTreeMap::<BTreeSet<Pos>, Vec<ObservedPistonDoorCandidate>>::new();
    let mut incomplete = BTreeSet::new();

    for (position, block) in index.world.iter() {
        if block.kind != BlockKind::Piston {
            continue;
        }
        let Some(open_direction) = observed_facing(&index, *position) else {
            continue;
        };
        let Some(axes) = plane_axes(open_direction) else {
            continue;
        };
        let open_positions = grid_positions(*position, axes);
        if !open_positions
            .iter()
            .all(|position| is_piston_facing(&index, *position, open_direction))
        {
            continue;
        }
        let close_direction = open_direction.opposite();
        let close_positions = open_positions
            .iter()
            .map(|position| offset_facing(*position, open_direction, 3))
            .collect::<Vec<_>>();
        let required_positions = open_positions
            .iter()
            .chain(&close_positions)
            .copied()
            .chain(
                open_positions
                    .iter()
                    .map(|position| offset_facing(*position, open_direction, 1)),
            )
            .chain(
                open_positions
                    .iter()
                    .map(|position| offset_facing(*position, open_direction, 2)),
            )
            .collect::<Vec<_>>();
        let outside = required_positions
            .iter()
            .copied()
            .filter(|position| !index.contains(*position))
            .collect::<BTreeSet<_>>();
        if !outside.is_empty() {
            incomplete.extend(outside);
            continue;
        }
        if !close_positions
            .iter()
            .all(|position| is_piston_facing(&index, *position, close_direction))
        {
            continue;
        }
        if !has_panel_evidence(&index, &open_positions, open_direction) {
            continue;
        }
        let candidate = ObservedPistonDoorCandidate {
            open_piston_origin: *position,
            open_direction,
            close_piston_origin: offset_facing(*position, open_direction, 3),
        };
        let mechanical_key = required_positions.into_iter().collect::<BTreeSet<_>>();
        candidate_groups
            .entry(mechanical_key)
            .or_default()
            .push(candidate);
    }

    if candidate_groups.is_empty() {
        if !incomplete.is_empty() {
            return Err(ObservedPistonDoorRecognitionError::IncompleteObservation {
                required_positions: incomplete.into_iter().collect(),
            });
        }
        return Err(ObservedPistonDoorRecognitionError::NoThreeByThreeCandidate);
    }
    if candidate_groups.len() != 1 {
        return Err(ObservedPistonDoorRecognitionError::Ambiguous {
            candidates: candidate_groups.into_values().flatten().collect(),
        });
    }
    let mut variants = candidate_groups
        .into_values()
        .next()
        .expect("candidate group count checked");
    variants.sort_by_key(|candidate| {
        (
            candidate.open_piston_origin,
            candidate.open_direction,
            candidate.close_piston_origin,
        )
    });
    let orientation_status = if variants.len() == 1 {
        ObservedPistonDoorOrientationStatus::Certain
    } else {
        ObservedPistonDoorOrientationStatus::Ambiguous
    };
    // Candidate generation walks the sorted world positions, which gives a
    // stable canonical orientation for symmetric layouts.  The alternative
    // interpretations remain in the JSON contract and the result is marked
    // ambiguous; no redstone or transition semantics are inferred here.
    let candidate = variants
        .first()
        .cloned()
        .expect("candidate group is non-empty");
    build_recognition(&index, candidate, orientation_status, variants)
}

fn build_recognition(
    index: &SnapshotIndex<'_>,
    candidate: ObservedPistonDoorCandidate,
    orientation_status: ObservedPistonDoorOrientationStatus,
    alternatives: Vec<ObservedPistonDoorCandidate>,
) -> Result<ObservedPistonDoor, ObservedPistonDoorRecognitionError> {
    let axes = plane_axes(candidate.open_direction).expect("candidate direction has axes");
    let open_origin = candidate.open_piston_origin;
    let close_origin = candidate.close_piston_origin;
    let closed_origin = offset_facing(open_origin, candidate.open_direction, 1);
    let open_plane_origin = offset_facing(open_origin, candidate.open_direction, 2);
    let orientation = ObservedPistonDoorOrientation {
        status: orientation_status,
        open_direction: candidate.open_direction,
        close_direction: candidate.open_direction.opposite(),
        width_axis: axes.width,
        height_axis: axes.height,
        alternatives,
    };
    let geometry = ObservedPistonDoorGeometry {
        open_piston_origin: open_origin,
        closed_plane_origin: closed_origin,
        open_plane_origin,
        close_piston_origin: close_origin,
        width: 3,
        height: 3,
    };

    let mut cells = Vec::with_capacity(9);
    let mut pistons = Vec::with_capacity(18);
    let mut piston_state_properties_complete = true;
    let mut cell_states = Vec::with_capacity(9);
    for grid_y in 0..3_u8 {
        for grid_x in 0..3_u8 {
            let open_piston = grid_position(open_origin, axes, grid_x, grid_y);
            let close_piston = offset_facing(open_piston, candidate.open_direction, 3);
            let closed_position = offset_facing(open_piston, candidate.open_direction, 1);
            let open_position = offset_facing(open_piston, candidate.open_direction, 2);
            let state = cell_state(
                &index.kind_at(closed_position),
                &index.kind_at(open_position),
            );
            let current_position = match state {
                ObservedPistonCellState::Closed => Some(closed_position),
                ObservedPistonCellState::Open => Some(open_position),
                ObservedPistonCellState::Transition => {
                    // A single transient block can still be located without
                    // guessing the door phase.  If both planes contain a
                    // block, leave the position unset because the snapshot
                    // does not identify which one is the carried panel.
                    match (
                        index.kind_at(closed_position) != BlockKind::Air,
                        index.kind_at(open_position) != BlockKind::Air,
                    ) {
                        (true, false) => Some(closed_position),
                        (false, true) => Some(open_position),
                        _ => None,
                    }
                }
                ObservedPistonCellState::Unknown => None,
            };
            let current_block =
                current_position.and_then(|position| index.block_evidence(position));
            cell_states.push(state);
            let open_observation =
                observed_piston(index, open_piston, ObservedPistonRole::OpenPusher).ok_or_else(
                    || {
                        ObservedPistonDoorRecognitionError::InvalidSnapshot(format!(
                            "piston at {open_piston:?} disappeared while building recognition"
                        ))
                    },
                )?;
            let close_observation =
                observed_piston(index, close_piston, ObservedPistonRole::ClosePusher).ok_or_else(
                    || {
                        ObservedPistonDoorRecognitionError::InvalidSnapshot(format!(
                            "piston at {close_piston:?} disappeared while building recognition"
                        ))
                    },
                )?;
            piston_state_properties_complete &=
                !matches!(open_observation.state, ObservedPistonState::Unknown)
                    && !matches!(close_observation.state, ObservedPistonState::Unknown);
            pistons.push(open_observation);
            pistons.push(close_observation);
            cells.push(ObservedPistonDoorCell {
                index: usize::from(grid_y) * 3 + usize::from(grid_x),
                grid_x,
                grid_y,
                closed_position,
                open_position,
                current_position,
                current_block,
                state,
                open_piston,
                close_piston,
            });
        }
    }

    let state = aggregate_state(&cell_states);
    let mut state_evidence = Vec::new();
    match state {
        ObservedPistonDoorState::Closed => state_evidence.push(
            "all nine cells occupy the closed plane and the open plane is observed as empty"
                .to_owned(),
        ),
        ObservedPistonDoorState::Open => state_evidence.push(
            "all nine cells occupy the open plane and the closed plane is observed as empty"
                .to_owned(),
        ),
        ObservedPistonDoorState::Transition => state_evidence.push(
            "cell occupancy is mixed or a transient piston part is present; stable state is not claimed"
                .to_owned(),
        ),
        ObservedPistonDoorState::Unknown => state_evidence.push(
            "the snapshot does not provide a complete, consistent occupancy for all nine cells"
                .to_owned(),
        ),
    }
    if !piston_state_properties_complete {
        state_evidence.push(
            "one or more piston extended properties are missing or invalid; piston phase is unknown"
                .to_owned(),
        );
    }
    if matches!(
        orientation_status,
        ObservedPistonDoorOrientationStatus::Ambiguous
    ) {
        state_evidence.push(
            "closed/open labels use a deterministic canonical orientation; the reverse role interpretation is retained"
                .to_owned(),
        );
    }

    let required_positions = cells
        .iter()
        .flat_map(|cell| [cell.closed_position, cell.open_position])
        .chain(pistons.iter().map(|piston| piston.position))
        .collect::<Vec<_>>();
    let required_bounds = bounds_for_positions(&required_positions)
        .expect("recognized 3x3 candidate always has required positions");
    let boundary_touched = required_positions
        .iter()
        .any(|position| index.is_boundary(*position));
    let control = control_summary(index, &pistons, boundary_touched);
    let mut unresolved = control.unresolved.clone();
    if matches!(state, ObservedPistonDoorState::Unknown) {
        unresolved.push("door state is unavailable from the observed occupancy".to_owned());
    }
    if matches!(state, ObservedPistonDoorState::Transition) {
        unresolved.push("door is observed during a mixed or transient state".to_owned());
    }
    if !piston_state_properties_complete {
        unresolved.push("piston extended state is incomplete in the snapshot".to_owned());
    }
    if matches!(
        orientation_status,
        ObservedPistonDoorOrientationStatus::Ambiguous
    ) {
        unresolved.push(
            "open/close role is ambiguous from a static symmetric mechanical shape; additional control or transition evidence is required"
                .to_owned(),
        );
    }
    unresolved.sort();
    unresolved.dedup();

    Ok(ObservedPistonDoor {
        schema_version: OBSERVED_PISTON_DOOR_SCHEMA.to_owned(),
        status: ObservedRecognitionStatus::Recognized,
        kind: "piston_door".to_owned(),
        size: [3, 3],
        orientation,
        geometry,
        state,
        state_evidence,
        cells,
        pistons,
        control,
        observation: ObservedPistonDoorObservation {
            snapshot_bounds: index.bounds,
            required_bounds,
            required_region_observed: required_positions
                .iter()
                .all(|position| index.contains(*position)),
            piston_state_properties_complete,
            boundary_touched,
        },
        unresolved,
    })
}

fn control_summary(
    index: &SnapshotIndex<'_>,
    pistons: &[ObservedPiston],
    boundary_touched: bool,
) -> ObservedPistonDoorControl {
    let analysis = analyze_world_region(&index.world, index.bounds);
    let piston_positions = pistons
        .iter()
        .map(|piston| piston.position)
        .collect::<BTreeSet<_>>();
    let input_positions = index
        .world
        .iter()
        .filter(|(_, block)| block.is_external_input_source())
        .map(|(position, _)| *position)
        .collect::<Vec<_>>();
    let wire_positions = index
        .world
        .iter()
        .filter(|(_, block)| block.kind == BlockKind::RedstoneWire)
        .map(|(position, _)| *position)
        .collect::<Vec<_>>();
    let repeater_positions = index
        .world
        .iter()
        .filter(|(_, block)| block.kind == BlockKind::Repeater)
        .map(|(position, _)| *position)
        .collect::<Vec<_>>();
    let piston_input_edges = analysis
        .graph
        .edges
        .iter()
        .filter(|edge| edge.kind == EdgeKind::PistonInput && piston_positions.contains(&edge.sink))
        .map(|edge| ObservedPistonInputEdge {
            source: edge.source,
            piston: edge.sink,
            kind: format!("{:?}", edge.kind).to_lowercase(),
        })
        .collect::<Vec<_>>();
    let mut reachable_pistons = BTreeSet::new();
    for input in &input_positions {
        reachable_pistons.extend(
            analysis
                .graph
                .reachable_from(*input)
                .intersection(&piston_positions)
                .copied(),
        );
    }
    let unreachable_pistons = piston_positions
        .difference(&reachable_pistons)
        .copied()
        .collect::<Vec<_>>();
    let mut unresolved = Vec::new();
    if input_positions.is_empty() {
        unresolved.push("no controllable input source was observed".to_owned());
    }
    if !unreachable_pistons.is_empty() {
        unresolved.push(format!(
            "{} of 18 piston inputs are not reachable from an observed source",
            unreachable_pistons.len()
        ));
    }
    if piston_input_edges.len() != pistons.len() {
        unresolved.push(format!(
            "only {} of {} direct piston input edges were observed; quasi-connectivity and update propagation are unavailable",
            piston_input_edges.len(),
            pistons.len()
        ));
    }
    if boundary_touched {
        unresolved.push(
            "the mechanical shape touches the snapshot boundary; external control may be outside the observation"
                .to_owned(),
        );
    }
    let complete = !input_positions.is_empty()
        && unreachable_pistons.is_empty()
        && piston_input_edges.len() == pistons.len()
        && !boundary_touched;
    ObservedPistonDoorControl {
        input_positions,
        wire_positions,
        repeater_positions,
        piston_input_edges,
        reachable_pistons: reachable_pistons.into_iter().collect(),
        unreachable_pistons,
        graph_node_count: analysis.graph.nodes.len(),
        graph_edge_count: analysis.graph.edges.len(),
        boundary_touched,
        complete,
        unresolved,
    }
}

fn observed_piston(
    index: &SnapshotIndex<'_>,
    position: Pos,
    role: ObservedPistonRole,
) -> Option<ObservedPiston> {
    let block = index.block(position)?;
    if block.kind != BlockKind::Piston {
        return None;
    }
    let record = index.record(position)?;
    let facing = observed_facing(index, position)?;
    let variant = block.piston_variant?;
    Some(ObservedPiston {
        position,
        role,
        facing,
        variant,
        state: observed_piston_state(index, position),
        observed_name: record.name.clone(),
        properties: record.properties.clone(),
    })
}

fn observed_piston_state(index: &SnapshotIndex<'_>, position: Pos) -> ObservedPistonState {
    let Some(block) = index.block(position) else {
        return ObservedPistonState::Unknown;
    };
    if block.kind == BlockKind::MovingPiston {
        return ObservedPistonState::Moving;
    }
    let Some(record) = index.record(position) else {
        return ObservedPistonState::Unknown;
    };
    match record.properties.get("extended").map(String::as_str) {
        Some("true") => ObservedPistonState::Extended,
        Some("false") => ObservedPistonState::Retracted,
        _ => ObservedPistonState::Unknown,
    }
}

fn observed_facing(index: &SnapshotIndex<'_>, position: Pos) -> Option<Facing> {
    let record = index.record(position)?;
    match record.properties.get("facing").map(String::as_str) {
        Some("north") => Some(Facing::North),
        Some("east") => Some(Facing::East),
        Some("south") => Some(Facing::South),
        Some("west") => Some(Facing::West),
        Some("up") => Some(Facing::Up),
        Some("down") => Some(Facing::Down),
        _ => None,
    }
}

fn is_piston_facing(index: &SnapshotIndex<'_>, position: Pos, facing: Facing) -> bool {
    index.kind_at(position) == BlockKind::Piston && observed_facing(index, position) == Some(facing)
}

fn plane_axes(normal: Facing) -> Option<PlaneAxes> {
    match normal {
        Facing::North | Facing::South => Some(PlaneAxes {
            width: Facing::East,
            height: Facing::Up,
        }),
        Facing::East | Facing::West => Some(PlaneAxes {
            width: Facing::South,
            height: Facing::Up,
        }),
        Facing::Up | Facing::Down => Some(PlaneAxes {
            width: Facing::East,
            height: Facing::South,
        }),
    }
}

fn grid_positions(origin: Pos, axes: PlaneAxes) -> Vec<Pos> {
    (0..3_u8)
        .flat_map(|grid_y| (0..3_u8).map(move |grid_x| grid_position(origin, axes, grid_x, grid_y)))
        .collect()
}

fn grid_position(origin: Pos, axes: PlaneAxes, grid_x: u8, grid_y: u8) -> Pos {
    offset_facing(
        offset_facing(origin, axes.width, i32::from(grid_x)),
        axes.height,
        i32::from(grid_y),
    )
}

fn offset_facing(position: Pos, facing: Facing, amount: i32) -> Pos {
    let offset = facing.offset();
    position.offset(offset.x * amount, offset.y * amount, offset.z * amount)
}

fn is_door_material(kind: BlockKind) -> bool {
    !matches!(
        kind,
        BlockKind::Air | BlockKind::Piston | BlockKind::PistonHead | BlockKind::MovingPiston
    ) && !kind.is_redstone_related()
}

fn has_panel_evidence(index: &SnapshotIndex<'_>, open_positions: &[Pos], normal: Facing) -> bool {
    open_positions.iter().any(|position| {
        let closed = offset_facing(*position, normal, 1);
        let open = offset_facing(*position, normal, 2);
        is_door_material(index.kind_at(closed))
            || is_door_material(index.kind_at(open))
            || matches!(
                index.kind_at(closed),
                BlockKind::PistonHead | BlockKind::MovingPiston
            )
            || matches!(
                index.kind_at(open),
                BlockKind::PistonHead | BlockKind::MovingPiston
            )
    })
}

fn cell_state(closed: &BlockKind, open: &BlockKind) -> ObservedPistonCellState {
    let closed_door = is_door_material(*closed);
    let open_door = is_door_material(*open);
    let transient = matches!(
        (closed, open),
        (BlockKind::PistonHead | BlockKind::MovingPiston, _)
            | (_, BlockKind::PistonHead | BlockKind::MovingPiston)
    );
    if transient {
        ObservedPistonCellState::Transition
    } else if closed_door && !open_door {
        ObservedPistonCellState::Closed
    } else if open_door && !closed_door {
        ObservedPistonCellState::Open
    } else if closed_door || open_door {
        ObservedPistonCellState::Transition
    } else {
        ObservedPistonCellState::Unknown
    }
}

fn aggregate_state(states: &[ObservedPistonCellState]) -> ObservedPistonDoorState {
    let set = states.iter().copied().collect::<BTreeSet<_>>();
    if set.len() == 1 {
        return match set.first().copied() {
            Some(ObservedPistonCellState::Closed) => ObservedPistonDoorState::Closed,
            Some(ObservedPistonCellState::Open) => ObservedPistonDoorState::Open,
            Some(ObservedPistonCellState::Transition) => ObservedPistonDoorState::Transition,
            Some(ObservedPistonCellState::Unknown) | None => ObservedPistonDoorState::Unknown,
        };
    }
    if set.contains(&ObservedPistonCellState::Transition)
        || (set.contains(&ObservedPistonCellState::Closed)
            && set.contains(&ObservedPistonCellState::Open))
    {
        ObservedPistonDoorState::Transition
    } else {
        ObservedPistonDoorState::Unknown
    }
}

fn bounds_for_positions(positions: &[Pos]) -> Option<RegionBounds> {
    let first = positions.first().copied()?;
    let (min, max) = positions
        .iter()
        .skip(1)
        .fold((first, first), |(min, max), pos| {
            (
                Pos::new(min.x.min(pos.x), min.y.min(pos.y), min.z.min(pos.z)),
                Pos::new(max.x.max(pos.x), max.y.max(pos.y), max.z.max(pos.z)),
            )
        });
    Some(RegionBounds::new(min, max))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn plane_axes_cover_all_piston_normals() {
        for normal in [
            Facing::North,
            Facing::East,
            Facing::South,
            Facing::West,
            Facing::Up,
            Facing::Down,
        ] {
            let axes = plane_axes(normal).expect("every normal has a plane");
            assert_ne!(axes.width, normal);
            assert_ne!(axes.height, normal);
            assert_ne!(axes.width, axes.height);
        }
    }

    #[test]
    fn mixed_cells_are_not_reported_as_stable() {
        assert_eq!(
            aggregate_state(&[
                ObservedPistonCellState::Closed,
                ObservedPistonCellState::Open,
            ]),
            ObservedPistonDoorState::Transition
        );
    }

    #[test]
    fn absent_panel_is_unknown() {
        assert_eq!(
            cell_state(&BlockKind::Air, &BlockKind::Air),
            ObservedPistonCellState::Unknown
        );
    }
}
