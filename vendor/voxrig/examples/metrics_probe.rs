use anyhow::{Context, Result};
use std::time::Duration;
use voxrig::{BotManager, Event, Player, Server};

#[tokio::main]
async fn main() -> Result<()> {
    let port = std::env::var("MC_PORT")
        .unwrap_or_else(|_| "25565".into())
        .parse()?;
    let manager = BotManager::new(Server::new("127.0.0.1", port));
    let bot = manager.connect(Player::offline("MetricBot")).await?;
    let mut events = bot.subscribe();
    bot.wait_until_ready().await?;

    // Deliberately invalid displacement: a vanilla server should correct it.
    bot.unstable().move_relative(1_000.0, 0.0).await?;
    loop {
        let event = tokio::time::timeout(Duration::from_secs(5), events.recv())
            .await
            .context("server did not answer the probe")??;
        if let Event::PositionCorrection(correction) = event {
            println!("correction distance: {:.3}", correction.distance);
            break;
        }
    }
    println!("metrics: {:?}", bot.physics_metrics().await);
    manager.disconnect_all().await?;
    Ok(())
}
