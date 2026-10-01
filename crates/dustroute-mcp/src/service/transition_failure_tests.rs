//! Offline transport failures exercise cleanup and restoration, not Minecraft physics.
use super::*;
use dustroute_translate::world_reverse::RegionBounds;
use std::sync::{
    Arc,
    atomic::{AtomicUsize, Ordering},
};
use tokio::{
    io::{AsyncBufReadExt, AsyncWriteExt, BufReader},
    net::TcpListener,
};

#[tokio::test]
async fn transition_retains_original_failure_and_cleanup_errors_without_replay() {
    for activation_fails in [false, true] {
        let lever = Pos::new(0, 81, 0);
        let snapshot:dustroute_translate::snapshot::MinecraftSnapshot=serde_json::from_value(json!({
            "min":{"x":-1,"y":80,"z":-1},"max":{"x":1,"y":82,"z":1},
            "blocks":[{"pos":{"x":0,"y":80,"z":0},"name":"minecraft:stone","properties":{}},
                {"pos":lever,"name":"minecraft:lever","properties":{"face":"floor","facing":"north","powered":"false"}}]})).unwrap();
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap().to_string();
        let activations = Arc::new(AtomicUsize::new(0));
        let count = activations.clone();
        let initial = snapshot.clone();
        let transport = tokio::spawn(async move {
            let mut reads = 0;
            let mut scans = 0;
            let mut waits = 0;
            loop {
                let (mut stream, _) = listener.accept().await.unwrap();
                let mut line = String::new();
                BufReader::new(&mut stream)
                    .read_line(&mut line)
                    .await
                    .unwrap();
                let req: Value = serde_json::from_str(&line).unwrap();
                let result: Result<Value, &str> = match req["method"].as_str().unwrap() {
                    "get_block" => {
                        reads += 1;
                        if reads == 1 {
                            Ok(crate::bridge::test_readback_response(
                                &req,
                                json!({"pos":lever,"name":"minecraft:lever","properties":{"face":"floor","facing":"north","powered":"false"}}),
                            ))
                        } else {
                            Err("restored lever read unavailable")
                        }
                    }
                    "scan_region" => {
                        scans += 1;
                        if scans == 1 {
                            Ok(crate::bridge::test_readback_response(
                                &req,
                                crate::bridge::test_scan_world(&req, json!(initial)),
                            ))
                        } else {
                            Err("restored region read unavailable")
                        }
                    }
                    "approach_lever" => Ok(json!({"pos":lever,"moved":false,"distance":1.0})),
                    "start_update_recording" => {
                        Ok(json!({"recording_id":"test","started_game_tick":42}))
                    }
                    "activate_lever" => {
                        count.fetch_add(1, Ordering::SeqCst);
                        if activation_fails {
                            Err("activation reply unavailable")
                        } else {
                            Ok(json!({"pos":lever,"before_powered":false,"after_powered":true}))
                        }
                    }
                    "wait_ticks" => {
                        waits += 1;
                        if waits == 1 {
                            Ok(json!({"waited_ticks":1}))
                        } else {
                            Err("restore wait unavailable")
                        }
                    }
                    "stop_update_recording" => Err("recording unavailable"),
                    method => panic!("unexpected method {method}"),
                };
                let response = match result {
                    Ok(result) => json!({"id":req["id"],"result":result}),
                    Err(error) => json!({"id":req["id"],"error":error}),
                };
                stream
                    .write_all(format!("{response}\n").as_bytes())
                    .await
                    .unwrap();
            }
        });
        let service = DustRouteMcp::with_policy_and_player(
            address,
            McpPolicy {
                read_only: false,
                ..Default::default()
            },
            "Tester",
        );
        let id = uuid::Uuid::new_v4();
        service
            .plans
            .table::<StoredTransitionPlan>()
            .lock()
            .await
            .insert(
                id,
                StoredTransitionPlan {
                    player: "Tester".into(),
                    dimension: "minecraft:overworld".into(),
                    bounds: RegionBounds::new(snapshot.min, snapshot.max),
                    lever,
                    original_powered: false,
                    initial_snapshot: snapshot,
                    observation_ticks: 1,
                    max_events: 16,
                    safety: TransitionSafetyAssessment {
                        safety: TransitionSafety::Ready,
                        reasons: vec![],
                    },
                    lifecycle: InvocationState::Previewed,
                },
            );
        let result: Value = serde_json::from_str(
            &service
                .invoke_transition_test(Parameters(RunTransitionParams {
                    operation_id: id.to_string(),
                    confirm: true,
                    contracts: None,
                }))
                .await,
        )
        .unwrap();
        assert_eq!(result["ok"], false, "{result}");
        assert_eq!(result["failure"]["progress"]["world"], "unknown");
        let secondary = result["failure"]["secondary"].as_array().unwrap();
        if activation_fails {
            assert_eq!(result["failure"]["primary"]["phase"], "submission");
            assert_eq!(secondary.len(), 1);
            assert!(
                secondary[0]["message"]
                    .as_str()
                    .unwrap()
                    .contains("recording unavailable")
            );
        } else {
            assert_eq!(result["failure"]["primary"]["phase"], "after_readback");
            assert!(
                result["error"]
                    .as_str()
                    .unwrap()
                    .contains("recording unavailable")
            );
            assert_eq!(secondary.len(), 3, "{result}");
            for (error, message) in secondary.iter().zip([
                "restore wait unavailable",
                "restored lever read unavailable",
                "restored region read unavailable",
            ]) {
                assert_eq!(error["phase"], "restore");
                assert!(error["message"].as_str().unwrap().contains(message));
            }
        }
        let before = activations.load(Ordering::SeqCst);
        let replay: Value = serde_json::from_str(
            &service
                .invoke_transition_test(Parameters(RunTransitionParams {
                    operation_id: id.to_string(),
                    confirm: true,
                    contracts: None,
                }))
                .await,
        )
        .unwrap();
        assert_eq!(replay["ok"], false);
        assert_eq!(activations.load(Ordering::SeqCst), before);
        assert_eq!(
            service.operations.get(id).await.unwrap().status,
            OperationStatus::Failed
        );
        transport.abort();
    }
}
