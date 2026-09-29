use anyhow::{Result, ensure};
use std::time::Duration;
use voxrig::{BotManager, ControlState, Player, Server};

#[tokio::main]
async fn main() -> Result<()> {
    let port = env("MC_PORT", 25565_u16)?;
    let username = std::env::var("BOT_USERNAME").unwrap_or_else(|_| "SurfaceProbe".into());
    let min_distance = env("MIN_DISTANCE", 0.0_f64)?;
    let max_distance = env("MAX_DISTANCE", f64::MAX)?;
    let manager = BotManager::new(Server::new("127.0.0.1", port));
    let bot = manager.connect(Player::offline(username)).await?;
    bot.wait_until_ready().await?;
    let start = bot.player().await;
    bot.set_control(ControlState {
        forward: true,
        ..Default::default()
    })
    .await;
    tokio::time::sleep(Duration::from_secs(2)).await;
    bot.clear_control().await;
    let end = bot.player().await;
    let distance = (end.x - start.x).hypot(end.z - start.z);
    let metrics = bot.physics_metrics().await;
    ensure!(
        (min_distance..=max_distance).contains(&distance),
        "distance {distance:.3} outside {min_distance:.3}..={max_distance:.3}"
    );
    ensure!(
        metrics.position_corrections == 0,
        "server corrected surface test: {metrics:?}"
    );
    println!("distance={distance:.3}; start={start:?}; end={end:?}; metrics={metrics:?}");
    manager.disconnect_all().await?;
    Ok(())
}

fn env<T: std::str::FromStr>(name: &str, default: T) -> Result<T>
where
    T::Err: std::error::Error + Send + Sync + 'static,
{
    Ok(std::env::var(name).map_or(Ok(default), |value| value.parse())?)
}
