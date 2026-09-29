use anyhow::Result;
use tokio::time::{Duration, timeout};
use voxrig::{BotManager, Event, Player, Server};

#[tokio::main]
async fn main() -> Result<()> {
    let manager = BotManager::new(Server::new("127.0.0.1", 25566));
    let bot = manager.connect(Player::offline("EntityProbe")).await?;
    let mut events = bot.subscribe();
    bot.wait_until_ready().await?;
    println!("READY_FOR_ENTITY");
    let deadline = tokio::time::Instant::now() + Duration::from_secs(30);
    let mut cow_id = None;
    let mut moved = false;
    while tokio::time::Instant::now() < deadline {
        let event = timeout(Duration::from_secs(2), events.recv()).await??;
        match event {
            Event::EntitySpawned(entity) if entity.type_name == Some("cow") => {
                cow_id = Some(entity.entity_id);
                println!("SPAWNED {entity:?}");
            }
            Event::EntityUpdated(entity)
                if Some(entity.entity_id) == cow_id && entity.position.x >= 4.5 && !moved =>
            {
                moved = true;
                println!("MOVED {entity:?}");
            }
            Event::EntitiesDestroyed { entity_ids }
                if cow_id.is_some_and(|id| entity_ids.contains(&id)) =>
            {
                anyhow::ensure!(moved);
                anyhow::ensure!(
                    bot.observe_entities(16.0)
                        .await
                        .iter()
                        .all(|entity| { Some(entity.entity_id) != cow_id })
                );
                println!("DESTROYED");
                manager.disconnect_all().await?;
                return Ok(());
            }
            _ => {}
        }
    }
    anyhow::bail!("entity lifecycle timed out")
}
