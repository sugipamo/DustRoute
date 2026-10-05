//! Opt-in fixture codec for external conformance runners, never a product CLI.
//! Independently retained server expectations are not rewritten by this adapter.
use dustroute_minecraft::time::piston_runtime::{ELECTRICAL_PROFILE, PistonEvent};
use dustroute_translate::electrical_replay::{
    ReplayInput, ReplayRequest, RestorationScope, replay_electrical,
};
use dustroute_translate::snapshot::MinecraftSnapshot;
use serde::Deserialize;
use serde_json::json;

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Trial {
    initial: MinecraftSnapshot,
    inputs: Vec<ReplayInput>,
    #[serde(default)]
    trace: bool,
    #[serde(default)]
    verify_restoration: bool,
    #[serde(default)]
    restoration_scope: RestorationScope,
    /// Retain commits and delivery metadata, omitting bulky continuation payloads.
    #[serde(default)]
    device_projection: bool,
}

fn project(trial: Trial) -> Result<serde_json::Value, Box<dyn std::error::Error>> {
    let rt = replay_electrical(ReplayRequest {
        initial: trial.initial,
        inputs: trial.inputs,
        verify_restoration: trial.verify_restoration,
        restoration_scope: trial.restoration_scope,
    })?;
    let blocks: Vec<_> = rt.view().world().iter().map(|(pos, b)| json!({"position":pos,"name":b.observed_name,"properties":b.observed_properties})).collect();
    let mut changes = Vec::new();
    for record in rt.trace() {
        if let Some(delta) = &record.delta {
            for change in &delta.changes {
                changes.push(json!({"tick": record.invocation.time.game_tick,
                    "section": record.invocation.time.section, "position": change.position,
                    "kind": change.after.kind, "name": change.after.observed_name,
                    "properties": change.after.observed_properties}));
            }
        }
    }
    let trace = if !trial.trace {
        None
    } else if trial.device_projection {
        Some(
            rt.trace()
                .iter()
                .map(|r| {
                    let inv = &r.invocation;
                    let payload = match &inv.call.payload {
                        PistonEvent::Initialize => json!("Initialize"),
                        PistonEvent::Device { callback, .. } => {
                            json!({"Device":{"callback":callback}})
                        }
                        PistonEvent::Notify { jobs } => {
                            json!({"Notify":{"jobs":jobs.front().into_iter().collect::<Vec<_>>()}})
                        }
                        PistonEvent::NotifyAnalogReaders { .. } => json!(inv.call.payload),
                        PistonEvent::Block { .. } | PistonEvent::CarrierTick => json!(inv.call.payload),
                        PistonEvent::ForceFinish { .. } => json!({"ForceFinish":{}}),
                        _ => serde_json::Value::Null,
                    };
                    json!({"invocation":{"id":inv.id,"cause":inv.cause,"root":inv.root,"kind":inv.kind,"time":inv.time,
                "call":{"target":inv.call.target,"payload":payload}},
                "result":r.result,"delta":r.delta,"carrier_changes":r.carrier_changes})
                })
                .collect::<Vec<_>>(),
        )
    } else {
        Some(rt.trace().iter().map(|r| json!(r)).collect())
    };
    Ok(
        json!({"source":"dustroute_model","profile":ELECTRICAL_PROFILE,"status":rt.status(),"pending":rt.pending_count(),"final_time":rt.view().time(),"blocks":blocks,"changes":changes,
                "restoration_verified": trial.verify_restoration,
                "restoration_scope": trial.restoration_scope,
                "trace_projection":if trial.device_projection {"device_circuit"} else {"full"}, "trace":trace}),
    )
}

#[test]
#[ignore = "explicit fixture adapter; requires DUSTROUTE_REPLAY_INPUT/OUTPUT/ERROR paths"]
fn export_fixture() {
    let input = std::env::var("DUSTROUTE_REPLAY_INPUT").expect("fixture input path");
    let output = std::env::var("DUSTROUTE_REPLAY_OUTPUT").expect("fixture output path");
    let error_path = std::env::var("DUSTROUTE_REPLAY_ERROR").expect("fixture error path");
    let result = (|| -> Result<(), Box<dyn std::error::Error>> {
        let executable = std::env::var("DUSTROUTE_REPLAY_EXECUTABLE")?;
        let file = std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(executable)?;
        use std::io::Write;
        write!(&file, "{}", std::env::current_exe()?.display())?;
        let trial = serde_json::from_slice(&std::fs::read(input)?)?;
        let report = project(trial)?;
        let file = std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(output)?;
        serde_json::to_writer_pretty(file, &report)?;
        Ok(())
    })();
    if let Err(error) = result {
        let file = std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(error_path)
            .expect("new fixture error path");
        use std::io::Write;
        writeln!(&file, "{error}").expect("retain native failure");
        panic!("fixture replay rejected: {error}");
    }
}
