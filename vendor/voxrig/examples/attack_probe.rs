use anyhow::Result;
use tokio::time::{Duration, timeout};
use voxrig::{BotManager, Event, Player, Server};

#[tokio::main]
async fn main() -> Result<()> {
    let manager = BotManager::new(Server::new("127.0.0.1", 25566));
    let bot = manager.connect(Player::offline("AttackProbe3")).await?;
    let mut events = bot.subscribe();
    bot.wait_until_ready().await?;
    println!("READY_FOR_COW");
    let cow = timeout(Duration::from_secs(10), async {
        loop {
            if let Event::EntitySpawned(entity) = events.recv().await? {
                if entity.type_name == Some("cow") {
                    return Ok::<_, anyhow::Error>(entity);
                }
            }
        }
    })
    .await??;
    let mut saw_hurt = false;
    for _ in 0..12 {
        bot.attack(cow.entity_id).await?;
        if let Ok(Ok(event)) = timeout(Duration::from_millis(100), events.recv()).await {
            match event {
                Event::EntityStatus {
                    entity_id,
                    status: 2,
                } if entity_id == cow.entity_id => {
                    saw_hurt = true;
                }
                Event::EntitiesDestroyed { entity_ids } if entity_ids.contains(&cow.entity_id) => {
                    anyhow::ensure!(saw_hurt);
                    println!("ATTACKED_AND_KILLED entity_id={}", cow.entity_id);
                    manager.disconnect_all().await?;
                    return Ok(());
                }
                _ => {}
            }
        }
    }
    timeout(Duration::from_secs(5), async {
        loop {
            if let Event::EntitiesDestroyed { entity_ids } = events.recv().await? {
                if entity_ids.contains(&cow.entity_id) {
                    return Ok::<(), anyhow::Error>(());
                }
            }
        }
    })
    .await??;
    anyhow::ensure!(saw_hurt);
    println!("ATTACKED_AND_KILLED entity_id={}", cow.entity_id);
    manager.disconnect_all().await?;
    Ok(())
}
