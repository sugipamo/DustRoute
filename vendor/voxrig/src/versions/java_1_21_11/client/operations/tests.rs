use super::*;

#[test]
fn inventory_is_received_only_and_unsupported_components_never_become_empty_slots() {
    let mut state = State {
        sequence: 9,
        ..State::default()
    };
    let mut full = vec![0, 0, 46];
    full.extend([0; 47]);
    receive(&mut state, ids::play_clientbound::WINDOW_ITEMS, &full).unwrap();
    assert!(
        state
            .operations
            .inventory
            .slots
            .iter()
            .all(|v| *v == InventorySlot::Empty)
    );
    let stone = items().iter().find(|i| i.name == "stone").unwrap().id;
    let mut changed = vec![0, 1];
    state.operations.inventory.pending_creative.push(0);
    state.operations.inventory.slots[36] = InventorySlot::Unavailable;
    put_varint(&mut changed, stone);
    changed.extend([0, 0]);
    receive(
        &mut state,
        ids::play_clientbound::SET_PLAYER_INVENTORY,
        &changed,
    )
    .unwrap();
    assert!(state.operations.inventory.pending_creative.is_empty());
    assert!(
        matches!(&state.operations.inventory.slots[36],InventorySlot::Item {item} if item.name=="minecraft:stone"&&item.count==1)
    );
    let prior = state.operations.inventory.clone();
    for length in 0..changed.len() {
        assert!(
            receive(
                &mut state,
                ids::play_clientbound::SET_PLAYER_INVENTORY,
                &changed[..length]
            )
            .is_err()
        );
        assert_eq!(state.operations.inventory.slots, prior.slots);
    }
    let mut unsupported = vec![0, 1];
    put_varint(&mut unsupported, stone);
    unsupported.extend([1, 0, 0]);
    receive(
        &mut state,
        ids::play_clientbound::SET_PLAYER_INVENTORY,
        &unsupported,
    )
    .unwrap();
    assert_eq!(
        state.operations.inventory.slots[36],
        InventorySlot::Unavailable
    );
    assert!(state.operations.inventory.unsupported_components);
    receive(&mut state, ids::play_clientbound::WINDOW_ITEMS, &full).unwrap();
    assert!(!state.operations.inventory.unsupported_components);
    // Other windows cannot silently preserve stale player inventory knowledge.
    receive(&mut state, ids::play_clientbound::WINDOW_ITEMS, &[1, 0, 0]).unwrap();
    assert!(
        state
            .operations
            .inventory
            .slots
            .iter()
            .all(|v| *v == InventorySlot::Unavailable)
    );
}

#[test]
fn game_mode_abilities_and_acknowledgements_are_validated_atomically() {
    let mut s = State::default();
    s.operations.reset_world(1).unwrap();
    assert_eq!(s.operations.game_mode, Some(GameMode::Creative));
    let mut abilities = vec![15];
    abilities.extend(0.05f32.to_be_bytes());
    abilities.extend(0.1f32.to_be_bytes());
    receive(&mut s, ids::play_clientbound::ABILITIES, &abilities).unwrap();
    assert_eq!(s.operations.abilities, Some(15));
    abilities[0] = 16;
    assert!(receive(&mut s, ids::play_clientbound::ABILITIES, &abilities).is_err());
    assert_eq!(s.operations.abilities, Some(15));
    receive(
        &mut s,
        ids::play_clientbound::ACKNOWLEDGE_PLAYER_DIGGING,
        &[3],
    )
    .unwrap();
    receive(
        &mut s,
        ids::play_clientbound::ACKNOWLEDGE_PLAYER_DIGGING,
        &[2],
    )
    .unwrap();
    assert_eq!(s.operations.ack, Some(3));
    let mut mode = vec![3];
    mode.extend(0.0f32.to_be_bytes());
    receive(&mut s, ids::play_clientbound::GAME_STATE_CHANGE, &mode).unwrap();
    assert_eq!(s.operations.game_mode, Some(GameMode::Survival));
    mode[1..].copy_from_slice(&f32::NAN.to_be_bytes());
    assert!(receive(&mut s, ids::play_clientbound::GAME_STATE_CHANGE, &mode).is_err());
    assert_eq!(s.operations.game_mode, Some(GameMode::Survival));
}

#[test]
fn item_registry_ids_are_unique_and_player_pose_rejects_nonfinite_values() {
    let mut ids = std::collections::BTreeSet::new();
    let mut names = std::collections::BTreeSet::new();
    for item in items() {
        assert!(ids.insert(item.id));
        assert!(names.insert(&item.name));
        assert!(item.stack_size > 0);
    }
    assert!(validate_pose([0.0; 3], [0.0, 90.0]).is_ok());
    assert!(validate_pose([f64::NAN; 3], [0.0; 2]).is_err());
    assert!(validate_pose([0.0; 3], [0.0, 91.0]).is_err());
    assert_eq!(
        super::super::super::wire::unpack_position(pack_position([-1, -64, 17]) as u64),
        [-1, -64, 17]
    );
}

#[test]
fn system_messages_preserve_native_components_and_mark_dropped_history() {
    let mut state = State::default();
    let message = b"\x0a\x08\x00\x04text\x00\x05hello\x00\x00";
    for sequence in 1..=130 {
        state.sequence = sequence;
        receive(&mut state, ids::play_clientbound::SYSTEM_CHAT, message).unwrap();
    }
    assert_eq!(state.operations.messages.len(), 128);
    assert_eq!(state.operations.messages_dropped_through, 2);
    assert_eq!(state.operations.messages[0].receive_sequence, 3);
    assert_eq!(
        state.operations.messages[0].component,
        Some(crate::text_component::TextNbt::Compound(
            std::collections::BTreeMap::from([(
                "text".into(),
                crate::text_component::TextNbt::String("hello".into())
            )])
        ))
    );
    assert!(
        receive(
            &mut state,
            ids::play_clientbound::SYSTEM_CHAT,
            &message[..message.len() - 1]
        )
        .is_err()
    );
    assert_eq!(state.operations.messages.len(), 128);
    state.operations.reset_configuration(130);
    assert!(state.operations.messages.is_empty());
    assert_eq!(state.operations.messages_dropped_through, 130);
}

#[test]
fn unprojectable_system_text_preserves_sequence_without_mutating_history_on_invalid_frames() {
    let mut state = State::default();
    let mut budget = vec![7];
    budget.extend(16384i32.to_be_bytes());
    budget.resize(budget.len() + 16384, 0);
    budget.push(1);
    let mut size = vec![8];
    size.extend(u16::MAX.to_be_bytes());
    size.resize(size.len() + usize::from(u16::MAX), b'a');
    size.push(0);
    let mut depth = vec![10];
    for _ in 0..33 {
        depth.extend([10, 0, 0]);
    }
    depth.extend([0; 34]);
    depth.push(0);
    for (index, data) in [budget, size, depth].into_iter().enumerate() {
        state.sequence = index as u64 + 1;
        receive(&mut state, ids::play_clientbound::SYSTEM_CHAT, &data).unwrap();
        let message = state.operations.messages.back().unwrap();
        assert_eq!(message.receive_sequence, state.sequence);
        assert!(message.component.is_none());
        assert_eq!(message.overlay, index == 0);
        let before = state.operations.messages.len();
        assert!(
            receive(
                &mut state,
                ids::play_clientbound::SYSTEM_CHAT,
                &data[..data.len() - 1]
            )
            .is_err()
        );
        assert_eq!(state.operations.messages.len(), before);
        let mut trailing = data;
        trailing.push(0);
        assert!(receive(&mut state, ids::play_clientbound::SYSTEM_CHAT, &trailing).is_err());
        assert_eq!(state.operations.messages.len(), before);
    }
}
