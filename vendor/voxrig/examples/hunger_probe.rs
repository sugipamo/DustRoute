use anyhow::Result;
use tokio::time::Duration;
use voxrig::{BotManager, ControlState, Hand, Player, Server};

#[tokio::main]
async fn main() -> Result<()> {
    let manager = BotManager::new(Server::new("127.0.0.1", 25566));
    let bot = manager.connect(Player::offline("HungerProbe2")).await?;
    bot.wait_until_ready().await?;
    println!("READY_FOR_APPLE_AND_HUNGER");
    bot.set_control(ControlState {
        forward: true,
        sprint: true,
        ..Default::default()
    })
    .await;
    for second in 0..30 {
        tokio::time::sleep(Duration::from_secs(1)).await;
        let state = bot.survival_state().await;
        println!(
            "SECOND {second} vitals={:?} effects={:?}",
            state.vitals, state.effects
        );
        if let Some(vitals) = state.vitals {
            if vitals.food >= 20 {
                continue;
            }
            bot.clear_control().await;
            let inventory = bot.inventory().await;
            let slot = inventory.player_slots()[36..=44]
                .iter()
                .enumerate()
                .find(|(_, item)| {
                    item.as_ref()
                        .is_some_and(|item| item.name() == Some("apple"))
                })
                .map(|(slot, _)| slot as u8)
                .ok_or_else(|| anyhow::anyhow!("apple missing"))?;
            bot.select_hotbar(slot).await?;
            bot.use_item_for(Hand::Main, Duration::from_secs(2)).await?;
            tokio::time::sleep(Duration::from_secs(1)).await;
            let after = bot.survival_state().await.vitals.unwrap();
            anyhow::ensure!(
                after.food > vitals.food,
                "food did not increase: {vitals:?} -> {after:?}"
            );
            println!("ATE {vitals:?} -> {after:?}");
            manager.disconnect_all().await?;
            return Ok(());
        }
    }
    anyhow::bail!("hunger did not decrease")
}
