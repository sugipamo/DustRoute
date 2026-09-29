//! Dig/place/readback regression; the isolated server supplies the fixture and inventory.
use std::time::Duration;
use voxrig::versions::java_1_16_1::{BlockFace, BlockPos, BotManager, Hand, Player, Server};

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let port = std::env::var("MC_PORT")?.parse()?;
    let manager = BotManager::new(Server::new("127.0.0.1", port));
    let bot = manager.connect(Player::offline("VersionAction")).await?;
    bot.wait_until_ready().await?;
    println!("READY_FOR_FIXTURE_AND_STONE");
    let target = BlockPos { x: 2, y: 4, z: 4 };
    bot.wait_for_block_state(target, Some(1), Duration::from_secs(20))
        .await?;
    let slot = tokio::time::timeout(Duration::from_secs(20), async {
        loop {
            let inventory = bot.inventory().await;
            if let Some(slot) = inventory.player_slots()[36..=44]
                .iter()
                .position(|s| s.as_ref().is_some_and(|s| s.name() == Some("stone")))
            {
                break slot as u8;
            }
            tokio::time::sleep(Duration::from_millis(50)).await;
        }
    })
    .await?;
    bot.select_hotbar(slot).await?;
    let acknowledgement = bot.dig_block(target, BlockFace::West).await?;
    anyhow::ensure!(acknowledgement.successful, "dig rejected");
    bot.wait_for_block_state(target, Some(0), Duration::from_secs(5))
        .await?;
    bot.place_block(
        Hand::Main,
        BlockPos { x: 2, y: 3, z: 4 },
        BlockFace::Up,
        [0.5, 1.0, 0.5],
        false,
    )
    .await?;
    bot.wait_for_block_state(target, Some(1), Duration::from_secs(5))
        .await?;
    println!("DIG_ACK_AND_PLACE_OBSERVATION_OK");
    manager.disconnect_all().await?;
    Ok(())
}
