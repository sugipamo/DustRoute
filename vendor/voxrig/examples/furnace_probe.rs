use anyhow::Result;
use tokio::time::{Duration, timeout};
use voxrig::{BlockFace, BlockPos, BotManager, ClickMode, Event, Hand, Player, Server};

#[tokio::main]
async fn main() -> Result<()> {
    let manager = BotManager::new(Server::new("127.0.0.1", 25566));
    let bot = manager.connect(Player::offline("FurnaceProbe4")).await?;
    let mut events = bot.subscribe();
    bot.wait_until_ready().await?;
    println!("READY_FOR_FUEL");
    timeout(Duration::from_secs(20), async {
        loop {
            let inventory = bot.inventory().await;
            let has_ore = inventory
                .player_slots()
                .iter()
                .flatten()
                .any(|item| item.name() == Some("iron_ore"));
            let has_coal = inventory
                .player_slots()
                .iter()
                .flatten()
                .any(|item| item.name() == Some("coal"));
            if has_ore && has_coal {
                return Ok::<(), anyhow::Error>(());
            }
            let _ = events.recv().await?;
        }
    })
    .await??;
    bot.place_block(
        Hand::Main,
        BlockPos { x: 2, y: 4, z: 0 },
        BlockFace::West,
        [0.0, 0.5, 0.5],
        false,
    )
    .await?;
    let window = timeout(Duration::from_secs(5), async {
        loop {
            if let Event::WindowOpened(window) = events.recv().await? {
                return Ok::<_, anyhow::Error>(window);
            }
        }
    })
    .await??;
    for (name, target) in [("iron_ore", 0), ("coal", 1)] {
        let slot = timeout(Duration::from_secs(5), async {
            loop {
                if let Some(slot) =
                    bot.inventory()
                        .await
                        .windows
                        .get(&window.id)
                        .and_then(|slots| {
                            slots
                                .iter()
                                .enumerate()
                                .skip(3)
                                .find(|(_, item)| {
                                    item.as_ref().is_some_and(|item| item.name() == Some(name))
                                })
                                .map(|(slot, _)| slot as i16)
                        })
                {
                    return Ok::<_, anyhow::Error>(slot);
                }
                let _ = events.recv().await?;
            }
        })
        .await??;
        bot.click_slot_and_wait(window.id, slot, 0, ClickMode::Normal)
            .await?;
        bot.click_slot_and_wait(window.id, target, 0, ClickMode::Normal)
            .await?;
    }
    let mut saw_progress = false;
    timeout(Duration::from_secs(20), async {
        loop {
            if bot
                .inventory()
                .await
                .windows
                .get(&window.id)
                .and_then(|slots| slots.get(2))
                .and_then(Option::as_ref)
                .is_some_and(|item| item.name() == Some("iron_ingot"))
            {
                return Ok::<(), anyhow::Error>(());
            }
            if let Event::WindowProperty(property) = events.recv().await? {
                if property.window_id == window.id && property.property == 2 && property.value > 0 {
                    saw_progress = true;
                }
            }
        }
    })
    .await??;
    anyhow::ensure!(saw_progress);
    bot.click_slot_and_wait(window.id, 2, 0, ClickMode::Normal)
        .await?;
    let destination = bot
        .inventory()
        .await
        .windows
        .get(&window.id)
        .and_then(|slots| {
            slots
                .iter()
                .enumerate()
                .skip(3)
                .find(|(_, item)| item.is_none())
                .map(|(slot, _)| slot as i16)
        })
        .ok_or_else(|| anyhow::anyhow!("no output destination"))?;
    bot.click_slot_and_wait(window.id, destination, 0, ClickMode::Normal)
        .await?;
    println!("SMELTED {:?}", bot.inventory().await);
    bot.close_window().await?;
    manager.disconnect_all().await?;
    Ok(())
}
