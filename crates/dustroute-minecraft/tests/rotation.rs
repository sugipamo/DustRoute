use dustroute_minecraft::{Block, BlockKind, Facing, Pos, RotationY, WireConnection};

#[test]
fn quarter_turn_rotates_native_and_modeled_states_and_is_reversible() {
    let mut wire = Block::new(BlockKind::RedstoneWire);
    wire.support_offset = Some(Pos::new(0, -1, 0));
    wire.wire_connections = Some(
        [
            (Facing::North, WireConnection::Side),
            (Facing::East, WireConnection::Up),
        ]
        .into(),
    );
    wire.observed_properties = [
        ("north".into(), "side".into()),
        ("east".into(), "up".into()),
        ("power".into(), "3".into()),
    ]
    .into();
    let rotated = RotationY::R90.block(&wire);
    assert_eq!(rotated.observed_properties["east"], "side");
    assert_eq!(rotated.observed_properties["south"], "up");
    assert_eq!(rotated.observed_properties["power"], "3");
    assert_eq!(
        rotated.wire_connections.as_ref().unwrap()[&Facing::South],
        WireConnection::Up
    );
    assert_eq!(RotationY::R270.block(&rotated), wire);
    assert_eq!(RotationY::R90.checked_pos(Pos::new(0, 0, i32::MIN)), None);
    assert_eq!(RotationY::R90.then(RotationY::R270), RotationY::R0);
}
