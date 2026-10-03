//! Explicit isolated non-OP control/standing/placement comparison.
use super::*;
use operations::{InventorySlot, PlacementStatus, SurvivalInput, SurvivalMotionStatus};
use serde_json::json;
use std::time::Duration;
async fn connect(name: &str) -> Bot {
    let bot = Bot::connect(ConnectionConfig::offline(
        crate::Server::new("127.0.0.1", 25572),
        name,
        MinecraftVersion::Java1_21_11,
    ))
    .await
    .unwrap();
    bot.wait_until_ready().await.unwrap();
    bot
}
#[tokio::test]
#[ignore = "requires explicit dedicated native fixture and console setup"]
async fn native_survival_walk_jump_collision_and_place() {
    motion_fixture(false).await;
}
#[tokio::test]
#[ignore = "requires explicit dedicated non-OP fixture; observer is comparison only"]
async fn native_survival_predicted_walk_jump_collision_and_place() {
    motion_fixture(true).await;
}
async fn motion_fixture(predicted: bool) {
    use std::io::Write;
    assert_eq!(std::env::var("NATIVE_MOVEMENT_PORT").unwrap(), "25572");
    let file = std::fs::OpenOptions::new()
        .create_new(true)
        .write(true)
        .open(std::env::var("NATIVE_MOVEMENT_OUTPUT").unwrap())
        .unwrap();
    let bot = connect("NatMineBot").await;
    let viewer = connect("NatMineView").await;
    println!(
        "FIXTURE movement: floor stone y=-61, air above; tp NatMineBot 0.5 -60 0.5; tp NatMineView 0.5 -60 4.5; clear NatMineBot; item replace entity NatMineBot inventory.0 with minecraft:dirt 3; enter"
    );
    std::io::stdout().flush().unwrap();
    timeout(
        Duration::from_secs(60),
        tokio::task::spawn_blocking(|| {
            let mut line = String::new();
            std::io::stdin().read_line(&mut line)
        }),
    )
    .await
    .unwrap()
    .unwrap()
    .unwrap();
    let api = bot.operations();
    let observer = viewer.operations();
    timeout(Duration::from_secs(5), async {
        loop {
            let p = api.player_state().await.unwrap();
            if p.inventory.slots[9]
                == (InventorySlot::Item {
                    item: operations::default_item("dirt", 3).unwrap(),
                })
                && api.standing_context().await.is_ok()
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
    bot.start_packet_trace(16_777_216).await.unwrap();
    viewer.start_packet_trace(16_777_216).await.unwrap();
    let before = api.player_state().await.unwrap();
    assert_eq!(before.game_mode, Some(operations::GameMode::Survival));
    let mut cases = Vec::new();
    let result = run_cases(&api, &observer, &viewer, &mut cases, predicted).await;

    let after = api.player_state().await;
    let history = api.operation_history().await;
    let trace = bot.stop_packet_trace().await.unwrap();
    let observer_trace = viewer.stop_packet_trace().await.unwrap();
    bot.disconnect().await.unwrap();
    viewer.disconnect().await.unwrap();
    serde_json::to_writer(file,&json!({"minecraft":"Java 1.21.11","scope":"isolated non-OP native dry walking, jump/landing, wall collision and ordinary placement; no stop acknowledgement claim","prediction_contract":predicted,"observer_role":if predicted {"comparison_only"} else {"endpoint_admission_and_comparison"},"before":before,"cases":cases,"after":after.as_ref().ok(),"history":history,"error":result.as_ref().err().map(ToString::to_string),"trace":trace,"observer_trace":observer_trace})).unwrap();
    result.unwrap();
}

async fn run_cases(
    api: &operations::Operations,
    observer: &operations::Operations,
    viewer: &Bot,
    cases: &mut Vec<serde_json::Value>,
    predicted: bool,
) -> Result<()> {
    use std::io::Write;

    let swap = api.swap_player_hotbar(9, 0).await?;
    api.wait_inventory_swap(&swap, Duration::from_secs(3))
        .await?;
    api.select_hotbar(0).await?;
    for name in ["walk", "jump", "wall"] {
        if predicted && name == "wall" {
            println!("PREDICTED_WALL: place the fixed test wall; enter");
            std::io::stdout().flush().unwrap();
            tokio::task::spawn_blocking(|| {
                let mut line = String::new();
                std::io::stdin().read_line(&mut line).unwrap();
            })
            .await
            .unwrap();
            tokio::time::sleep(Duration::from_millis(200)).await;
        }
        let mut inputs = vec![SurvivalInput::default(); if name == "wall" { 40 } else { 30 }];
        if name == "jump" {
            inputs[0].jump = true;
        } else {
            for i in inputs.iter_mut().take(if name == "wall" { 20 } else { 8 }) {
                i.forward = 1;
            }
        }
        if name == "wall" {
            let refused = api.preview_survival_motion(-90.0, &inputs).await?;
            if !matches!(
                refused.terminal_clearance,
                operations::TerminalClearance::RequiresReplan { .. }
            ) {
                return Err(Error::new(
                    ErrorKind::State,
                    anyhow::anyhow!("wall-touch endpoint admitted"),
                ));
            }
            let history = serde_json::to_value(api.operation_history().await).unwrap();
            let controls: Vec<_> = inputs
                .iter()
                .map(|input| operations::SurvivalControl {
                    yaw: -90.,
                    input: *input,
                })
                .collect();
            let result = if predicted {
                api.start_predicted_survival_path(&controls).await
            } else {
                api.start_survival_motion(-90.0, &inputs, observer).await
            };
            if result.is_ok() {
                return Err(Error::new(
                    ErrorKind::State,
                    anyhow::anyhow!("wall-touch input was sent"),
                ));
            }
            assert_eq!(
                history,
                serde_json::to_value(api.operation_history().await).unwrap()
            );
            cases.push(json!({"phase":"wall_endpoint_refused_before_io","preview":refused}));
            inputs = vec![SurvivalInput::default(); 60];
            for i in inputs.iter_mut().take(20) {
                i.forward = 1;
            }
            for i in inputs.iter_mut().skip(30).take(4) {
                i.forward = -1;
            }
        }
        let controls: Vec<_> = inputs
            .iter()
            .map(|input| operations::SurvivalControl {
                yaw: -90.,
                input: *input,
            })
            .collect();
        let start = if predicted {
            api.start_predicted_survival_path(&controls).await?
        } else {
            api.start_survival_motion(-90.0, &inputs, observer).await?
        };
        println!("MOTION {name} started run {}", start.run_id);
        std::io::stdout().flush().unwrap();
        let mut comparison_samples = Vec::new();
        let run = timeout(Duration::from_secs(40), async {
            loop {
                let r = api.survival_motion().await.unwrap();
                if predicted {
                    let seen = observer.visible_players().await?;
                    if let Some(p) = seen.players.into_iter().find(|p| p.name == "NatMineBot") {
                        comparison_samples.push(p);
                    }
                }
                if matches!(
                    r.status,
                    SurvivalMotionStatus::Observed
                        | SurvivalMotionStatus::Predicted
                        | SurvivalMotionStatus::RequiresInspection
                ) {
                    break Ok::<_, Error>(r);
                }
                tokio::time::sleep(Duration::from_millis(20)).await;
            }
        })
        .await
        .context("motion result timeout")??;
        cases.push(json!({"phase":name,"motion":run}));
        let expected = if predicted {
            SurvivalMotionStatus::Predicted
        } else {
            SurvivalMotionStatus::Observed
        };
        if run.status != expected {
            return Err(Error::new(
                ErrorKind::State,
                anyhow::anyhow!("motion {name}: {:?}", run.problem),
            ));
        }
        let standing = api.standing_context().await?;
        if predicted {
            if name == "jump"
                && !comparison_samples
                    .iter()
                    .any(|p| p.position[1] > start.preview.initial.position[1] + 0.5)
            {
                return Err(Error::new(
                    ErrorKind::State,
                    anyhow::anyhow!("jump rise not independently observed"),
                ));
            }
            cases.push(json!({"phase":format!("{name}_trajectory_comparison"),
                "samples":comparison_samples,"used_for_native_admission":false}));
        }
        if name == "wall" && !run.preview.frames.iter().any(|f| f.horizontal_collision) {
            return Err(Error::new(
                ErrorKind::State,
                anyhow::anyhow!("wall did not collide"),
            ));
        }
        let x = standing.position[0].floor() as i32 + 2;
        let support = if name == "wall" {
            [standing.position[0].floor() as i32, -61, 2]
        } else {
            [x, if name == "walk" { -61 } else { -60 }, 0]
        };
        let point = [
            f64::from(support[0]) + 0.5,
            f64::from(support[1] + 1),
            f64::from(support[2]) + 0.5,
        ];
        let d: [f64; 3] = std::array::from_fn(|i| point[i] - standing.eye_position[i]);
        api.look([
            (-d[0]).atan2(d[2]).to_degrees() as f32,
            (-d[1]).atan2(d[0].hypot(d[2])).to_degrees() as f32,
        ])
        .await?;
        let intent = api
            .place_survival_cube(support, crate::BlockFace::Up)
            .await?;
        let placement = api
            .wait_survival_placement(&intent, Duration::from_secs(3))
            .await?;
        if !matches!(placement, PlacementStatus::ObservedPlaced { .. }) {
            return Err(Error::new(
                ErrorKind::State,
                anyhow::anyhow!("placement {placement:?}"),
            ));
        }
        let independent = timeout(Duration::from_secs(3), async {
            loop {
                let observation = viewer
                    .observe_region(Region {
                        min: intent.target,
                        max: intent.target,
                    })
                    .await?;
                if observation.blocks[0]
                    .state
                    .as_ref()
                    .is_some_and(|s| s.name == "minecraft:dirt")
                {
                    break Ok::<_, Error>(observation);
                }
                tokio::time::sleep(Duration::from_millis(20)).await;
            }
        })
        .await
        .context("independent placement timeout")??;
        if predicted {
            let position = timeout(Duration::from_secs(5), async {
                loop {
                    let seen = observer.visible_players().await?;
                    if let Some(p) = seen.players.iter().find(|p| p.name == "NatMineBot") {
                        if (0..3).all(|i| {
                            (p.position[i] - standing.position[i]).abs()
                                <= p.motion.position_error[i] + 1e-9
                        }) {
                            break Ok::<_, Error>(p.clone());
                        }
                    }
                    tokio::time::sleep(Duration::from_millis(20)).await;
                }
            })
            .await
            .context("comparison-only endpoint timeout")??;
            cases.push(
                json!({"phase":format!("{name}_independent_comparison"),"position":position,
                "after_native_placement":true,"physical_error_bound_claimed":false}),
            );
        }
        cases.push(json!({"phase":format!("{name}_placement"),"standing":standing,"intent":intent,"placement":placement,"independent":independent}));
        println!("PLACED after {name}");
        std::io::stdout().flush().unwrap();
    }
    Ok(())
}
