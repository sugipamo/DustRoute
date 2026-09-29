use anyhow::{Result, ensure};
use std::time::Duration;
use voxrig::{BotManager, ControlState, Player, Server};

#[tokio::main]
async fn main() -> Result<()> {
    let port = std::env::var("MC_PORT")
        .unwrap_or_else(|_| "25565".into())
        .parse()?;
    let manager = BotManager::new(Server::new("127.0.0.1", port));
    let walk = manager.connect(Player::offline("WalkProbe")).await?;
    walk.wait_until_ready().await?;
    let walk_start = walk.player().await;
    walk.set_control(ControlState {
        forward: true,
        ..Default::default()
    })
    .await;
    tokio::time::sleep(Duration::from_secs(2)).await;
    walk.clear_control().await;
    let walk_end = walk.player().await;
    let walk_distance = (walk_end.x - walk_start.x).hypot(walk_end.z - walk_start.z);
    let walk_metrics = walk.physics_metrics().await;
    manager.disconnect("WalkProbe").await?;
    let sprint = manager.connect(Player::offline("SprintProbe")).await?;
    sprint.wait_until_ready().await?;
    let sprint_start = sprint.player().await;
    sprint
        .set_control(ControlState {
            forward: true,
            sprint: true,
            ..Default::default()
        })
        .await;
    tokio::time::sleep(Duration::from_secs(2)).await;
    sprint.clear_control().await;
    let sprint_end = sprint.player().await;
    let sprint_distance = (sprint_end.x - sprint_start.x).hypot(sprint_end.z - sprint_start.z);
    let sprint_metrics = sprint.physics_metrics().await;
    ensure!(
        sprint_distance > walk_distance * 1.2,
        "sprint {sprint_distance:.3} was not faster than walk {walk_distance:.3}"
    );
    ensure!(
        walk_metrics.position_corrections == 0 && sprint_metrics.position_corrections == 0,
        "server corrected sprint test"
    );
    println!(
        "walk={walk_distance:.3}; sprint={sprint_distance:.3}; sprint metrics={sprint_metrics:?}"
    );
    manager.disconnect_all().await?;
    Ok(())
}
