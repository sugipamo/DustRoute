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
    let bot = connect("NatMineBot", port).await;
    let viewer = connect("NatMineView", port).await;
    let target = [2, -59, 0];
    let mut cases = Vec::new();
    for (name, delay_ms, abort, disconnect) in [
        ("normal_finish", 1100, false, false),
        ("early_finish_abort", 50, true, false),
        ("early_finish_disconnect", 50, false, true),
    ] {
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
        bot.start_packet_trace(8_388_608).await.unwrap();
        viewer.start_packet_trace(8_388_608).await.unwrap();
        let start = Instant::now();
        let before = sample(&bot, target, start).await;
        let mut inputs = Vec::new();
        let mut observations = Vec::new();
        action(&bot, target, 0, 1, start, &mut inputs).await;
        tokio::time::sleep(Duration::from_millis(delay_ms)).await;
        action(&bot, target, 2, 2, start, &mut inputs).await;
        if abort {
            action(&bot, target, 1, 3, start, &mut inputs).await;
        }
        let immediately = sample(&bot, target, start).await;
        if name != "normal_finish" {
            assert!(
                !air(&immediately),
                "trial completed before late-result observation"
            );
        }
        if disconnect {
            bot.disconnect().await.unwrap();
        }
        for _ in 0..180 {
            tokio::time::sleep(Duration::from_millis(50)).await;
            let observed = sample(&viewer, target, start).await;
            let removed = air(&observed);
            observations.push(observed);
            if removed && !disconnect {
                break;
            }
        }
        let after = observations.last().unwrap();
        if disconnect {
            assert!(!air(after), "old disconnected miner still completed later");
        } else {
            assert!(
                air(after),
                "native mining did not complete within observation bound"
            );
        }
        let bot_trace = if disconnect {
            None
        } else {
            Some(bot.stop_packet_trace().await.unwrap())
        };
        let viewer_trace = viewer.stop_packet_trace().await.unwrap();
        let players = viewer.operations().visible_players().await.unwrap();
        if disconnect {
            assert!(!players.players.iter().any(|p| p.name == "NatMineBot"));
        }
        cases.push(json!({"name":name,"ground":ground,"before":before,"inputs":inputs,"immediately":immediately,"observations":observations,"miner_trace":bot_trace,"viewer_trace":viewer_trace,"visible_players_after":players}));
        println!(
            "CASE_VERIFIED {name}: removed={} elapsed_ms={}",
            air(after),
            after["elapsed_ms"]
        );
    }
    viewer.disconnect().await.unwrap();
    serde_json::to_writer(file,&json!({"minecraft_version":"Java 1.21.11","scope":"isolated non-OP survival; test-private native action packets; console fixture setup only","cases":cases})).unwrap();
}
