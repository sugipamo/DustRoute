use anyhow::Result;
use tokio::time::{Duration, timeout};
use voxrig::{BotManager, Player, Server};

#[tokio::main]
async fn main() -> Result<()> {
    let manager = BotManager::new(Server::new("127.0.0.1", 25566));
    let bot = manager.connect(Player::offline("StateProbe")).await?;
    let mut events = bot.subscribe();
    bot.wait_until_ready().await?;

    let deadline = tokio::time::Instant::now() + Duration::from_secs(10);
    while tokio::time::Instant::now() < deadline {
        if let Ok(Ok(event)) = timeout(Duration::from_millis(500), events.recv()).await {
            println!("EVENT {event:?}");
        }
        let state = bot.survival_state().await;
        if state.vitals.is_some()
            && state.difficulty.is_some()
            && !state.attributes.is_empty()
            && state.spawn_position.is_some()
        {
            println!("STATE {state:?}");
            manager.disconnect_all().await?;
            return Ok(());
        }
    }
    anyhow::bail!(
        "survival state did not become complete: {:?}",
        bot.survival_state().await
    )
}
