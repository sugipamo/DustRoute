//! Isolated-server probe for the explicitly versioned observation API.
use voxrig::{Client, ConnectionConfig, MinecraftVersion, Region, Server};

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let port = std::env::var("MC_PORT")?.parse()?;
    let version = match std::env::var("MC_VERSION").as_deref().unwrap_or("1.16.1") {
        "1.16.1" => MinecraftVersion::Java1_16_1,
        "1.21.11" => MinecraftVersion::Java1_21_11,
        value => anyhow::bail!("unsupported probe version {value}"),
    };
    let client = Client::connect(ConnectionConfig::offline(
        Server::new("127.0.0.1", port),
        "VersionProbe",
        version,
    ))
    .await?;
    client.wait_until_ready().await?;
    // Setup coordinates are explicit; no commands or world mutations occur here.
    let region = Region {
        min: [0, 3, 0],
        max: [2, 5, 0],
    };
    let mut observation = client.observe_region(region).await?;
    for _ in 0..100 {
        if observation.blocks.iter().all(|b| b.state.is_some()) {
            break;
        }
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;
        observation = client.observe_region(region).await?;
    }
    anyhow::ensure!(
        observation.blocks.iter().all(|b| b.state.is_some()),
        "region is not loaded"
    );
    let state = |p| {
        observation
            .blocks
            .iter()
            .find(|b| b.position == p)
            .unwrap()
            .state
            .as_ref()
            .unwrap()
    };
    anyhow::ensure!(
        state([0, 4, 0]).name == "minecraft:quartz_stairs",
        "missing prepared stair"
    );
    anyhow::ensure!(
        state([0, 4, 0]).properties["shape"] == "inner_left",
        "stair shape was lost"
    );
    anyhow::ensure!(
        state([0, 4, 0]).properties["facing"] == "north",
        "stair facing was lost"
    );
    println!("VERSION_OBSERVATION {:?}", observation);
    client.disconnect().await?;
    Ok(())
}
