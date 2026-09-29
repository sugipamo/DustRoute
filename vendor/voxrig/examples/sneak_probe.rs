use anyhow::{Result, ensure};
use std::time::Duration;
use voxrig::{BotManager, ControlState, Player, Server};

#[tokio::main]
async fn main() -> Result<()> {
    let port = std::env::var("MC_PORT")
        .unwrap_or_else(|_| "25565".into())
        .parse()?;
    let manager = BotManager::new(Server::new("127.0.0.1", port));
    let username = std::env::var("MC_USERNAME").unwrap_or_else(|_| "SneakProbe".into());
    let bot = manager.connect(Player::offline(username)).await?;
    bot.wait_until_ready().await?;
    bot.look(0.0, 0.0).await?;
    bot.set_control(ControlState {
        forward: true,
        sneak: true,
        ..Default::default()
    })
    .await;
    tokio::time::sleep(Duration::from_secs(3)).await;
    bot.clear_control().await;
    let player = bot.player().await;
    let metrics = bot.physics_metrics().await;
    ensure!(player.y >= 3.999_999, "sneak allowed fall: y={}", player.y);
    ensure!(player.z <= 3.300_001, "sneak crossed edge: z={}", player.z);
    ensure!(
        metrics.position_corrections == 0,
        "server corrected sneak edge protection"
    );
    println!(
        "edge position=({:.3},{:.3},{:.3}); metrics={metrics:?}",
        player.x, player.y, player.z
    );
    manager.disconnect_all().await?;
    Ok(())
}
