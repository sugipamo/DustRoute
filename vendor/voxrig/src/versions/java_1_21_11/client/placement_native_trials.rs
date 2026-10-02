//! Opt-in non-OP placement/material comparison on the dedicated fixture only.
use super::*;
use operations::{InventorySlot, PlacementStatus};
use serde_json::json;
use std::time::Duration;
async fn connect(name: &str, port: u16) -> Bot {
    let bot = Bot::connect(ConnectionConfig::offline(
        crate::Server::new("127.0.0.1", port),
        name,
        MinecraftVersion::Java1_21_11,
    ))
    .await
    .unwrap();
    bot.wait_until_ready().await.unwrap();
    bot
}
#[tokio::test]
#[ignore = "requires dedicated native survival fixture, console setup and explicit environment"]
async fn native_survival_placement_and_material_consumption() {
    use std::io::Write;
    let port: u16 = std::env::var("NATIVE_PLACEMENT_PORT")
        .unwrap()
        .parse()
        .unwrap();
    assert_eq!(port, 25572);
    let file = std::fs::OpenOptions::new()
        .create_new(true)
        .write(true)
        .open(std::env::var("NATIVE_PLACEMENT_OUTPUT").unwrap())
        .unwrap();
    let bot = connect("NatMineBot", port).await;
    let viewer = connect("NatMineView", port).await;
    println!(
        "FIXTURE placement: dry stone floor y=-61, air above; tp NatMineBot 0.5 -60 0.5; tp NatMineView 0.5 -60 4.5; clear NatMineBot; item replace entity NatMineBot inventory.0 with minecraft:dirt 3; enter"
    );
    std::io::stdout().flush().unwrap();
    assert!(
        timeout(
            Duration::from_secs(60),
            tokio::task::spawn_blocking(|| {
                let mut line = String::new();
                std::io::stdin().read_line(&mut line)
            })
        )
        .await
        .unwrap()
        .unwrap()
        .unwrap()
            > 0
    );
    let api = bot.operations();
    // The fixture baseline is checked from receipts, not a fixed login delay.
    timeout(Duration::from_secs(5), async {
        loop {
            let p = api.player_state().await.unwrap();
            if p.inventory.slots[9]
                == (InventorySlot::Item {
                    item: operations::default_item("dirt", 3).unwrap(),
                })
                && p.inventory.slots[36] == InventorySlot::Empty
                && api.standing_context().await.is_ok()
            {
                break;
            }
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
    })
    .await
    .unwrap();
    bot.start_packet_trace(8_388_608).await.unwrap();
    viewer.start_packet_trace(8_388_608).await.unwrap();
    let before = api.player_state().await.unwrap();
    assert_eq!(before.game_mode, Some(operations::GameMode::Survival));
    let swap = api.swap_player_hotbar(9, 0).await.unwrap();
    let swap_result = api
        .wait_inventory_swap(&swap, Duration::from_secs(3))
        .await
        .unwrap();
    api.select_hotbar(0).await.unwrap();
    let mut cases = Vec::new();
    for (support, face, point, target) in [
        (
            [2, -61, 0],
            crate::BlockFace::Up,
            [2.5, -60.0, 0.5],
            [2, -60, 0],
        ),
        (
            [2, -60, 0],
            crate::BlockFace::Up,
            [2.5, -59.0, 0.5],
            [2, -59, 0],
        ),
        (
            [2, -59, 0],
            crate::BlockFace::West,
            [2.0, -58.5, 0.5],
            [1, -59, 0],
        ),
    ] {
        let ground = api.standing_context().await.unwrap();
        let delta: [f64; 3] = std::array::from_fn(|i| point[i] - ground.eye_position[i]);
        let yaw = (-delta[0]).atan2(delta[2]).to_degrees() as f32;
        let pitch = (-delta[1]).atan2(delta[0].hypot(delta[2])).to_degrees() as f32;
        api.look([yaw, pitch]).await.unwrap();
        let begin = Instant::now();
        let intent = api.place_survival_cube(support, face).await.unwrap();
        assert_eq!(intent.target, target);
        assert!(api.select_hotbar(1).await.is_err());
        let result = api
            .wait_survival_placement(&intent, Duration::from_secs(3))
            .await
            .unwrap();
        assert!(
            matches!(result, PlacementStatus::ObservedPlaced { .. }),
            "{result:?}"
        );
        let observed = timeout(Duration::from_secs(3), async {
            loop {
                let observation = viewer
                    .observe_region(Region {
                        min: target,
                        max: target,
                    })
                    .await
                    .unwrap();
                if observation.blocks[0]
                    .state
                    .as_ref()
                    .is_some_and(|b| b.name == "minecraft:dirt")
                {
                    break observation;
                }
                tokio::time::sleep(Duration::from_millis(20)).await;
            }
        })
        .await
        .unwrap();
        cases.push(json!({"intent":intent,"result":result,"independent_observer":observed,"elapsed_ms":begin.elapsed().as_millis()}));
    }
    let after = api.player_state().await.unwrap();
    assert_eq!(after.inventory.slots[36], InventorySlot::Empty);
    assert_eq!(after.inventory.slots[9], InventorySlot::Empty);
    assert!(
        api.place_survival_cube([2, -59, 0], crate::BlockFace::West)
            .await
            .is_err()
    );
    // No additional world edit is sent for the empty-hand refusal.
    let refusal = api.operation_history().await;
    let trace = bot.stop_packet_trace().await.unwrap();
    let observer_trace = viewer.stop_packet_trace().await.unwrap();
    bot.disconnect().await.unwrap();
    viewer.disconnect().await.unwrap();
    serde_json::to_writer(file,&json!({"minecraft_version":"Java 1.21.11","scope":"isolated non-OP survival; console fixture setup only; ordinary inventory swap and three chained public-API placements", "before":before,"swap":swap_result,"cases":cases,"after":after,"empty_hand_refusal_history":refusal,"miner_trace":trace,"observer_trace":observer_trace})).unwrap();
}
