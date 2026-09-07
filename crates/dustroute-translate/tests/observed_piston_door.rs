use std::collections::BTreeMap;

use dustroute_translate::{
    Facing, MinecraftSnapshot, MinecraftSnapshotBlock, ObservedPistonCellState, ObservedPistonDoor,
    ObservedPistonDoorOrientationStatus, ObservedPistonDoorRecognitionError,
    ObservedPistonDoorState, Pos, recognize_observed_piston_door,
};

const FIXTURE: &str = include_str!("fixtures/observed_3x3_piston_door_closed.json");

fn fixture() -> MinecraftSnapshot {
    serde_json::from_str(FIXTURE).expect("observed fixture must be valid JSON")
}

fn translated(mut snapshot: MinecraftSnapshot, offset: Pos) -> MinecraftSnapshot {
    snapshot.min = snapshot.min.offset(offset.x, offset.y, offset.z);
    snapshot.max = snapshot.max.offset(offset.x, offset.y, offset.z);
    for block in &mut snapshot.blocks {
        block.pos = block.pos.offset(offset.x, offset.y, offset.z);
    }
    snapshot
}

fn open_snapshot() -> MinecraftSnapshot {
    let mut snapshot = fixture();
    snapshot.blocks.retain(|block| block.pos.z != 0);
    for x in 0..3 {
        for y in 0..3 {
            snapshot.blocks.push(MinecraftSnapshotBlock {
                pos: Pos::new(x, y, 1),
                name: "minecraft:stone".to_owned(),
                properties: BTreeMap::new(),
            });
        }
    }
    snapshot
}

#[test]
fn recognizes_closed_observed_reference_and_reports_control_boundary() {
    let recognition = recognize_observed_piston_door(&fixture()).expect("fixture should match");
    assert_eq!(
        recognition.schema_version,
        "dustroute.observed-3x3-piston-door.v1"
    );
    assert_eq!(recognition.kind, "piston_door");
    assert_eq!(recognition.size, [3, 3]);
    assert_eq!(recognition.state, ObservedPistonDoorState::Closed);
    assert_eq!(recognition.orientation.open_direction, Facing::South);
    assert_eq!(recognition.orientation.close_direction, Facing::North);
    assert_eq!(
        recognition.orientation.status,
        ObservedPistonDoorOrientationStatus::Ambiguous
    );
    assert_eq!(recognition.orientation.alternatives.len(), 2);
    assert_eq!(recognition.cells.len(), 9);
    assert_eq!(recognition.pistons.len(), 18);
    assert!(
        recognition
            .pistons
            .iter()
            .all(|piston| piston.state == dustroute_translate::ObservedPistonState::Retracted)
    );
    assert_eq!(recognition.control.repeater_positions.len(), 18);
    assert_eq!(recognition.control.piston_input_edges.len(), 18);
    assert!(!recognition.control.complete);
    assert!(
        recognition
            .control
            .unresolved
            .iter()
            .any(|reason| reason.contains("controllable input"))
    );
    assert!(recognition.observation.required_region_observed);
    assert!(recognition.observation.piston_state_properties_complete);
    let encoded = serde_json::to_string(&recognition).expect("recognition is JSON serializable");
    let decoded: ObservedPistonDoor =
        serde_json::from_str(&encoded).expect("recognition JSON round-trips");
    assert_eq!(decoded, recognition);
}

#[test]
fn recognizes_translated_open_reference_without_inventing_a_control_source() {
    let recognition =
        recognize_observed_piston_door(&translated(open_snapshot(), Pos::new(37, 11, 29)))
            .expect("translated open fixture should match");
    assert_eq!(recognition.state, ObservedPistonDoorState::Open);
    assert_eq!(
        recognition.geometry.open_piston_origin,
        Pos::new(37, 11, 28)
    );
    assert_eq!(
        recognition.geometry.closed_plane_origin,
        Pos::new(37, 11, 29)
    );
    assert_eq!(recognition.geometry.open_plane_origin, Pos::new(37, 11, 30));
    assert!(
        recognition
            .cells
            .iter()
            .all(|cell| cell.state == ObservedPistonCellState::Open)
    );
    assert!(recognition.control.input_positions.is_empty());
    assert!(!recognition.control.complete);
}

#[test]
fn vertical_mechanical_shape_is_recognized_but_control_remains_explicitly_unresolved() {
    let mut blocks = Vec::new();
    for x in 0..3 {
        for z in 0..3 {
            blocks.push(MinecraftSnapshotBlock {
                pos: Pos::new(x, 0, z),
                name: "minecraft:stone".to_owned(),
                properties: BTreeMap::new(),
            });
            blocks.push(MinecraftSnapshotBlock {
                pos: Pos::new(x, -1, z),
                name: "minecraft:piston".to_owned(),
                properties: BTreeMap::from([
                    ("facing".to_owned(), "up".to_owned()),
                    ("extended".to_owned(), "false".to_owned()),
                ]),
            });
            blocks.push(MinecraftSnapshotBlock {
                pos: Pos::new(x, 2, z),
                name: "minecraft:piston".to_owned(),
                properties: BTreeMap::from([
                    ("facing".to_owned(), "down".to_owned()),
                    ("extended".to_owned(), "false".to_owned()),
                ]),
            });
        }
    }
    let snapshot = MinecraftSnapshot {
        min: Pos::new(-1, -2, -1),
        max: Pos::new(3, 3, 3),
        blocks,
    };
    let recognition = recognize_observed_piston_door(&snapshot).expect("vertical shape matches");
    assert_eq!(recognition.orientation.open_direction, Facing::Up);
    assert_eq!(recognition.orientation.width_axis, Facing::East);
    assert_eq!(recognition.orientation.height_axis, Facing::South);
    assert_eq!(recognition.state, ObservedPistonDoorState::Closed);
    assert_eq!(recognition.pistons.len(), 18);
    assert!(!recognition.control.complete);
    assert!(recognition.control.piston_input_edges.is_empty());
    assert!(
        recognition
            .control
            .unresolved
            .iter()
            .any(|reason| reason.contains("direct piston input edges"))
    );
}

#[test]
fn non_three_by_three_shapes_fail_closed() {
    let mut snapshot = fixture();
    snapshot
        .blocks
        .retain(|block| block.pos != Pos::new(2, 2, -1));
    let error = recognize_observed_piston_door(&snapshot).expect_err("2x3 shape must be rejected");
    assert_eq!(
        error,
        ObservedPistonDoorRecognitionError::NoThreeByThreeCandidate
    );
}

#[test]
fn competing_door_shapes_fail_closed_as_ambiguous() {
    let first = fixture();
    let second = translated(fixture(), Pos::new(12, 0, 0));
    let mut blocks = first.blocks;
    blocks.extend(second.blocks);
    let snapshot = MinecraftSnapshot {
        min: Pos::new(-4, -1, -4),
        max: Pos::new(16, 3, 5),
        blocks,
    };
    let error =
        recognize_observed_piston_door(&snapshot).expect_err("two shapes must be ambiguous");
    assert!(matches!(
        error,
        ObservedPistonDoorRecognitionError::Ambiguous { .. }
    ));
}

#[test]
fn missing_required_region_fails_closed_as_incomplete() {
    let mut snapshot = fixture();
    snapshot.max.z = 1;
    snapshot.blocks.retain(|block| block.pos.z <= 1);
    let error =
        recognize_observed_piston_door(&snapshot).expect_err("missing close side must fail");
    assert!(matches!(
        error,
        ObservedPistonDoorRecognitionError::IncompleteObservation { .. }
    ));
}

#[test]
fn missing_piston_state_is_reported_without_guessing_the_piston_phase() {
    let mut snapshot = fixture();
    let piston = snapshot
        .blocks
        .iter_mut()
        .find(|block| block.name == "minecraft:piston")
        .expect("open piston");
    piston.properties.remove("extended");
    let recognition = recognize_observed_piston_door(&snapshot).expect("shape remains observable");
    assert_eq!(recognition.state, ObservedPistonDoorState::Closed);
    assert!(!recognition.observation.piston_state_properties_complete);
    assert!(
        recognition
            .unresolved
            .iter()
            .any(|reason| reason.contains("extended state"))
    );
    assert_eq!(
        recognition.pistons[0].state,
        dustroute_translate::ObservedPistonState::Unknown
    );
}

#[test]
fn transient_panel_part_is_exposed_and_door_state_is_transition() {
    let mut snapshot = fixture();
    snapshot
        .blocks
        .retain(|block| block.pos != Pos::new(0, 0, 0));
    snapshot.blocks.push(MinecraftSnapshotBlock {
        pos: Pos::new(0, 0, 1),
        name: "minecraft:piston_head".to_owned(),
        properties: BTreeMap::from([
            ("facing".to_owned(), "south".to_owned()),
            ("short".to_owned(), "false".to_owned()),
            ("type".to_owned(), "normal".to_owned()),
        ]),
    });

    let recognition = recognize_observed_piston_door(&snapshot).expect("transition is observable");
    assert_eq!(recognition.state, ObservedPistonDoorState::Transition);
    let cell = recognition.cells.first().expect("row-major cell exists");
    assert_eq!(cell.state, ObservedPistonCellState::Transition);
    assert_eq!(cell.current_position, Some(Pos::new(0, 0, 1)));
    assert_eq!(
        cell.current_block
            .as_ref()
            .map(|block| block.observed_name.as_str()),
        Some("minecraft:piston_head")
    );
}

#[test]
fn moving_piston_panel_part_is_retained_as_transition_evidence() {
    let mut snapshot = fixture();
    snapshot
        .blocks
        .retain(|block| block.pos != Pos::new(0, 0, 0));
    snapshot.blocks.push(MinecraftSnapshotBlock {
        pos: Pos::new(0, 0, 0),
        name: "minecraft:moving_piston".to_owned(),
        properties: BTreeMap::from([
            ("facing".to_owned(), "south".to_owned()),
            ("type".to_owned(), "normal".to_owned()),
        ]),
    });

    let recognition =
        recognize_observed_piston_door(&snapshot).expect("moving state is observable");
    assert_eq!(recognition.state, ObservedPistonDoorState::Transition);
    let cell = recognition.cells.first().expect("row-major cell exists");
    assert_eq!(cell.state, ObservedPistonCellState::Transition);
    assert_eq!(cell.current_position, Some(Pos::new(0, 0, 0)));
    assert_eq!(
        cell.current_block.as_ref().map(|block| block.kind),
        Some(dustroute_translate::BlockKind::MovingPiston)
    );
}

#[test]
fn missing_panel_occupancy_is_unknown_instead_of_inferred_open_or_closed() {
    let mut snapshot = fixture();
    snapshot
        .blocks
        .retain(|block| block.pos.z != 0 || block.pos == Pos::new(0, 0, 0));

    let recognition = recognize_observed_piston_door(&snapshot).expect("mechanical shape remains");
    assert_eq!(recognition.state, ObservedPistonDoorState::Unknown);
    assert!(recognition.cells.iter().any(|cell| {
        cell.state == ObservedPistonCellState::Unknown && cell.current_position.is_none()
    }));
    assert!(
        recognition
            .unresolved
            .iter()
            .any(|reason| reason.contains("door state is unavailable"))
    );
}

#[test]
fn block_outside_declared_bounds_is_rejected_as_invalid_snapshot() {
    let mut snapshot = fixture();
    snapshot.blocks.push(MinecraftSnapshotBlock {
        pos: Pos::new(99, 0, 0),
        name: "minecraft:stone".to_owned(),
        properties: BTreeMap::new(),
    });
    let error =
        recognize_observed_piston_door(&snapshot).expect_err("out-of-bounds data is invalid");
    assert!(matches!(
        error,
        ObservedPistonDoorRecognitionError::InvalidSnapshot(_)
    ));
}
