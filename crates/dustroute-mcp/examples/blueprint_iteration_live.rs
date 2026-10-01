//! Opt-in public MCP trial in an initially empty, disposable Vanilla server region.
//! Run through tools/verify_blueprint_iteration_live.py, never on a player server.
#[cfg(not(feature = "voxrig"))]
fn main() {
    eprintln!("requires --features voxrig");
}

#[cfg(feature = "voxrig")]
#[allow(dead_code)]
#[path = "../../dustroute-translate/tests/support/runtime_blueprint.rs"]
mod fixture;

#[cfg(feature = "voxrig")]
#[tokio::main(worker_threads = 2)]
async fn main() -> anyhow::Result<()> {
    trial::run().await
}

#[cfg(feature = "voxrig")]
mod trial {
    use dustroute_mcp::BotBridge;
    use dustroute_physical::Pos;
    use rmcp::{
        ServiceExt,
        model::{CallToolRequestParams, ClientInfo, ContentBlock},
        service::{RoleClient, RunningService},
    };
    use serde_json::{Value, json};
    use std::{
        fs::File,
        io::Write,
        process::Stdio,
        time::{Duration, Instant},
    };
    use tokio::process::{Child, Command};
    use voxrig::{ConnectionConfig, MinecraftVersion, Server};

    type Client = RunningService<RoleClient, ClientInfo>;
    const DIM: &str = "minecraft:overworld";
    const MIN: Pos = Pos::new(1280, 179, 1200);
    const MAX: Pos = Pos::new(1307, 185, 1207);
    const A: Pos = Pos::new(1282, 181, 1202);
    const B: Pos = Pos::new(1292, 181, 1202);
    const OBSERVER: Pos = Pos::new(1303, 181, 1202);

    fn region_jobs() -> bool {
        std::env::var("DUSTROUTE_REGION_JOB_PROBE").as_deref() == Ok("1")
    }
    fn record(file: &mut File, value: Value) -> anyhow::Result<()> {
        serde_json::to_writer(&mut *file, &value)?;
        writeln!(file)?;
        file.flush()?;
        Ok(())
    }
    async fn barrier(message: String) -> anyhow::Result<()> {
        println!("{message}");
        std::io::stdout().flush()?;
        let reply = tokio::task::spawn_blocking(|| {
            let mut reply = String::new();
            std::io::stdin()
                .read_line(&mut reply)
                .map(|count| (count, reply))
        })
        .await??;
        anyhow::ensure!(
            reply.0 > 0 && reply.1.trim() == "OK",
            "operator verification failed or closed"
        );
        Ok(())
    }
    async fn start(port: u16, file: &mut File) -> anyhow::Result<(Client, Child)> {
        let stderr = File::options()
            .create(true)
            .append(true)
            .open(format!("{}.mcp-stderr.log", std::env::var("TRACE_OUTPUT")?))?;
        let mut child = Command::new(std::env::var("MCP_PROCESS_BIN")?)
            .env("TOKIO_WORKER_THREADS", "2")
            .env("DUSTROUTE_BOT_BACKEND", "voxrig")
            .env("DUSTROUTE_SERVER_ADDRESS", format!("127.0.0.1:{port}"))
            .env("DUSTROUTE_BOT_NAME", "DustRouteBot")
            .env("DUSTROUTE_ASSIST_PLAYER", "dustroutetest")
            .env("DUSTROUTE_MCP_TRANSPORT", "stdio")
            .env("DUSTROUTE_MCP_TOOL_PROFILE", "default")
            .env("DUSTROUTE_MC_AUTH", "offline")
            .env("DUSTROUTE_MC_VERSION", "1.21.11")
            .env("DUSTROUTE_READ_ONLY", "false")
            .env("DUSTROUTE_PREVIEW_REQUIRED", "true")
            .env("DUSTROUTE_ALLOWED_PLAYERS", "dustroutetest")
            .env(
                "DUSTROUTE_ALLOWED_REGION",
                if region_jobs() {
                    "1279,178,1199,1312,211,1232"
                } else {
                    "1280,179,1200,1307,185,1207"
                },
            )
            .env(
                "DUSTROUTE_MAX_SCAN_VOLUME",
                if region_jobs() { "65536" } else { "4096" },
            )
            .env("DUSTROUTE_MAX_PLACEMENT_BLOCKS", "256")
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(stderr)
            .spawn()?;
        let input = child.stdin.take().unwrap();
        let output = child.stdout.take().unwrap();
        let client = tokio::time::timeout(
            Duration::from_secs(30),
            ClientInfo::default().serve((output, input)),
        )
        .await??;
        record(
            file,
            json!({"stage":"mcp_started","pid":child.id(),"port":port}),
        )?;
        barrier("POSITION_CLIENTS".into()).await?;
        Ok((client, child))
    }
    async fn stop((client, mut child): (Client, Child), file: &mut File) -> anyhow::Result<()> {
        client.cancel().await?;
        let status = tokio::time::timeout(Duration::from_secs(15), child.wait()).await??;
        anyhow::ensure!(status.success(), "MCP exited {status}");
        record(file, json!({"stage":"mcp_stopped","clean_exit":true}))
    }
    async fn call_unchecked(
        client: &Client,
        file: &mut File,
        name: &str,
        args: Value,
    ) -> anyhow::Result<Value> {
        println!(
            "TOOL {name} {}",
            args.pointer("/blueprint/action").unwrap_or(&Value::Null)
        );
        let started = Instant::now();
        let response = tokio::time::timeout(
            Duration::from_secs(240),
            client.call_tool(
                CallToolRequestParams::new(name.to_owned())
                    .with_arguments(args.as_object().unwrap().clone()),
            ),
        )
        .await??;
        let ContentBlock::Text(text) = &response.content[0] else {
            anyhow::bail!("expected JSON text")
        };
        let result: Value = serde_json::from_str(&text.text)?;
        record(
            file,
            json!({"stage":"tool","name":name,"args":args,"response":result,"elapsed_ms":started.elapsed().as_secs_f64()*1000.0}),
        )?;
        Ok(result)
    }
    async fn call(
        client: &Client,
        file: &mut File,
        name: &str,
        args: Value,
        ok: bool,
    ) -> anyhow::Result<Value> {
        let result = call_unchecked(client, file, name, args).await?;
        anyhow::ensure!(result["ok"] == ok, "{name}: {result}");
        Ok(result)
    }
    async fn show_apply(client: &Client, file: &mut File, op: &Value) -> anyhow::Result<Value> {
        call(
            client,
            file,
            "show_operation",
            json!({"operation_id":op}),
            true,
        )
        .await?;
        call(
            client,
            file,
            "invoke_operation",
            json!({"operation_id":op,"confirm":true}),
            true,
        )
        .await
    }
    async fn adopt(client: &Client, file: &mut File, generated: &Value) -> anyhow::Result<Value> {
        call(
            client,
            file,
            "test_circuit_change",
            json!({"blueprint":{"action":"import","records":generated["records"]}}),
            true,
        )
        .await?;
        let mut request = generated["request"].clone();
        request.as_object_mut().unwrap().remove("id");
        let op = call(
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
            json!({"operation_id":op["operation_id"]}),
            true,
        )
        .await?;
        call(client, file, "invoke_operation", json!({"operation_id":op["operation_id"],"confirm":true,"blueprint_decision":{"action":"adopt"}}), true).await?;
        Ok(request["candidate_state"]["id"].clone())
    }
    async fn checkpoint(actor: &BotBridge, file: &mut File, stage: &str) -> anyhow::Result<Value> {
        checkpoint_region(actor, file, stage, MIN, MAX).await
    }
    async fn checkpoint_region(
        actor: &BotBridge,
        file: &mut File,
        stage: &str,
        min: Pos,
        max: Pos,
    ) -> anyhow::Result<Value> {
        actor.wait_ticks(20, DIM).await?;
        let sample = actor.scan_region_observed(min, max, DIM).await?;
        let snapshot = serde_json::to_value(sample.snapshot)?;
        let value = json!({"stage":stage,"snapshot":snapshot,"readback":sample.readback});
        record(file, value.clone())?;
        // Independent server console predicates are required before advancing.
        barrier(format!("CHECKPOINT {value}")).await?;
        Ok(snapshot)
    }
    fn non_air(snapshot: &Value) -> usize {
        snapshot["blocks"]
            .as_array()
            .unwrap()
            .iter()
            .filter(|b| b["name"] != "minecraft:air")
            .count()
    }
    fn block_name(snapshot: &Value, pos: Pos) -> &str {
        snapshot["blocks"]
            .as_array()
            .unwrap()
            .iter()
            .find(|b| b["pos"] == json!(pos))
            .and_then(|b| b["name"].as_str())
            .unwrap_or("minecraft:air")
    }
    async fn write(
        actor: &BotBridge,
        file: &mut File,
        pos: Pos,
        state: &str,
    ) -> anyhow::Result<()> {
        record(
            file,
            json!({"stage":"external_fixture_write","position":pos,"state":state}),
        )?;
        actor
            .write_blocks(
                &[dustroute_mcp::bridge_protocol::CommandWrite {
                    pos,
                    state: state.parse().map_err(anyhow::Error::msg)?,
                }],
                DIM,
            )
            .await?;
        actor.wait_ticks(20, DIM).await?;
        Ok(())
    }
    fn design(namespace: &str, material: &str) -> Value {
        json!({"namespace":namespace,"name":"Small live building",
            "known_region":{"min":{"x":-1,"y":-1,"z":-1},"max":{"x":4,"y":3,"z":3}},
            "parts":[{"name":"floor","shapes":[{"kind":"fill","material":"stone","region":{"min":{"x":0,"y":0,"z":0},"max":{"x":2,"y":0,"z":0}}}]},
                {"name":"window","shapes":[{"kind":"blocks","material":material,"positions":[{"x":1,"y":1,"z":1}]}]}],
            "spaces":[{"name":"gap","region":{"min":{"x":2,"y":1,"z":1},"max":{"x":2,"y":1,"z":1}}}]})
    }
    async fn place(
        client: &Client,
        file: &mut File,
        revision: &Value,
        origin: Pos,
    ) -> anyhow::Result<Value> {
        let plan = call(client,file,"new_placement",json!({"assembly_revision_id":revision,"assembly_target":{"source_anchor":{"x":0,"y":0,"z":0},"target_anchor":origin,"rotation":"r0"}}),true).await?;
        call(
            client,
            file,
            "invoke_operation",
            json!({"operation_id":plan["operation_id"],"confirm":true}),
            false,
        )
        .await?;
        let result = show_apply(client, file, &plan["operation_id"]).await?;
        anyhow::ensure!(result["status"] == "verified", "construction not verified");
        Ok(plan["operation_id"].clone())
    }
    async fn capture_change(
        client: &Client,
        file: &mut File,
        min: Pos,
        max: Pos,
        pos: Pos,
        block: &str,
        look_at: Pos,
    ) -> anyhow::Result<Value> {
        barrier(format!("AIM_CLIENT {}", json!(look_at))).await?;
        let deadline = Instant::now() + Duration::from_secs(3);
        let capture = loop {
            // The default public profile excludes the debug-only gaze tool.
            // Retry only this read-only capture's bounded targeting barrier.
            let capture = call_unchecked(
                client,
                file,
                "get_world",
                json!({"region":{"min":min,"max":max},"include_block_list":true}),
            )
            .await?;
            if capture["ok"] == true {
                break capture;
            }
            let reason = capture["error"].as_str().unwrap_or("");
            anyhow::ensure!(
                Instant::now() < deadline
                    && matches!(
                        reason,
                        "look at a block in the work region"
                            | "work region must contain the current gaze target"
                    ),
                "dummy targeting/capture did not complete: {capture}"
            );
            tokio::time::sleep(Duration::from_millis(50)).await;
        };
        call(
            client,
            file,
            "test_circuit_change",
            json!({"circuit_id":capture["circuit_id"],"changes":[{"position":pos,"block":block}]}),
            true,
        )
        .await
    }
    async fn region_job_trial(
        actor: &BotBridge,
        client: &Client,
        file: &mut File,
    ) -> anyhow::Result<Value> {
        let min = Pos::new(1280, 179, 1200);
        let max = Pos::new(1311, 210, 1231);
        let guard_min = Pos::new(1279, 178, 1199);
        let guard_max = Pos::new(1312, 211, 1232);
        let empty = checkpoint_region(
            actor,
            file,
            "job_initial_full_context",
            guard_min,
            guard_max,
        )
        .await?;
        anyhow::ensure!(
            non_air(&empty) == 0,
            "job fixture must be initially all air"
        );
        let capture = call(
            client,
            file,
            "get_world",
            json!({"region":{"min":min,"max":max}}),
            true,
        )
        .await?;
        anyhow::ensure!(
            capture["target"].is_null()
                && capture["scan"]["volume"] == 32768
                && capture["observation_capabilities"]["backend"] == "voxrig",
            "explicit native capture failed"
        );
        let changes = (181..183)
            .flat_map(|y| {
                (1282..1287).flat_map(move |x| {
                    (1202..1210).map(move |z| {
                        if (x, y, z) == (1282, 182, 1202) {
                            json!({"position":{"x":x,"y":y,"z":z},"block":"minecraft:lever",
                                "properties":{"face":"floor","facing":"north","powered":"true"}})
                        } else if (x,y,z)==(1282,181,1202) {
                            json!({"position":{"x":x,"y":y,"z":z},"block":"minecraft:redstone_lamp","properties":{"lit":"true"}})
                        } else {
                            json!({"position":{"x":x,"y":y,"z":z},"block":"minecraft:stone"})
                        }
                    })
                })
            })
            .collect::<Vec<_>>();
        let revision = call(
            client,
            file,
            "test_circuit_change",
            json!({"circuit_id":capture["circuit_id"],"changes":changes,"simulation_ticks":1}),
            true,
        )
        .await?;
        // Deliberately request the dependent region first. The work plan
        // must place its floor support in the other region before the lever.
        let regions = json!([
            {"min":{"x":1282,"y":182,"z":1202},"max":{"x":1286,"y":182,"z":1209}},
            {"min":{"x":1282,"y":181,"z":1202},"max":{"x":1286,"y":181,"z":1209}}]);
        let first = call(
            client,
            file,
            "new_placement",
            json!({"revision_id":revision["revision_id"],"work_regions":regions}),
            true,
        )
        .await?;
        anyhow::ensure!(
            first["job"]["regions"][0]["region"]["min"]["y"] == 181,
            "cross-region support did not precede its dependent lever"
        );
        let job_id = first["job_id"].clone();
        call(
            client,
            file,
            "invoke_operation",
            json!({"operation_id":first["operation_id"],"confirm":true}),
            false,
        )
        .await?;
        show_apply(client, file, &first["operation_id"]).await?;
        let small_min = Pos::new(1280, 179, 1200);
        let small_max = Pos::new(1293, 183, 1211);
        let first_state =
            checkpoint_region(actor, file, "job_first_region", small_min, small_max).await?;
        anyhow::ensure!(non_air(&first_state) == 40, "first region state mismatch");
        anyhow::ensure!(
            first_state["blocks"]
                .as_array()
                .unwrap()
                .iter()
                .find(|b| b["pos"] == json!(Pos::new(1282, 181, 1202)))
                .unwrap()["properties"]["lit"]
                == "false",
            "model-derived lamp intermediate was not OFF on the server"
        );
        Ok(json!({"job_id":job_id,"revision_id":revision["revision_id"],"regions":regions}))
    }
    async fn finish_region_job_trial(
        actor: &BotBridge,
        session: &Client,
        file: &mut File,
        ids: &Value,
    ) -> anyhow::Result<()> {
        let job_id = &ids["job_id"];
        let history = call(
            session,
            file,
            "manage_construction_job",
            json!({"job_id":job_id,"action":"get"}),
            true,
        )
        .await?;
        anyhow::ensure!(
            history["job"]["completed_regions"] == 1
                && history["executable_plan_restored"] == false,
            "restart history mismatch"
        );
        let next = call(
            session,
            file,
            "manage_construction_job",
            json!({"job_id":job_id,"action":"plan_next"}),
            true,
        )
        .await?;
        call(
            session,
            file,
            "show_operation",
            json!({"operation_id":next["operation_id"]}),
            true,
        )
        .await?;
        let guard = Pos::new(1281, 181, 1201);
        write(actor, file, guard, "minecraft:stone").await?;
        call(
            session,
            file,
            "invoke_operation",
            json!({"operation_id":next["operation_id"],"confirm":true}),
            false,
        )
        .await?;
        let small_min = Pos::new(1280, 179, 1200);
        let small_max = Pos::new(1293, 183, 1211);
        let drift = checkpoint_region(
            actor,
            file,
            "job_protected_drift_refusal",
            small_min,
            small_max,
        )
        .await?;
        anyhow::ensure!(non_air(&drift) == 41, "refused stage wrote blocks");
        write(actor, file, guard, "minecraft:air").await?;
        show_apply(session, file, &next["operation_id"]).await?;
        let full = checkpoint_region(actor, file, "job_both_regions", small_min, small_max).await?;
        anyhow::ensure!(non_air(&full) == 80, "completed job state mismatch");
        anyhow::ensure!(
            full["blocks"]
                .as_array()
                .unwrap()
                .iter()
                .find(|b| b["pos"] == json!(Pos::new(1282, 181, 1202)))
                .unwrap()["properties"]["lit"]
                == "true",
            "later region did not naturally light the prior region's lamp"
        );
        Ok(())
    }
    async fn native_no_write_boundary_trial(
        actor: &BotBridge,
        client: &Client,
        file: &mut File,
    ) -> anyhow::Result<()> {
        let min = Pos::new(1280, 179, 1200);
        let max = Pos::new(1293, 183, 1211);
        let lamp = Pos::new(1282, 181, 1202);
        let source = Pos::new(1283, 181, 1202);
        let capture = call(
            client,
            file,
            "get_world",
            json!({"region":{"min":min,"max":max}}),
            true,
        )
        .await?;
        let revision=call(client,file,"test_circuit_change",json!({"circuit_id":capture["circuit_id"],"changes":[{"position":lamp,"block":"minecraft:redstone_lamp","properties":{"lit":"false"}}]}),true).await?;
        let setup = call(
            client,
            file,
            "new_placement",
            json!({"revision_id":revision["revision_id"]}),
            true,
        )
        .await?;
        show_apply(client, file, &setup["operation_id"]).await?;
        let initial = checkpoint_region(actor, file, "noop_initial_lamp", min, max).await?;
        anyhow::ensure!(non_air(&initial) == 1, "no-write fixture setup mismatch");
        let capture = call(
            client,
            file,
            "get_world",
            json!({"region":{"min":min,"max":max}}),
            true,
        )
        .await?;
        let revision = call(
            client,
            file,
            "test_circuit_change",
            json!({"circuit_id":capture["circuit_id"],"changes":[
            {"position":source,"block":"minecraft:redstone_block"},
            {"position":lamp,"block":"minecraft:redstone_lamp","properties":{"lit":"true"}}]}),
            true,
        )
        .await?;
        let job=call(client,file,"new_placement",json!({"revision_id":revision["revision_id"],"work_regions":[{"min":source,"max":source},{"min":lamp,"max":lamp}]}),true).await?;
        let id = job["job_id"].clone();
        show_apply(client, file, &job["operation_id"]).await?;
        let powered = checkpoint_region(actor, file, "noop_naturally_powered", min, max).await?;
        anyhow::ensure!(non_air(&powered) == 2, "no-write fixture power mismatch");
        let next = call(
            client,
            file,
            "manage_construction_job",
            json!({"job_id":id,"action":"plan_next"}),
            true,
        )
        .await?;
        anyhow::ensure!(
            next["no_write_checkpoint"] == true && next["steps"] == json!([]),
            "satisfied region scheduled writes"
        );
        show_apply(client, file, &next["operation_id"]).await?;
        let unchanged =
            checkpoint_region(actor, file, "noop_confirmed_without_writes", min, max).await?;
        anyhow::ensure!(
            unchanged == powered,
            "no-write checkpoint changed the world"
        );
        for index in [1, 0] {
            let undo = call(
                client,
                file,
                "manage_construction_job",
                json!({"job_id":id,"action":"plan_undo"}),
                true,
            )
            .await?;
            if index == 1 {
                anyhow::ensure!(
                    undo["no_write_checkpoint"] == true,
                    "no-op inverse scheduled writes"
                );
            }
            show_apply(client, file, &undo["operation_id"]).await?;
        }
        let restored = checkpoint_region(actor, file, "noop_restored_lamp", min, max).await?;
        anyhow::ensure!(
            restored == initial,
            "no-write trial did not restore baseline"
        );
        call(
            client,
            file,
            "undo_operation",
            json!({"operation_id":setup["operation_id"],"confirm":true}),
            true,
        )
        .await?;
        Ok(())
    }
    async fn undo_region_job_trial(
        actor: &BotBridge,
        session: &Client,
        file: &mut File,
        ids: &Value,
    ) -> anyhow::Result<()> {
        let job_id = &ids["job_id"];
        for (index, count) in [(1, 40), (0, 0)] {
            let undo = call(
                session,
                file,
                "manage_construction_job",
                json!({"job_id":job_id,"action":"plan_undo"}),
                true,
            )
            .await?;
            anyhow::ensure!(
                undo["job_stage"]["region_index"] == index && undo["job_stage"]["undo"] == true,
                "undo order mismatch"
            );
            show_apply(session, file, &undo["operation_id"]).await?;
            let snapshot = checkpoint_region(
                actor,
                file,
                &format!("job_undo_region_{index}"),
                Pos::new(1280, 179, 1200),
                Pos::new(1293, 183, 1211),
            )
            .await?;
            anyhow::ensure!(non_air(&snapshot) == count, "job undo state mismatch");
        }
        let next = call(
            session,
            file,
            "manage_construction_job",
            json!({"job_id":job_id,"action":"plan_next"}),
            true,
        )
        .await?;
        call(
            session,
            file,
            "show_operation",
            json!({"operation_id":next["operation_id"]}),
            true,
        )
        .await?;
        call(
            session,
            file,
            "manage_construction_job",
            json!({"job_id":job_id,"action":"cancel"}),
            true,
        )
        .await?;
        call(
            session,
            file,
            "invoke_operation",
            json!({"operation_id":next["operation_id"],"confirm":true}),
            false,
        )
        .await?;
        native_no_write_boundary_trial(actor, session, file).await?;
        let final_state = checkpoint_region(
            actor,
            file,
            "job_final_full_context",
            Pos::new(1279, 178, 1199),
            Pos::new(1312, 211, 1232),
        )
        .await?;
        anyhow::ensure!(
            non_air(&final_state) == 0,
            "job fixture not restored to air"
        );
        Ok(())
    }
    pub async fn run() -> anyhow::Result<()> {
        anyhow::ensure!(
            std::env::var("DUSTROUTE_LIVE_BLUEPRINT_PROBE").as_deref() == Ok("1"),
            "explicit opt-in required"
        );
        let state = std::path::PathBuf::from(std::env::var("DUSTROUTE_STATE_DIR")?);
        let recovery = std::env::var("PROBE_RECOVERY_INSTANCES").ok();
        anyhow::ensure!(
            recovery.is_some() || !state.exists(),
            "fresh state directory required"
        );
        let mut file = File::options()
            .write(true)
            .create_new(true)
            .open(std::env::var("TRACE_OUTPUT")?)?;
        let port: u16 = std::env::var("MC_PORT")?.parse()?;
        let actor = BotBridge::connect_voxrig(ConnectionConfig::offline(
            Server::new("127.0.0.1", port),
            "dustroutetest",
            MinecraftVersion::Java1_21_11,
        ))
        .await?;
        let session = start(port, &mut file).await?;
        if region_jobs() {
            let result = region_job_trial(&actor, &session.0, &mut file).await;
            stop(session, &mut file).await?;
            let ids = result?;
            let session = start(port, &mut file).await?;
            let result = finish_region_job_trial(&actor, &session.0, &mut file, &ids).await;
            stop(session, &mut file).await?;
            result?;
            let session = start(port, &mut file).await?;
            let result = undo_region_job_trial(&actor, &session.0, &mut file, &ids).await;
            stop(session, &mut file).await?;
            result?;
            record(
                &mut file,
                json!({"stage":"complete","passed":true,"restored_to_air":true,"probe":"region_jobs","ids":ids}),
            )?;
            return Ok(());
        }
        if let Some(recovery) = recovery {
            let ids: Vec<Value> = serde_json::from_str(&recovery)?;
            checkpoint(&actor, &mut file, "before_recovery_removal").await?;
            for id in ids {
                let plan = call(
                    &session.0,
                    &mut file,
                    "manage_assembly",
                    json!({"action":"plan_removal","instance_id":id}),
                    true,
                )
                .await?;
                show_apply(&session.0, &mut file, &plan["operation_id"]).await?;
            }
            let snapshot = checkpoint(&actor, &mut file, "registered_fixtures_removed").await?;
            if let Ok(external) = std::env::var("PROBE_RECOVERY_EXTERNAL_WRITES") {
                let external: Vec<Value> = serde_json::from_str(&external)?;
                for write_record in external {
                    let pos: Pos = serde_json::from_value(write_record["position"].clone())?;
                    if block_name(&snapshot, pos) == "minecraft:air" {
                        continue;
                    }
                    let block = snapshot["blocks"]
                        .as_array()
                        .unwrap()
                        .iter()
                        .find(|b| b["pos"] == json!(pos))
                        .unwrap();
                    let properties = block["properties"]
                        .as_object()
                        .unwrap()
                        .iter()
                        .map(|(k, v)| format!("{k}={}", v.as_str().unwrap()))
                        .collect::<Vec<_>>()
                        .join(",");
                    let actual = if properties.is_empty() {
                        block["name"].as_str().unwrap().to_owned()
                    } else {
                        format!("{}[{properties}]", block["name"].as_str().unwrap())
                    };
                    anyhow::ensure!(
                        actual == write_record["state"].as_str().unwrap(),
                        "owned external fixture changed before cleanup"
                    );
                    write(&actor, &mut file, pos, "minecraft:air").await?;
                }
            }
            anyhow::ensure!(
                non_air(&checkpoint(&actor, &mut file, "recovered_empty").await?) == 0,
                "recovery left blocks"
            );
            stop(session, &mut file).await?;
            record(
                &mut file,
                json!({"stage":"completed","passed":true,"restored_to_air":true,"recovery_only":true}),
            )?;
            return Ok(());
        }
        let result = workflow(&actor, &session.0, &mut file).await;
        record(
            &mut file,
            json!({"stage":"workflow_result","error":result.as_ref().err().map(|e|format!("{e:#}"))}),
        )?;
        stop(session, &mut file).await?;
        result?;
        // A new OS process reloads catalog, instances and history; executable plans remain absent.
        let session = start(port, &mut file).await?;
        let ids: Value =
            serde_json::from_slice(&std::fs::read(std::env::var("PROBE_IDS_OUTPUT")?)?)?;
        let history = call(
            &session.0,
            &mut file,
            "get_operation",
            json!({"operation_id":ids["scoped_operation"]}),
            true,
        )
        .await?;
        anyhow::ensure!(
            history["record"]["state"] == "undone"
                && history["record"]["edit_scope"] == ids["scope"]
                && history["executable_plan_restored"] == false,
            "history reload mismatch"
        );
        call(
            &session.0,
            &mut file,
            "undo_operation",
            json!({"operation_id":ids["scoped_operation"],"confirm":true}),
            false,
        )
        .await?;
        for id in ids["instances"].as_array().unwrap() {
            let saved = call(
                &session.0,
                &mut file,
                "manage_assembly",
                json!({"action":"get","instance_id":id}),
                true,
            )
            .await?;
            anyhow::ensure!(
                saved["instance"]["state"] == "removed",
                "removed instance reload mismatch"
            );
        }
        let final_snapshot = checkpoint(&actor, &mut file, "after_restart_empty").await?;
        anyhow::ensure!(non_air(&final_snapshot) == 0, "restart left blocks");
        stop(session, &mut file).await?;
        record(
            &mut file,
            json!({"stage":"completed","passed":true,"restored_to_air":true}),
        )?;
        println!("COMPLETED");
        Ok(())
    }
    async fn workflow(actor: &BotBridge, client: &Client, file: &mut File) -> anyhow::Result<()> {
        let baseline = checkpoint(actor, file, "initial_empty").await?;
        anyhow::ensure!(
            non_air(&baseline) == 0,
            "owned region must initially be empty"
        );
        let previous = design("live.iteration.before", "glass");
        let mut invalid = previous.clone();
        invalid["parts"][1]["shapes"][0]["positions"][0] = json!({"x":-1,"y":0,"z":0});
        let rejected = call(
            client,
            file,
            "test_circuit_change",
            json!({"blueprint":{"action":"generate_building_design","request":invalid}}),
            false,
        )
        .await?;
        anyhow::ensure!(
            rejected["errors"].is_array()
                && rejected["writes_minecraft"] == false
                && rejected["catalog_changed"] == false,
            "structured authoring error missing"
        );
        let f = super::fixture::fixture(false, false);
        let source = json!({"records":{"types":f.catalog.type_revisions().collect::<Vec<_>>(),"revisions":f.catalog.revisions().collect::<Vec<_>>(),"assemblies":[f.base]},"request":f.request});
        adopt(client, file, &source).await?;
        let bad=call(client,file,"test_circuit_change",json!({"blueprint":{"action":"generate_building_design","request":{
            "namespace":"live.iteration.bad-motion","name":"Insufficient motion allocation","known_region":f.context.known_region,
            "parts":[{"name":"marker","shapes":[{"kind":"blocks","material":"stone","positions":[{"x":2,"y":1,"z":0}]}]}],
            "component":{"name":"engine","assembly_revision_id":source["request"]["candidate_state"]["id"],"source_anchor":{"x":0,"y":0,"z":0},"target_anchor":{"x":0,"y":0,"z":0},"rotation":"r0","reserved_space":{"min":{"x":-1,"y":0,"z":0},"max":{"x":1,"y":1,"z":0}}}
        }}}),false).await?;
        anyhow::ensure!(
            bad["errors"][0]["code"] == "verification_not_established"
                && bad["errors"][0]["diagnostics"]["status"] == "failed",
            "failure evidence missing"
        );
        anyhow::ensure!(
            bad["errors"][0]["diagnostics"]["findings"]
                .as_array()
                .unwrap()
                .iter()
                .any(|finding| finding["evidence"]["observation"]["stage"]
                    == "committed_runtime_state"
                    && finding["evidence"]["position"] == json!({"x":2,"y":1,"z":0})),
            "runtime position evidence missing"
        );
        anyhow::ensure!(
            non_air(&checkpoint(actor, file, "errors_no_world_writes").await?) == 0,
            "error wrote world"
        );
        let generated = call(
            client,
            file,
            "test_circuit_change",
            json!({"blueprint":{"action":"generate_building_design","request":previous}}),
            true,
        )
        .await?;
        let base = adopt(client, file, &generated["result"]).await?;
        let instance_a = place(client, file, &base, A).await?;
        let original = checkpoint(actor, file, "base_placed").await?;
        anyhow::ensure!(
            non_air(&original) == 4
                && block_name(&original, Pos::new(A.x + 1, A.y + 1, A.z + 1)) == "minecraft:glass",
            "base layout mismatch"
        );
        let design = design("live.iteration.after", "tinted_glass");
        let request = json!({"base_assembly_revision_id":base,"previous":previous,"design":design});
        let mut wrong = request.clone();
        wrong["previous"]["spaces"] = json!([]);
        let rejected = call(
            client,
            file,
            "test_circuit_change",
            json!({"blueprint":{"action":"generate_building_design_update","request":wrong}}),
            false,
        )
        .await?;
        anyhow::ensure!(
            rejected["errors"][0]["code"] == "base_design_mismatch",
            "base mismatch not detected"
        );
        let update = call(
            client,
            file,
            "test_circuit_change",
            json!({"blueprint":{"action":"generate_building_design_update","request":request}}),
            true,
        )
        .await?;
        anyhow::ensure!(
            update["result"]["diff"]["blocks"].as_array().unwrap().len() == 1
                && update["result"]["retained_revisions"]["floor"]
                    == "live.iteration.before.floor.v1"
                && update["result"]["placed_instances_modified"] == false,
            "immutable update mismatch"
        );
        let updated = adopt(client, file, &update["result"]).await?;
        let instance_b = place(client, file, &updated, B).await?;
        let both = checkpoint(actor, file, "updated_placed_base_unchanged").await?;
        anyhow::ensure!(
            non_air(&both) == 8
                && block_name(&both, Pos::new(A.x + 1, A.y + 1, A.z + 1)) == "minecraft:glass"
                && block_name(&both, Pos::new(B.x + 1, B.y + 1, B.z + 1))
                    == "minecraft:tinted_glass",
            "update overwrote source instance"
        );
        let stored = call(
            client,
            file,
            "manage_assembly",
            json!({"action":"get","instance_id":instance_a}),
            true,
        )
        .await?;
        anyhow::ensure!(
            stored["instance"]["assembly_revision_id"] == base,
            "old instance changed its source revision"
        );
        record(
            file,
            json!({"stage":"base_instance_remains_pinned","instance":stored} ),
        )?;

        let edit = Pos::new(A.x, A.y, A.z);
        let guard = Pos::new(A.x + 1, A.y + 1, A.z + 1);
        let scope =
            json!({"editable":[{"min":edit,"max":edit}],"protected":[{"min":guard,"max":guard}]});
        let revision = capture_change(
            client,
            file,
            Pos::new(A.x - 1, A.y - 1, A.z - 1),
            Pos::new(A.x + 4, A.y + 3, A.z + 3),
            edit,
            "minecraft:smooth_quartz",
            edit,
        )
        .await?;
        let denied = json!({"editable":[{"min":guard,"max":guard}],"protected":[]});
        let refusal = call(
            client,
            file,
            "new_placement",
            json!({"revision_id":revision["revision_id"],"edit_scope":denied}),
            false,
        )
        .await?;
        anyhow::ensure!(
            refusal["error"]
                .as_str()
                .unwrap()
                .contains("declared write"),
            "undeclared write accepted"
        );
        let plan = call(
            client,
            file,
            "new_placement",
            json!({"revision_id":revision["revision_id"],"edit_scope":scope}),
            true,
        )
        .await?;
        let op = plan["operation_id"].clone();
        call(
            client,
            file,
            "show_operation",
            json!({"operation_id":op}),
            true,
        )
        .await?;
        write(actor, file, guard, "minecraft:stone").await?;
        call(
            client,
            file,
            "invoke_operation",
            json!({"operation_id":op,"confirm":true}),
            false,
        )
        .await?;
        let drift = checkpoint(actor, file, "protected_drift_blocks_apply").await?;
        anyhow::ensure!(
            block_name(&drift, edit) == "minecraft:stone",
            "refused apply changed editable cell"
        );
        write(actor, file, guard, "minecraft:glass").await?;
        show_apply(client, file, &op).await?;
        let changed = checkpoint(actor, file, "scoped_edit_applied").await?;
        anyhow::ensure!(
            block_name(&changed, edit) == "minecraft:smooth_quartz"
                && block_name(&changed, guard) == "minecraft:glass",
            "scoped edit mismatch"
        );
        write(actor, file, guard, "minecraft:stone").await?;
        call(
            client,
            file,
            "undo_operation",
            json!({"operation_id":op,"confirm":true}),
            false,
        )
        .await?;
        let drift = checkpoint(actor, file, "protected_drift_blocks_undo").await?;
        anyhow::ensure!(
            block_name(&drift, edit) == "minecraft:smooth_quartz",
            "refused undo changed editable cell"
        );
        write(actor, file, guard, "minecraft:glass").await?;
        call(
            client,
            file,
            "undo_operation",
            json!({"operation_id":op,"confirm":true}),
            true,
        )
        .await?;
        let restored = checkpoint(actor, file, "scoped_edit_undone").await?;
        anyhow::ensure!(
            block_name(&restored, edit) == "minecraft:stone"
                && block_name(&restored, guard) == "minecraft:glass",
            "undo did not restore source"
        );

        write(
            actor,
            file,
            OBSERVER,
            "minecraft:observer[facing=east,powered=false]",
        )
        .await?;
        let watched = Pos::new(OBSERVER.x + 1, OBSERVER.y, OBSERVER.z);
        let obs_revision = capture_change(
            client,
            file,
            Pos::new(1301, 180, 1201),
            Pos::new(1306, 183, 1204),
            watched,
            "minecraft:stone",
            OBSERVER,
        )
        .await?;
        let denied = json!({"editable":[{"min":watched,"max":watched}],"protected":[{"min":OBSERVER,"max":OBSERVER}]});
        let rejected = call(
            client,
            file,
            "new_placement",
            json!({"revision_id":obs_revision["revision_id"],"edit_scope":denied}),
            false,
        )
        .await?;
        anyhow::ensure!(
            rejected["error"]
                .as_str()
                .unwrap()
                .contains("protected state changed"),
            "transient observer pulse not guarded"
        );
        let before = checkpoint(actor, file, "protected_observer_pulse_refused").await?;
        anyhow::ensure!(
            block_name(&before, watched) == "minecraft:air",
            "refused observer edit wrote world"
        );
        let scope2 = json!({"editable":[{"min":OBSERVER,"max":watched}],"protected":[]});
        let allowed = call(
            client,
            file,
            "new_placement",
            json!({"revision_id":obs_revision["revision_id"],"edit_scope":scope2}),
            true,
        )
        .await?;
        show_apply(client, file, &allowed["operation_id"]).await?;
        let after = checkpoint(actor, file, "observer_edit_allowed_settled").await?;
        anyhow::ensure!(
            block_name(&after, watched) == "minecraft:stone",
            "allowed observer edit missing"
        );
        call(
            client,
            file,
            "undo_operation",
            json!({"operation_id":allowed["operation_id"],"confirm":true}),
            true,
        )
        .await?;
        checkpoint(actor, file, "observer_edit_undone").await?;
        write(actor, file, OBSERVER, "minecraft:air").await?;
        for id in [&instance_b, &instance_a] {
            let removal = call(
                client,
                file,
                "manage_assembly",
                json!({"action":"plan_removal","instance_id":id}),
                true,
            )
            .await?;
            show_apply(client, file, &removal["operation_id"]).await?;
        }
        anyhow::ensure!(
            non_air(&checkpoint(actor, file, "all_fixtures_removed").await?) == 0,
            "removal left blocks"
        );
        let ids = json!({"scoped_operation":op,"scope":scope,"instances":[instance_a,instance_b],"base":base,"updated":updated});
        serde_json::to_writer(
            File::options()
                .write(true)
                .create_new(true)
                .open(std::env::var("PROBE_IDS_OUTPUT")?)?,
            &ids,
        )?;
        Ok(())
    }
}
