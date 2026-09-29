use anyhow::{Result, ensure};
use std::time::Duration;
use voxrig::{BotManager, ControlState, Player, Server};

#[tokio::main]
async fn main() -> Result<()> {
    let port = std::env::var("MC_PORT")
        .unwrap_or_else(|_| "25565".into())
        .parse()?;
    let manager = BotManager::new(Server::new("127.0.0.1", port));
    let (one, two) = tokio::try_join!(
        manager.connect(Player::offline("ControlOne")),
        manager.connect(Player::offline("ControlTwo")),
    )?;
    tokio::try_join!(one.wait_until_ready(), two.wait_until_ready())?;
    let before_one = one.player().await;
    let before_two = two.player().await;
    one.set_control(ControlState {
        forward: true,
        ..Default::default()
    })
    .await;
    two.set_control(ControlState {
        back: true,
        ..Default::default()
    })
    .await;
    tokio::time::sleep(Duration::from_secs(2)).await;
    one.clear_control().await;
    two.clear_control().await;
    let after_one = one.player().await;
    let after_two = two.player().await;
    let distance_one = (after_one.x - before_one.x).hypot(after_one.z - before_one.z);
    let distance_two = (after_two.x - before_two.x).hypot(after_two.z - before_two.z);
    ensure!(
        distance_one > 1.0 && distance_two > 1.0,
        "controls did not move both bots"
    );
    let metrics = manager.physics_metrics().await;
    ensure!(
        metrics.values().all(|m| m.position_corrections == 0),
        "server corrected normal control movement"
    );
    println!("distances: {distance_one:.2}, {distance_two:.2}; metrics: {metrics:?}");
    manager.disconnect_all().await?;
    Ok(())
}
