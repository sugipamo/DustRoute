use anyhow::Result;
use tokio::time::{Duration, timeout};
use voxrig::{BlockFace, BlockPos, BotManager, ClickMode, Event, Hand, Player, Server};

#[tokio::main]
async fn main() -> Result<()> {
    let manager = BotManager::new(Server::new("127.0.0.1", 25566));
    let bot = manager.connect(Player::offline("WindowProbe")).await?;
    let mut events = bot.subscribe();
    bot.wait_until_ready().await?;
    println!("READY_FOR_CHEST");
    tokio::time::sleep(Duration::from_secs(2)).await;
    bot.place_block(
        Hand::Main,
        BlockPos { x: 2, y: 4, z: 0 },
        BlockFace::West,
        [0.0, 0.5, 0.5],
        false,
    )
    .await?;

    let deadline = tokio::time::Instant::now() + Duration::from_secs(20);
    let mut window_id = None;
    let mut clicked = false;
    while tokio::time::Instant::now() < deadline {
        let event = timeout(Duration::from_secs(2), events.recv()).await??;
        if let Event::WindowOpened(window) = event {
            println!("OPENED {window:?}");
            window_id = Some(window.id);
        }
        let inventory = bot.inventory().await;
        if let Some(id) = window_id {
            let has_stone = inventory
                .windows
                .get(&id)
                .and_then(|slots| slots.first())
                .and_then(Option::as_ref)
                .is_some_and(|item| item.name() == Some("stone"));
            if has_stone && !clicked {
                bot.click_slot(id, 0, 0, ClickMode::Shift).await?;
                clicked = true;
            }
        }
        if let Some(id) = window_id {
            let transferred = inventory
                .windows
                .get(&id)
                .is_some_and(|slots| slots.first().is_some_and(Option::is_none));
            if clicked && transferred {
                println!("TRANSFERRED {inventory:?}");
                bot.close_window().await?;
                manager.disconnect_all().await?;
                return Ok(());
            }
        }
    }
    anyhow::bail!("window transfer timed out")
}
