//! Diagnostic replay of explicit observed inputs through the shared runtime.
//! This predicts only the supplied initial state and input order. It neither
//! certifies a live capture nor grants adoption, placement or restart authority.
use crate::snapshot::{MinecraftSnapshot, assembly_from_snapshot};
use dustroute_minecraft::time::piston_runtime::{
    ElectricalPistonRuntime, PistonEvent, new_piston_runtime, schedule_device_use_after_tick,
    schedule_electrical_input, schedule_electrical_input_after_tick,
};
use dustroute_minecraft::time::runtime::RuntimeLimits;
use dustroute_minecraft::{BlockKind, Pos, Region};
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ReplayInput {
    pub tick: u64,
    pub position: Pos,
    pub powered: Option<bool>,
    #[serde(default)]
    pub use_device: bool,
    #[serde(default)]
    pub after_world_tick: bool,
}

#[derive(Clone, Copy, Debug, Default, Deserialize, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum RestorationScope {
    #[default]
    MovementOrDevice,
    /// A negative diagnostic: no carriers, resume a changed input's callbacks.
    InputNotifications,
}

pub struct ReplayRequest {
    pub initial: MinecraftSnapshot,
    pub inputs: Vec<ReplayInput>,
    pub verify_restoration: bool,
    pub restoration_scope: RestorationScope,
}

/// Retains exact delivery metadata and world state in their owning Rust types.
/// JSON fixture IO and optional trace projections belong to test adapters.
/// Restoration compares the exact checkpoint and representative behavior future
/// without promoting a saved observation or fixture to execution permission.
pub fn replay_electrical(
    trial: ReplayRequest,
) -> Result<ElectricalPistonRuntime, Box<dyn std::error::Error>> {
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
    Ok(rt)
}
