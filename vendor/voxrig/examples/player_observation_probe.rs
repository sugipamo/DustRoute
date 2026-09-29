//! Two native clients on the owned isolated server; no entity simulation.
use std::{
    io::{self, Write},
    time::Duration,
};
use voxrig::{Client, ConnectionConfig, MinecraftVersion, Server};

async fn connect(name: &str) -> anyhow::Result<Client> {
    let client = Client::connect(ConnectionConfig::offline(
        Server::new("127.0.0.1", std::env::var("MC_PORT")?.parse()?),
        name,
        MinecraftVersion::Java1_21_11,
    ))
    .await?;
    client.wait_until_ready().await?;
    Ok(client)
}
async fn pause(message: &str) -> anyhow::Result<()> {
    println!("{message}");
    io::stdout().flush()?;
    let read = tokio::task::spawn_blocking(|| {
        let mut line = String::new();
        io::stdin().read_line(&mut line)
    })
    .await??;
    anyhow::ensure!(read > 0, "closed stdin");
    Ok(())
}
#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let file = std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(std::env::var("TRACE_OUTPUT")?)?;
    let observer = connect("ViewProbe").await?;
    observer.start_packet_trace(8_388_608).await?;
    let builder = connect("BuildProbe").await?;
    pause("READY; creative both, OP BuildProbe; teleport ViewProbe to 100.5 182 103.5 and BuildProbe to 101.123 182 102.5 with yaw 90 pitch 30; enter").await?;
    tokio::time::sleep(Duration::from_secs(2)).await;
    let view = observer.java_1_21_11_operations()?;
    let actions = builder.java_1_21_11_operations()?;
    let initial = view.visible_players().await?;
    actions.set_flying(true).await?;
    actions
        .move_flying([102.623, 183.0, 102.5], [-90.0, -30.0])
        .await?;
    tokio::time::sleep(Duration::from_secs(1)).await;
    let moved = view.visible_players().await?;
    actions
        .send_command("attribute @s minecraft:scale base set 2")
        .await?;
    tokio::time::sleep(Duration::from_secs(1)).await;
    let scaled = view.visible_players().await?;
    actions
        .send_command("attribute @s minecraft:scale base set 1")
        .await?;
    tokio::time::sleep(Duration::from_millis(300)).await;
    builder.disconnect().await?;
    tokio::time::sleep(Duration::from_secs(1)).await;
    let departed = view.visible_players().await?;
    let reconnected = connect("BuildProbe").await?;
    tokio::time::sleep(Duration::from_secs(1)).await;
    let joined = view.visible_players().await?;
    let trace = observer.stop_packet_trace().await?;
    serde_json::to_writer(
        file,
        &serde_json::json!({
            "initial":initial,"moved":moved,"scaled":scaled,"departed":departed,"joined":joined,"trace":trace
        }),
    )?;
    let named = |v: &voxrig::versions::java_1_21_11::players::PlayerObservations| {
        v.players.iter().find(|p| p.name == "BuildProbe").cloned()
    };
    let before = named(&initial).ok_or_else(|| anyhow::anyhow!("initial player missing"))?;
    let after = named(&moved).ok_or_else(|| anyhow::anyhow!("moved player missing"))?;
    for (observed, expected) in after.position.iter().zip([102.623, 183.0, 102.5]) {
        anyhow::ensure!(
            (observed - expected).abs() < 1.0 / 4096.0,
            "position mismatch"
        );
    }
    anyhow::ensure!(
        (after.rotation[0] + 90.0).abs() < 1.5 && (after.rotation[1] + 30.0).abs() < 1.5,
        "rotation mismatch"
    );
    let large = named(&scaled).ok_or_else(|| anyhow::anyhow!("scaled player missing"))?;
    anyhow::ensure!(
        large.scale == 2.0
            && (large.eye_position.unwrap()[1] - large.position[1] - 3.24).abs() < 0.00001,
        "scale mismatch"
    );
    anyhow::ensure!(named(&departed).is_none(), "departed entity retained");
    let new = named(&joined).ok_or_else(|| anyhow::anyhow!("reconnected player missing"))?;
    anyhow::ensure!(
        new.uuid == before.uuid && new.entity_id != before.entity_id && new.scale == 1.0,
        "reconnect identity mismatch"
    );
    pause("PLAYER_OBSERVATIONS_MATCH; capture written, check server position and scale, then enter to disconnect").await?;
    reconnected.disconnect().await?;
    observer.disconnect().await?;
    Ok(())
}
