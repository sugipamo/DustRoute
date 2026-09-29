use anyhow::{Context, Result, ensure};
use std::time::Duration;
use voxrig::{BotManager, Player, Server};

#[tokio::main]
async fn main() -> Result<()> {
    let port = std::env::var("MC_PORT")
        .unwrap_or_else(|_| "25565".into())
        .parse()?;
    let jumps: usize = std::env::var("JUMPS")
        .unwrap_or_else(|_| "100".into())
        .parse()?;
    let manager = BotManager::new(Server::new("127.0.0.1", port));
    let username = std::env::var("BOT_USERNAME").unwrap_or_else(|_| "JumpProbe".into());
    let bot = manager.connect(Player::offline(username)).await?;
    bot.wait_until_ready().await?;
    wait_for(&bot, true).await?;
    let baseline = bot.player().await.y;
    let mut maximum = baseline;
    for _ in 0..jumps {
        bot.jump().await?;
        wait_for(&bot, false).await?;
        while !bot.player().await.on_ground {
            maximum = maximum.max(bot.player().await.y);
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
        ensure!(
            (bot.player().await.y - baseline).abs() < 1.0e-6,
            "jump did not return to baseline"
        );
    }
    let metrics = bot.physics_metrics().await;
    ensure!(
        metrics.position_corrections == 0,
        "server corrected jump trajectory"
    );
    println!("{jumps} jumps; baseline={baseline:.5}; max={maximum:.5}; metrics={metrics:?}");
    manager.disconnect_all().await?;
    Ok(())
}

async fn wait_for(bot: &voxrig::Bot, on_ground: bool) -> Result<()> {
    tokio::time::timeout(Duration::from_secs(3), async {
        while bot.player().await.on_ground != on_ground {
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    })
    .await
    .context("ground-state transition timed out")?;
    Ok(())
}
