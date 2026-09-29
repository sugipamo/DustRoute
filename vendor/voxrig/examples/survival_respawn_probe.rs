use anyhow::Result;
use tokio::time::{Duration, timeout};
use voxrig::{BotManager, CombatEvent, Event, Player, Server};

#[tokio::main]
async fn main() -> Result<()> {
    let manager = BotManager::new(Server::new("127.0.0.1", 25566));
    let bot = manager.connect(Player::offline("RespawnProbe")).await?;
    let mut events = bot.subscribe();
    bot.wait_until_ready().await?;
    println!("READY");

    let deadline = tokio::time::Instant::now() + Duration::from_secs(30);
    let mut saw_death = false;
    let mut saw_respawn = false;
    let mut saw_position = false;
    while tokio::time::Instant::now() < deadline {
        let event = timeout(Duration::from_secs(2), events.recv()).await??;
        match event {
            Event::Combat(CombatEvent::Death { .. }) => {
                saw_death = true;
                bot.respawn().await?;
            }
            Event::Respawn(_) => saw_respawn = true,
            Event::Position(_) if saw_respawn => saw_position = true,
            _ => {}
        }
        let state = bot.survival_state().await;
        if saw_death
            && saw_respawn
            && saw_position
            && state.vitals.is_some_and(|vitals| vitals.health > 0.0)
            && !state.attributes.is_empty()
        {
            println!("RESPAWNED {state:?}");
            manager.disconnect_all().await?;
            return Ok(());
        }
    }
    anyhow::bail!("death/respawn sequence timed out")
}
