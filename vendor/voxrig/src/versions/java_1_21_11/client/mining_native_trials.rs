//! Explicit opt-in comparison, on a dedicated non-OP survival fixture only.
//! Raw action writes are test-private; they do not bypass a production operation.
use super::*;
use serde_json::{Value, json};
use std::{io, time::Duration};

async fn phase(message: &str) {
    use std::io::Write;
    println!("{message}");
    io::stdout().flush().unwrap();
    let read = timeout(
        Duration::from_secs(60),
        tokio::task::spawn_blocking(|| {
            let mut line = String::new();
            io::stdin().read_line(&mut line)
        }),
    )
    .await
    .unwrap()
    .unwrap()
    .unwrap();
    assert!(read > 0, "fixture control stdin closed");
}
async fn connect(name: &str, port: u16) -> Bot {
    let bot = Bot::connect(ConnectionConfig::offline(
        crate::Server::new("127.0.0.1", port),
        name,
        MinecraftVersion::Java1_21_11,
    ))
    .await
    .unwrap();
    bot.wait_until_ready().await.unwrap();
    bot
}
async fn sample(bot: &Bot, target: [i32; 3], start: Instant) -> Value {
    let observation = bot
        .observe_region(Region {
            min: target,
            max: target,
        })
        .await
        .unwrap();
    json!({"elapsed_ms":start.elapsed().as_millis(),"received":observation,"player":bot.operations().player_state().await.unwrap()})
}
fn air(sample: &Value) -> bool {
    matches!(
        sample["received"]["blocks"][0]["state"]["name"].as_str(),
        Some("minecraft:air")
    )
}
async fn action(
    bot: &Bot,
    target: [i32; 3],
    kind: u8,
    sequence: i32,
    start: Instant,
    inputs: &mut Vec<Value>,
) {
    let mut payload = vec![kind];
    let position = ((i64::from(target[0]) & 0x3ffffff) << 38)
        | ((i64::from(target[2]) & 0x3ffffff) << 12)
        | (i64::from(target[1]) & 0xfff);
    payload.extend(position.to_be_bytes());
    payload.push(4);
    put_varint(&mut payload, sequence);
    inputs.push(json!({"elapsed_ms":start.elapsed().as_millis(),"kind":kind,"interaction_sequence":sequence,"payload_hex":hex::encode(&payload)}));
    bot.session
        .send(ids::play_serverbound::BLOCK_DIG, &payload)
        .await
        .unwrap();
}

#[tokio::test]
#[ignore = "requires dedicated native 1.21.11 localhost server, fixture console and explicit environment"]
async fn native_survival_mining_finish_abort_and_disconnect() {
    comparison(false).await;
}

#[tokio::test]
#[ignore = "requires dedicated native 1.21.11 localhost server, fixture console and explicit environment"]
async fn native_survival_mining_api_finish_abort_and_disconnect() {
    comparison(true).await;
}

#[tokio::test]
#[ignore = "requires isolated vanilla 1.21.11 non-OP fixture and explicit console controller"]
async fn native_survival_same_profile_mining_recovery() {
    use crate::checked_survival::{
        MiningRecoveryBoundary, MiningRecoveryTarget, MiningStatus, PlacementStatus,
    };
    use std::io::{Seek, Write};
    let port: u16 = std::env::var("NATIVE_MINING_PORT")
        .unwrap()
        .parse()
        .unwrap();
    assert_eq!(port, 25572);
    let mut file = std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(std::env::var("NATIVE_MINING_OUTPUT").unwrap())
        .unwrap();
    let mut bot = connect("NatMineBot", port).await;
    let viewer = connect("NatMineView", port).await;
    let target = [2, -59, 0];
    let config = ConnectionConfig::offline(
        crate::Server::new("127.0.0.1", port),
        "NatMineBot",
        MinecraftVersion::Java1_21_11,
    );
    let mut cases = Vec::new();
    for (name, wait_ms) in [
        ("normal_finish", 1100),
        ("early_finish_abort", 50),
        ("external_air_immediate_replacement", 50),
    ] {
        phase(&format!("PROFILE_FIXTURE {name}: prepare dry floor, support [2,-60,0], target [2,-59,0], one supplied cobblestone in hotbar 0, empty hand 1; enter")).await;
        // Fixture writes precede the case, not a production readiness/retirement fence.
        tokio::time::sleep(Duration::from_millis(500)).await;
        let source = crate::Client::from_java_1_21_11(bot.clone())
            .survival()
            .unwrap();
        source.select_hotbar(1).await.unwrap();
        source.look([-90.0, 3.0]).await.unwrap();
        bot.start_packet_trace(8_388_608).await.unwrap();
        viewer.start_packet_trace(8_388_608).await.unwrap();
        let start = Instant::now();
        let intent = source
            .start_survival_mining(target, crate::BlockFace::West)
            .await
            .unwrap();
        let recovery = source
            .prepare_mining_profile_recovery(&intent)
            .await
            .unwrap();
        tokio::time::sleep(Duration::from_millis(wait_ms)).await;
        source.finish_survival_mining(&intent).await.unwrap();
        if name != "normal_finish" {
            source.abort_survival_mining(&intent).await.unwrap();
        }
        let external = if name == "external_air_immediate_replacement" {
            phase("PROFILE_EXTERNAL: console supplies air then immediate original stone at [2,-59,0]; enter").await;
            assert!(start.elapsed() < Duration::from_secs(3));
            Some(start.elapsed().as_millis())
        } else {
            None
        };
        if name == "normal_finish" {
            assert!(matches!(
                source
                    .wait_survival_mining(&intent, Duration::from_secs(3))
                    .await
                    .unwrap(),
                MiningStatus::ObservedRemoved { .. }
            ));
        }
        let old_trace = bot.stop_packet_trace().await.unwrap();
        // No viewer watch, profile removal wait or world read participates here.
        recovery.close_source().await.unwrap();
        let closed_ms = start.elapsed().as_millis();
        let mut fresh = recovery
            .reconnect(config.clone(), MiningRecoveryTarget::OriginalOrAir)
            .await
            .unwrap();
        let connected_ms = start.elapsed().as_millis();
        assert!(matches!(
            fresh.evidence.boundary,
            MiningRecoveryBoundary::SameProfileLogin { .. }
        ));
        assert_ne!(fresh.evidence.connection_id, intent.connection_id);
        assert!(fresh.evidence.old_history.connection_closed && fresh.evidence.interaction_ready);
        assert!(
            recovery
                .clone()
                .reconnect(config.clone(), MiningRecoveryTarget::OriginalOrAir)
                .await
                .is_err()
        );
        let initial_recovery = json!(fresh.evidence);
        let mut retained_target_samples = Vec::new();
        if fresh.evidence.target.name != "minecraft:air" {
            let begin = Instant::now();
            // Observe without any new mining: another miner's result could otherwise
            // hide the old delayed effect. Neither these reads nor their duration
            // authorize recovery; that admission has already completed natively.
            while begin.elapsed() < Duration::from_secs(9) {
                let observed = sample(&viewer, target, begin).await;
                let own = fresh
                    .client
                    .observe_region(Region {
                        min: target,
                        max: target,
                    })
                    .await
                    .unwrap();
                assert_eq!(own.blocks[0].state.as_ref(), Some(&fresh.evidence.target));
                assert_eq!(
                    observed["received"]["blocks"][0]["state"],
                    json!(fresh.evidence.target)
                );
                retained_target_samples.push(json!({"observer":observed,"own":own}));
                tokio::time::sleep(Duration::from_millis(50)).await;
            }
        }
        let followup = if fresh.evidence.target.name != "minecraft:air" {
            fresh.operations.select_hotbar(1).await.unwrap();
            fresh.operations.look([-90.0, 3.0]).await.unwrap();
            let mine = fresh
                .operations
                .start_survival_mining(target, crate::BlockFace::West)
                .await
                .unwrap();
            tokio::time::sleep(Duration::from_millis(mine.estimated_wait_ms)).await;
            let result = fresh
                .operations
                .observe_survival_mining(&mine)
                .await
                .unwrap();
            if matches!(result, MiningStatus::Mining { .. }) {
                fresh
                    .operations
                    .finish_survival_mining(&mine)
                    .await
                    .unwrap();
            }
            let removed = fresh
                .operations
                .wait_survival_mining(&mine, Duration::from_secs(3))
                .await
                .unwrap();
            assert!(matches!(removed, MiningStatus::ObservedRemoved { .. }));
            let second = fresh
                .operations
                .prepare_mining_profile_recovery(&mine)
                .await
                .unwrap();
            second.close_source().await.unwrap();
            fresh = second
                .reconnect(
                    config.clone(),
                    MiningRecoveryTarget::Exact(crate::NativeBlockState {
                        name: "minecraft:air".into(),
                        properties: Default::default(),
                    }),
                )
                .await
                .unwrap();
            Some(json!({"intent":mine,"result":removed,"recovery":fresh.evidence}))
        } else {
            None
        };
        fresh.client.start_packet_trace(8_388_608).await.unwrap();
        fresh.operations.select_hotbar(0).await.unwrap();
        let standing = fresh.operations.standing_context().await.unwrap();
        let eye = standing.eye_position;
        let hit = [2.5, -59.0, 0.5];
        let delta: [f64; 3] = std::array::from_fn(|i| hit[i] - eye[i]);
        let aim = [
            (-delta[0]).atan2(delta[2]).to_degrees() as f32,
            (-delta[1]).atan2(delta[0].hypot(delta[2])).to_degrees() as f32,
        ];
        fresh.operations.look(aim).await.unwrap();
        let placement = fresh
            .operations
            .place_survival_cube([2, -60, 0], crate::BlockFace::Up)
            .await
            .unwrap();
        assert_eq!(placement.target, target);
        let placed = fresh
            .operations
            .wait_survival_placement(&placement, Duration::from_secs(3))
            .await
            .unwrap();
        assert!(matches!(placed, PlacementStatus::ObservedPlaced { .. }));
        timeout(Duration::from_secs(3), async {
            loop {
                if sample(&viewer, target, start).await["received"]["blocks"][0]["state"]["name"]
                    == "minecraft:cobblestone"
                {
                    break;
                }
                tokio::time::sleep(Duration::from_millis(25)).await;
            }
        })
        .await
        .unwrap();
        let begin = Instant::now();
        let mut samples = Vec::new();
        // Bounded independent post-placement comparison; never retirement authority.
        while begin.elapsed() < Duration::from_secs(9) {
            let observed = sample(&viewer, target, begin).await;
            assert_eq!(
                observed["received"]["blocks"][0]["state"]["name"],
                "minecraft:cobblestone"
            );
            samples.push(observed);
            tokio::time::sleep(Duration::from_millis(50)).await;
        }
        cases.push(json!({"name":name,"intent":intent,"external_completed_ms":external,"closed_ms":closed_ms,"fresh_admitted_ms":connected_ms,"old_trace":old_trace,"initial_recovery":initial_recovery,"retained_target_samples":retained_target_samples,"followup_mining":followup,"placement":placement,"placement_result":placed,"samples":samples,"fresh_trace":fresh.client.stop_packet_trace().await.unwrap(),"viewer_trace":viewer.stop_packet_trace().await.unwrap()}));
        file.set_len(0).unwrap();
        file.rewind().unwrap();
        serde_json::to_writer(&mut file,&json!({"minecraft_version":"Java 1.21.11","all_cases_executed":cases.len()==3,"observer_role":"independent test comparison only; no production retirement watch","cases":cases})).unwrap();
        file.flush().unwrap();
        println!("PROFILE_CASE_VERIFIED {name}");
        bot = fresh.client.java_1_21_11_operations().unwrap().bot.clone();
    }
    bot.disconnect().await.unwrap();
    viewer.disconnect().await.unwrap();
}

async fn comparison(native_api: bool) {
    let port: u16 = std::env::var("NATIVE_MINING_PORT")
        .unwrap()
        .parse()
        .unwrap();
    assert_eq!(port, 25572, "reserved isolated trial port required");
    let output = std::env::var("NATIVE_MINING_OUTPUT").unwrap();
    let file = std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(output)
        .unwrap();
    let mut bot = connect("NatMineBot", port).await;
    let viewer = connect("NatMineView", port).await;
    let target = [2, -59, 0];
    let mut cases = Vec::new();
    for (name, delay_ms, abort, disconnect) in [
        ("normal_finish", 1100, false, false),
        ("early_finish_abort", 50, true, false),
        ("external_air_immediate_replacement", 50, true, false),
        ("early_finish_disconnect", 50, false, true),
    ] {
        if !native_api && name == "external_air_immediate_replacement" {
            continue;
        }
        phase(&format!("FIXTURE {name}: console prepares dry floor/air; target {target:?} {} ; tp NatMineBot 0.5 -60 0.5 -90 3; tp NatMineView 0.5 -60 4.5; clear NatMineBot; enter",if name=="normal_finish" {"dirt"} else {"stone"})).await;
        tokio::time::sleep(Duration::from_secs(1)).await;
        let ground = bot.operations().standing_context().await.unwrap();
        assert!(ground.on_ground && !ground.submerged);
        let player = bot.operations().player_state().await.unwrap();
        assert_eq!(player.game_mode, Some(operations::GameMode::Survival));
        assert!(
            player
                .local_player
                .health
                .as_ref()
                .is_some_and(|h| h.health > 0.0)
        );
        assert!(
            player.inventory.slots[36..45]
                .iter()
                .all(|v| *v == operations::InventorySlot::Empty)
        );
        assert!(
            bot.operations()
                .player_state()
                .await
                .unwrap()
                .interaction_loading
                .notification_dispatched()
        );
        bot.operations().look([-90.0, 3.0]).await.unwrap();
        if native_api {
            bot.operations().select_hotbar(0).await.unwrap();
        }
        bot.start_packet_trace(8_388_608).await.unwrap();
        viewer.start_packet_trace(8_388_608).await.unwrap();
        let start = Instant::now();
        let before = sample(&bot, target, start).await;
        let mut inputs = Vec::new();
        let mut observations = Vec::new();
        let intent = if native_api {
            let intent = bot
                .operations()
                .start_survival_mining(target, crate::BlockFace::West)
                .await
                .unwrap();
            inputs.push(json!({"action":0,"sequence":intent.start_sequence,"elapsed_ms":start.elapsed().as_millis(),"intent":intent}));
            Some(intent)
        } else {
            action(&bot, target, 0, 1, start, &mut inputs).await;
            None
        };
        let retirement_watch = if let Some(intent) = &intent {
            Some(
                bot.operations()
                    .prepare_survival_mining_retirement(intent, &viewer.operations())
                    .await
                    .unwrap(),
            )
        } else {
            None
        };
        tokio::time::sleep(Duration::from_millis(delay_ms)).await;
        if let Some(intent) = &intent {
            let sequence = bot
                .operations()
                .finish_survival_mining(intent)
                .await
                .unwrap();
            inputs.push(
                json!({"action":2,"sequence":sequence,"elapsed_ms":start.elapsed().as_millis()}),
            );
        } else {
            action(&bot, target, 2, 2, start, &mut inputs).await;
        }
        if abort {
            if let Some(intent) = &intent {
                let sequence = bot
                    .operations()
                    .abort_survival_mining(intent)
                    .await
                    .unwrap();
                inputs.push(json!({"action":1,"sequence":sequence,"elapsed_ms":start.elapsed().as_millis()}));
            } else {
                action(&bot, target, 1, 3, start, &mut inputs).await;
            }
        }
        let immediately = sample(&bot, target, start).await;
        if name != "normal_finish" {
            assert!(
                !air(&immediately),
                "trial completed before late-result observation"
            );
        }
        let pending_api_result = if let Some(intent) = &intent {
            let pending = bot
                .operations()
                .observe_survival_mining(intent)
                .await
                .unwrap();
            if name != "normal_finish" {
                assert!(matches!(
                    pending,
                    operations::MiningStatus::PendingAfterFinish { .. }
                ));
                assert!(bot.operations().select_hotbar(1).await.is_err());
            }
            Some(pending)
        } else {
            None
        };
        let external_input = if name == "external_air_immediate_replacement" {
            let requested_ms = start.elapsed().as_millis();
            phase("EXTERNAL_INPUT: console setblock 2 -59 0 minecraft:air then setblock 2 -59 0 minecraft:stone in one input batch; enter").await;
            assert!(
                start.elapsed() < Duration::from_secs(3),
                "external input was too late for the declared delayed-miner comparison"
            );
            Some(
                json!({"requested_ms":requested_ms,"console_completed_ms":start.elapsed().as_millis(),"operations":["setblock 2 -59 0 minecraft:air","setblock 2 -59 0 minecraft:stone"],"scope":"console-controlled external edits; exact commands/server log and received packet order retained; no claim that the bot necessarily receives intermediate air"}),
            )
        } else {
            None
        };
        if disconnect {
            bot.disconnect().await.unwrap();
        }
        for _ in 0..180 {
            tokio::time::sleep(Duration::from_millis(50)).await;
            let observed = sample(&viewer, target, start).await;
            let removed = air(&observed);
            observations.push(observed);
            if removed && !disconnect && external_input.is_none() {
                break;
            }
        }
        let after = observations.last().unwrap();
        if disconnect {
            assert!(!air(after), "old disconnected miner still completed later");
        } else if external_input.is_none() {
            assert!(
                air(after),
                "native mining did not complete within observation bound"
            );
        }
        let after_api_result = if let Some(intent) = &intent {
            if disconnect {
                let history = bot.operations().operation_history().await;
                assert!(history.connection_closed);
                assert!(history.mining.as_ref().unwrap().removal.is_none());
                json!({"closed_history":history})
            } else {
                let result = bot
                    .operations()
                    .wait_survival_mining(intent, Duration::from_secs(2))
                    .await
                    .unwrap();
                if external_input.is_none() {
                    assert!(matches!(
                        result,
                        operations::MiningStatus::ObservedRemoved { .. }
                    ));
                }
                assert!(bot.operations().select_hotbar(1).await.is_err());
                json!(result)
            }
        } else {
            Value::Null
        };
        let bot_trace = if disconnect {
            None
        } else {
            Some(bot.stop_packet_trace().await.unwrap())
        };
        let players = viewer.operations().visible_players().await.unwrap();
        if disconnect {
            assert!(!players.players.iter().any(|p| p.name == "NatMineBot"));
        }
        println!(
            "CASE_VERIFIED {name}: removed={} elapsed_ms={}",
            air(after),
            after["elapsed_ms"]
        );
        let recovery = if let Some(watch) = &retirement_watch {
            if !disconnect {
                bot.disconnect().await.unwrap();
            }
            let retired = bot
                .operations()
                .wait_survival_mining_retirement(
                    watch,
                    &viewer.operations(),
                    Duration::from_secs(3),
                )
                .await
                .unwrap();
            assert!(matches!(
                retired,
                operations::MiningRetirementStatus::Retired { .. }
            ));
            let expected = after["received"]["blocks"][0]["state"].clone();
            let expected: crate::NativeBlockState = serde_json::from_value(expected).unwrap();
            let recovered = bot
                .operations()
                .reconnect_survival_mining(
                    watch,
                    &viewer.operations(),
                    ConnectionConfig::offline(
                        crate::Server::new("127.0.0.1", port),
                        "NatMineBot",
                        MinecraftVersion::Java1_21_11,
                    ),
                    expected,
                )
                .await
                .unwrap();
            assert!(recovered.evidence.old_history.connection_closed);
            assert_ne!(
                recovered.evidence.connection_id,
                recovered.evidence.old_history.connection_id
            );
            assert!(recovered.evidence.interaction_ready);
            recovered.operations.select_hotbar(0).await.unwrap();
            assert!(
                bot.operations()
                    .reconnect_survival_mining(
                        watch,
                        &viewer.operations(),
                        ConnectionConfig::offline(
                            crate::Server::new("127.0.0.1", port),
                            "NatMineBot",
                            MinecraftVersion::Java1_21_11
                        ),
                        recovered.evidence.target.clone()
                    )
                    .await
                    .is_err(),
                "same retirement receipt authorized a second recovery login"
            );
            let followup = if disconnect {
                // No test-private PLAYER_LOADED, console edit or fixed login
                // sleep. Mine the retained stone through the fresh public API.
                let fresh = recovered.operations.bot.clone();
                fresh.start_packet_trace(8_388_608).await.unwrap();
                let begin = Instant::now();
                let loading = recovered
                    .operations
                    .player_state()
                    .await
                    .unwrap()
                    .interaction_loading;
                let connected_ms = fresh.session.started.elapsed().as_millis();
                recovered.operations.look([-90.0, 3.0]).await.unwrap();
                let new_intent = recovered
                    .operations
                    .start_survival_mining(target, crate::BlockFace::West)
                    .await
                    .unwrap();
                tokio::time::sleep(Duration::from_millis(new_intent.estimated_wait_ms)).await;
                recovered
                    .operations
                    .finish_survival_mining(&new_intent)
                    .await
                    .unwrap();
                let result = recovered
                    .operations
                    .wait_survival_mining(&new_intent, Duration::from_secs(2))
                    .await
                    .unwrap();
                assert!(matches!(
                    result,
                    operations::MiningStatus::ObservedRemoved { .. }
                ));
                let observed = timeout(Duration::from_secs(2), async {
                    loop {
                        let observed = sample(&viewer, target, begin).await;
                        if air(&observed) {
                            break observed;
                        }
                        tokio::time::sleep(Duration::from_millis(25)).await;
                    }
                })
                .await
                .unwrap();
                Some(
                    json!({"start_after_connection_ms":connected_ms,"loading":loading,"intent":new_intent,"result":result,"observer":observed,"trace":fresh.stop_packet_trace().await.unwrap(),"elapsed_ms":begin.elapsed().as_millis()}),
                )
            } else {
                None
            };
            let evidence = json!({"retired":retired,"recovery":recovered.evidence,"new_mutation":"ordinary selection after common loading; final disconnect case also mines the retained stone", "followup_mining":followup});
            bot = recovered.operations.bot.clone();
            Some(evidence)
        } else {
            None
        };
        let viewer_trace = viewer.stop_packet_trace().await.unwrap();
        cases.push(json!({"name":name,"ground":ground,"before":before,"inputs":inputs,"immediately":immediately,"observations":observations,"miner_trace":bot_trace,"viewer_trace":viewer_trace,"visible_players_after":players,"pending_api_result":pending_api_result,"after_api_result":after_api_result,"external_input":external_input,"recovery":recovery}));
    }
    if native_api {
        bot.disconnect().await.unwrap();
    }
    viewer.disconnect().await.unwrap();
    serde_json::to_writer(file,&json!({"minecraft_version":"Java 1.21.11","scope":if native_api {"isolated non-OP survival; public intent/result mining API; console fixture setup only"} else {"isolated non-OP survival; test-private native action packets; console fixture setup only"},"all_cases_executed":cases.len()==if native_api {4} else {3},"cases":cases})).unwrap();
}
