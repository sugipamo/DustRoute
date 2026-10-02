//! Declared temporary-access acceptance through public native APIs only.
use super::*;
use serde_json::{Value, json};
use std::{io::Write, time::Duration};
use voxrig::checked_survival::InventorySlot;
use voxrig::{
    BlockFace, Client, ConnectionConfig, MinecraftVersion, NativeBlockState, Region, Server,
};

fn rotation(eye: [f64; 3], point: [f64; 3]) -> [f32; 2] {
    let d: [f64; 3] = std::array::from_fn(|i| point[i] - eye[i]);
    [
        (-d[0]).atan2(d[2]).to_degrees() as f32,
        (-d[1]).atan2(d[0].hypot(d[2])).to_degrees() as f32,
    ]
}
fn hypothetical_eye(s: &SurvivalScenario) -> [f64; 3] {
    let p = s.position();
    [p[0], p[1] + f64::from(1.62f32), p[2]]
}
fn controls(yaw: f32, jump: bool) -> Vec<SurvivalControl> {
    (0..28)
        .map(|t| SurvivalControl {
            yaw,
            input: SurvivalInput {
                forward: i8::from(t < 8),
                strafe: 0,
                jump: jump && t == 0,
            },
        })
        .collect()
}
fn block(name: &str) -> NativeBlockState {
    NativeBlockState {
        name: format!("minecraft:{name}"),
        properties: Default::default(),
    }
}
#[tokio::test]
#[ignore = "dedicated non-OP 1.21.11 fixture with console preparation and explicit environment"]
async fn native_temporary_access_place_climb_retreat_cleanup() {
    assert_eq!(
        std::env::var("DUSTROUTE_SURVIVAL_ACCESS_PORT").unwrap(),
        "25572"
    );
    let file = std::fs::OpenOptions::new()
        .create_new(true)
        .write(true)
        .open(std::env::var("DUSTROUTE_SURVIVAL_ACCESS_OUTPUT").unwrap())
        .unwrap();
    let mut bot = super::native_trial::connect("NatMineBot").await;
    let viewer = super::native_trial::connect("NatMineView").await;
    println!(
        "FIXTURE access: stone floor y=-61 x=-5..12 z=-5..8; air above; bot 0.5 -60 0.5; viewer 0.5 -60 5.5; clear bot, inventory.0 dirt 3; enter"
    );
    std::io::stdout().flush().unwrap();
    tokio::time::timeout(
        Duration::from_secs(60),
        tokio::task::spawn_blocking(|| {
            let mut s = String::new();
            std::io::stdin().read_line(&mut s)
        }),
    )
    .await
    .unwrap()
    .unwrap()
    .unwrap();
    let ops = bot.survival().unwrap();
    tokio::time::timeout(Duration::from_secs(5), async {
        loop {
            let p = ops.player_state().await.unwrap();
            if p.inventory.slots[9]
                == (InventorySlot::Item {
                    item: voxrig::versions::java_1_21_11::operations::default_item("dirt", 3)
                        .unwrap(),
                })
                && ops.standing_context().await.is_ok()
            {
                break;
            }
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
    })
    .await
    .unwrap();
    bot.start_packet_trace(8_388_608).await.unwrap();
    viewer.start_packet_trace(8_388_608).await.unwrap();
    let before = ops.player_state().await.unwrap();
    let mut events = vec![];
    let mut traces = vec![];
    let result = exercise(&mut bot, &viewer, &mut events, &mut traces).await;
    let last = bot.survival().unwrap();
    let after = last.player_state().await.ok();
    let history = last.operation_history().await;
    if let Ok(trace) = bot.stop_packet_trace().await {
        traces.push(json!(trace));
    }
    let observer_trace = viewer.stop_packet_trace().await.unwrap();
    let disconnect = bot.disconnect().await.map_err(|e| e.to_string());
    viewer.disconnect().await.unwrap();
    serde_json::to_writer(file,&json!({"scope":"isolated non-OP common durable executor; checked temporary-only plan; not adopted roofed construction", "before":before,"events":events,"after":after,"history":history,"traces":traces,"observer_trace":observer_trace,"error":result.as_ref().err(),"disconnect_error":disconnect.err()})).unwrap();
    result.unwrap();
}

async fn exercise(
    bot: &mut Client,
    viewer: &Client,
    events: &mut Vec<Value>,
    traces: &mut Vec<Value>,
) -> Result<(), String> {
    use crate::survival_construction::{
        ConstructionAction, ConstructionScope, ConstructionSite, PlacementPurpose,
        preview_construction_sequence,
    };
    use crate::survival_execution::{ExecutionProgress, SurvivalExecutor};
    use dustroute_library::world_edit::WorldEditScope;
    use dustroute_translate::{
        snapshot::{MinecraftSnapshot, MinecraftSnapshotBlock},
        world::{Pos, Region as SiteRegion},
    };
    let ops = bot.survival().map_err(|e| e.to_string())?;
    let capture = ops
        .capture_survival_scene(Region {
            min: [-5, -62, -5],
            max: [11, -53, 9],
        })
        .await
        .map_err(|e| e.to_string())?;
    let mut scenario = capture.scenario();
    let mut actions = Vec::new();
    for x in [3, 2, 1] {
        let r = rotation(
            hypothetical_eye(&scenario),
            [f64::from(x) + 0.5, -60.0, 0.5],
        );
        let p = scenario
            .preview_cube_placement([x, -61, 0], BlockFace::Up, r, "dirt")
            .map_err(|e| e.to_string())?;
        actions.push(ConstructionAction::Place {
            purpose: PlacementPurpose::Temporary,
            support: p.support,
            face: BlockFace::Up,
            rotation: r,
            material: "dirt".into(),
        });
        scenario = scenario.after_edits(&[p.edit]).map_err(|e| e.to_string())?;
    }
    for (yaw, jump) in [(-90.0, true), (90.0, false)] {
        let input = controls(yaw, jump);
        scenario = scenario.after_path(&input).map_err(|e| e.to_string())?;
        actions.push(ConstructionAction::Move { controls: input });
    }
    for x in 1..=3 {
        let r = rotation(hypothetical_eye(&scenario), [f64::from(x), -59.5, 0.5]);
        let edit = scenario
            .preview_cube_removal([x, -60, 0], BlockFace::West, r)
            .map_err(|e| e.to_string())?;
        scenario = scenario.after_edits(&[edit]).map_err(|e| e.to_string())?;
        actions.push(ConstructionAction::RemoveTemporary {
            target: [x, -60, 0],
            face: BlockFace::West,
            rotation: r,
        });
    }
    let r = capture.region();
    let region = |a: [i32; 3], b: [i32; 3]| {
        SiteRegion::new(Pos::new(a[0], a[1], a[2]), Pos::new(b[0], b[1], b[2]))
    };
    let mut baseline = MinecraftSnapshot {
        min: Pos::new(r.min[0], r.min[1], r.min[2]),
        max: Pos::new(r.max[0], r.max[1], r.max[2]),
        blocks: Vec::new(),
    };
    let initial_scenario = capture.scenario();
    for x in r.min[0]..=r.max[0] {
        for y in r.min[1]..=r.max[1] {
            for z in r.min[2]..=r.max[2] {
                let b = initial_scenario
                    .block([x, y, z])
                    .map_err(|e| e.to_string())?;
                if b != block("air") {
                    baseline.blocks.push(MinecraftSnapshotBlock {
                        pos: Pos::new(x, y, z),
                        name: b.name,
                        properties: b.properties,
                    });
                }
            }
        }
    }
    let temporary = region([1, -60, 0], [3, -60, 0]);
    let scope = ConstructionScope {
        observed: region(r.min, r.max),
        edits: WorldEditScope {
            editable: vec![temporary],
            protected: vec![],
        },
        temporary: vec![temporary],
        travel: TravelBounds {
            min: [-4.0, -60.0, -4.0],
            max: [10.0, -54.0, 8.0],
        },
        retreat: TravelBounds {
            min: [-1.0, -60.0, -1.0],
            max: [1.0, -60.0, 1.0],
        },
    };
    let site = ConstructionSite::temporary_work(&baseline, scope).map_err(|e| e.to_string())?;
    let plan = preview_construction_sequence(
        &capture,
        &site,
        &actions,
        &std::collections::BTreeMap::from([("minecraft:dirt".into(), 3)]),
    )
    .map_err(|e| e.to_string())?;
    events.push(json!({"phase":"common_checked_plan","plan":plan}));
    let directory = std::path::PathBuf::from(format!(
        "{}.journal",
        std::env::var("DUSTROUTE_SURVIVAL_ACCESS_OUTPUT").unwrap()
    ));
    let config = ConnectionConfig::offline(
        Server::new("127.0.0.1", 25572),
        "NatMineBot",
        MinecraftVersion::Java1_21_11,
    );
    let mut executor =
        SurvivalExecutor::create(bot.clone(), viewer.clone(), config, plan, &directory)
            .await
            .map_err(|e| e.to_string())?;
    let mut reconnects = 0;
    loop {
        let result = executor.advance().await;
        if executor.record().reconnects != reconnects {
            if let Ok(trace) = bot.stop_packet_trace().await {
                traces.push(json!(trace));
            }
            *bot = executor.client().clone();
            bot.start_packet_trace(8_388_608)
                .await
                .map_err(|e| e.to_string())?;
            reconnects = executor.record().reconnects;
        }
        let progress = match result {
            Ok(p) => p,
            Err(e) => {
                events.push(json!({"phase":"common_executor_stopped","record":executor.record()}));
                return Err(e.to_string());
            }
        };
        events.push(json!({"phase":"common_executor_progress","progress":progress}));
        if let ExecutionProgress::MiningStarted {
            target: [1, -60, 0],
            selected_hotbar,
            attempt: 1,
        } = progress
        {
            let boundary = std::env::var("DUSTROUTE_SURVIVAL_ACCESS_BOUNDARY").unwrap_or_default();
            if boundary == "cancel" || boundary == "disconnect" || boundary == "target" {
                if boundary == "cancel" {
                    // Drop the actual common executor future while it is waiting
                    // on the START result, then prove it cannot dispatch again.
                    assert!(
                        tokio::time::timeout(Duration::from_millis(1), executor.advance())
                            .await
                            .is_err()
                    );
                } else if boundary == "disconnect" {
                    executor
                        .client()
                        .disconnect()
                        .await
                        .map_err(|e| e.to_string())?;
                    assert!(executor.advance().await.is_err());
                } else {
                    println!(
                        "FIXTURE target: START retained; setblock 1 -60 0 minecraft:stone; enter"
                    );
                    std::io::stdout().flush().unwrap();
                    tokio::time::timeout(
                        Duration::from_secs(60),
                        tokio::task::spawn_blocking(|| {
                            let mut s = String::new();
                            std::io::stdin().read_line(&mut s)
                        }),
                    )
                    .await
                    .map_err(|e| e.to_string())?
                    .map_err(|e| e.to_string())?
                    .map_err(|e| e.to_string())?;
                    assert!(executor.advance().await.is_err());
                }
                let refusal = executor.advance().await.unwrap_err();
                assert_eq!(refusal.code, "execution_needs_inspection");
                let history = executor
                    .client()
                    .survival()
                    .unwrap()
                    .operation_history()
                    .await;
                assert!(history.mining.as_ref().unwrap().finish.is_none());
                executor.cancel().map_err(|e| e.to_string())?;
                let record = executor.record().clone();
                drop(executor);
                let diagnosis =
                    crate::survival_execution::diagnose(&directory).map_err(|e| e.to_string())?;
                assert_eq!(
                    diagnosis.continuation,
                    crate::survival_execution::Continuation::NeedsInspection
                );
                let remaining = viewer
                    .observe_region(Region {
                        min: [1, -60, 0],
                        max: [3, -60, 0],
                    })
                    .await
                    .map_err(|e| e.to_string())?;
                assert_eq!(remaining.blocks.len(), 3);
                assert!(
                    remaining
                        .blocks
                        .iter()
                        .all(|b| b.state.as_ref().is_some_and(|s| s.name != "minecraft:air"))
                );
                events.push(json!({"phase":"expected_boundary_stop","boundary":boundary,"record":record,"diagnosis":diagnosis,"refusal":refusal,"history":history,"remaining":remaining}));
                return Ok(());
            }
            if std::env::var("DUSTROUTE_SURVIVAL_ACCESS_INJECT_HAND").as_deref() == Ok("1") {
                println!(
                    "FIXTURE interrupt: START retained; replace NatMineBot hotbar.{selected_hotbar} with dirt 1; enter"
                );
                std::io::stdout().flush().unwrap();
                tokio::time::timeout(
                    Duration::from_secs(60),
                    tokio::task::spawn_blocking(|| {
                        let mut s = String::new();
                        std::io::stdin().read_line(&mut s)
                    }),
                )
                .await
                .map_err(|e| e.to_string())?
                .map_err(|e| e.to_string())?
                .map_err(|e| e.to_string())?;
            }
        }
        if matches!(progress, ExecutionProgress::Completed) {
            break;
        }
    }
    events.push(json!({"phase":"common_executor_complete","record":executor.record()}));
    Ok(())
}
