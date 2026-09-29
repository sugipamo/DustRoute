use anyhow::{Context, Result, ensure};
use std::time::Duration;
use voxrig::{BotManager, ControlState, Player, Server};

#[tokio::main]
async fn main() -> Result<()> {
    let port = std::env::var("MC_PORT")
        .unwrap_or_else(|_| "25565".into())
        .parse()?;
    let manager = BotManager::new(Server::new("127.0.0.1", port));
    let bot = manager.connect(Player::offline("CourseProbe")).await?;
    bot.wait_until_ready().await?;
    wait_ground(&bot).await?;
    let baseline = bot.player().await.y;
    bot.jump().await?;
    wait_air(&bot).await?;
    let mut max_y = baseline;
    while !bot.player().await.on_ground {
        max_y = max_y.max(bot.player().await.y);
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    ensure!(
        max_y <= baseline + 0.200_001,
        "player crossed low ceiling: {max_y}"
    );

    bot.look(180.0, 0.0).await?;
    bot.set_control(ControlState {
        forward: true,
        ..Default::default()
    })
    .await;
    tokio::time::timeout(Duration::from_secs(5), async {
        while bot.player().await.y >= 3.9 {
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    })
    .await
    .context("bot did not fall into test hole")?;
    bot.clear_control().await;
    wait_ground(&bot).await?;
    let player = bot.player().await;
    ensure!(
        (player.y - 3.0).abs() < 1.0e-6,
        "expected hole floor y=3, got {}",
        player.y
    );
    let metrics = bot.physics_metrics().await;
    ensure!(
        metrics.position_corrections == 0,
        "server corrected course movement"
    );
    println!(
        "ceiling max={max_y:.3}; hole landing y={:.3}; metrics={metrics:?}",
        player.y
    );
    manager.disconnect_all().await?;
    Ok(())
}
async fn wait_ground(bot: &voxrig::Bot) -> Result<()> {
    tokio::time::timeout(Duration::from_secs(3), async {
        while !bot.player().await.on_ground {
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    })
    .await
    .context("ground wait timed out")?;
    Ok(())
}
async fn wait_air(bot: &voxrig::Bot) -> Result<()> {
    tokio::time::timeout(Duration::from_secs(3), async {
        while bot.player().await.on_ground {
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    })
    .await
    .context("air wait timed out")?;
    Ok(())
}
