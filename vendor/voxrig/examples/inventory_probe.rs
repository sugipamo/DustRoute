use anyhow::Result;
use tokio::time::{Duration, timeout};
use voxrig::{BotManager, Event, Player, Server};

#[tokio::main]
async fn main() -> Result<()> {
    let manager = BotManager::new(Server::new("127.0.0.1", 25566));
    let bot = manager.connect(Player::offline("InventoryProbe")).await?;
    let mut events = bot.subscribe();
    bot.wait_until_ready().await?;
    println!("READY_FOR_GIVE");

    let deadline = tokio::time::Instant::now() + Duration::from_secs(30);
    let mut dropped = false;
    while tokio::time::Instant::now() < deadline {
        let event = timeout(Duration::from_secs(2), events.recv()).await??;
        if !matches!(
            event,
            Event::InventoryUpdated { .. } | Event::SlotUpdated(_)
        ) {
            continue;
        }
        let inventory = bot.inventory().await;
        let occupied = inventory
            .player_slots()
            .iter()
            .enumerate()
            .find_map(|(slot, item)| item.as_ref().map(|item| (slot, item.clone())));
        if let Some((slot @ 36..=44, item)) = occupied {
            if !dropped {
                println!("RECEIVED slot={slot} item={item:?}");
                bot.select_hotbar((slot - 36) as u8).await?;
                bot.drop_selected(true).await?;
                dropped = true;
            }
        } else if dropped {
            println!("DROPPED inventory={inventory:?}");
            manager.disconnect_all().await?;
            return Ok(());
        }
    }
    anyhow::bail!("inventory give/drop sequence timed out")
}
