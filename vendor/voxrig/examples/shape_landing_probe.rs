use anyhow::{Context, Result, ensure};
use std::time::Duration;
use voxrig::{BotManager, Player, Server};

#[tokio::main]
async fn main() -> Result<()> {
    let port = std::env::var("MC_PORT")
        .unwrap_or_else(|_| "25565".into())
        .parse()?;
    let expected: f64 = std::env::var("EXPECTED_Y")
        .context("EXPECTED_Y is required")?
        .parse()?;
    let username = std::env::var("MC_USERNAME").unwrap_or_else(|_| "ShapeProbe".into());
    let manager = BotManager::new(Server::new("127.0.0.1", port));
    let bot = manager.connect(Player::offline(username)).await?;
    let mut events = bot.subscribe();
    bot.wait_until_ready().await?;
    if tokio::time::timeout(Duration::from_secs(5), async {
        while !bot.player().await.on_ground {
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    })
    .await
    .is_err()
    {
        let mut corrections = Vec::new();
        while let Ok(event) = events.try_recv() {
            if let voxrig::Event::PositionCorrection(c) = event {
                corrections.push(c);
            }
        }
        anyhow::bail!(
            "landing timed out: player={:?}, block={:?}, motion={:?}, metrics={:?}, last corrections={:?}",
            bot.player().await,
            bot.block(0, 3, 0).await,
            bot.motion().await,
            bot.physics_metrics().await,
            corrections.iter().rev().take(3).collect::<Vec<_>>()
        );
    }
    let player = bot.player().await;
    let metrics = bot.physics_metrics().await;
    ensure!(
        (player.y - expected).abs() < 1.0e-6,
        "expected y={expected}, got {}",
        player.y
    );
    ensure!(
        metrics.position_corrections == 0,
        "server corrected shape landing"
    );
    println!("landed y={:.5}; metrics={metrics:?}", player.y);
    manager.disconnect_all().await?;
    Ok(())
}
