use anyhow::{Context, Result, ensure};
use std::{collections::HashMap, sync::Arc, time::Duration};
use voxrig::{BotManager, ChunkPos, Player, Server};

#[tokio::main]
async fn main() -> Result<()> {
    let port = std::env::var("MC_PORT")
        .unwrap_or_else(|_| "25565".into())
        .parse()?;
    let manager = BotManager::new(Server::new("127.0.0.1", port));
    let first = manager.connect(Player::offline("ApiProbeA")).await?;
    let second = manager.connect(Player::offline("ApiProbeB")).await?;

    for bot in [&first, &second] {
        bot.wait_until_ready().await?;
        let player = bot.player().await;
        let center = ChunkPos {
            x: (player.x.floor() as i32).div_euclid(16),
            z: (player.z.floor() as i32).div_euclid(16),
        };
        bot.wait_for_chunk(center, Duration::from_secs(10)).await?;
    }

    let commands = first.command_tree_snapshot().await;
    let command_count = commands.value.as_ref().map_or(0, |tree| tree.nodes.len());
    ensure!(command_count > 0, "server command tree was not populated");

    let tags = first.tags_snapshot().await;
    let tag_count = tags.value.blocks.len()
        + tags.value.items.len()
        + tags.value.fluids.len()
        + tags.value.entities.len();
    ensure!(tag_count > 0, "server tags were not populated");

    let recipes = first.server_recipes_snapshot().await;
    ensure!(
        !recipes.value.recipes.is_empty(),
        "server recipes were not populated"
    );

    let mut first_chunks = HashMap::new();
    for position in first.loaded_chunks().await {
        if let Some(chunk) = first.chunk_snapshot(position).await {
            first_chunks.insert(position, chunk);
        }
    }
    let mut shared_sections = 0usize;
    for position in second.loaded_chunks().await {
        let Some(left) = first_chunks.get(&position) else {
            continue;
        };
        let Some(right) = second.chunk_snapshot(position).await else {
            continue;
        };
        for (section_y, left_data) in &left.sections {
            if right
                .sections
                .get(section_y)
                .is_some_and(|right_data| Arc::ptr_eq(left_data, right_data))
            {
                shared_sections += 1;
            }
        }
    }
    ensure!(
        shared_sections > 0,
        "no chunk sections were shared between bots"
    );

    let storage = manager.chunk_storage_stats();
    let environment = first.environment_state().await;
    println!(
        "commands={command_count} tags={tag_count} recipes={} chunks={} shared_sections={shared_sections} live_sections={} fluid={:?} climbable={}",
        recipes.value.recipes.len(),
        first_chunks.len(),
        storage.live_sections,
        environment.fluid,
        environment.climbable,
    );

    manager
        .disconnect_all()
        .await
        .context("disconnecting probes")?;
    Ok(())
}
