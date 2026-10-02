//! Opt-in public-API edge aiming comparison, independent of caller construction plans.
use super::*;
use operations::{
    InventorySlot, PlacementStatus, SurvivalControl, SurvivalInput, SurvivalMotionStatus,
};
use serde_json::{Value, json};
use std::{io::Write, time::Duration};

async fn connect(name: &str) -> crate::Client {
    let client = crate::Client::connect(ConnectionConfig::offline(
        crate::Server::new("127.0.0.1", 25572),
        name,
        MinecraftVersion::Java1_21_11,
    ))
    .await
    .unwrap();
    client.wait_until_ready().await.unwrap();
    client
}
fn fail(message: impl ToString) -> Error {
    Error::new(ErrorKind::State, anyhow::anyhow!(message.to_string()))
}
fn controls(yaw: f32, active: usize) -> Vec<SurvivalControl> {
    (0..active + 20)
        .map(|t| SurvivalControl {
            yaw,
            input: SurvivalInput {
                forward: i8::from(t < active),
                ..Default::default()
            },
        })
        .collect()
}
async fn finish(
    api: &crate::checked_survival::Operations,
) -> Result<operations::SurvivalMotionRecord> {
    timeout(Duration::from_secs(40), async {
        loop {
            let run = api
                .survival_motion()
                .await
                .ok_or_else(|| fail("missing run"))?;
            match run.status {
                SurvivalMotionStatus::Observed => return Ok(run),
                SurvivalMotionStatus::RequiresInspection => {
                    return Err(fail(format!("motion: {:?}", run.problem)));
                }
                _ => tokio::time::sleep(Duration::from_millis(20)).await,
            }
        }
    })
    .await
    .context("movement observation timeout")?
}

#[tokio::test]
#[ignore = "declared dedicated non-OP fixture; explicit port/output and console gate"]
async fn native_edge_move_place_and_retreat() {
    assert_eq!(std::env::var("NATIVE_EDGE_PORT").unwrap(), "25572");
    let file = std::fs::OpenOptions::new()
        .create_new(true)
        .write(true)
        .open(std::env::var("NATIVE_EDGE_OUTPUT").unwrap())
        .unwrap();
    let bot = connect("NatMineBot").await;
    let viewer = connect("NatMineView").await;
    println!(
        "FIXTURE edge: fresh isolated world; stone floor y=-61 x/z=-5..6; air y=-60..-54; stone 0 -60 0; bot 0.6 -59 0.5; viewer 0.5 -60 4.5; clear bot; inventory.0 dirt 1; enter"
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
    let api = bot.survival().unwrap();
    timeout(Duration::from_secs(5), async {
        loop {
            let p = api.player_state().await.unwrap();
            if p.game_mode == Some(operations::GameMode::Survival)
                && p.inventory.slots[9]
                    == (InventorySlot::Item {
                        item: operations::default_item("dirt", 1).unwrap(),
                    })
                && api
                    .standing_context()
                    .await
                    .is_ok_and(|s| s.position == [0.6, -59.0, 0.5])
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
    let before = api.player_state().await.unwrap();
    let mut events = vec![];
    let result = exercise(&bot, &viewer, &mut events).await;
    let after = api.player_state().await.ok();
    let history = api.operation_history().await;
    let trace = bot.stop_packet_trace().await.unwrap();
    let observer_trace = viewer.stop_packet_trace().await.unwrap();
    let disconnect = bot.disconnect().await;
    viewer.disconnect().await.unwrap();
    serde_json::to_writer(file, &json!({"scope":"isolated non-OP edge move/rest/side placement/material decrement/retreat; not full construction or cleanup", "before":before,"after":after,"events":events,"history":history,"trace":trace,"observer_trace":observer_trace,"error":result.as_ref().err().map(ToString::to_string),"disconnect_error":disconnect.err().map(|e|e.to_string())})).unwrap();
    result.unwrap();
}
async fn exercise(
    bot: &crate::Client,
    viewer: &crate::Client,
    events: &mut Vec<Value>,
) -> Result<()> {
    let api = bot.survival()?;
    let observer = viewer.survival()?;
    let scene = api
        .capture_survival_scene(Region {
            min: [-4, -62, -4],
            max: [5, -54, 5],
        })
        .await?;
    let initial = scene.scenario();
    let mut candidate = None;
    for active in 1..=8 {
        let out = controls(-90.0, active);
        let edge = match initial.after_path(&out) {
            Ok(edge) => edge,
            Err(error) => {
                events.push(json!({"phase":"candidate_rejected","active":active,"stage":"outbound","error":error.to_string()}));
                continue;
            }
        };
        let p = edge.position();
        if !(1.08..=1.23).contains(&p[0]) || p[1] != -59.0 || p[2] != 0.5 {
            events.push(json!({"phase":"candidate_rejected","active":active,"stage":"edge_position","position":p}));
            continue;
        }
        let rotation = [
            90.0,
            (f64::from(1.62f32) + 0.5).atan2(p[0] - 1.0).to_degrees() as f32,
        ];
        let place = match edge.preview_cube_placement(
            [0, -60, 0],
            crate::BlockFace::East,
            rotation,
            "dirt",
        ) {
            Ok(place) => place,
            Err(error) => {
                events.push(json!({"phase":"candidate_rejected","active":active,"stage":"placement","position":p,"error":error.to_string()}));
                continue;
            }
        };
        let placed = edge.after_edits(std::slice::from_ref(&place.edit))?;
        let back = controls(90.0, active);
        let returned = match placed.after_path(&back) {
            Ok(returned) => returned,
            Err(error) => {
                events.push(json!({"phase":"candidate_rejected","active":active,"stage":"retreat","error":error.to_string()}));
                continue;
            }
        };
        if (returned.position()[0] - 0.6).abs() > 0.25 || returned.position()[1] != -59.0 {
            events.push(json!({"phase":"candidate_rejected","active":active,"stage":"retreat_position","position":returned.position()}));
            continue;
        }
        candidate = Some((out, back, place, placed));
        break;
    }
    let (out, back, place, placed) =
        candidate.ok_or_else(|| fail("no declared safe edge candidate within 1..8 ticks"))?;
    let expected_out = initial.preview_path(&out)?;
    let expected_back = placed.preview_path(&back)?;
    events.push(json!({"phase":"complete_hypothetical_sequence","outbound":expected_out,"placement":place,"retreat":expected_back}));
    api.validate_survival_scene(&scene).await?;
    let swap = api.swap_player_hotbar(9, 0).await?;
    api.wait_inventory_swap(&swap, Duration::from_secs(3))
        .await?;
    api.select_hotbar(0).await?;
    let preview = api.preview_survival_path(&out).await?;
    if preview.frames != expected_out.frames {
        return Err(fail("outbound differs from hypothetical frames"));
    }
    api.start_previewed_survival_motion(&preview, &observer)
        .await?;
    let run = finish(&api).await?;
    events.push(
        json!({"phase":"observed_edge","motion":run,"standing":api.standing_context().await?}),
    );
    api.look(place.rotation).await?;
    let intent = api
        .place_survival_cube(place.support, crate::BlockFace::East)
        .await?;
    if intent.target != place.edit.position
        || intent.expected != place.edit.after
        || intent.cursor != place.cursor
    {
        return Err(fail(
            "actual side placement differs from hypothetical target/cursor",
        ));
    }
    let receipt = api
        .wait_survival_placement(&intent, Duration::from_secs(3))
        .await?;
    if !matches!(receipt, PlacementStatus::ObservedPlaced { .. }) {
        return Err(fail(format!("{receipt:?}")));
    }
    let seen = timeout(Duration::from_secs(3), async {
        loop {
            let r = viewer
                .observe_region(Region {
                    min: intent.target,
                    max: intent.target,
                })
                .await?;
            if r.blocks[0].state.as_ref() == Some(&intent.expected) {
                return Ok::<_, Error>(r);
            }
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
    })
    .await
    .context("independent placement timeout")??;
    events.push(json!({"phase":"observed_placement","receipt":receipt,"independent":seen}));
    let preview = api.preview_survival_path(&back).await?;
    if preview.frames != expected_back.frames {
        return Err(fail("retreat differs from hypothetical frames"));
    }
    api.start_previewed_survival_motion(&preview, &observer)
        .await?;
    events.push(json!({"phase":"observed_retreat","motion":finish(&api).await?}));
    if api.player_state().await?.inventory.slots[36] != InventorySlot::Empty {
        return Err(fail("material not fully consumed"));
    }
    let final_region = viewer
        .observe_region(Region {
            min: [-1, -61, -1],
            max: [2, -59, 1],
        })
        .await?;
    for b in &final_region.blocks {
        let expected = if b.position == [0, -60, 0] || b.position[1] == -61 {
            "minecraft:stone"
        } else if b.position == [1, -60, 0] {
            "minecraft:dirt"
        } else {
            "minecraft:air"
        };
        if b.state
            .as_ref()
            .is_none_or(|s| s.name != expected || !s.properties.is_empty())
        {
            return Err(fail(format!("unexpected final cell: {b:?}")));
        }
    }
    events.push(json!({"phase":"final_region","observation":final_region}));
    Ok(())
}
