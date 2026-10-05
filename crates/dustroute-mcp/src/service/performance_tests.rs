//! Bounded offline measurement of production handlers. The bridge fixture
//! includes air like the native client, but its transport and waits are mocks.
use super::test_support::*;
use super::*;
use crate::performance::measure;

fn decoded(reply: &CallToolResult) -> Value {
    let value = decode_reply(reply).unwrap();
    assert_eq!(value["ok"], true, "{value}");
    value
}

fn emit(report: &crate::performance::Measurement) {
    println!("PERFORMANCE {report}");
}

#[tokio::test]
async fn shared_circuit_contents_keep_owner_and_expiry_authorization_separate() {
    let service = DustRouteMcp::with_test_transport("127.0.0.1:1");
    let snapshot: dustroute_translate::snapshot::MinecraftSnapshot =
        serde_json::from_value(super::electrical_edit_tests::machine()).unwrap();
    let shared = service.bridge.share_snapshot(snapshot.clone()).unwrap();
    let duplicate = service.bridge.share_snapshot(snapshot).unwrap();
    assert!(shared.shares_storage_with(&duplicate));
    let circuit = |player: &str, snapshot: crate::snapshot_content::SharedSnapshot, expires_at| {
        StoredCircuit {
            player: player.into(),
            dimension: "minecraft:overworld".into(),
            bounds: dustroute_translate::world_reverse::RegionBounds::new(
                snapshot.min,
                snapshot.max,
            ),
            target: None,
            snapshot,
            expansion: ExpansionEvidence::Unspecified {},
            complete: true,
            expires_at,
        }
    };
    let alice = service
        .store_circuit(circuit(
            "Alice",
            shared.clone(),
            Instant::now() + Duration::from_secs(300),
        ))
        .await;
    let bob = service
        .store_circuit(circuit(
            "Bob",
            duplicate,
            Instant::now() + Duration::from_secs(300),
        ))
        .await;
    assert_ne!(alice, bob);
    assert!(
        service
            .load_circuit(&alice.to_string(), "Bob")
            .await
            .is_err()
    );
    let alice = service
        .load_circuit(&alice.to_string(), "Alice")
        .await
        .unwrap()
        .1;
    let bob = service
        .load_circuit(&bob.to_string(), "Bob")
        .await
        .unwrap()
        .1;
    assert!(alice.snapshot.shares_storage_with(&bob.snapshot));
    let expired = service
        .store_circuit(circuit("Alice", shared, Instant::now()))
        .await;
    assert!(
        service
            .load_circuit(&expired.to_string(), "Alice")
            .await
            .is_err()
    );
    assert!(alice.snapshot.shares_storage_with(&bob.snapshot));
}

#[tokio::test]
#[ignore = "run alone in a fresh process to separate cold Law compilation from warm execution"]
async fn profile_law_initialization() {
    let initializers: [(&str, fn()); 6] = [
        ("motion_laws", || {
            std::hint::black_box(dustroute_translate::world::piston_motion_law::builtin_laws());
        }),
        ("electrical_laws", || {
            std::hint::black_box(dustroute_translate::world::piston_electrical_law::builtin_laws());
        }),
        ("device_programs", || {
            std::hint::black_box(dustroute_translate::world::device_program::programs());
        }),
        ("spatial_laws", || {
            std::hint::black_box(dustroute_translate::world::spatial::builtin_spatial_laws());
        }),
        ("dust_law", || {
            std::hint::black_box(dustroute_translate::world::dust_law::builtin_dust_law());
        }),
        ("comparator_signal", || {
            std::hint::black_box(
                dustroute_translate::world::device_callback_law::comparator_signal(1, 1, false),
            );
        }),
    ];
    for (name, initialize) in initializers {
        for pass in ["cold", "warm"] {
            let (_, report) = measure(&format!("{pass}_{name}"), async {
                initialize();
            })
            .await;
            emit(&report);
        }
    }
}

#[tokio::test]
#[ignore = "opt-in wall-time measurements; no Minecraft connection or writes"]
async fn profile_observation_and_edit_phases() {
    for sample in 1..=3 {
        // Use the same filesystem as normal local state, rather than /tmp,
        // whose fsync behavior can differ (for example, on tmpfs).
        let temporary_name = temporary();
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../.local")
            .join(temporary_name.file_name().unwrap());
        let (fake, address, bridge) = start_construction_bridge(
            crate::service::test_support::DurableRegistry::Edits(root.join("world-edits")),
        )
        .await;
        let original = super::electrical_edit_tests::machine();
        {
            let mut state = fake.lock().unwrap();
            state.snapshot = Some(original.clone());
            state.gaze_target = Some(json!({"x":101,"y":101,"z":105}));
            state.resize_scan = true;
            state.include_air = true;
        }
        let mut service = DustRouteMcp::with_test_transport_and_player(
            &address,
            McpPolicy {
                read_only: false,
                ..Default::default()
            },
            "Tester",
        );
        service.state_store = PlanStateStore::new(root.clone(), 3600);
        let (text, report) = measure(
            &format!("adaptive_interior_{sample}"),
            service.get_world(Parameters(
                serde_json::from_value(json!({"max_components":64})).unwrap(),
            )),
        )
        .await;
        let observation = decoded(&text);
        assert_eq!(report.phases[&crate::performance::Phase::Scan].cells, 4221);
        assert_eq!(observation["expansion"]["scanned_tiles"], 1);
        emit(&report);

        // Same machine, moved across all three tile boundaries. No blocks are
        // added; this isolates tile alignment from circuit complexity.
        {
            let mut boundary = original.clone();
            for point in boundary["blocks"]
                .as_array_mut()
                .unwrap()
                .iter_mut()
                .map(|b| &mut b["pos"])
            {
                point["x"] = json!(point["x"].as_i64().unwrap() - 86);
                point["y"] = json!(point["y"].as_i64().unwrap() - 86);
                point["z"] = json!(point["z"].as_i64().unwrap() - 90);
            }
            let mut state = fake.lock().unwrap();
            state.snapshot = Some(boundary);
            state.gaze_target = Some(json!({"x":15,"y":15,"z":15}));
        }
        let (text, report) = measure(
            &format!("adaptive_boundary_{sample}"),
            service.get_world(Parameters(
                serde_json::from_value(json!({"max_components":64})).unwrap(),
            )),
        )
        .await;
        let observation = decoded(&text);
        assert_eq!(observation["expansion"]["scanned_tiles"], 8);
        assert_eq!(report.phases[&crate::performance::Phase::Scan].cells, 32893);
        emit(&report);
        {
            let mut state = fake.lock().unwrap();
            state.snapshot = Some(original.clone());
            state.gaze_target = Some(json!({"x":101,"y":101,"z":105}));
        }
        let (text, report) = measure(
            &format!("explicit_region_{sample}"),
            service.get_world(Parameters(
                serde_json::from_value(
                    json!({"region":{"min":original["min"],"max":original["max"]}}),
                )
                .unwrap(),
            )),
        )
        .await;
        let capture = decoded(&text);
        assert_eq!(report.phases[&crate::performance::Phase::Scan].calls, 1);
        assert_eq!(report.phases[&crate::performance::Phase::Scan].cells, 336);
        emit(&report);

        let (text, report) = measure(
            &format!("revision_{sample}"),
            service.test_circuit_change(Parameters(
                serde_json::from_value(json!({"circuit_id":capture["circuit_id"],"changes":[
                    {"position":{"x":101,"y":101,"z":106},"block":"minecraft:slime_block"},
                    {"position":{"x":101,"y":101,"z":107},"block":"minecraft:glass"}
                ]}))
                .unwrap(),
            )),
        )
        .await;
        let revision = decoded(&text);
        assert_eq!(
            revision["validation"]["electrical_modification"]["status"],
            "passed"
        );
        assert_eq!(
            report.phases[&crate::performance::Phase::ModelProof].calls,
            1
        );
        emit(&report);

        let (text, report) = measure(
            &format!("plan_{sample}"),
            service.new_placement(Parameters(
                serde_json::from_value(json!({"revision_id":revision["revision_id"]})).unwrap(),
            )),
        )
        .await;
        let plan = decoded(&text);
        assert_eq!(
            report.phases[&crate::performance::Phase::ModelProof].calls,
            1
        );
        assert_eq!(
            report.phases[&crate::performance::Phase::Wait].requested_ticks,
            20
        );
        assert_eq!(plan["steps"].as_array().unwrap().len(), 2);
        emit(&report);

        let (text, report) = measure(
            &format!("preview_{sample}"),
            service.show_operation(Parameters(
                serde_json::from_value(json!({"operation_id":plan["operation_id"]})).unwrap(),
            )),
        )
        .await;
        decoded(&text);
        assert_eq!(
            report.phases[&crate::performance::Phase::Wait].requested_ticks,
            20
        );
        emit(&report);
        fake.lock().unwrap().steps = plan["steps"].as_array().unwrap().iter().cloned().collect();
        let (text, report) = measure(
            &format!("apply_mock_only_{sample}"),
            service.invoke_operation(Parameters(
                serde_json::from_value(json!({"operation_id":plan["operation_id"],"confirm":true}))
                    .unwrap(),
            )),
        )
        .await;
        decoded(&text);
        assert_eq!(
            report.phases[&crate::performance::Phase::ModelProof].calls,
            1
        );
        assert_eq!(
            report.phases[&crate::performance::Phase::Checkpoint].calls,
            3
        );
        assert_eq!(report.phases[&crate::performance::Phase::Write].commands, 2);
        assert_eq!(report.phases[&crate::performance::Phase::Write].calls, 1);
        assert_eq!(report.phases[&crate::performance::Phase::Scan].calls, 4);
        assert_eq!(
            report.phases[&crate::performance::Phase::Wait].requested_ticks,
            24
        );
        emit(&report);
        assert_eq!(fake.lock().unwrap().writes, 2);
        bridge.abort();
        std::fs::remove_dir_all(root).unwrap();
    }
}
