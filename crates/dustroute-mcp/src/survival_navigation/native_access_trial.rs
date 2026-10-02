//! Declared temporary-access acceptance through public native APIs only.
use super::*;
use serde_json::{Value, json};
use std::{io::Write, time::Duration};
use voxrig::checked_survival::{
    HypotheticalBlockEdit, HypotheticalPlacement, InventorySlot, MiningRetirementStatus,
    MiningStatus, PlacementStatus,
};
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
async fn observe(viewer: &Client, target: [i32; 3], expected: &str) -> Result<Value, String> {
    tokio::time::timeout(Duration::from_secs(3), async {
        loop {
            let r = viewer
                .observe_region(Region {
                    min: target,
                    max: target,
                })
                .await
                .map_err(|e| e.to_string())?;
            if r.blocks[0]
                .state
                .as_ref()
                .is_some_and(|b| b.name == expected)
            {
                return Ok(json!(r));
            }
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
    })
    .await
    .map_err(|_| format!("independent observation timeout at {target:?}"))?
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
    serde_json::to_writer(file,&json!({"scope":"isolated non-OP temporary dirt access; preplanned placement/climb/retreat/removal geometry; explicit mining retirement and fresh recovery; not adopted roofed construction", "before":before,"events":events,"after":after,"history":history,"traces":traces,"observer_trace":observer_trace,"error":result.as_ref().err(),"disconnect_error":disconnect.err()})).unwrap();
    result.unwrap();
}

async fn exercise(
    bot: &mut Client,
    viewer: &Client,
    events: &mut Vec<Value>,
    traces: &mut Vec<Value>,
) -> Result<(), String> {
    let ops = bot.survival().map_err(|e| e.to_string())?;
    let observer = viewer.survival().map_err(|e| e.to_string())?;
    let capture = ops
        .capture_survival_scene(Region {
            min: [-5, -62, -5],
            max: [11, -53, 9],
        })
        .await
        .map_err(|e| e.to_string())?;
    let mut scenario = capture.scenario();
    let mut placements: Vec<HypotheticalPlacement> = vec![];
    // Far-to-near support clicks preserve visibility while building this platform.
    for x in [3, 2, 1] {
        let r = rotation(
            hypothetical_eye(&scenario),
            [f64::from(x) + 0.5, -60.0, 0.5],
        );
        let p = scenario
            .preview_cube_placement([x, -61, 0], BlockFace::Up, r, "dirt")
            .map_err(|e| e.to_string())?;
        scenario = scenario
            .after_edits(std::slice::from_ref(&p.edit))
            .map_err(|e| e.to_string())?;
        placements.push(p);
    }
    let up = controls(-90.0, true);
    let down = controls(90.0, false);
    let climb = scenario.preview_path(&up).map_err(|e| e.to_string())?;
    let elevated = scenario.after_path(&up).map_err(|e| e.to_string())?;
    if elevated.position()[1] != -59.0 {
        return Err("planned climb did not reach platform".into());
    }
    let retreat = elevated.preview_path(&down).map_err(|e| e.to_string())?;
    let mut returned = elevated.after_path(&down).map_err(|e| e.to_string())?;
    if returned.position()[1] != -60.0 {
        return Err("planned retreat did not reach ground".into());
    }
    let mut removals: Vec<HypotheticalBlockEdit> = vec![];
    for x in 1..=3 {
        let r = rotation(hypothetical_eye(&returned), [f64::from(x), -59.5, 0.5]);
        let edit = returned
            .preview_cube_removal([x, -60, 0], BlockFace::West, r)
            .map_err(|e| e.to_string())?;
        returned = returned
            .after_edits(std::slice::from_ref(&edit))
            .map_err(|e| e.to_string())?;
        removals.push(edit);
    }
    events.push(json!({"phase":"complete_geometric_plan","placements":placements,"climb":climb,"retreat":retreat,"removals":removals,"temporary_items_required_without_recovery":3}));
    ops.validate_survival_scene(&capture)
        .await
        .map_err(|e| e.to_string())?;
    let swap = ops
        .swap_player_hotbar(9, 0)
        .await
        .map_err(|e| e.to_string())?;
    ops.wait_inventory_swap(&swap, Duration::from_secs(3))
        .await
        .map_err(|e| e.to_string())?;
    ops.select_hotbar(0).await.map_err(|e| e.to_string())?;
    for p in &placements {
        ops.look(p.rotation).await.map_err(|e| e.to_string())?;
        let intent = ops
            .place_survival_cube(p.support, BlockFace::Up)
            .await
            .map_err(|e| e.to_string())?;
        if intent.target != p.edit.position
            || intent.before != p.edit.before
            || intent.expected != p.edit.after
            || intent.cursor != p.cursor
        {
            return Err("real placement differs from hypothetical target/cursor".into());
        }
        let status = ops
            .wait_survival_placement(&intent, Duration::from_secs(3))
            .await
            .map_err(|e| e.to_string())?;
        if !matches!(status, PlacementStatus::ObservedPlaced { .. }) {
            return Err(format!("{status:?}"));
        }
        let seen = observe(viewer, intent.target, "minecraft:dirt").await?;
        events
            .push(json!({"phase":"placement","intent":intent,"status":status,"independent":seen}));
    }
    for (name, input, hypothetical) in [("climb", &up, &climb), ("retreat", &down, &retreat)] {
        let preview = ops
            .preview_survival_path(input)
            .await
            .map_err(|e| e.to_string())?;
        if preview.frames != hypothetical.frames {
            return Err(format!(
                "{name} live preview differs from hypothetical frames"
            ));
        }
        ops.start_previewed_survival_motion(&preview, &observer)
            .await
            .map_err(|e| e.to_string())?;
        let record = super::native_trial::finish(&ops).await?;
        events.push(json!({"phase":name,"motion":record}));
    }
    for x in 1..=3 {
        let mut complete = false;
        for attempt in 1..=3 {
            use crate::survival_cleanup::{
                CleanupReconciliation, CleanupRecoveryPlan, choose_empty_hand,
            };
            let current = bot.survival().map_err(|e| e.to_string())?;
            // Each bounded attempt reselects geometry and inventory on a new session.
            let scene = current
                .capture_survival_scene(Region {
                    min: [-5, -62, -5],
                    max: [11, -53, 9],
                })
                .await
                .map_err(|e| e.to_string())?;
            let future = scene.scenario();
            let r = rotation(hypothetical_eye(&future), [f64::from(x), -59.5, 0.5]);
            let edit = future
                .preview_cube_removal([x, -60, 0], BlockFace::West, r)
                .map_err(|e| e.to_string())?;
            let owned = &removals[(x - 1) as usize];
            if edit.position != owned.position
                || edit.before != owned.before
                || edit.after != owned.after
            {
                return Err(
                    "fresh removal differs from the originally owned temporary block".into(),
                );
            }
            let player = current.player_state().await.map_err(|e| e.to_string())?;
            let empty = choose_empty_hand(&player).map_err(|e| format!("{e:?}"))?;
            current
                .select_hotbar(empty)
                .await
                .map_err(|e| e.to_string())?;
            current.look(r).await.map_err(|e| e.to_string())?;
            let intent = current
                .start_survival_mining(edit.position, BlockFace::West)
                .await
                .map_err(|e| e.to_string())?;
            events.push(json!({"phase":"removal_start","target":edit.position,"attempt":attempt,"intent":intent,"selected_from":player}));
            // Optional declared external-input trial, strictly after START. It
            // supplies one item; it does not remove/restore blocks or clear guards.
            if x == 1
                && attempt == 1
                && std::env::var("DUSTROUTE_SURVIVAL_ACCESS_INJECT_HAND").as_deref() == Ok("1")
            {
                println!(
                    "FIXTURE interrupt: START retained; replace NatMineBot hotbar.{empty} with dirt 1; enter"
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
                let interrupted = current
                    .wait_survival_mining(&intent, Duration::from_secs(3))
                    .await
                    .map_err(|e| e.to_string())?;
                if !matches!(interrupted, MiningStatus::RequiresInspection { .. }) {
                    return Err("declared inventory interruption was not observed".into());
                }
            } else {
                // Scheduling estimate only. A received interruption bypasses FINISH.
                let status = current
                    .wait_survival_mining(&intent, Duration::from_millis(intent.estimated_wait_ms))
                    .await
                    .map_err(|e| e.to_string())?;
                if matches!(status, MiningStatus::Mining { .. }) {
                    if let Err(error) = current.finish_survival_mining(&intent).await {
                        if !matches!(
                            current.observe_survival_mining(&intent).await,
                            Ok(MiningStatus::RequiresInspection { .. })
                        ) {
                            return Err(error.to_string());
                        }
                    }
                    current
                        .wait_survival_mining(&intent, Duration::from_secs(5))
                        .await
                        .map_err(|e| e.to_string())?;
                }
            }
            let status = current
                .observe_survival_mining(&intent)
                .await
                .map_err(|e| e.to_string())?;
            let history = current.operation_history().await;
            let record = history
                .mining
                .as_ref()
                .ok_or("mining history unavailable")?;
            let plan =
                CleanupRecoveryPlan::for_record(&edit, record).map_err(|e| format!("{e:?}"))?;
            let recovery = current
                .prepare_mining_retirement(&intent, &observer)
                .await
                .map_err(|e| e.to_string())?;
            events.push(json!({"phase":"removal_outcome","attempt":attempt,"plan":plan,"status":status,"history":history}));
            traces.push(json!(
                bot.stop_packet_trace().await.map_err(|e| e.to_string())?
            ));
            recovery.close_source().await.map_err(|e| e.to_string())?;
            let retirement = recovery
                .wait(Duration::from_secs(3))
                .await
                .map_err(|e| e.to_string())?;
            if !matches!(retirement, MiningRetirementStatus::Retired { .. }) {
                return Err(format!("{retirement:?}"));
            }
            let target = viewer
                .observe_region(Region {
                    min: edit.position,
                    max: edit.position,
                })
                .await
                .map_err(|e| e.to_string())?;
            let expected = plan
                .target_for_reconnect(target.blocks[0].state.as_ref())
                .map_err(|e| format!("{e:?}"))?;
            let recovered = recovery
                .reconnect(
                    ConnectionConfig::offline(
                        Server::new("127.0.0.1", 25572),
                        "NatMineBot",
                        MinecraftVersion::Java1_21_11,
                    ),
                    expected,
                )
                .await
                .map_err(|e| e.to_string())?;
            *bot = recovered.client;
            bot.start_packet_trace(8_388_608)
                .await
                .map_err(|e| e.to_string())?;
            let fresh = recovered
                .operations
                .capture_survival_scene(scene.region())
                .await
                .map_err(|e| e.to_string())?;
            let decision = plan
                .reconcile_fresh(&recovered.evidence, &fresh)
                .map_err(|e| format!("{e:?}"))?;
            events.push(json!({"phase":"recovery","target":target,"attempt":attempt,"retirement":retirement,
                "evidence":recovered.evidence,"decision":decision,"fresh_inventory":recovered.operations.player_state().await.map_err(|e|e.to_string())?}));
            if decision == CleanupReconciliation::AlreadyAbsent {
                complete = true;
                break;
            }
        }
        if !complete {
            return Err(format!(
                "bounded cleanup attempts exhausted at x={x}; no further retry"
            ));
        }
    }
    let final_region = viewer
        .observe_region(Region {
            min: [-1, -61, -1],
            max: [4, -59, 1],
        })
        .await
        .map_err(|e| e.to_string())?;
    for cell in &final_region.blocks {
        let expected = if cell.position[1] == -61 {
            "stone"
        } else {
            "air"
        };
        if cell.state.as_ref() != Some(&block(expected)) {
            return Err(format!("final site differs at {:?}", cell.position));
        }
    }
    events.push(json!({"phase":"complete_cleanup","independent":final_region}));
    Ok(())
}
