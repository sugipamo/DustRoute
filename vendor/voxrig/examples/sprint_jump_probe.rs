use anyhow::{Context, Result, ensure};
use std::time::Duration;
use voxrig::{BotManager, ControlState, Player, Server};

#[tokio::main]
async fn main() -> Result<()> {
    let port = std::env::var("MC_PORT")
        .unwrap_or_else(|_| "25565".into())
        .parse()?;
    let manager = BotManager::new(Server::new("127.0.0.1", port));
    let bot = manager.connect(Player::offline("SprintJump")).await?;
    bot.wait_until_ready().await?;
    tokio::time::timeout(Duration::from_secs(3), async {
        while !bot.player().await.on_ground {
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    })
    .await
    .context("initial landing timed out")?;
    let start = bot.player().await;
    bot.set_control(ControlState {
        forward: true,
        sprint: true,
        jump: true,
        ..Default::default()
    })
    .await;
    tokio::time::sleep(Duration::from_millis(100)).await;
    bot.set_control(ControlState {
        forward: true,
        sprint: true,
        ..Default::default()
    })
    .await;
    tokio::time::timeout(Duration::from_secs(3), async {
        while bot.player().await.on_ground {
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
        while !bot.player().await.on_ground {
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    })
    .await
    .context("sprint jump timed out")?;
    bot.clear_control().await;
    let end = bot.player().await;
    let distance = (end.x - start.x).hypot(end.z - start.z);
    let metrics = bot.physics_metrics().await;
    ensure!(distance > 2.0, "sprint jump distance too short: {distance}");
    ensure!(
        metrics.position_corrections == 0,
        "server corrected sprint jump"
    );
    println!("distance={distance:.3}; metrics={metrics:?}");
    manager.disconnect_all().await?;
    Ok(())
}
