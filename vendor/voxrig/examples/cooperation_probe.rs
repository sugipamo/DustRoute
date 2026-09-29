use anyhow::{Context, Result};
use tokio::time::{Duration, timeout};
use voxrig::{
    BlockFace, BlockPos, Bot, BotManager, ClickMode, ControlState, Event, Hand, Player, Server,
};

async fn open_chest(bot: &Bot) -> Result<(i8, tokio::sync::broadcast::Receiver<Event>)> {
    let mut events = bot.subscribe();
    bot.place_block(
        Hand::Main,
        BlockPos { x: 2, y: 4, z: 0 },
        BlockFace::Up,
        [0.5, 1.0, 0.5],
        false,
    )
    .await?;
    let id = timeout(Duration::from_secs(15), async {
        loop {
            if let Event::WindowOpened(window) = events.recv().await? {
                return Ok::<_, anyhow::Error>(window.id);
            }
        }
    })
    .await
    .context("waiting for window open")??;
    timeout(Duration::from_secs(15), async {
        loop {
            if bot
                .inventory()
                .await
                .windows
                .get(&id)
                .is_some_and(|slots| !slots.is_empty())
            {
                return Ok::<(), anyhow::Error>(());
            }
            let _ = events.recv().await?;
        }
    })
    .await
    .context("waiting for window items")??;
    Ok((id, events))
}

#[tokio::main]
async fn main() -> Result<()> {
    let manager = BotManager::new(Server::new("127.0.0.1", 25566));
    let (a, b) = tokio::try_join!(
        manager.connect(Player::offline("CoopBotA")),
        manager.connect(Player::offline("CoopBotB")),
    )?;
    let mut b_events = b.subscribe();
    tokio::try_join!(a.wait_until_ready(), b.wait_until_ready())?;
    for bot in [&a, &b] {
        bot.look(-90.0, 0.0).await?;
        bot.set_control(ControlState {
            forward: true,
            ..Default::default()
        })
        .await;
    }
    tokio::time::sleep(Duration::from_millis(350)).await;
    a.clear_control().await;
    b.clear_control().await;
    println!(
        "POSITIONS A={:?} B={:?} chest={:?}",
        a.player().await,
        b.player().await,
        a.block(2, 4, 0).await
    );
    anyhow::ensure!(manager.usernames().await.len() == 2);
    a.send_chat("handoff-ready").await?;
    timeout(Duration::from_secs(15), async {
        loop {
            match b_events.recv().await? {
                Event::Chat(chat) if chat.json.contains("handoff-ready") => {
                    return Ok::<(), anyhow::Error>(());
                }
                Event::Error { kind, message } => anyhow::bail!("B error {kind}: {message}"),
                _ => {}
            }
        }
    })
    .await
    .context("waiting for shared chat")??;
    println!("CHAT_SHARED");

    let (a_window, _) = open_chest(&a).await.context("A opens chest")?;
    a.click_slot_and_wait(a_window, 0, 0, ClickMode::Normal)
        .await?;
    let a_destination = a.inventory().await.windows[&a_window]
        .iter()
        .enumerate()
        .skip(27)
        .find(|(_, item)| item.is_none())
        .map(|(slot, _)| slot as i16)
        .unwrap();
    a.click_slot_and_wait(a_window, a_destination, 0, ClickMode::Normal)
        .await?;
    a.click_slot_and_wait(a_window, a_destination, 0, ClickMode::Normal)
        .await?;
    a.click_slot_and_wait(a_window, 0, 0, ClickMode::Normal)
        .await?;
    a.close_window().await?;

    let (b_window, _) = open_chest(&b).await.context("B opens chest")?;
    b.click_slot_and_wait(b_window, 0, 0, ClickMode::Normal)
        .await?;
    let b_destination = b.inventory().await.windows[&b_window]
        .iter()
        .enumerate()
        .skip(27)
        .find(|(_, item)| item.is_none())
        .map(|(slot, _)| slot as i16)
        .unwrap();
    b.click_slot_and_wait(b_window, b_destination, 0, ClickMode::Normal)
        .await?;
    anyhow::ensure!(
        b.inventory().await.windows[&b_window][b_destination as usize]
            .as_ref()
            .is_some_and(|item| item.name() == Some("stone"))
    );
    println!(
        "COOPERATED usernames={:?} inventory={:?}",
        manager.usernames().await,
        b.inventory().await
    );
    b.close_window().await?;
    manager.disconnect_all().await?;
    Ok(())
}
