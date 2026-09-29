use anyhow::Result;
use voxrig::{BotManager, ControlState, Player, Server};

#[tokio::main]
async fn main() -> Result<()> {
    let host = std::env::var("MC_HOST").unwrap_or_else(|_| "127.0.0.1".into());
    let port = std::env::var("MC_PORT")
        .unwrap_or_else(|_| "25565".into())
        .parse()?;
    let manager = BotManager::new(Server::new(host, port));

    let (alpha, beta) = tokio::try_join!(
        manager.connect(Player::offline("AlphaBot")),
        manager.connect(Player::offline("BetaBot")),
    )?;
    tokio::try_join!(alpha.wait_until_ready(), beta.wait_until_ready())?;

    alpha.look(90.0, 0.0).await?;
    beta.set_control(ControlState {
        forward: true,
        ..Default::default()
    })
    .await;
    let (alpha_blocks, beta_blocks) =
        tokio::try_join!(alpha.observe_snapshot(1), beta.observe_snapshot(1))?;

    println!("managed: {:?}", manager.usernames().await);
    println!("client: {:?}", voxrig::Bot::client_info());
    println!("AlphaBot observed {} blocks", alpha_blocks.value.len());
    println!("BetaBot observed {} blocks", beta_blocks.value.len());
    manager.disconnect_all().await?;
    Ok(())
}
