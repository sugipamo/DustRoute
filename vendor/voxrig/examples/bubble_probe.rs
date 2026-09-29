use anyhow::{Result, ensure};
use std::time::Duration;
use voxrig::{BotManager, Player, Server};

#[tokio::main]
async fn main() -> Result<()> {
    let port = std::env::var("MC_PORT")
        .unwrap_or_else(|_| "25565".into())
        .parse()?;
    let username = std::env::var("BOT_USERNAME").unwrap_or_else(|_| "BubbleProbe".into());
    let direction: i8 = std::env::var("DIRECTION")
        .unwrap_or_else(|_| "1".into())
        .parse()?;
    let manager = BotManager::new(Server::new("127.0.0.1", port));
    let bot = manager.connect(Player::offline(username)).await?;
    bot.wait_until_ready().await?;
    bot.observe(2).await?;
    let start = bot.player().await;
    let state = bot
        .block(
            start.x.floor() as i32,
            start.y.floor() as i32 - 2,
            start.z.floor() as i32,
        )
        .await;
    tokio::time::sleep(Duration::from_secs(3)).await;
    let end = bot.player().await;
    let metrics = bot.physics_metrics().await;
    ensure!(
        f64::from(direction) * (end.y - start.y) > 1.0,
        "bubble column state {state:?} moved the wrong way: {start:?} -> {end:?}"
    );
    ensure!(
        metrics.position_corrections == 0,
        "server corrected bubble test: {metrics:?}"
    );
    println!("state={state:?}; start={start:?}; end={end:?}; metrics={metrics:?}");
    manager.disconnect_all().await?;
    Ok(())
}
