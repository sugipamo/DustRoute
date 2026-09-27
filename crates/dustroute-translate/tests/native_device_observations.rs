//! Local callback law checks against directly instrumented target decisions.
//! Delivery conformance is checked separately against retained runtime traces.
use dustroute_minecraft::device_callback_law::{ObserverCallback, builtin_laws};
use serde_json::Value;

#[test]
fn recorded_shape_admission_and_pulse_writes_agree_with_native_observer_law() {
    let evidence: Value = serde_json::from_str(include_str!(
        "fixtures/reference-3x3-observer-probes-b-v1.json"
    ))
    .unwrap();
    let events = evidence["events"].as_array().unwrap();
    let commits = evidence["state_commits"].as_array().unwrap();
    let mut shapes = 0;
    let mut ticks = 0;
    let mut ready_not_queued = 0;
    for (index, event) in events.iter().enumerate() {
        let powered = event["state"].as_str().unwrap().contains("powered=true");
        let queued = event["queued"].as_bool().unwrap();
        match event["event"].as_str().unwrap() {
            "shape" => {
                shapes += 1;
                let state = event["argument_state"].as_str().unwrap();
                let powered = state.contains("powered=true");
                let front = state.contains(&format!("facing={}", event["side"].as_str().unwrap()));
                let effect =
                    builtin_laws().observer(ObserverCallback::Shape, powered, queued, front);
                if front && !powered {
                    assert_eq!(events[index + 1]["event"], "schedule_before");
                    assert_eq!(events[index + 2]["event"], "schedule_after");
                    assert_eq!(events[index + 1]["position"], event["position"]);
                    assert_eq!(events[index + 1]["queued"], queued);
                    assert_eq!(events[index + 2]["queued"], true);
                    assert_eq!(effect.delay, if queued { 0 } else { 2 });
                } else {
                    assert_eq!(effect.delay, 0);
                    assert_ne!(
                        events.get(index + 1).map(|e| &e["event"]),
                        Some(&Value::from("schedule_before"))
                    );
                }
                if !queued && event["ticking"] == true {
                    ready_not_queued += 1;
                }
            }
            "tick" => {
                ticks += 1;
                assert_eq!(
                    event["ticking"], false,
                    "executing tick has left ready batch"
                );
                let effect =
                    builtin_laws().observer(ObserverCallback::Tick, powered, queued, false);
                let write = commits
                    .iter()
                    .find(|c| {
                        let p = c["position"].as_array().unwrap();
                        c["tick"] == event["tick"]
                            && event["position"]
                                == format!("BlockPos{{x={}, y={}, z={}}}", p[0], p[1], p[2])
                    })
                    .expect("recorded tick must have an observed palette write");
                let property = |key: &str| {
                    write[key][1]
                        .as_array()
                        .unwrap()
                        .iter()
                        .find(|p| p[0] == "powered")
                        .unwrap()[1]
                        == "true"
                };
                assert_eq!(property("before"), powered);
                assert!(effect.write && effect.notify);
                assert_eq!(property("after"), effect.powered);
                assert_eq!(effect.delay, if powered { 0 } else { 2 });
            }
            "schedule_before" | "schedule_after" => {}
            other => panic!("unexpected observation {other}"),
        }
    }
    assert_eq!((shapes, ticks), (266, 108));
    assert_eq!(ticks, commits.len());
    assert!(
        ready_not_queued > 0,
        "capture must exercise the scheduler distinction"
    );
}
