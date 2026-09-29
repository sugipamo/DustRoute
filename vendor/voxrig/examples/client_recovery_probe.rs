//! Observe native moving-piston chunk data on an isolated, already prepared server.
//! This probe does not place blocks, issue commands, or change server ticking.
use std::{fs::OpenOptions, time::Duration};
use voxrig::{Client, ConnectionConfig, MinecraftVersion, Region, Server};

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let port = std::env::var("MC_PORT")?.parse()?;
    let directory = std::env::var("TRACE_DIR")?;
    let region = Region {
        min: [102, 179, 102],
        max: [108, 182, 106],
    };
    for attempt in 0..12 {
        let client = Client::connect(ConnectionConfig::offline(
            Server::new("127.0.0.1", port),
            format!("Recovery{attempt:02}"),
            MinecraftVersion::Java1_21_11,
        ))
        .await?;
        client.start_packet_trace(16_777_216).await?;
        let mut samples = Vec::new();
        let mut restored = false;
        let mut loaded = false;
        let failure = async {
            client.wait_until_ready().await?;
            for _ in 0..100 {
                let sample = client.observe_client_region(region).await?;
                loaded |= sample.received.blocks.iter().all(|b| b.state.is_some());
                restored |= sample.blocks.iter().any(|b| {
                    b.moving
                        .as_ref()
                        .is_some_and(|m| m.chunk_sequence.is_some())
                });
                samples.push(sample);
                tokio::time::sleep(Duration::from_millis(10)).await;
            }
            Ok::<(), voxrig::Error>(())
        }
        .await
        .err()
        .map(|e| e.to_string());
        let trace = client.stop_packet_trace().await?;
        let disconnect_error = client.disconnect().await.err().map(|e| e.to_string());
        let file = OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(format!("{directory}/client-recovery-{attempt:02}.json.gz"))?;
        let mut encoder = flate2::write::GzEncoder::new(file, flate2::Compression::default());
        serde_json::to_writer(
            &mut encoder,
            &serde_json::json!({"attempt":attempt,"loaded":loaded,"restored":restored,"samples":samples,"trace":trace,"failure":failure,"disconnect_error":disconnect_error}),
        )?;
        encoder.finish()?;
        println!(
            "attempt={attempt} loaded={loaded} restored={restored} complete={}",
            trace.complete
        );
        anyhow::ensure!(trace.complete, "incomplete capture");
        anyhow::ensure!(failure.is_none(), "capture failed: {failure:?}");
        if restored {
            return Ok(());
        }
        tokio::time::sleep(Duration::from_millis(137)).await;
    }
    anyhow::bail!("no native moving carrier captured; all attempts retained")
}
