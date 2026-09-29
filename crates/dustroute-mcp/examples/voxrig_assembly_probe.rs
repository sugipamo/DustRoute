//! Explicit isolated-server MCP lifecycle trial. Never run against a player world.
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
#[allow(dead_code)]
#[path = "../../dustroute-translate/tests/support/reference_door_blueprint.rs"]
mod door_fixture;

#[cfg(feature = "voxrig")]
mod trial {
    use dustroute_mcp::{BotBridge, DustRouteMcp, McpConfig, McpPolicy};
    use dustroute_physical::Pos;
    use rmcp::{
        ServiceExt,
        model::{CallToolRequestParams, ClientInfo, ContentBlock},
        service::{RoleClient, RunningService},
    };
    use serde_json::{Value, json};
    use std::{fs::File, io::Write, time::Duration};
    use voxrig::{ConnectionConfig, MinecraftVersion, Server};
    const DIM: &str = "minecraft:overworld";
    const ORIGIN: Pos = Pos::new(100, 180, 100);
    type McpClient = RunningService<RoleClient, ClientInfo>;
    type Session = (McpClient, tokio::task::JoinHandle<anyhow::Result<()>>);
    fn translated(p: Pos) -> Pos {
        Pos::new(p.x + ORIGIN.x, p.y + ORIGIN.y, p.z + ORIGIN.z)
    }
    fn record(file: &mut File, value: &Value) -> anyhow::Result<()> {
        serde_json::to_writer(&mut *file, value)?;
        writeln!(file)?;
        file.flush()?;
        Ok(())
    }
    async fn pause(message: &str) -> anyhow::Result<()> {
        println!("{message}");
        std::io::stdout().flush()?;
        let n = tokio::task::spawn_blocking(|| {
            let mut line = String::new();
            std::io::stdin().read_line(&mut line)
        })
        .await??;
        anyhow::ensure!(n > 0, "closed stdin");
        Ok(())
    }
    async fn start(port: u16) -> anyhow::Result<Session> {
        let policy = McpPolicy {
            read_only: false,
            allowed_region: Some(dustroute_mcp::discovery::RegionBoundsDto {
                min: Pos::new(80, 170, 80),
                max: Pos::new(120, 205, 120),
            }),
            allowed_players: ["McpProbe".into()].into(),
            ..McpPolicy::default()
        };
        let service = DustRouteMcp::connect_voxrig(
            McpConfig::new(format!("127.0.0.1:{port}"), "McpProbe", "127.0.0.1:1")?,
            policy,
            "DustRouteBot",
        )
        .await?;
        let (server_io, client_io) = tokio::io::duplex(1024 * 1024);
        let server = tokio::spawn(async move {
            service.serve(server_io).await?.waiting().await?;
            Ok(())
        });
        Ok((ClientInfo::default().serve(client_io).await?, server))
    }
    async fn stop((client, server): Session) -> anyhow::Result<()> {
        client.cancel().await?;
        server.await??;
        Ok(())
    }
    async fn call(
        client: &McpClient,
        file: &mut File,
        name: &str,
        args: Value,
        ok: bool,
    ) -> anyhow::Result<Value> {
        println!("MCP {name} {}", args.get("action").unwrap_or(&Value::Null));
        let response = client
            .call_tool(
                CallToolRequestParams::new(name.to_owned())
                    .with_arguments(args.as_object().unwrap().clone()),
            )
            .await?;
        let ContentBlock::Text(text) = &response.content[0] else {
            anyhow::bail!("expected JSON tool response")
        };
        let result: Value = serde_json::from_str(&text.text)?;
        record(
            file,
            &json!({"stage":"tool","name":name,"args":args,"response":result}),
        )?;
        anyhow::ensure!(result["ok"] == ok, "{name}: {result}");
        Ok(result)
    }
    async fn show_apply(client: &McpClient, file: &mut File, id: &Value) -> anyhow::Result<Value> {
        call(
            client,
            file,
            "show_operation",
            json!({"operation_id":id}),
            true,
        )
        .await?;
        call(
            client,
            file,
            "invoke_operation",
            json!({"operation_id":id,"confirm":true}),
            true,
        )
        .await
    }
    async fn write(actor: &BotBridge, p: Pos, state: &str) -> anyhow::Result<()> {
        actor
            .write_blocks(
                &[dustroute_mcp::bridge_protocol::CommandWrite {
                    pos: p,
                    state: state.parse().map_err(anyhow::Error::msg)?,
                }],
                DIM,
            )
            .await?;
        actor.wait_ticks(20, DIM).await?;
        Ok(())
    }
    async fn capture(
        actor: &BotBridge,
        file: &mut File,
        stage: &str,
        min: Pos,
        max: Pos,
    ) -> anyhow::Result<()> {
        let observation = actor.scan_client_region(min, max, DIM).await?;
        record(
            file,
            &json!({"stage":stage,"after_client":observation.observation()}),
        )?;
        Ok(())
    }
    pub async fn main() -> anyhow::Result<()> {
        let state_dir = std::path::PathBuf::from(std::env::var("DUSTROUTE_STATE_DIR")?);
        anyhow::ensure!(
            !state_dir.exists(),
            "trial needs a new, isolated state directory"
        );
        let mut file = File::options()
            .write(true)
            .create_new(true)
            .open(std::env::var("TRACE_OUTPUT")?)?;
        let port: u16 = std::env::var("MC_PORT")?.parse()?;
        let case = std::env::var("PROBE_CASE").unwrap_or_else(|_| "door".into());
        anyhow::ensure!(matches!(case.as_str(), "door" | "flight"), "unknown case");
        let actor = BotBridge::connect_voxrig(ConnectionConfig::offline(
            Server::new("127.0.0.1", port),
            "McpProbe",
            MinecraftVersion::Java1_21_11,
        ))
        .await?;
        let mut session = Some(start(port).await?);
        pause("READY; creative+OP DustRouteBot and McpProbe; tp both near 100 190 104; owned area 80 170 80 .. 120 205 120 must be clear; enter").await?;
        let result = run(&actor, &mut session, &mut file, port, &case).await;
        record(
            &mut file,
            &json!({"stage":"result","error":result.as_ref().err().map(|e|format!("{e:#}"))}),
        )?;
        // Hold clients even after failure so the exact failed world can be captured
        // and cleaned by the isolated server operator before disconnection.
        pause("TRIAL_FINISHED; capture flushed. Independently inspect/confirm and clean the declared area, revoke OP, then enter").await?;
        if let Some(session) = session.take() {
            stop(session).await?;
        }
        result
    }
    async fn run(
        actor: &BotBridge,
        session: &mut Option<Session>,
        file: &mut File,
        port: u16,
        case: &str,
    ) -> anyhow::Result<()> {
        let client = &session.as_ref().unwrap().0;
        let (records, mut request) = if case == "door" {
            let f = super::door_fixture::ordinary_fixture();
            (
                json!({"types":f.catalog.type_revisions().collect::<Vec<_>>(),"classifications":f.catalog.classifications().collect::<Vec<_>>(),"revisions":f.catalog.revisions().collect::<Vec<_>>(),"assemblies":[f.base]}),
                serde_json::to_value(f.request)?,
            )
        } else {
            let generated=call(client,file,"test_circuit_change",json!({"blueprint":{"action":"generate_flying_machine","request":{"namespace":"native.flight","body":"honey_nose","distance":3,"rotation":"r270","mirrored":true}}}),true).await?;
            anyhow::ensure!(
                generated["result"]["verification"]["status"] == "passed",
                "generation failed"
            );
            (
                generated["result"]["records"].clone(),
                generated["result"]["request"].clone(),
            )
        };
        request.as_object_mut().unwrap().remove("id");
        let min = translated(serde_json::from_value(
            request["behavior_context"]["known_region"]["min"].clone(),
        )?);
        let max = translated(serde_json::from_value(
            request["behavior_context"]["known_region"]["max"].clone(),
        )?);
        let input = translated(serde_json::from_value(
            request["behavior_context"]["input_levers"][0].clone(),
        )?);
        record(
            file,
            &json!({"stage":"bounds","min":min,"max":max,"input":input,"case":case}),
        )?;
        actor.wait_ticks(20, DIM).await?;
        let initial = actor.scan_client_region(min, max, DIM).await?;
        anyhow::ensure!(
            initial
                .snapshot()
                .blocks
                .iter()
                .all(|b| b.name == "minecraft:air"),
            "owned target not empty"
        );
        call(
            client,
            file,
            "test_circuit_change",
            json!({"blueprint":{"action":"import","records":records}}),
            true,
        )
        .await?;
        let proposal = call(
            client,
            file,
            "test_circuit_change",
            json!({"blueprint":{"action":"propose_update","request":request}}),
            true,
        )
        .await?;
        call(
            client,
            file,
            "show_operation",
            json!({"operation_id":proposal["operation_id"]}),
            true,
        )
        .await?;
        call(client,file,"invoke_operation",json!({"operation_id":proposal["operation_id"],"confirm":true,"blueprint_decision":{"action":"adopt"}}),true).await?;
        let plan=call(client,file,"new_placement",json!({"assembly_revision_id":request["candidate_state"]["id"],"assembly_target":{"source_anchor":{"x":0,"y":0,"z":0},"target_anchor":ORIGIN,"rotation":"r0"}}),true).await?;
        let id = plan["operation_id"].clone();
        call(
            client,
            file,
            "invoke_operation",
            json!({"operation_id":id,"confirm":true}),
            false,
        )
        .await?;
        call(
            client,
            file,
            "show_operation",
            json!({"operation_id":id}),
            true,
        )
        .await?;
        write(actor, min, "minecraft:stone").await?;
        call(
            client,
            file,
            "invoke_operation",
            json!({"operation_id":id,"confirm":true}),
            false,
        )
        .await?;
        write(actor, min, "minecraft:air").await?;
        let applied = call(
            client,
            file,
            "invoke_operation",
            json!({"operation_id":id,"confirm":true}),
            true,
        )
        .await?;
        anyhow::ensure!(applied["status"] == "verified", "placement unverified");
        capture(actor, file, "placed", min, max).await?;
        pause("PLACED; client capture flushed; independently confirm the full region, then enter")
            .await?;
        stop(session.take().unwrap()).await?;
        tokio::time::sleep(Duration::from_millis(250)).await;
        *session = Some(start(port).await?);
        actor.wait_ticks(20, DIM).await?;
        let client = &session.as_ref().unwrap().0;
        let saved = call(
            client,
            file,
            "manage_assembly",
            json!({"action":"get","instance_id":id}),
            true,
        )
        .await?;
        anyhow::ensure!(
            saved["fresh_observation"] == false,
            "archive promoted to fresh evidence"
        );
        let observed = call(
            client,
            file,
            "manage_assembly",
            json!({"action":"observe","instance_id":id}),
            true,
        )
        .await?;
        anyhow::ensure!(
            observed["observation"]["status"] == "matches",
            "fresh observation mismatch"
        );
        call(
            client,
            file,
            "undo_operation",
            json!({"operation_id":id,"confirm":true}),
            false,
        )
        .await?;
        if case == "door" {
            let removed = translated(Pos::new(0, 9, 0));
            write(actor, removed, "minecraft:air").await?;
            let diagnosis = call(
                client,
                file,
                "manage_assembly",
                json!({"action":"diagnose","instance_id":id}),
                true,
            )
            .await?;
            anyhow::ensure!(
                diagnosis["diagnosis"]["summary"]["by_kind"]["missing"] == 1,
                "missing-part diagnosis mismatch"
            );
            let repair = call(
                client,
                file,
                "manage_assembly",
                json!({"action":"plan_reconstruction","instance_id":id}),
                true,
            )
            .await?;
            call(
                client,
                file,
                "invoke_operation",
                json!({"operation_id":repair["operation_id"],"confirm":true}),
                false,
            )
            .await?;
            call(
                client,
                file,
                "show_operation",
                json!({"operation_id":repair["operation_id"]}),
                true,
            )
            .await?;
            write(actor, min, "minecraft:stone").await?;
            call(
                client,
                file,
                "invoke_operation",
                json!({"operation_id":repair["operation_id"],"confirm":true}),
                false,
            )
            .await?;
            write(actor, min, "minecraft:air").await?;
            let repaired = call(
                client,
                file,
                "invoke_operation",
                json!({"operation_id":repair["operation_id"],"confirm":true}),
                true,
            )
            .await?;
            anyhow::ensure!(repaired["status"] == "verified", "repair unverified");
        }
        for powered in if case == "door" {
            vec![true, false]
        } else {
            vec![true]
        } {
            let activation = actor.activate_lever(input, DIM).await?;
            anyhow::ensure!(activation.after_powered == powered, "input toggle mismatch");
            actor.wait_ticks(100, DIM).await?;
            let diagnosis = call(
                client,
                file,
                "manage_assembly",
                json!({"action":"diagnose","instance_id":id}),
                true,
            )
            .await?;
            anyhow::ensure!(
                diagnosis["diagnosis"]["status"] == "matches_reference",
                "operating reference mismatch"
            );
            let stage = if powered { "operated" } else { "reopened" };
            capture(actor, file, stage, min, max).await?;
            if case == "door" {
                let aperture = actor
                    .scan_client_region(
                        translated(Pos::new(0, 6, -1)),
                        translated(Pos::new(0, 8, 1)),
                        DIM,
                    )
                    .await?;
                anyhow::ensure!(
                    aperture.snapshot().blocks.iter().all(|b| b.name
                        == if powered {
                            "minecraft:smooth_quartz"
                        } else {
                            "minecraft:air"
                        }),
                    "aperture mismatch"
                );
            }
            pause("OPERATED; client capture flushed; independently confirm the full region, then enter").await?;
        }
        let mut removal_args = json!({"action":"plan_removal","instance_id":id});
        if case == "flight" {
            removal_args["removal_reference"] = json!("observed_inputs");
        }
        let removal = call(client, file, "manage_assembly", removal_args, true).await?;
        call(
            client,
            file,
            "invoke_operation",
            json!({"operation_id":removal["operation_id"],"confirm":true}),
            false,
        )
        .await?;
        let undone = show_apply(client, file, &removal["operation_id"]).await?;
        anyhow::ensure!(undone["status"] == "verified", "removal unverified");
        capture(actor, file, "removed", min, max).await?;
        let empty = actor.scan_client_region(min, max, DIM).await?;
        anyhow::ensure!(
            empty
                .snapshot()
                .blocks
                .iter()
                .all(|b| b.name == "minecraft:air"),
            "removal left blocks"
        );
        stop(session.take().unwrap()).await?;
        tokio::time::sleep(Duration::from_millis(250)).await;
        *session = Some(start(port).await?);
        actor.wait_ticks(20, DIM).await?;
        let final_observation = call(
            &session.as_ref().unwrap().0,
            file,
            "manage_assembly",
            json!({"action":"observe","instance_id":id}),
            true,
        )
        .await?;
        anyhow::ensure!(
            final_observation["instance"]["state"] == "removed"
                && final_observation["observation"]["status"] == "matches",
            "removed archive mismatch"
        );
        Ok(())
    }
}
