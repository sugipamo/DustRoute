//! Local callback law checks against directly instrumented target decisions.
//! Delivery conformance is checked separately against retained runtime traces.
use dustroute_minecraft::device_program::{
    BoolProperty, Callback, Property, Query, ResolvedEffect, program,
};
use dustroute_minecraft::{Block, BlockKind};

fn decision(callback: Callback, powered: bool, queued: bool, front: bool) -> Vec<ResolvedEffect> {
    let mut block = Block::new(BlockKind::Observer);
    block.powered = Some(powered);
    program(&block)
        .unwrap()
        .prepare(callback, &block, |query| {
            Ok(match query {
                Query::Constant { value } => *value,
                Query::Powered => powered.into(),
                Query::TickQueued => queued.into(),
                Query::SourceAtFront => front.into(),
                Query::ReceivingPower
                | Query::ReceivingLevel
                | Query::SideLevel { .. }
                | Query::State { .. } => panic!("unexpected observer query"),
                Query::TickCollected
                | Query::SourceOffAxis
                | Query::GateOutputLevel
                | Query::GateOutputChanged
                | Query::GateInputPowered
                | Query::SideGatePowered
                | Query::OutputGateMisaligned => panic!("unexpected gate query"),
            })
        })
        .unwrap()
        .unwrap()
        .effects()
        .to_vec()
}
fn delay(effects: &[ResolvedEffect]) -> u64 {
    effects
        .iter()
        .find_map(|effect| {
            if let ResolvedEffect::Schedule { delay, .. } = effect {
                Some(*delay)
            } else {
                None
            }
        })
        .unwrap_or(0)
}

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
                let effect = decision(Callback::Shape, powered, queued, front);
                if front && !powered {
                    assert_eq!(events[index + 1]["event"], "schedule_before");
                    assert_eq!(events[index + 2]["event"], "schedule_after");
                    assert_eq!(events[index + 1]["position"], event["position"]);
                    assert_eq!(events[index + 1]["queued"], queued);
                    assert_eq!(events[index + 2]["queued"], true);
                    assert_eq!(delay(&effect), if queued { 0 } else { 2 });
                } else {
                    assert_eq!(delay(&effect), 0);
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
                let effect = decision(Callback::Tick, powered, queued, false);
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
                assert!(
                    effect
                        .iter()
                        .any(|e| matches!(e, ResolvedEffect::Notify { .. }))
                );
                let next_powered = effect
                    .iter()
                    .find_map(|e| match e {
                        ResolvedEffect::WriteState { values, .. } => {
                            values.iter().find_map(|(p, v)| {
                                (*p == Property::Bool(BoolProperty::Powered)).then_some(*v != 0)
                            })
                        }
                        _ => None,
                    })
                    .expect("observed write");
                assert_eq!(property("after"), next_powered);
                assert_eq!(delay(&effect), if powered { 0 } else { 2 });
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
