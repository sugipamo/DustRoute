use anyhow::Result;
use tokio::time::{Duration, timeout};
use voxrig::{BotManager, Event, Player, Server};

#[tokio::main]
async fn main() -> Result<()> {
    let manager = BotManager::new(Server::new("127.0.0.1", 25566));
    let bot = manager.connect(Player::offline("SoundProbe3")).await?;
    let mut events = bot.subscribe();
    bot.wait_until_ready().await?;
    anyhow::ensure!(
        bot.player_list()
            .await
            .entries
            .values()
            .any(|entry| entry.name == "SoundProbe3")
    );
    bot.send_chat("structured-chat-probe").await?;
    timeout(Duration::from_secs(5), async {
        loop {
            if let Event::Chat(chat) = events.recv().await? {
                if chat.json.contains("structured-chat-probe") {
                    println!("CHAT {chat:?}");
                    return Ok::<(), anyhow::Error>(());
                }
            }
        }
    })
    .await??;
    println!("READY_FOR_SOUND");
    let sound = timeout(Duration::from_secs(15), async {
        loop {
            if let Event::Sound(sound) = events.recv().await? {
                println!("OBSERVED_SOUND {sound:?}");
                if sound.sound_name.as_deref().is_some_and(|name| {
                    name == "entity.zombie.ambient" || name == "minecraft:entity.zombie.ambient"
                }) {
                    return Ok::<_, anyhow::Error>(sound);
                }
            }
        }
    })
    .await??;
    println!("SOUND {sound:?}");
    manager.disconnect_all().await?;
    Ok(())
}
