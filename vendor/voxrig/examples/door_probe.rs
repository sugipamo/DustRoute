use anyhow::{Context, Result, ensure};
use std::time::Duration;
use voxrig::{BotManager, ControlState, Player, Server};

#[tokio::main]
async fn main() -> Result<()> {
    let port = std::env::var("MC_PORT")
        .unwrap_or_else(|_| "25565".into())
        .parse()?;
    let open = std::env::var("DOOR_OPEN")
        .unwrap_or_else(|_| "false".into())
        .parse::<bool>()?;
    let manager = BotManager::new(Server::new("127.0.0.1", port));
    let bot = manager
        .connect(Player::offline(if open {
            "OpenDoorProbe"
        } else {
            "ClosedDoorProbe"
        }))
        .await?;
    bot.wait_until_ready().await?;
    tokio::time::timeout(Duration::from_secs(3), async {
        while !bot.player().await.on_ground {
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    })
    .await
    .context("initial landing timed out")?;
    bot.look(0.0, 0.0).await?;
    bot.set_control(ControlState {
        forward: true,
        ..Default::default()
    })
    .await;
    tokio::time::sleep(Duration::from_secs(3)).await;
    bot.clear_control().await;
    let player = bot.player().await;
    let metrics = bot.physics_metrics().await;
    if open {
        ensure!(player.z > 8.0, "open door blocked bot at z={}", player.z);
    } else {
        ensure!(
            player.z < 6.0,
            "closed door allowed bot through at z={}",
            player.z
        );
        ensure!(
            bot.motion().await.collided_horizontal,
            "closed door did not produce horizontal collision"
        );
    }
    ensure!(
        metrics.position_corrections == 0,
        "server corrected door traversal"
    );
    println!(
        "open={open}; z={:.3}; door_state={:?}; metrics={metrics:?}",
        player.z,
        bot.block(0, 4, 5).await
    );
    manager.disconnect_all().await?;
    Ok(())
}
