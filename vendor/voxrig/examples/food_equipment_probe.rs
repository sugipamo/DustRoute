use anyhow::Result;
use tokio::time::{Duration, timeout};
use voxrig::{BotManager, ClickMode, ControlState, Hand, Player, Server};

#[tokio::main]
async fn main() -> Result<()> {
    let manager = BotManager::new(Server::new("127.0.0.1", 25566));
    let bot = manager.connect(Player::offline("FoodEquip3")).await?;
    let mut events = bot.subscribe();
    bot.wait_until_ready().await?;
    println!("READY_FOR_ITEMS");
    timeout(Duration::from_secs(20), async {
        loop {
            let inventory = bot.inventory().await;
            let apple = inventory
                .player_slots()
                .iter()
                .flatten()
                .any(|item| item.name() == Some("apple"));
            let shield = inventory
                .player_slots()
                .iter()
                .flatten()
                .any(|item| item.name() == Some("shield"));
            if apple && shield {
                return Ok::<(), anyhow::Error>(());
            }
            let _ = events.recv().await?;
        }
    })
    .await??;
    let shield_slot = bot
        .inventory()
        .await
        .player_slots()
        .iter()
        .enumerate()
        .find(|(_, item)| {
            item.as_ref()
                .is_some_and(|item| item.name() == Some("shield"))
        })
        .map(|(slot, _)| slot as i16)
        .unwrap();
    bot.click_slot_and_wait(0, shield_slot, 0, ClickMode::Normal)
        .await?;
    bot.click_slot_and_wait(0, 45, 0, ClickMode::Normal).await?;
    anyhow::ensure!(
        bot.inventory().await.player_slots()[45]
            .as_ref()
            .is_some_and(|item| item.name() == Some("shield"))
    );
    println!("EQUIPPED_SHIELD READY_FOR_HUNGER");
    bot.set_control(ControlState {
        forward: true,
        sprint: true,
        jump: true,
        ..Default::default()
    })
    .await;
    timeout(Duration::from_secs(45), async {
        loop {
            if bot
                .survival_state()
                .await
                .vitals
                .is_some_and(|vitals| vitals.food < 20)
            {
                return Ok::<(), anyhow::Error>(());
            }
            let _ = events.recv().await?;
        }
    })
    .await??;
    bot.clear_control().await;
    let (apple_slot, food_before) = {
        let inventory = bot.inventory().await;
        let slot = inventory.player_slots()[36..=44]
            .iter()
            .enumerate()
            .find(|(_, item)| {
                item.as_ref()
                    .is_some_and(|item| item.name() == Some("apple"))
            })
            .map(|(slot, _)| slot as u8)
            .unwrap();
        let food = bot.survival_state().await.vitals.unwrap().food;
        (slot, food)
    };
    bot.select_hotbar(apple_slot).await?;
    bot.use_item_for(Hand::Main, Duration::from_secs(2)).await?;
    timeout(Duration::from_secs(5), async {
        loop {
            if bot
                .survival_state()
                .await
                .vitals
                .is_some_and(|vitals| vitals.food > food_before)
            {
                return Ok::<(), anyhow::Error>(());
            }
            let _ = events.recv().await?;
        }
    })
    .await??;
    println!(
        "ATE food_before={food_before} state={:?}",
        bot.survival_state().await
    );
    manager.disconnect_all().await?;
    Ok(())
}
