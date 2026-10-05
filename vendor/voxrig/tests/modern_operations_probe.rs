//! Opt-in fixture adapter. JSON encoding is test-only; native state decides outcomes.
//! Explicit isolated-server creative operations trial. No survival or pathfinding claim.
use std::{
    io::{self, Write},
    time::Duration,
};
use voxrig::{BlockFace, Client, ConnectionConfig, MinecraftVersion, Region, Server};

async fn observe(
    client: &Client,
    name: &str,
) -> anyhow::Result<voxrig::versions::java_1_21_11::reconstruction::ClientObservation> {
    let region = Region {
        min: [100, 181, 100],
        max: [100, 181, 100],
    };
    for _ in 0..80 {
        let view = client.observe_client_region(region).await?;
        anyhow::ensure!(
            view.issue.is_none(),
            "incomplete observation: {:?}",
            view.issue
        );
        if view.blocks[0]
            .state
            .as_ref()
            .is_some_and(|s| s.name == name)
        {
            return Ok(view);
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    anyhow::bail!("expected {name} was not observed")
}

#[tokio::test(flavor = "multi_thread")]
#[ignore = "explicit isolated-server fixture capture; requires prepared world and environment"]
async fn retain_fixture() -> anyhow::Result<()> {
    let file = std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(std::env::var("TRACE_OUTPUT")?)?;
    let client = Client::connect(ConnectionConfig::offline(
        Server::new("127.0.0.1", std::env::var("MC_PORT")?.parse()?),
        "OpsProbe",
        MinecraftVersion::Java1_21_11,
    ))
    .await?;
    client.wait_until_ready().await?;
    println!(
        "READY_FOR_FIXTURE_AND_TELEPORT; prepare stone at 100 180 100 and air above, creative OP bot at 100.5 182 103.5, then enter"
    );
    io::stdout().flush()?;
    let count = tokio::task::spawn_blocking(|| {
        let mut line = String::new();
        io::stdin().read_line(&mut line)
    })
    .await??;
    anyhow::ensure!(count > 0, "closed stdin");
    tokio::time::sleep(Duration::from_secs(2)).await;
    let operations = client.java_1_21_11_operations()?;
    let before = operations.player_state().await?;
    let initial = observe(&client, "minecraft:air").await?;
    client.start_packet_trace(4_194_304).await?;
    anyhow::ensure!(
        operations.set_creative_hotbar(9, None).await.is_err(),
        "invalid hotbar accepted"
    );
    operations.set_flying(true).await?;
    operations
        .move_flying([100.5, 182.0, 102.5], [180.0, 60.0])
        .await?;
    operations.look([180.0, 60.0]).await?;
    operations
        .set_creative_hotbar(0, Some(("minecraft:stone", 1)))
        .await?;
    operations.select_hotbar(0).await?;
    tokio::time::sleep(Duration::from_millis(150)).await;
    let placement = operations
        .use_on_block([100, 180, 100], BlockFace::Up, [0.5, 1.0, 0.5])
        .await?;
    let placed = observe(&client, "minecraft:stone").await?;
    let removal = operations
        .dig_creative([100, 181, 100], BlockFace::Up)
        .await?;
    let removed = observe(&client, "minecraft:air").await?;
    operations
        .send_command("tellraw @s {\"text\":\"VOXRIG_OPERATIONS_COMMAND\"}")
        .await?;
    operations.send_command("time query gametime").await?;
    tokio::time::sleep(Duration::from_secs(1)).await;
    let after = operations.player_state().await?;
    let messages = operations
        .system_messages_after(before.receive_sequence)
        .await?;
    let trace = client.stop_packet_trace().await?;
    let record = serde_json::json!({"before":before,"initial":initial,"placed":placed,"removed":removed,"after":after,"placement_sequence":placement,"removal_sequence":removal,"messages":messages,"trace":trace});
    serde_json::to_writer(file, &record)?;
    anyhow::ensure!(
        after.acknowledged_interaction.is_some_and(|s| s >= removal),
        "no interaction acknowledgement"
    );
    anyhow::ensure!(
        messages
            .iter()
            .any(|m| m.literal_text() == Some("VOXRIG_OPERATIONS_COMMAND")),
        "command result missing"
    );
    println!(
        "OPERATIONS_MATCH; capture written, check server position before pressing enter to disconnect"
    );
    io::stdout().flush()?;
    tokio::task::spawn_blocking(|| {
        let mut line = String::new();
        io::stdin().read_line(&mut line)
    })
    .await??;
    client.disconnect().await?;
    Ok(())
}
