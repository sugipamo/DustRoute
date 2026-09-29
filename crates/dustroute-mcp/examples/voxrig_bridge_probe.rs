//! Explicit isolated-server trial, built only with --features voxrig.
#[cfg(not(feature = "voxrig"))]
fn main() {
    eprintln!("requires --features voxrig");
}
#[cfg(feature = "voxrig")]
#[tokio::main]
async fn main() -> anyhow::Result<()> {
    trial::main().await
}

#[cfg(feature = "voxrig")]
mod trial {
    use dustroute_mcp::{
        BotBridge,
        bridge_protocol::{CommandWrite, PhysicalChange},
    };
    use dustroute_physical::Pos;
    use serde_json::{Value, json};
    use std::io::{self, Write};
    use voxrig::{Client, ConnectionConfig, MinecraftVersion, Server};
    const DIM: &str = "minecraft:overworld";
    async fn pause(message: &str) -> anyhow::Result<()> {
        println!("{message}");
        io::stdout().flush()?;
        let n = tokio::task::spawn_blocking(|| {
            let mut line = String::new();
            io::stdin().read_line(&mut line)
        })
        .await??;
        anyhow::ensure!(n > 0, "closed stdin");
        Ok(())
    }
    pub async fn main() -> anyhow::Result<()> {
        let file = std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(std::env::var("TRACE_OUTPUT")?)?;
        let config = |name: &str| -> anyhow::Result<_> {
            Ok(ConnectionConfig::offline(
                Server::new("127.0.0.1", std::env::var("MC_PORT")?.parse()?),
                name,
                MinecraftVersion::Java1_21_11,
            ))
        };
        let bridge = BotBridge::connect_voxrig(config("BridgeProbe")?).await?;
        let remote = Client::connect(config("AimProbe")?).await?;
        remote.wait_until_ready().await?;
        pause("READY; creative both, OP BridgeProbe; clear owned 98 180 98 .. 106 187 106; teleport BridgeProbe 100.5 182 103.5 and AimProbe 104.5 182 103.5 yaw 180 pitch 0; enter").await?;
        bridge.wait_ticks(20, DIM).await?;
        let mut steps = Vec::new();
        let result = run(&bridge, &remote, &mut steps).await;
        serde_json::to_writer(
            file,
            &json!({"steps":steps,"error":result.as_ref().err().map(ToString::to_string)}),
        )?;
        if result.is_ok() {
            pause("NATIVE_BRIDGE_MATCH; capture written; independently check lever 100 181 100 powered=true, target 102 181 100 air, marker 106 181 100 stone; enter").await?;
        }
        remote.disconnect().await?;
        result
    }
    async fn run(
        bridge: &BotBridge,
        remote: &Client,
        steps: &mut Vec<Value>,
    ) -> anyhow::Result<()> {
        steps.push(json!({"stage":"status","value":bridge.status().await?}));
        let recording = bridge
            .start_update_recording(Pos::new(98, 180, 98), Pos::new(106, 187, 106), DIM, 1024)
            .await?;
        steps.push(json!({"stage":"recording_started","value":recording}));
        let changes: [CommandWrite; 5] = [
            (Pos::new(100, 180, 100), "minecraft:stone"),
            (
                Pos::new(100, 181, 100),
                "minecraft:lever[face=floor,facing=north,powered=false]",
            ),
            (Pos::new(102, 180, 100), "minecraft:stone"),
            (Pos::new(104, 183, 100), "minecraft:stone"),
            (Pos::new(106, 181, 100), "minecraft:stone"),
        ]
        .map(|(pos, state)| CommandWrite {
            pos,
            state: state.parse().unwrap(),
        });
        steps.push(
            json!({"stage":"command_submission","value":bridge.write_blocks(&changes,DIM).await?}),
        );
        bridge.wait_ticks(5, DIM).await?;
        steps.push(json!({"stage":"players","value":bridge.visible_players().await?}));
        let target = bridge.observe_player("AimProbe", 8.0).await?;
        anyhow::ensure!(
            target.targeted_block == Some(Pos::new(104, 183, 100)),
            "target mismatch: {target:?}"
        );
        steps.push(json!({"stage":"target","value":target}));
        if std::env::var("PROBE_OUTLINE").as_deref() == Ok("1") {
            // Explicit opt-in: AimProbe also needs OP on the isolated fixture server.
            outline_targets(bridge, remote, steps).await?;
        }
        steps.push(json!({"stage":"preview","value":bridge.preview_region("AimProbe",Pos::new(100,180,100),Pos::new(106,183,100),DIM).await?}));
        let activation = bridge.activate_lever(Pos::new(100, 181, 100), DIM).await?;
        anyhow::ensure!(
            !activation.before_powered && activation.after_powered,
            "toggle mismatch"
        );
        steps.push(json!({"stage":"lever","value":activation}));
        let place = PhysicalChange::Place {
            pos: Pos::new(102, 181, 100),
            item: "minecraft:stone".into(),
            state: "minecraft:stone".parse().unwrap(),
            reference: Pos::new(102, 180, 100),
            face: Pos::new(0, 1, 0),
        };
        steps.push(json!({"stage":"physical_placement","value":bridge.place_physical_blocks(&[place],DIM).await?}));
        let placed = bridge
            .scan_client_region(Pos::new(102, 181, 100), Pos::new(102, 181, 100), DIM)
            .await?;
        anyhow::ensure!(
            placed.snapshot().blocks[0].name == "minecraft:stone",
            "placement missing"
        );
        steps.push(json!({"stage":"placed","value":placed}));
        steps.push(json!({"stage":"physical_removal","value":bridge.place_physical_blocks(&[PhysicalChange::Dig{pos:Pos::new(102,181,100)}],DIM).await?}));
        let after = bridge
            .scan_client_region(Pos::new(98, 180, 98), Pos::new(106, 187, 106), DIM)
            .await?;
        anyhow::ensure!(
            after
                .snapshot()
                .blocks
                .iter()
                .find(|b| b.pos == Pos::new(102, 181, 100))
                .unwrap()
                .name
                == "minecraft:air",
            "removal missing"
        );
        steps.push(json!({"stage":"after","value":after}));
        let recorded = bridge
            .stop_update_recording(&recording.recording_id, DIM)
            .await?;
        anyhow::ensure!(
            !recorded.truncated
                && recorded.clock == dustroute_mcp::bridge::RecordingClock::ClientFrame20Hz,
            "recording incomplete or wrong clock"
        );
        anyhow::ensure!(
            recorded.events.iter().all(|e| e.native_packet.is_some()),
            "packet order missing"
        );
        anyhow::ensure!(
            recorded
                .events
                .iter()
                .any(|e| e.pos == Pos::new(100, 181, 100)
                    && e.after
                        .as_ref()
                        .is_some_and(|s| s.properties.get("powered").is_some_and(|v| v == "true"))),
            "lever event missing"
        );
        steps.push(json!({"stage":"recording","value":recorded}));
        steps.push(json!({"stage":"maximum_wait","value":bridge.wait_ticks(200,DIM).await?}));
        // The explicit server-confirmed API must never promote client evidence.
        anyhow::ensure!(
            bridge
                .scan_region_confirmed(Pos::new(100, 180, 100), Pos::new(100, 180, 100), DIM)
                .await
                .is_err(),
            "client scan was labelled server-confirmed"
        );
        Ok(())
    }
    async fn outline_targets(
        bridge: &BotBridge,
        remote: &Client,
        steps: &mut Vec<Value>,
    ) -> anyhow::Result<()> {
        let p = Pos::new(104, 183, 100);
        bridge
            .write_blocks(
                &[CommandWrite {
                    pos: Pos::new(104, 182, 100),
                    state: "minecraft:stone".parse().unwrap(),
                }],
                DIM,
            )
            .await?;
        remote
            .java_1_21_11_operations()?
            .send_command("tp @s 104.5 185 100.5 0 90")
            .await?;
        for state in [
            "minecraft:redstone_wire[east=none,north=none,power=0,south=none,west=none]",
            "minecraft:lever[face=floor,facing=north,powered=false]",
            "minecraft:repeater[delay=1,facing=north,locked=false,powered=false]",
            "minecraft:comparator[facing=north,mode=compare,powered=false]",
        ] {
            bridge
                .write_blocks(
                    &[CommandWrite {
                        pos: p,
                        state: state.parse().unwrap(),
                    }],
                    DIM,
                )
                .await?;
            bridge.wait_ticks(10, DIM).await?;
            let target = bridge.observe_player("AimProbe", 8.0).await?;
            steps.push(json!({"stage":"outline_target","state":state,"value":target}));
            anyhow::ensure!(
                target.targeted_block == Some(p)
                    && target.targeted_face.as_deref() == Some("up")
                    && target.targeting_geometry
                        == Some(dustroute_mcp::bridge::TargetingGeometry::BlockOutline),
                "thin target mismatch: {target:?}"
            );
        }
        let obstruction = Pos::new(104, 184, 100);
        bridge
            .write_blocks(
                &[CommandWrite {
                    pos: obstruction,
                    state: "minecraft:stone".parse().unwrap(),
                }],
                DIM,
            )
            .await?;
        bridge.wait_ticks(10, DIM).await?;
        let target = bridge.observe_player("AimProbe", 8.0).await?;
        steps.push(json!({"stage":"outline_occlusion","value":target}));
        anyhow::ensure!(
            target.targeted_block == Some(obstruction),
            "occlusion mismatch"
        );
        bridge
            .write_blocks(
                &[CommandWrite {
                    pos: obstruction,
                    state: "minecraft:light[level=15,waterlogged=false]"
                        .parse()
                        .unwrap(),
                }],
                DIM,
            )
            .await?;
        bridge.wait_ticks(10, DIM).await?;
        let error = bridge.observe_player("AimProbe", 8.0).await.unwrap_err();
        anyhow::ensure!(
            error.to_string().contains("outline geometry unsupported"),
            "unexpected error: {error}"
        );
        steps.push(json!({"stage":"outline_unsupported","error":error.to_string()}));
        bridge
            .write_blocks(
                &[CommandWrite {
                    pos: obstruction,
                    state: "minecraft:air".parse().unwrap(),
                }],
                DIM,
            )
            .await?;
        Ok(())
    }
}
