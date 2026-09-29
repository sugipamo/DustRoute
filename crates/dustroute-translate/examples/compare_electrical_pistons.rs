//! Replay measured input times at actual world coordinates. No live evidence
//! is inferred here; the capture runner compares this output separately.
use dustroute_minecraft::time::piston_runtime::{
    ELECTRICAL_PROFILE, ElectricalPistonRuntime, PistonEvent, new_piston_runtime,
    schedule_device_use_after_tick, schedule_electrical_input,
    schedule_electrical_input_after_tick,
};
use dustroute_minecraft::time::runtime::RuntimeLimits;
use dustroute_minecraft::{BlockKind, Pos, Region};
use dustroute_translate::snapshot::MinecraftSnapshot;
use dustroute_translate::snapshot::assembly_from_snapshot;
use serde::{Deserialize, Serialize};
use serde_json::json;

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Input {
    tick: u64,
    position: Pos,
    powered: Option<bool>,
    #[serde(default)]
    use_device: bool,
    #[serde(default)]
    after_world_tick: bool,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Trial {
    initial: MinecraftSnapshot,
    inputs: Vec<Input>,
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

#[derive(Clone, Copy, Default, Deserialize, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
enum RestorationScope {
    #[default]
    MovementOrDevice,
    /// Explicit negative trial: no carriers, resume a changed input's callbacks.
    InputNotifications,
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let path = std::env::args()
        .nth(1)
        .ok_or("usage: compare_electrical_pistons INPUT.json")?;
    let trial: Trial = serde_json::from_str(&std::fs::read_to_string(path)?)?;
    let region = Region::new(trial.initial.min, trial.initial.max);
    // The historical analysis importer infers wire shapes. Live comparisons
    // must preserve the exact observed arm map and validate it independently.
    let assembly = assembly_from_snapshot(&trial.initial, "literal live capture", vec![region])?;
    let catalog = dustroute_library::blueprint::BlueprintCatalog::default();
    let world = assembly.inspect(&catalog)?.proposed_world();
    let mut rt = new_piston_runtime(world, region, RuntimeLimits::default())?;
    for input in trial.inputs {
        if input.use_device {
            if input.powered.is_some() || !input.after_world_tick {
                return Err(
                    "device use requires a post-world boundary and no powered assignment".into(),
                );
            }
            schedule_device_use_after_tick(&mut rt, input.tick, input.position)?;
        } else if input.after_world_tick {
            schedule_electrical_input_after_tick(
                &mut rt,
                input.tick,
                input.position,
                input.powered.ok_or("lever input needs powered")?,
            )?;
        } else {
            schedule_electrical_input(
                &mut rt,
                input.tick,
                input.position,
                input.powered.ok_or("lever input needs powered")?,
            )?;
        }
    }
    let mut exact_checkpoint = None;
    let mut behavior_state = None;
    let mut device_state_changed = false;
    let mut input_state_changed = false;
    let mut had_carrier = false;
    if trial.verify_restoration {
        while let Some(record) = rt.microstep()? {
            device_state_changed |=
                !record.output_changes.is_empty() || !record.history_changes.is_empty();
            input_state_changed |=
                matches!(record.invocation.call.payload, PistonEvent::Input { .. })
                    && record
                        .delta
                        .as_ref()
                        .is_some_and(|d| d.changes.iter().any(|c| c.before != c.after));
            had_carrier |= record
                .carrier_changes
                .iter()
                .any(|(_, _, after)| after.is_some());
            let state_changed = match trial.restoration_scope {
                RestorationScope::MovementOrDevice => device_state_changed,
                RestorationScope::InputNotifications => input_state_changed,
            };
            if exact_checkpoint.is_none()
                && !rt.at_input_boundary()
                && (state_changed
                    || record
                        .carrier_changes
                        .iter()
                        .any(|(_, _, after)| after.is_some()))
            {
                exact_checkpoint = Some(rt.checkpoint());
            }
            if behavior_state.is_none()
                && rt.at_input_boundary()
                && (state_changed
                    || rt
                        .view()
                        .world()
                        .iter()
                        .any(|(_, b)| b.kind == BlockKind::MovingPiston))
            {
                behavior_state = Some(rt.behavior_state()?);
            }
        }
        if trial.restoration_scope == RestorationScope::InputNotifications {
            if had_carrier {
                return Err(
                    "input-notification restoration trial unexpectedly moved a block".into(),
                );
            }
            if !input_state_changed {
                return Err("input-notification restoration trial requires a changed input".into());
            }
        }
        let mut exact = ElectricalPistonRuntime::from_checkpoint(
            &exact_checkpoint
                .ok_or("trial did not exercise a suspended movement/device checkpoint")?,
        )?;
        let mut representative = ElectricalPistonRuntime::from_behavior_state(
            &behavior_state.ok_or("trial did not exercise a movement/device behavior root")?,
        )?;
        exact.run_until_idle()?;
        representative.run_until_idle()?;
        if exact.state_key() != rt.state_key()
            || representative.view().world() != rt.view().world()
            || representative.behavior_state()? != rt.behavior_state()?
        {
            return Err("restoring a recorded moving trial changed its future".into());
        }
    } else {
        rt.run_until_idle()?;
    }
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
    println!(
        "{}",
        serde_json::to_string_pretty(
            &json!({"source":"dustroute_model","profile":ELECTRICAL_PROFILE,"status":rt.status(),"pending":rt.pending_count(),"final_time":rt.view().time(),"blocks":blocks,"changes":changes,
                "restoration_verified": trial.verify_restoration,
                "restoration_scope": trial.restoration_scope,
                "trace_projection":if trial.device_projection {"device_circuit"} else {"full"}, "trace":trace})
        )?
    );
    Ok(())
}
