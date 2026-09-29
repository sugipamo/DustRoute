use anyhow::{Result, ensure};
use std::time::Duration;
use voxrig::{BotManager, ControlState, Player, Server};

#[tokio::main]
async fn main() -> Result<()> {
    let port = std::env::var("MC_PORT")
        .unwrap_or_else(|_| "25565".into())
        .parse()?;
    let manager = BotManager::new(Server::new("127.0.0.1", port));
    let username = std::env::var("BOT_USERNAME").unwrap_or_else(|_| "LiquidProbe".into());
    let bot = manager.connect(Player::offline(username)).await?;
    bot.wait_until_ready().await?;
    bot.observe(2).await?;
    let start = bot.player().await;
    let state = bot
        .block(
            start.x.floor() as i32,
            start.y.floor() as i32,
            start.z.floor() as i32,
        )
        .await;
    tokio::time::sleep(Duration::from_secs(2)).await;
    let sunk = bot.player().await;
    bot.set_control(ControlState {
        forward: true,
        jump: true,
        ..Default::default()
    })
    .await;
    tokio::time::sleep(Duration::from_secs(3)).await;
    bot.clear_control().await;
    let end = bot.player().await;
    let metrics = bot.physics_metrics().await;
    ensure!(
        sunk.y < start.y,
        "stationary bot in state {state:?} did not sink: {start:?} -> {sunk:?}"
    );
    ensure!(end.z > sunk.z + 1.0, "bot did not move through water");
    ensure!(end.y > sunk.y, "held jump did not lift bot in water");
    ensure!(
        metrics.position_corrections == 0,
        "server corrected liquid test: {metrics:?}"
    );
    println!("start={start:?}; sunk={sunk:?}; end={end:?}; metrics={metrics:?}");
    manager.disconnect_all().await?;
    Ok(())
}
