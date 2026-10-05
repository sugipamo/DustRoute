//! Public job stops after a normal mining recovery, using a fresh isolated
//! vanilla fixture. Console changes are external inputs, never executor hooks.
use super::*;
use crate::survival_error::SurvivalErrorCode;
use crate::survival_execution::{Continuation, ExecutionEvidence, ExecutionPhase};

#[derive(Clone, Copy, Debug)]
enum Change {
    Site,
    Materials,
    Position,
    Connection,
}
impl Change {
    fn from_environment() -> Self {
        match std::env::var("DUSTROUTE_CONTINUOUS_CHANGE")
            .unwrap()
            .as_str()
        {
            "site" => Self::Site,
            "materials" => Self::Materials,
            "position" => Self::Position,
            "connection" => Self::Connection,
            value => panic!("undeclared continuous-work fixture: {value}"),
        }
    }

    fn accepts(self, code: SurvivalErrorCode) -> bool {
        use SurvivalErrorCode::*;
        // The console input can arrive inside an already admitted operation.
        // Preserve its actual failure; do not assume an atomic step boundary.
        match self {
            Self::Site => matches!(code, SiteChanged | RecoverySiteChanged),
            Self::Materials => matches!(
                code,
                MaterialUnavailable | NativeRefused | PlacementUnconfirmed
            ),
            Self::Position => matches!(
                code,
                MovementNeedsInspection
                    | MovementPlanChanged
                    | BodyScopeChanged
                    | PlacementPlanChanged
                    | RecoveryPlanChanged
                    | RecoveryProvenanceMismatch
                    | MiningNeedsInspection
                    | NativeRefused
            ),
            Self::Connection => matches!(
                code,
                NativeRefused | MovementNeedsInspection | MovementMissing | MiningNeedsInspection
            ),
        }
    }
}

async fn console_input(marker: &str) {
    println!("{marker}");
    std::io::stdout().flush().unwrap();
    tokio::time::timeout(
        Duration::from_secs(30),
        tokio::task::spawn_blocking(|| {
            let mut line = String::new();
            assert!(std::io::stdin().read_line(&mut line).unwrap() > 0);
        }),
    )
    .await
    .expect("fixture controller response")
    .unwrap();
}

async fn get(client: &Client, id: &Value, detailed: bool) -> Value {
    call(
        client,
        "survival_construction",
        json!({"action":"get","job_id":id,"include_record":detailed}),
    )
    .await
}

#[tokio::test]
#[ignore = "explicit fresh non-OP localhost fixture; bounded mid-work console change after first mining recovery"]
async fn native_public_continuous_change() {
    assert_eq!(
        std::env::var("DUSTROUTE_SURVIVAL_ROOF_PORT").unwrap(),
        "25572"
    );
    assert_eq!(
        std::env::var("DUSTROUTE_SURVIVAL_ROOF_EXECUTE").unwrap(),
        "1"
    );
    let change = Change::from_environment();
    let output = std::path::PathBuf::from(std::env::var("DUSTROUTE_SURVIVAL_ROOF_OUTPUT").unwrap());
    let root = std::path::PathBuf::from(format!("{}.state", output.display()));
    assert!(!output.exists() && !root.exists());
    let mut service = DustRouteMcp::connect_voxrig(
        McpConfig::new("127.0.0.1:25572", "Tester").unwrap(),
        McpPolicy {
            read_only: false,
            ..Default::default()
        },
        "NatMineBot",
    )
    .await
    .unwrap();
    assert!(service.survival_observer.is_none());
    service.state_store = PlanStateStore::new(root, 3600);
    let native = service.bridge.survival_bridge().unwrap();
    let observer = comparison_observer().await;
    console_input("FIXTURE roof: continuous-work trial; enter").await;
    let (client, server) = serve(service.clone()).await;
    let spec = json!(fixture::design().unwrap().specification);
    let generated = generate(&client, spec.clone()).await;
    adopt(&client, &generated).await;
    let plan = call(
        &client,
        "survival_construction",
        plan_request(spec, generated["request"]["candidate_state"]["id"].clone()),
    )
    .await;
    assert_eq!(plan["ok"], true, "{plan}");
    let id = plan["job_id"].clone();
    let steps = plan["preview"]["plan"]["steps"].as_array().unwrap();
    let first_removal = steps
        .iter()
        .position(|s| s["kind"] == "remove_temporary")
        .unwrap()
        + 1;
    let total = steps.len();
    let start = call(
        &client,
        "survival_construction",
        json!({"action":"start","job_id":id,"confirmed":true}),
    )
    .await;
    assert_eq!(start["ok"], true, "{start}");
    let deadline = Instant::now() + Duration::from_secs(300);
    let at_request = loop {
        let status = get(&client, &id, false).await;
        assert!(
            !matches!(
                status["status"]["state"].as_str(),
                Some("completed" | "needs_inspection" | "admission_refused")
            ),
            "{status}"
        );
        if status["completed_steps"].as_u64().unwrap_or(0) >= first_removal as u64 {
            break status;
        }
        assert!(
            Instant::now() < deadline,
            "first normal mining recovery timed out"
        );
        tokio::time::sleep(Duration::from_millis(100)).await;
    };
    let before_change = get(&client, &id, true).await;
    assert!(
        before_change["diagnosis"]["record"]["reconnects"]
            .as_u64()
            .unwrap()
            >= 1
    );
    // The background executor remains running during this handshake. The
    // controller records input times; request progress is not an application ack.
    console_input(&format!("INJECT continuous {change:?}:")).await;
    let final_result = loop {
        let status = get(&client, &id, false).await;
        match status["status"]["state"].as_str() {
            Some("needs_inspection") => break get(&client, &id, true).await,
            Some("completed" | "admission_refused" | "cancelled_needs_inspection") => {
                panic!("unexpected continuous-work outcome: {status}");
            }
            _ => {}
        }
        assert!(
            Instant::now() < deadline,
            "changed prerequisites did not stop the job"
        );
        tokio::time::sleep(Duration::from_millis(100)).await;
    };
    let job_id: uuid::Uuid = serde_json::from_value(id.clone()).unwrap();
    let directory = service
        .state_store
        .survival_job_root()
        .join(job_id.to_string());
    let status: JobStatus = load(&directory.join("status.store")).unwrap();
    let JobStatus::NeedsInspection {
        reason:
            InspectionReason::Execution {
                error,
                completed_steps,
            },
    } = status
    else {
        panic!("execution failure must retain its typed reason");
    };
    assert!(
        change.accepts(error.code),
        "unexpected {change:?} cause: {error}"
    );
    if error.code == SurvivalErrorCode::NativeRefused {
        assert!(error.native_error_kind.is_some());
    }
    assert!(completed_steps >= first_removal && completed_steps < total);
    assert_eq!(final_result["completed_steps"], completed_steps);
    assert_eq!(final_result["next_action"], "inspect_execution");
    assert_eq!(final_result["execution_authority_restored"], false);
    let record_path = directory.join("execution/record.store");
    let stopped_bytes = std::fs::read(&record_path).unwrap();
    let diagnosis = crate::survival_execution::diagnose(&directory.join("execution")).unwrap();
    assert_eq!(diagnosis.record.completed_steps, completed_steps);
    assert_eq!(diagnosis.record.continuation, Continuation::NeedsInspection);
    let stopped = diagnosis.record.events.last().unwrap();
    assert_eq!(stopped.phase, ExecutionPhase::Stopped);
    let ExecutionEvidence::Stopped {
        error: saved_error, ..
    } = &stopped.evidence
    else {
        panic!("stopped journal must retain the execution failure");
    };
    assert_eq!(saved_error.code, error.code);
    assert_eq!(saved_error.native_error_kind, error.native_error_kind);
    assert!(
        native.lease_survival().is_err(),
        "uncertain source must stay quarantined"
    );
    let observation = observer
        .observe_region(region(fixture::scope().observed))
        .await
        .unwrap();
    if matches!(change, Change::Site) {
        assert!(observation.blocks.iter().any(|b| b.position == [8, -60, 8]
            && b.state.as_ref().is_some_and(|s| s.name == "minecraft:dirt")));
    }
    let replay = call(
        &client,
        "survival_construction",
        json!({"action":"start","job_id":id,"confirmed":true}),
    )
    .await;
    assert_eq!(replay["error"]["code"], "job_already_started");
    assert_eq!(replay["next_action"], "inspect_job");
    // Check scheduler/journal quiescence, not that hidden server effects are
    // impossible. An uncertain already-dispatched operation is not retried.
    tokio::time::sleep(Duration::from_secs(1)).await;
    let after_replay = get(&client, &id, false).await;
    assert_eq!(after_replay["status"], final_result["status"]);
    assert_eq!(std::fs::read(&record_path).unwrap(), stopped_bytes);
    assert!(native.lease_survival().is_err());
    let evidence = json!({"change":format!("{change:?}"),"plan":plan,"start":start,
        "at_injection_request":at_request,"before_change":before_change,
        "final":final_result,"after_replay":after_replay,"replay":replay,
        "comparison_observation_after_stop":observation,
        "production_observer_configured":false,"source_quarantined":true,
        "hidden_operation_retirement_proven":false});
    let bytes = serde_json::to_vec_pretty(&evidence).unwrap();
    assert!(
        bytes.len() <= 64 * 1024 * 1024,
        "bounded explicit trial evidence"
    );
    std::fs::OpenOptions::new()
        .create_new(true)
        .write(true)
        .open(&output)
        .unwrap()
        .write_all(&bytes)
        .unwrap();
    observer.disconnect().await.unwrap();
    stop(client, server).await;
    // No handback/repair is manufactured for a quarantined source. The fixture
    // controller stops this isolated server normally after the test process exits.
}
