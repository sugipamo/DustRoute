use anyhow::{Result, ensure};
use std::time::Duration;
use voxrig::{BotManager, ControlState, Player, Server};

#[tokio::main]
async fn main() -> Result<()> {
    let port = std::env::var("MC_PORT")
        .unwrap_or_else(|_| "25565".into())
        .parse()?;
    let manager = BotManager::new(Server::new("127.0.0.1", port));
    let bot = manager.connect(Player::offline("StepProbe")).await?;
    bot.wait_until_ready().await?;
    bot.look(0.0, 0.0).await?;
    bot.set_control(ControlState {
        forward: true,
        ..Default::default()
    })
    .await;
    let mut max_y = bot.player().await.y;
    for _ in 0..200 {
        max_y = max_y.max(bot.player().await.y);
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    bot.clear_control().await;
    let player = bot.player().await;
    let metrics = bot.physics_metrics().await;
    ensure!(max_y >= 4.5, "did not step onto slab: max={max_y}");
    ensure!(player.z > 4.0, "did not pass slab: z={}", player.z);
    ensure!(
        metrics.position_corrections == 0,
        "server corrected step-up"
    );
    println!("max_y={max_y:.3}; z={:.3}; metrics={metrics:?}", player.z);
    manager.disconnect_all().await?;
    Ok(())
}
