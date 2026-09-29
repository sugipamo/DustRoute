use anyhow::Result;
use tokio::time::Duration;
use voxrig::{BotManager, Player, Server};

#[tokio::main]
async fn main() -> Result<()> {
    let username = std::env::args()
        .nth(1)
        .unwrap_or_else(|| "InspectProbe".into());
    let manager = BotManager::new(Server::new("127.0.0.1", 25566));
    let bot = manager.connect(Player::offline(username)).await?;
    bot.wait_until_ready().await?;
    tokio::time::sleep(Duration::from_secs(1)).await;
    for (slot, item) in bot.inventory().await.player_slots().iter().enumerate() {
        if let Some(item) = item {
            println!("slot={slot} name={:?} item={item:?}", item.name());
        }
    }
    manager.disconnect_all().await?;
    Ok(())
}
