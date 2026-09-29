use anyhow::{Result, ensure};
use std::time::Duration;
use voxrig::{BotManager, ControlState, Player, Server};

#[tokio::main]
async fn main() -> Result<()> {
    let port = std::env::var("MC_PORT")
        .unwrap_or_else(|_| "25565".into())
        .parse()?;
    let manager = BotManager::new(Server::new("127.0.0.1", port));
    let bot = manager.connect(Player::offline("FlatProbe")).await?;
    bot.wait_until_ready().await?;
    let before = bot.player().await;
    bot.look(90.0, 0.0).await?;
    bot.set_control(ControlState {
        forward: true,
        ..Default::default()
    })
    .await;
    tokio::time::sleep(Duration::from_secs(26)).await;
    bot.clear_control().await;
    let after = bot.player().await;
    let distance = (after.x - before.x).hypot(after.z - before.z);
    let metrics = bot.physics_metrics().await;
    ensure!(distance >= 100.0, "only moved {distance:.2} blocks");
    ensure!(
        (after.y - before.y).abs() < 1.0e-6,
        "height changed on flat course"
    );
    ensure!(
        metrics.position_corrections == 0,
        "server corrected flat movement"
    );
    println!(
        "distance={distance:.2}; y={:.3}; metrics={metrics:?}",
        after.y
    );
    manager.disconnect_all().await?;
    Ok(())
}
