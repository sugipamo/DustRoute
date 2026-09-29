use anyhow::{Result, ensure};
use std::time::Duration;
use voxrig::{BotManager, ControlState, Player, Server};

#[tokio::main]
async fn main() -> Result<()> {
    let port = std::env::var("MC_PORT")
        .unwrap_or_else(|_| "25565".into())
        .parse()?;
    let manager = BotManager::new(Server::new("127.0.0.1", port));
    let bot = manager.connect(Player::offline("WallProbe")).await?;
    bot.wait_until_ready().await?;
    bot.look(0.0, 0.0).await?;
    bot.set_control(ControlState {
        forward: true,
        ..Default::default()
    })
    .await;
    tokio::time::sleep(Duration::from_secs(10)).await;
    bot.clear_control().await;
    let player = bot.player().await;
    let motion = bot.motion().await;
    let metrics = bot.physics_metrics().await;
    ensure!(player.z <= 4.700_001, "player crossed wall: z={}", player.z);
    ensure!(
        motion.collided_horizontal,
        "horizontal collision was not recorded"
    );
    ensure!(
        metrics.position_corrections == 0,
        "server corrected wall collision"
    );
    println!(
        "position=({:.3},{:.3},{:.3}); motion={motion:?}; metrics={metrics:?}",
        player.x, player.y, player.z
    );
    manager.disconnect_all().await?;
    Ok(())
}
