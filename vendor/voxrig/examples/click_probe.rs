use anyhow::Result;
use tokio::time::{Duration, timeout};
use voxrig::{BotManager, ClickMode, Player, Server};

#[tokio::main]
async fn main() -> Result<()> {
    let manager = BotManager::new(Server::new("127.0.0.1", 25566));
    let bot = manager.connect(Player::offline("ClickProbe")).await?;
    let mut events = bot.subscribe();
    bot.wait_until_ready().await?;
    println!("READY");
    loop {
        let _ = events.recv().await?;
        if bot
            .inventory()
            .await
            .player_slots()
            .get(36)
            .is_some_and(Option::is_some)
        {
            break;
        }
    }
    println!("BEFORE {:?}", bot.inventory().await);
    bot.click_slot(0, 36, 0, ClickMode::Normal).await?;
    for _ in 0..10 {
        match timeout(Duration::from_millis(500), events.recv()).await {
            Ok(Ok(event)) => println!("EVENT {event:?}"),
            _ => break,
        }
    }
    println!("AFTER {:?}", bot.inventory().await);
    manager.disconnect_all().await?;
    Ok(())
}
