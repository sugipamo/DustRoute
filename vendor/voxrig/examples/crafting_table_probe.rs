use anyhow::Result;
use tokio::time::{Duration, timeout};
use voxrig::{BlockFace, BlockPos, BotManager, Event, Hand, Player, Server};

#[tokio::main]
async fn main() -> Result<()> {
    let manager = BotManager::new(Server::new("127.0.0.1", 25566));
    let bot = manager.connect(Player::offline("TableCraft3")).await?;
    let mut events = bot.subscribe();
    bot.wait_until_ready().await?;
    println!("READY_FOR_MATERIALS");
    timeout(Duration::from_secs(20), async {
        loop {
            let inventory = bot.inventory().await;
            let planks = inventory
                .player_slots()
                .iter()
                .flatten()
                .filter(|item| item.name() == Some("oak_planks"))
                .map(|item| i32::from(item.count))
                .sum::<i32>();
            let sticks = inventory
                .player_slots()
                .iter()
                .flatten()
                .filter(|item| item.name() == Some("stick"))
                .map(|item| i32::from(item.count))
                .sum::<i32>();
            if planks >= 3 && sticks >= 2 {
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
    bot.place_recipe(window.id, "minecraft:wooden_pickaxe", false)
        .await?;
    timeout(Duration::from_secs(5), async {
        loop {
            if bot
                .inventory()
                .await
                .windows
                .get(&window.id)
                .and_then(|slots| slots.first())
                .is_some_and(|item| {
                    item.as_ref()
                        .is_some_and(|item| item.name() == Some("wooden_pickaxe"))
                })
            {
                return Ok::<(), anyhow::Error>(());
            }
            let _ = events.recv().await?;
        }
    })
    .await??;
    bot.take_crafting_result(window.id, 9).await?;
    let inventory = bot.inventory().await;
    anyhow::ensure!(
        inventory
            .windows
            .get(&window.id)
            .into_iter()
            .flatten()
            .flatten()
            .any(|item| item.name() == Some("wooden_pickaxe")),
        "wooden pickaxe missing: {inventory:?}"
    );
    println!("CRAFTED_PICKAXE {inventory:?}");
    bot.close_window().await?;
    manager.disconnect_all().await?;
    Ok(())
}
