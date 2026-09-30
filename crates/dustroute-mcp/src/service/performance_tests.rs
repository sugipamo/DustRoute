//! Bounded offline measurement of production handlers. The bridge fixture
//! includes air like the native client, but its transport and waits are mocks.
use super::test_support::*;
use super::*;
use crate::performance::measure;

fn decoded(text: &str) -> Value {
    let value: Value = serde_json::from_str(text).unwrap();
    assert_eq!(value["ok"], true, "{value}");
    value
}

fn emit(report: &crate::performance::Measurement) {
    println!("PERFORMANCE {}", serde_json::to_string(report).unwrap());
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
        let (fake, address, bridge) = start_construction_bridge(root.join("world-edits")).await;
        let original = super::electrical_edit_tests::machine();
        {
            let mut state = fake.lock().unwrap();
            state.snapshot = Some(original.clone());
            state.gaze_target = Some(json!({"x":101,"y":101,"z":105}));
            state.resize_scan = true;
            state.include_air = true;
        }
        let mut service = DustRouteMcp::with_policy_and_player(
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
        assert_eq!(report.phases["scan"].cells, 4221);
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
        assert_eq!(report.phases["scan"].cells, 32893);
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
        assert_eq!(report.phases["scan"].calls, 1);
        assert_eq!(report.phases["scan"].cells, 336);
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
        assert_eq!(report.phases["model_proof"].calls, 1);
        emit(&report);

        let (text, report) = measure(
            &format!("plan_{sample}"),
            service.new_placement(Parameters(
                serde_json::from_value(json!({"revision_id":revision["revision_id"]})).unwrap(),
            )),
        )
        .await;
        let plan = decoded(&text);
        assert_eq!(report.phases["model_proof"].calls, 1);
        assert_eq!(report.phases["wait"].requested_ticks, 20);
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
        assert_eq!(report.phases["wait"].requested_ticks, 20);
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
        assert_eq!(report.phases["model_proof"].calls, 1);
        assert_eq!(report.phases["checkpoint"].calls, 6);
        assert_eq!(report.phases["write"].commands, 2);
        assert_eq!(report.phases["scan"].calls, 6);
        emit(&report);
        assert_eq!(fake.lock().unwrap().writes, 2);
        bridge.abort();
        std::fs::remove_dir_all(root).unwrap();
    }
}
