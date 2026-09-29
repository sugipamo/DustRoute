use anyhow::{Result, ensure};
use std::time::Duration;
use voxrig::{BotManager, Player, Server};

#[tokio::main]
async fn main() -> Result<()> {
    let port = std::env::var("MC_PORT")
        .unwrap_or_else(|_| "25565".into())
        .parse()?;
    let manager = BotManager::new(Server::new("127.0.0.1", port));
    let bot = manager.connect(Player::offline("SpecialProbe")).await?;
    bot.wait_until_ready().await?;
    let start = bot.player().await;
    let mut landed = false;
    let mut rebound_height = 0.0_f64;
    for _ in 0..160 {
        tokio::time::sleep(Duration::from_millis(50)).await;
        let player = bot.player().await;
        if player.y < 1.01 {
            landed = true;
        } else if landed {
            rebound_height = rebound_height.max(player.y);
        }
    }
    let metrics = bot.physics_metrics().await;
    ensure!(landed, "bot never landed on slime from {start:?}");
    ensure!(
        rebound_height > 3.0,
        "slime did not bounce: max y={rebound_height}"
    );
    ensure!(
        metrics.position_corrections == 0,
        "server corrected bounce: {metrics:?}"
    );
    println!("start={start:?}; rebound_max_y={rebound_height:.3}; metrics={metrics:?}");
    manager.disconnect_all().await?;
    Ok(())
}
