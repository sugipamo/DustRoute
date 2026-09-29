//! Single-player regression in a prepared, isolated Java 1.16.1 world.
use std::time::Duration;
use voxrig::versions::java_1_16_1::{BotManager, ChunkPos, ControlState, Player, Server};

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let port = std::env::var("MC_PORT")?.parse()?;
    let manager = BotManager::new(Server::new("127.0.0.1", port));
    let bot = manager.connect(Player::offline("VersionControl")).await?;
    bot.wait_until_ready().await?;
    let initial = bot.player().await;
    bot.wait_for_chunk(
        ChunkPos {
            x: (initial.x.floor() as i32).div_euclid(16),
            z: (initial.z.floor() as i32).div_euclid(16),
        },
        Duration::from_secs(5),
    )
    .await?;
    tokio::time::sleep(Duration::from_secs(2)).await;
    let before = bot.player().await;
    let before_metrics = bot.physics_metrics().await;
    bot.set_control(ControlState {
        forward: true,
        ..Default::default()
    })
    .await;
    tokio::time::sleep(Duration::from_secs(2)).await;
    bot.clear_control().await;
    let after = bot.player().await;
    let after_metrics = bot.physics_metrics().await;
    println!(
        "before={before:?} after={after:?} before_metrics={before_metrics:?} after_metrics={after_metrics:?}"
    );
    manager.disconnect_all().await?;
    anyhow::ensure!(
        (after.x - before.x).hypot(after.z - before.z) > 1.0,
        "movement failed"
    );
    anyhow::ensure!(
        before_metrics.position_corrections == after_metrics.position_corrections,
        "server corrected movement after initial settling"
    );
    Ok(())
}
