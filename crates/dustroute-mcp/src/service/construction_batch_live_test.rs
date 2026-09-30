//! Opt-in isolated native measurement of the real shared executor. Ordinary
//! tests never connect or write. No server or existing bot restart is involved.
use super::*;
use crate::assembly_registry::TargetServer;
use crate::performance::measure;
use dustroute_translate::piston_construction::{ElectricalConstruction, construction_batches};
use dustroute_translate::world_reverse::RegionBounds;

const DIMENSION: &str = "minecraft:overworld";

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
#[ignore = "writes a dedicated initially empty live region; requires explicit opt-in"]
async fn profile_native_construction_batches() -> anyhow::Result<()> {
    use voxrig::{Client, ConnectionConfig, MinecraftVersion, Server};
    anyhow::ensure!(
        std::env::var("DUSTROUTE_LIVE_BATCH_PROBE").as_deref() == Ok("1"),
        "explicit live measurement opt-in required"
    );
    let output = std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(std::env::var("TRACE_OUTPUT")?)?;
    let port: u16 = std::env::var("MC_PORT")?.parse()?;
    let config = || {
        ConnectionConfig::offline(
            Server::new("127.0.0.1", port),
            "dustroutetest",
            MinecraftVersion::Java1_21_11,
        )
    };
    // The dedicated whitelisted test actor is already an operator on the local
    // isolated server. Position only this actor through its native client API.
    let setup = Client::connect(config()).await?;
    setup.wait_until_ready().await?;
    let controls = setup.java_1_21_11_operations()?;
    controls.set_flying(true).await?;
    controls.send_command("tp @s 1231.5 184 1201.5").await?;
    let deadline = Instant::now() + Duration::from_secs(3);
    loop {
        let state = controls.player_state().await?;
        if state.position_from_server && state.position == Some([1231.5, 184.0, 1201.5]) {
            break;
        }
        anyhow::ensure!(
            Instant::now() < deadline,
            "test actor teleport not observed"
        );
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    setup.disconnect().await?;
    tokio::time::sleep(Duration::from_millis(200)).await;
    let bridge = BotBridge::connect_voxrig(config()).await?;
    bridge.wait_ticks(40, DIMENSION).await?;
    let bounds = RegionBounds::new(Pos::new(1216, 179, 1200), Pos::new(1247, 185, 1203));
    let policy = McpPolicy {
        read_only: false,
        allowed_region: Some(bounds.into()),
        max_placement_blocks: 32,
        ..Default::default()
    };
    let baseline = bridge
        .scan_region_fresh(bounds.min, bounds.max, DIMENSION)
        .await?
        .into_stationary_record()
        .map_err(anyhow::Error::msg)?;
    let baseline = (*baseline.snapshot).clone();
    anyhow::ensure!(
        dustroute_translate::snapshot::index_literal_snapshot(&baseline)
            .map_err(anyhow::Error::msg)?
            .is_empty(),
        "measurement region must be completely observed air before any write"
    );
    let status = bridge.status().await?;
    let target = TargetServer::observed(&status, DIMENSION).map_err(anyhow::Error::msg)?;
    let executor = construction_executor::ConstructionExecutor {
        bridge: &bridge,
        policy: &policy,
        target: &target,
    };
    let region = dustroute_translate::world::Region::new(bounds.min, bounds.max);
    let mut samples = Vec::new();
    for count in [2, 16] {
        let mut world = dustroute_translate::world::World::new();
        for x in 0..count {
            world.place(BlockKind::Solid, Pos::new(1217 + x, 181, 1201));
        }
        let proof = ElectricalConstruction::new(&world, region, Default::default())
            .map_err(anyhow::Error::msg)?;
        anyhow::ensure!(
            construction_batches(proof.build_steps()).count() == 1,
            "fixture must exercise one fresh model batch"
        );
        // Retain the old single-command execution schedule as a comparison.
        // Decoding intentionally discards the private fresh batching result.
        let sequential: Vec<dustroute_translate::piston_construction::ElectricalConstructionStep> =
            serde_json::from_value(serde_json::to_value(proof.build_steps())?)?;
        anyhow::ensure!(
            construction_batches(&sequential).count() == count as usize,
            "reference must observe each command independently"
        );
        for sample in 1..=3 {
            for (mode, steps) in [
                ("sequential", sequential.as_slice()),
                ("batched", proof.build_steps()),
            ] {
                let mut verified = 0;
                let mut readbacks = Vec::new();
                let (result, measurement) = measure(
                    "native_construction_apply",
                    executor.execute(&baseline, steps, |progress| {
                        match progress {
                            construction_executor::StageProgress::Verified(n) => verified = n,
                            construction_executor::StageProgress::Readback(r) => readbacks.push(*r),
                        }
                        Ok(())
                    }),
                )
                .await;
                result.map_err(anyhow::Error::msg)?;
                anyhow::ensure!(
                    verified == count as usize,
                    "entire measured sequence must be verified"
                );
                let batches = construction_batches(steps).count();
                anyhow::ensure!(
                    readbacks.len() == 2 * batches
                        && measurement.phases["write"].commands == count as u64,
                    "complete readback boundaries and write count required"
                );
                samples.push(json!({"blocks":count,"sample":sample,"mode":mode,"verified_steps":verified,"execution_batches":batches,"measurement":measurement,"readbacks":readbacks}));
                println!(
                    "LIVE_BATCH blocks={count} sample={sample} mode={mode} ms={:.3}",
                    measurement.elapsed_ms
                );
                executor
                    .execute(proof.settled(), proof.remove_steps(), |_| Ok(()))
                    .await
                    .map_err(anyhow::Error::msg)?;
            }
        }
    }
    let final_readback = bridge
        .scan_region_fresh(bounds.min, bounds.max, DIMENSION)
        .await?
        .into_stationary_record()
        .map_err(anyhow::Error::msg)?;
    anyhow::ensure!(
        dustroute_translate::snapshot::index_literal_snapshot(&final_readback.snapshot)
            .map_err(anyhow::Error::msg)?
            .is_empty(),
        "complete live region must return to air"
    );
    serde_json::to_writer_pretty(
        output,
        &json!({"context":{"date":"2026-09-30","server":"Vanilla Java 1.21.11","backend":"Voxrig native","profile":"unoptimized debug","dummy":"dustroutetest","bounds":bounds,"fixture":"independent stone blocks in initially observed air","scope":"real shared executor including waits and full-region readbacks; no MCP dispatch, planning, preview, stationary prechecks or durable callback writes","reference":"same freshly modeled command sequence deserialized to retain per-command readback","world_changes":"owned fixture writes and verified removals only; existing machines and bot unchanged"},"samples":samples,"final_readback":final_readback.readback,"restored_to_air":true}),
    )?;
    // Dropping the final bridge disconnects the temporary test actor.
    println!("LIVE_BATCH_RESTORED_TO_AIR");
    Ok(())
}
