use anyhow::Result;
use tokio::time::{Duration, timeout};
use voxrig::{BotManager, Player, Server};

#[tokio::main]
async fn main() -> Result<()> {
    let manager = BotManager::new(Server::new("127.0.0.1", 25566));
    let bot = manager.connect(Player::offline("CraftProbe6")).await?;
    let mut events = bot.subscribe();
    bot.wait_until_ready().await?;
    println!("READY_FOR_LOGS");

    timeout(Duration::from_secs(20), async {
        loop {
            let inventory = bot.inventory().await;
            if inventory
                .player_slots()
                .iter()
                .flatten()
                .any(|item| item.name() == Some("oak_log") && item.count >= 2)
            {
                return Ok::<(), anyhow::Error>(());
            }
            let _ = events.recv().await?;
        }
    })
    .await??;

    let one_log = [Some(37), None, None, None];
    bot.craft_once(0, 2, 2, &one_log).await?;
    bot.craft_once(0, 2, 2, &one_log).await?;
    let table = [Some(15), Some(15), Some(15), Some(15)];
    bot.craft_once(0, 2, 2, &table).await?;
    let sticks = [Some(15), None, Some(15), None];
    bot.craft_once(0, 2, 2, &sticks).await?;

    let inventory = bot.inventory().await;
    let has_table = inventory
        .player_slots()
        .iter()
        .flatten()
        .any(|item| item.name() == Some("crafting_table"));
    let has_sticks = inventory
        .player_slots()
        .iter()
        .flatten()
        .any(|item| item.name() == Some("stick") && item.count >= 4);
    anyhow::ensure!(
        has_table && has_sticks,
        "unexpected crafted inventory: {inventory:?}"
    );
    println!("CRAFTED {inventory:?}");
    manager.disconnect_all().await?;
    Ok(())
}
