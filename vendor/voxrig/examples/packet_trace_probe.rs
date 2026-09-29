//! Captures native packets while toggling the prepared isolated stair/piston fixture.
use std::{
    io::{self, Write},
    time::Duration,
};
use voxrig::{BlockFace, Client, ConnectionConfig, MinecraftVersion, Region, Server};

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let port = std::env::var("MC_PORT")?.parse()?;
    let output = std::env::var("TRACE_OUTPUT")?;
    let fixture =
        std::env::var("FIXTURE_ID").unwrap_or_else(|_| "device-stairs-inner-top-left-r0".into());
    let file = std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(output)?;
    let client = Client::connect(ConnectionConfig::offline(
        Server::new("127.0.0.1", port),
        "PacketProbe",
        MinecraftVersion::Java1_21_11,
    ))
    .await?;
    client.wait_until_ready().await?;
    println!("READY_FOR_FIXTURE_AND_TELEPORT; press enter after setup");
    io::stdout().flush()?;
    tokio::task::spawn_blocking(|| {
        let mut line = String::new();
        io::stdin().read_line(&mut line)
    })
    .await??;
    tokio::time::sleep(Duration::from_secs(5)).await;
    let region = Region {
        min: [95, 177, 94],
        max: [105, 183, 103],
    };
    let before = client.observe_region(region).await?;
    anyhow::ensure!(
        before.blocks.iter().all(|b| b.state.is_some()),
        "fixture region not fully loaded"
    );
    if std::env::var("TRACE_MODE").as_deref() == Ok("observe") {
        serde_json::to_writer_pretty(file, &before)?;
        println!("FRESH_OBSERVATION_RETAINED");
        client.disconnect().await?;
        return Ok(());
    }
    let target = [102, 180, 99];
    let initial = before
        .blocks
        .iter()
        .find(|b| b.position == target)
        .unwrap()
        .state
        .as_ref()
        .unwrap();
    anyhow::ensure!(
        initial.name == "minecraft:quartz_stairs" && initial.properties["shape"] == "inner_left",
        "unexpected initial stair state"
    );
    client.start_packet_trace(16_777_216).await?;
    client
        .interact_block([99, 180, 100], BlockFace::East)
        .await?;
    let wait_ms: u64 = std::env::var("INPUT_WAIT_MS")
        .unwrap_or_else(|_| "800".into())
        .parse()?;
    let started = tokio::time::Instant::now();
    let mut transient = Vec::new();
    while started.elapsed() < Duration::from_millis(wait_ms) {
        tokio::time::sleep(Duration::from_millis(10)).await;
        let sample = client.observe_client_region(region).await?;
        if transient.len() < 200 {
            transient.push(sample);
        }
    }
    let on = client.observe_region(region).await?;
    let on_client = client.observe_client_region(region).await?;
    client
        .interact_block([99, 180, 100], BlockFace::East)
        .await?;
    let started = tokio::time::Instant::now();
    while started.elapsed() < Duration::from_millis(500) {
        tokio::time::sleep(Duration::from_millis(10)).await;
        transient.push(client.observe_client_region(region).await?);
    }
    tokio::time::sleep(Duration::from_millis(3500)).await;
    let after = client.observe_region(region).await?;
    let after_client = client.observe_client_region(region).await?;
    let trace = client.stop_packet_trace().await?;
    let record = serde_json::json!({"fixture":fixture,"input_wait_ms":wait_ms,"settling_ms":4000,"before":before,"on":on,"after":after,"trace":trace,"on_client":on_client,"after_client":after_client,"transient":transient});
    serde_json::to_writer_pretty(file, &record)?;
    println!(
        "TRACE_RETAINED complete={} packets={} target={}",
        record["trace"]["complete"],
        record["trace"]["records"].as_array().unwrap().len(),
        record["after"]["blocks"]
            .as_array()
            .unwrap()
            .iter()
            .find(|b| b["position"] == serde_json::json!(target))
            .unwrap()
    );
    client.disconnect().await?;
    anyhow::ensure!(
        record["after_client"]["issue"].is_null(),
        "client reconstruction incomplete: {}",
        record["after_client"]["issue"]
    );
    Ok(())
}
