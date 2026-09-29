use anyhow::{Result, ensure};
use std::time::Duration;
use voxrig::{BotManager, ControlState, Player, Server};

#[tokio::main]
async fn main() -> Result<()> {
    let port = std::env::var("MC_PORT")
        .unwrap_or_else(|_| "25565".into())
        .parse()?;
    let manager = BotManager::new(Server::new("127.0.0.1", port));
    let username = std::env::var("BOT_USERNAME").unwrap_or_else(|_| "ClimbProbe".into());
    let bot = manager.connect(Player::offline(username)).await?;
    bot.wait_until_ready().await?;
    let start = bot.player().await;
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
        end.y > start.y + 3.0,
        "bot did not climb ladder: {start:?} -> {end:?}"
    );
    ensure!(
        metrics.position_corrections == 0,
        "server corrected climb: {metrics:?}"
    );
    println!("start={start:?}; end={end:?}; metrics={metrics:?}");
    manager.disconnect_all().await?;
    Ok(())
}
