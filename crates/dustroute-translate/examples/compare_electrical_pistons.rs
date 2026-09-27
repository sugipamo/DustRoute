//! Replay measured input times at actual world coordinates. No live evidence
//! is inferred here; the capture runner compares this output separately.
use dustroute_minecraft::time::piston_runtime::{
    ELECTRICAL_PROFILE, ElectricalPistonRuntime, new_piston_runtime, schedule_electrical_input,
    schedule_electrical_input_after_tick,
};
use dustroute_minecraft::time::runtime::RuntimeLimits;
use dustroute_minecraft::{BlockKind, Pos, Region};
use dustroute_translate::MinecraftSnapshot;
use dustroute_translate::snapshot::assembly_from_snapshot;
use serde::Deserialize;
use serde_json::json;

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Input {
    tick: u64,
    position: Pos,
    powered: bool,
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
        if input.after_world_tick {
            schedule_electrical_input_after_tick(
                &mut rt,
                input.tick,
                input.position,
                input.powered,
            )?;
        } else {
            schedule_electrical_input(&mut rt, input.tick, input.position, input.powered)?;
        }
    }
    let mut exact_checkpoint = None;
    let mut behavior_state = None;
    if trial.verify_restoration {
        while let Some(record) = rt.microstep()? {
            if exact_checkpoint.is_none()
                && !rt.at_input_boundary()
                && record
                    .carrier_changes
                    .iter()
                    .any(|(_, _, after)| after.is_some())
            {
                exact_checkpoint = Some(rt.checkpoint());
            }
            if behavior_state.is_none()
                && rt.at_input_boundary()
                && rt
                    .view()
                    .world()
                    .iter()
                    .any(|(_, b)| b.kind == BlockKind::MovingPiston)
            {
                behavior_state = Some(rt.behavior_state()?);
            }
        }
        let mut exact = ElectricalPistonRuntime::from_checkpoint(
            &exact_checkpoint.ok_or("trial did not exercise a suspended moving checkpoint")?,
        )?;
        let mut representative = ElectricalPistonRuntime::from_behavior_state(
            &behavior_state.ok_or("trial did not exercise a moving behavior root")?,
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
    println!(
        "{}",
        serde_json::to_string_pretty(
            &json!({"source":"dustroute_model","profile":ELECTRICAL_PROFILE,"status":rt.status(),"pending":rt.pending_count(),"final_time":rt.view().time(),"blocks":blocks,"changes":changes,
                "restoration_verified": trial.verify_restoration, "trace": if trial.trace { Some(rt.trace()) } else { None }})
        )?
    );
    Ok(())
}
