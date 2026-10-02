//! Received inventory races retain evidence and never release the old miner.
use super::*;

fn slot_packet(slot: u8, item: bool) -> Vec<u8> {
    let mut p = vec![0, 2, 0, slot];
    if item {
        p.push(1);
        put_varint(&mut p, default_item("dirt", 1).unwrap().item_id);
        p.extend([0, 0]);
    } else {
        p.push(0);
    }
    p
}
async fn inspection(f: &Fixture, intent: &MiningIntent) -> MiningRecord {
    let MiningStatus::RequiresInspection { record } =
        f.api.observe_survival_mining(intent).await.unwrap()
    else {
        panic!("expected retained inspection");
    };
    record
}

#[tokio::test]
async fn hand_change_is_latched_before_poll_and_survives_empty_and_air() {
    let mut f = Fixture::new().await;
    let intent = f.start().await;
    f.receive(ids::play_clientbound::SET_SLOT, &slot_packet(37, true))
        .await;
    f.receive(ids::play_clientbound::SET_SLOT, &slot_packet(36, false))
        .await;
    assert!(matches!(
        f.api.observe_survival_mining(&intent).await.unwrap(),
        MiningStatus::Mining { .. }
    ));
    f.receive(ids::play_clientbound::SET_SLOT, &slot_packet(36, true))
        .await;
    // No status poll between the occupied and empty receipts.
    let first = f.api.operation_history().await.mining.unwrap();
    let change = first.inventory_change.unwrap();
    assert_eq!(change.kind, MiningInventoryChangeKind::SelectedHandChanged);
    assert!(change.sole_cause);
    assert_eq!(change.hand_receive_sequence, Some(change.receive_sequence));
    assert!(change.receive_sequence > intent.after_sequence);
    assert!(matches!(change.original_hand, InventorySlot::Item { .. }));
    f.receive(ids::play_clientbound::SET_SLOT, &slot_packet(36, false))
        .await;
    f.change(TARGET, &native("air")).await;
    let current = inspection(&f, &intent).await;
    assert_eq!(
        current.inventory_change.unwrap().receive_sequence,
        change.receive_sequence
    );
    assert!(current.removal.is_none());
    assert!(f.api.finish_survival_mining(&intent).await.is_err());
    assert!(f.api.select_hotbar(1).await.is_err());
    assert!(
        timeout(Duration::from_millis(20), read_packet(&mut f.peer, None))
            .await
            .is_err()
    );
    // Inventory changes do not poison the independent retirement watch.
    let mut observer = Fixture::new_id(43).await;
    observer.profile(42).await;
    let source = crate::Client::from_java_1_21_11(f.api.bot.clone())
        .survival()
        .unwrap();
    let independent = crate::Client::from_java_1_21_11(observer.api.bot.clone())
        .survival()
        .unwrap();
    let retirement = source
        .prepare_mining_retirement(&intent, &independent)
        .await
        .unwrap();
    retirement.close_source().await.unwrap();
    observer.remove_profile(42).await;
    assert!(matches!(
        retirement.wait(Duration::from_millis(20)).await.unwrap(),
        MiningRetirementStatus::Retired { .. }
    ));
    assert!(
        retirement
            .source_history()
            .await
            .mining
            .unwrap()
            .inventory_change
            .is_some()
    );
    f.stop().await;
    observer.stop().await;
}

#[tokio::test]
async fn inventory_diagnosis_distinguishes_selection_cursor_and_screen() {
    for (id, payload, expected) in [
        (
            ids::play_clientbound::HELD_ITEM_SLOT,
            vec![1],
            MiningInventoryChangeKind::SelectionChanged,
        ),
        (
            ids::play_clientbound::SET_CURSOR_ITEM,
            slot_packet(36, true)[4..].to_vec(),
            MiningInventoryChangeKind::PlayerScreenChanged,
        ),
        (
            ids::play_clientbound::SET_SLOT,
            vec![1, 2, 0, 36, 0],
            MiningInventoryChangeKind::PlayerScreenChanged,
        ),
    ] {
        let mut f = Fixture::new().await;
        let intent = f.start().await;
        f.receive(id, &payload).await;
        let record = inspection(&f, &intent).await;
        assert_eq!(record.inventory_change.unwrap().kind, expected);
        assert!(f.api.finish_survival_mining(&intent).await.is_err());
        f.stop().await;
    }
}

#[tokio::test]
async fn foreign_target_conflicts_are_not_reclassified_as_inventory_only() {
    for inventory_first in [false, true] {
        let mut f = Fixture::new().await;
        let intent = f.start().await;
        if inventory_first {
            f.receive(ids::play_clientbound::SET_SLOT, &slot_packet(36, true))
                .await;
        }
        f.change(TARGET, &native("dirt")).await;
        if !inventory_first {
            f.receive(ids::play_clientbound::SET_SLOT, &slot_packet(36, true))
                .await;
        }
        let record = inspection(&f, &intent).await;
        assert!(!record.inventory_change.unwrap().sole_cause);
        f.stop().await;
    }
}

#[tokio::test]
async fn dig_helper_returns_inventory_inspection_without_finish_or_replay() {
    let mut f = Fixture::new().await;
    f.change(TARGET, &native("dirt")).await;
    let api = f.api.clone();
    let run = tokio::spawn(async move {
        api.dig_survival_cube(TARGET, crate::BlockFace::West, Duration::from_secs(2))
            .await
    });
    let start = read_packet(&mut f.peer, None).await.unwrap();
    assert_eq!(start.0, ids::play_serverbound::BLOCK_DIG);
    assert_eq!(start.1[0], 0);
    f.receive(ids::play_clientbound::SET_SLOT, &slot_packet(36, true))
        .await;
    let MiningStatus::RequiresInspection { record } = run.await.unwrap().unwrap() else {
        panic!("helper did not retain typed inspection");
    };
    assert!(record.start_dispatched);
    assert!(record.finish.is_none());
    assert!(record.inventory_change.unwrap().sole_cause);
    assert!(
        timeout(Duration::from_millis(20), read_packet(&mut f.peer, None))
            .await
            .is_err()
    );
    f.stop().await;
}
