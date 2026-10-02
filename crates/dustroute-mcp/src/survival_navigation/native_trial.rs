//! Explicit isolated non-OP route selection, ordinary placement and return.
use super::*;
use serde_json::json;
use std::{
    io::Write,
    time::{Duration, Instant},
};
use voxrig::versions::java_1_21_11::operations::{
    InventorySlot, PlacementStatus, SurvivalMotionStatus,
};
use voxrig::{BlockFace, Client, ConnectionConfig, MinecraftVersion, Region, Server};
pub(super) async fn connect(name: &str) -> Client {
    let c = Client::connect(ConnectionConfig::offline(
        Server::new("127.0.0.1", 25572),
        name,
        MinecraftVersion::Java1_21_11,
    ))
    .await
    .unwrap();
    c.wait_until_ready().await.unwrap();
    c
}
pub(super) async fn finish(ops: &Operations) -> Result<SurvivalMotionRecord, String> {
    tokio::time::timeout(Duration::from_secs(40), async {
        loop {
            let r = ops.survival_motion().await.ok_or("missing movement run")?;
            if r.status == SurvivalMotionStatus::Observed {
                return Ok(r);
            }
            if r.status == SurvivalMotionStatus::RequiresInspection {
                return Err(format!("{:?}", r.problem));
            }
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
    })
    .await
    .map_err(|_| "movement result timeout".to_owned())?
}
#[tokio::test]
#[ignore = "explicit isolated non-OP fixture and console preparation required"]
async fn native_route_around_wall_place_and_return() {
    assert_eq!(
        std::env::var("DUSTROUTE_SURVIVAL_ROUTE_PORT").unwrap(),
        "25572"
    );
    let file = std::fs::OpenOptions::new()
        .create_new(true)
        .write(true)
        .open(std::env::var("DUSTROUTE_SURVIVAL_ROUTE_OUTPUT").unwrap())
        .unwrap();
    let bot = connect("NatMineBot").await;
    let viewer = connect("NatMineView").await;
    let ops = bot.java_1_21_11_operations().unwrap();
    let observer = viewer.java_1_21_11_operations().unwrap();
    println!(
        "FIXTURE route: stone floor y=-61; air above; wall x=2,y=-60..-58,z=-1..0; bot 0.5 -60 0.5, viewer 0.5 -60 5.5; clear bot and inventory.0 dirt 1; enter"
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
    tokio::time::timeout(Duration::from_secs(5), async {
        loop {
            let p = ops.player_state().await.unwrap();
            if p.inventory.slots[9]
                == (InventorySlot::Item {
                    item: voxrig::versions::java_1_21_11::operations::default_item("dirt", 1)
                        .unwrap(),
                })
                && ops.standing_context().await.is_ok()
                && observer
                    .visible_players()
                    .await
                    .unwrap()
                    .players
                    .iter()
                    .any(|p| p.name == "NatMineBot")
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
    let mut evidence = Vec::new();
    let result = exercise(&ops, &observer, &viewer, &mut evidence).await;
    let after = ops.player_state().await.ok();
    let history = ops.operation_history().await;
    let trace = bot.stop_packet_trace().await.unwrap();
    let observer_trace = viewer.stop_packet_trace().await.unwrap();
    bot.disconnect().await.unwrap();
    viewer.disconnect().await.unwrap();
    serde_json::to_writer(file,&json!({"scope":"bounded native search around a three-high wall; non-OP normal placement and freshly revalidated return; not full Blueprint construction","before":before,"evidence":evidence,"after":after,"history":history,"error":result.as_ref().err(),"trace":trace,"observer_trace":observer_trace})).unwrap();
    result.unwrap();
}
async fn exercise(
    ops: &Operations,
    observer: &Operations,
    viewer: &Client,
    evidence: &mut Vec<serde_json::Value>,
) -> Result<(), String> {
    let swap = ops
        .swap_player_hotbar(9, 0)
        .await
        .map_err(|e| e.to_string())?;
    ops.wait_inventory_swap(&swap, Duration::from_secs(3))
        .await
        .map_err(|e| e.to_string())?;
    ops.select_hotbar(0).await.map_err(|e| e.to_string())?;
    let request = RouteRequest {
        travel: TravelBounds {
            min: [-4.0, -60.0, -4.0],
            max: [10.0, -55.0, 8.0],
        },
        goal: TravelBounds {
            min: [3.8, -60.0, 1.8],
            max: [4.7, -60.0, 2.7],
        },
        max_previews: 384,
    };
    let hypothetical_start = Instant::now();
    let captured = ops
        .capture_survival_scene(Region {
            min: [-5, -62, -5],
            max: [11, -53, 9],
        })
        .await
        .map_err(|e| e.to_string())?;
    let scenario = captured.scenario();
    let hypothetical = plan_hypothetical_route(&scenario, request.clone())
        .await
        .map_err(|e| format!("{e:?}"))?;
    let hypothetical_arrival = hypothetical
        .after_outbound(&scenario)
        .map_err(|e| format!("{e:?}"))?;
    evidence.push(json!({"phase":"hypothetical_route","plan":hypothetical,"arrival":hypothetical_arrival.position(),"capture_and_search_ms":hypothetical_start.elapsed().as_millis()}));
    let air = voxrig::NativeBlockState {
        name: "minecraft:air".into(),
        properties: Default::default(),
    };
    let future = scenario
        .after_edits(&[
            voxrig::versions::java_1_21_11::operations::HypotheticalBlockEdit {
                position: [1, -60, 1],
                before: air.clone(),
                after: voxrig::NativeBlockState {
                    name: "minecraft:dirt".into(),
                    properties: Default::default(),
                },
            },
        ])
        .map_err(|e| e.to_string())?;
    let changed = future
        .preview_path(&hypothetical.outbound().controls)
        .map_err(|e| e.to_string())?;
    if changed.frames == hypothetical.outbound().frames
        || hypothetical.after_outbound(&future).is_ok()
    {
        return Err("future terrain did not invalidate the original hypothetical route".into());
    }
    ops.validate_survival_scene(&captured)
        .await
        .map_err(|e| e.to_string())?;
    if scenario.block([1, -60, 1]).map_err(|e| e.to_string())? != air {
        return Err("future edit leaked to original scene".into());
    }
    evidence.push(json!({"phase":"hypothetical_obstruction","prediction":changed,"old_route_refused":true,"live_capture_still_valid":true}));
    let start = Instant::now();
    let plan = plan_survival_route(ops, request)
        .await
        .map_err(|e| format!("{e:?}"))?;
    if plan.outbound().frames != hypothetical.outbound().frames {
        return Err("captured and live route predictions differ".into());
    }
    evidence.push(json!({"phase":"plan","plan":plan,"elapsed_ms":start.elapsed().as_millis()}));
    println!(
        "ROUTE {} previews, {} outbound ticks, {} ms",
        plan.search().previews,
        plan.outbound().controls.len(),
        start.elapsed().as_millis()
    );
    std::io::stdout().flush().unwrap();
    plan.start(ops, observer).await.map_err(|e| e.to_string())?;
    let arrival = finish(ops).await?;
    evidence.push(json!({"phase":"arrival","motion":arrival}));
    let standing = ops.standing_context().await.map_err(|e| e.to_string())?;
    let point = [5.5, -60.0, 2.5];
    let d: [f64; 3] = std::array::from_fn(|i| point[i] - standing.eye_position[i]);
    ops.look([
        (-d[0]).atan2(d[2]).to_degrees() as f32,
        (-d[1]).atan2(d[0].hypot(d[2])).to_degrees() as f32,
    ])
    .await
    .map_err(|e| e.to_string())?;
    let intent = ops
        .place_survival_cube([5, -61, 2], BlockFace::Up)
        .await
        .map_err(|e| e.to_string())?;
    let result = ops
        .wait_survival_placement(&intent, Duration::from_secs(3))
        .await
        .map_err(|e| e.to_string())?;
    if !matches!(result, PlacementStatus::ObservedPlaced { .. }) {
        return Err(format!("{result:?}"));
    }
    let observed = tokio::time::timeout(Duration::from_secs(3), async {
        loop {
            let r = viewer
                .observe_region(Region {
                    min: intent.target,
                    max: intent.target,
                })
                .await
                .map_err(|e| e.to_string())?;
            if r.blocks[0]
                .state
                .as_ref()
                .is_some_and(|s| s.name == "minecraft:dirt")
            {
                break Ok::<_, String>(r);
            }
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
    })
    .await
    .map_err(|_| "placement observer timeout".to_owned())??;
    evidence
        .push(json!({"phase":"placement","intent":intent,"result":result,"independent":observed}));
    let return_preview = plan
        .preview_return(ops)
        .await
        .map_err(|e| format!("{e:?}"))?;
    evidence.push(json!({"phase":"return_revalidation","preview":return_preview}));
    ops.start_previewed_survival_motion(&return_preview, observer)
        .await
        .map_err(|e| e.to_string())?;
    let returned = finish(ops).await?;
    evidence.push(json!({"phase":"returned","motion":returned}));
    let p = ops
        .standing_context()
        .await
        .map_err(|e| e.to_string())?
        .position;
    if (p[0] - 0.5).hypot(p[2] - 0.5) > 0.35 || (p[1] + 60.0).abs() > 0.125 {
        return Err("return outside initial standing area".into());
    }
    Ok(())
}
