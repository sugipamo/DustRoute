use anyhow::Result;
use tokio::time::{Duration, timeout};
use voxrig::{BlockFace, BlockPos, BotManager, Event, Hand, Player, Server};

#[tokio::main]
async fn main() -> Result<()> {
    let manager = BotManager::new(Server::new("127.0.0.1", 25566));
    let bot = manager.connect(Player::offline("InteractProbe")).await?;
    bot.wait_until_ready().await?;
    let target = BlockPos { x: 2, y: 4, z: 0 };
    println!("READY_FOR_BLOCK");
    tokio::time::sleep(Duration::from_secs(2)).await;
    let acknowledgement = bot.dig_block(target, BlockFace::West).await?;
    println!("DUG {acknowledgement:?}");
    anyhow::ensure!(acknowledgement.successful);
    println!("READY_FOR_GIVE");

    let mut events = bot.subscribe();
    let deadline = tokio::time::Instant::now() + Duration::from_secs(20);
    let mut placed = false;
    while tokio::time::Instant::now() < deadline {
        let event = timeout(Duration::from_secs(2), events.recv()).await??;
        if !placed {
            let inventory = bot.inventory().await;
            if let Some((slot, _)) =
                inventory.player_slots()[36..=44]
                    .iter()
                    .enumerate()
                    .find(|(_, item)| {
                        item.as_ref()
                            .is_some_and(|item| item.name() == Some("stone"))
                    })
            {
                bot.select_hotbar(slot as u8).await?;
                bot.swing_arm(Hand::Main).await?;
                bot.place_block(
                    Hand::Main,
                    BlockPos { x: 2, y: 3, z: 0 },
                    BlockFace::Up,
                    [0.5, 1.0, 0.5],
                    false,
                )
                .await?;
                placed = true;
            }
        }
        if let Event::BlockChanged {
            x: 2,
            y: 4,
            z: 0,
            state_id,
        } = event
            && placed
            && state_id != 0
        {
            println!("PLACED state_id={state_id}");
            manager.disconnect_all().await?;
            return Ok(());
        }
    }
    anyhow::bail!("place sequence timed out")
}
