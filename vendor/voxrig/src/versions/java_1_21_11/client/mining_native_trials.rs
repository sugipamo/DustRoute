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
        ("early_finish_disconnect", 50, false, true),
        ("external_air_immediate_replacement", 50, true, false),
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
        bot.session
            .send(ids::play_serverbound::PLAYER_LOADED, &[])
            .await
            .unwrap();
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
            if recovered.evidence.interaction_ready {
                recovered.operations.select_hotbar(0).await.unwrap();
            } else {
                assert!(recovered.operations.select_hotbar(0).await.is_err());
            }
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
            let evidence = json!({"retired":retired,"recovery":recovered.evidence,"new_mutation":"refused until native interaction loading is validated"});
            bot = recovered.operations.bot.clone();
            Some(evidence)
        } else {
            None
        };
        let viewer_trace = viewer.stop_packet_trace().await.unwrap();
        cases.push(json!({"name":name,"ground":ground,"before":before,"inputs":inputs,"immediately":immediately,"observations":observations,"miner_trace":bot_trace,"viewer_trace":viewer_trace,"visible_players_after":players,"pending_api_result":pending_api_result,"after_api_result":after_api_result,"external_input":external_input,"recovery":recovery}));
        if native_api
            && bot
                .operations()
                .operation_history()
                .await
                .recovery_loading_pending
        {
            println!(
                "CONTINUATION_BLOCKED: native interaction loading unvalidated; remaining cases not executed"
            );
            break;
        }
    }
    if native_api {
        bot.disconnect().await.unwrap();
    }
    viewer.disconnect().await.unwrap();
    serde_json::to_writer(file,&json!({"minecraft_version":"Java 1.21.11","scope":if native_api {"isolated non-OP survival; public intent/result mining API; console fixture setup only"} else {"isolated non-OP survival; test-private native action packets; console fixture setup only"},"all_cases_executed":cases.len()==if native_api {4} else {3},"cases":cases})).unwrap();
}
