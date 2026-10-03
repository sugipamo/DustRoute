use super::*;
use dustroute_physical::Pos;
use dustroute_translate::{
    piston_construction::ElectricalModification, snapshot::MinecraftSnapshot,
};
use serde_json::{Value, json};
use std::sync::{
    Arc,
    atomic::{AtomicUsize, Ordering},
};
use tokio::{
    io::{AsyncBufReadExt, AsyncWriteExt, BufReader},
    net::TcpListener,
};

#[tokio::test]
async fn checkpoint_failures_distinguish_no_submission_from_verified_unsaved_world() {
    for fail_on_verified in [false, true] {
        let before = MinecraftSnapshot {
            min: Pos::new(0, 80, 0),
            max: Pos::new(2, 82, 2),
            blocks: vec![],
        };
        let mut after = before.clone();
        after
            .blocks
            .push(dustroute_translate::snapshot::MinecraftSnapshotBlock {
                pos: Pos::new(1, 81, 1),
                name: "minecraft:stone".into(),
                properties: Default::default(),
            });
        let proof = ElectricalModification::new(&before, &after, Default::default()).unwrap();
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let bridge = BotBridge::with_test_transport(listener.local_addr().unwrap().to_string());
        let writes = Arc::new(AtomicUsize::new(0));
        let count = writes.clone();
        let initial = before.clone();
        let transport = tokio::spawn(async move {
            loop {
                let (mut stream, _) = listener.accept().await.unwrap();
                let mut line = String::new();
                BufReader::new(&mut stream)
                    .read_line(&mut line)
                    .await
                    .unwrap();
                let req: Value = serde_json::from_str(&line).unwrap();
                let result = match req["method"].as_str().unwrap() {
                    "status" => {
                        json!({"connected":true,"username":"bot","host":"localhost","port":25565,"version":"1.21.11","dimension":"minecraft:overworld","enabled_features":["minecraft:vanilla"]})
                    }
                    "scan_region" => crate::bridge::test_readback_response(
                        &req,
                        crate::bridge::test_scan_world(
                            &req,
                            json!(if count.load(Ordering::SeqCst) == 0 {
                                &initial
                            } else {
                                &after
                            }),
                        ),
                    ),
                    "submit_command_batch" => {
                        count.fetch_add(1, Ordering::SeqCst);
                        json!({"protocol":crate::bridge_protocol::MUTATION_PROTOCOL,"submitted_changes":1})
                    }
                    "wait_ticks" => json!({"waited_ticks":1}),
                    method => panic!("unexpected method {method}"),
                };
                stream
                    .write_all(format!("{}\n", json!({"id":req["id"],"result":result})).as_bytes())
                    .await
                    .unwrap();
            }
        });
        let target = TargetServer {
            host: "localhost".into(),
            port: 25565,
            version: "1.21.11".into(),
            dimension: "minecraft:overworld".into(),
            enabled_features: vec!["minecraft:vanilla".into()],
        };
        let policy = McpPolicy {
            read_only: false,
            ..Default::default()
        };
        let executor = ConstructionExecutor {
            bridge: &bridge,
            policy: &policy,
            target: &target,
        };
        let report = executor
            .execute(&before, proof.steps(false), |stage| {
                if matches!(stage, StageProgress::Verified(_)) == fail_on_verified
                    && matches!(
                        stage,
                        StageProgress::Verified(_) | StageProgress::WriteIntent(_)
                    )
                {
                    Err(FailureCause::new(
                        CauseKind::Persistence,
                        "checkpoint unavailable",
                    ))
                } else {
                    Ok(())
                }
            })
            .await
            .unwrap_err();
        assert_eq!(report.progress.persistence, PersistenceOutcome::Uncertain);
        assert_eq!(report.progress.durable_verified_steps, Some(0));
        if fail_on_verified {
            assert_eq!(writes.load(Ordering::SeqCst), 1);
            assert_eq!(report.progress.world, WorldOutcome::Verified);
            assert_eq!(report.progress.verified_steps, 1);
            assert_eq!(report.primary.phase, Some(FailurePhase::CheckpointSave));
        } else {
            assert_eq!(writes.load(Ordering::SeqCst), 0);
            assert_eq!(report.progress.world, WorldOutcome::NotAttempted);
            assert_eq!(report.primary.phase, Some(FailurePhase::IntentSave));
        }
        transport.abort();
    }
}
