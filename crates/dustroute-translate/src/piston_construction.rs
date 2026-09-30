//! Model-derived construction sequence for the expanded electrical profile.
//! This is not adoption authority, a live observation or a generic placement
//! certificate. The MCP layer must independently establish those conditions.
mod batching;
mod diagnostics;
mod modification;
mod order;
pub mod policy;
mod snapshot;

pub use batching::{ElectricalConstructionBatch, construction_batches};
pub use modification::ElectricalModification;
pub use snapshot::electrical_snapshot;
use snapshot::literal_world;
use std::collections::{BTreeMap, BTreeSet};

use dustroute_minecraft::time::piston_runtime::{ElectricalPistonRuntime, new_piston_runtime};
use dustroute_minecraft::time::runtime::RuntimeLimits;
use dustroute_minecraft::{Pos, Region, World};
use serde::{Deserialize, Serialize};

use crate::snapshot::MinecraftSnapshot;

#[derive(Clone, Debug, Eq, PartialEq, Deserialize, Serialize)]
pub struct ElectricalConstructionStep {
    pub position: Pos,
    pub state: String,
    /// Settling margin after the modeled last pending root. No live result is
    /// accepted without matching the full expected observation afterwards.
    pub wait_ticks: u64,
    pub expected: MinecraftSnapshot,
    /// Fresh model result only. Saved or caller-provided steps cannot grant
    /// permission to omit an intermediate live observation.
    #[serde(skip)]
    immediate_idle: bool,
}

#[derive(Clone, Debug)]
pub struct ElectricalConstruction {
    initial: MinecraftSnapshot,
    settled: MinecraftSnapshot,
    build: Vec<ElectricalConstructionStep>,
    remove: Vec<ElectricalConstructionStep>,
}

impl ElectricalConstruction {
    pub fn new(world: &World, region: Region, limits: RuntimeLimits) -> Result<Self, String> {
        let initial = electrical_snapshot(world, region)?;
        let literal = literal_world(&initial)?;
        let mut reference =
            new_piston_runtime(literal.clone(), region, limits).map_err(|e| e.to_string())?;
        reference.run_until_idle().map_err(|e| e.to_string())?;
        let settled = electrical_snapshot(reference.view().world(), region)?;
        let mut runtime =
            new_piston_runtime(World::new(), region, limits).map_err(|e| e.to_string())?;
        runtime.run_until_idle().map_err(|e| e.to_string())?;
        let mut remaining = order::ordered_blocks(&literal);
        let observer_predecessors = order::observer_predecessors(&remaining, &literal)?;
        let mut installed = BTreeSet::new();
        let mut build = Vec::new();
        while !remaining.is_empty() {
            let index = order::next_build_index(
                &remaining,
                &observer_predecessors,
                &installed,
                runtime.view().world(),
            )?;
            let (pos, block) = remaining.remove(index);
            let requested = initial
                .blocks
                .iter()
                .find(|b| b.pos == pos)
                .expect("literal block")
                .clone();
            let (block, state) = snapshot::initialization_request(block, requested);
            let start = runtime.view().time().game_tick;
            runtime.install_now(pos, block).map_err(|e| e.to_string())?;
            build.push(settled_step(&mut runtime, region, pos, state, start)?);
            installed.insert(pos);
        }
        let constructed = electrical_snapshot(runtime.view().world(), region)?;
        diagnostics::verify_constructed(&constructed, &settled, &build)?;
        let remove = teardown(&mut runtime, region)?;
        Ok(Self {
            initial,
            settled,
            build,
            remove,
        })
    }
    pub fn initial(&self) -> &MinecraftSnapshot {
        &self.initial
    }
    pub fn settled(&self) -> &MinecraftSnapshot {
        &self.settled
    }
    pub fn build_steps(&self) -> &[ElectricalConstructionStep] {
        &self.build
    }
    pub fn remove_steps(&self) -> &[ElectricalConstructionStep] {
        &self.remove
    }

    /// A comparison reference from the declared design, not a simulation or
    /// reconstruction of the damaged observation. Apply inputs in the supplied
    /// order, settling each one. Other valid histories may have other states.
    pub fn operating_reference(
        &self,
        inputs: &[(Pos, bool)],
        limits: RuntimeLimits,
    ) -> Result<MinecraftSnapshot, String> {
        let runtime = self.operating_runtime(inputs, limits)?;
        electrical_snapshot(
            runtime.view().world(),
            Region::new(self.initial.min, self.initial.max),
        )
    }

    /// Tear down the same settled reference used by diagnosis. The runtime is
    /// replayed from the declared initial conditions, never from a live snapshot.
    /// Callers must independently match the full live region to this baseline.
    pub fn operating_removal(
        &self,
        inputs: &[(Pos, bool)],
        limits: RuntimeLimits,
    ) -> Result<(MinecraftSnapshot, Vec<ElectricalConstructionStep>), String> {
        let mut runtime = self.operating_runtime(inputs, limits)?;
        let region = Region::new(self.initial.min, self.initial.max);
        let baseline = electrical_snapshot(runtime.view().world(), region)?;
        Ok((baseline, teardown(&mut runtime, region)?))
    }

    fn operating_runtime(
        &self,
        inputs: &[(Pos, bool)],
        limits: RuntimeLimits,
    ) -> Result<ElectricalPistonRuntime, String> {
        let world = literal_world(&self.initial)?;
        let region = Region::new(self.initial.min, self.initial.max);
        let mut seen = BTreeSet::new();
        for (position, _) in inputs {
            if !seen.insert(*position)
                || !world
                    .get(*position)
                    .is_some_and(|b| b.kind == dustroute_minecraft::BlockKind::Lever)
            {
                return Err("comparison inputs must name distinct declared levers".into());
            }
        }
        let mut runtime = new_piston_runtime(world, region, limits).map_err(|e| e.to_string())?;
        runtime.run_until_idle().map_err(|e| e.to_string())?;
        for (position, powered) in inputs {
            runtime
                .input_now(*position, *powered)
                .map_err(|e| e.to_string())?;
            runtime.run_until_idle().map_err(|e| e.to_string())?;
        }
        Ok(runtime)
    }

    /// Reconstruct the declared initial state by tearing down an observed
    /// supported layout, then using the ordinary construction sequence.
    ///
    /// This models an explicit empty-queue assumption. A snapshot is not a
    /// runtime checkpoint, proof of ownership, or evidence of live readiness.
    /// Callers must review the entire removal scope and reobserve before writes.
    pub fn reconstruction_steps(
        &self,
        observed: &MinecraftSnapshot,
        limits: RuntimeLimits,
    ) -> Result<Vec<ElectricalConstructionStep>, String> {
        if observed.min != self.initial.min || observed.max != self.initial.max {
            return Err("reconstruction requires the complete original region".into());
        }
        let literal = literal_world(observed)?;
        let region = Region::new(observed.min, observed.max);
        let baseline = electrical_snapshot(&literal, region)?;
        // Missing/moved parts are admitted. Extra or different material is a
        // conflict; identical player-added material cannot establish ownership.
        let mut available = BTreeMap::<&str, usize>::new();
        for block in &self.initial.blocks {
            *available.entry(&block.name).or_default() += 1;
        }
        for block in &baseline.blocks {
            if block.name == "minecraft:piston_head" {
                continue; // The runtime validates every stable head/body pair.
            }
            let count = available.entry(&block.name).or_default();
            if *count == 0 {
                return Err(format!(
                    "reconstruction material conflict at {:?}: {} exceeds the declared inventory",
                    block.pos, block.name
                ));
            }
            *count -= 1;
        }
        // Explicit command states can retain a wire shape that the fresh-world
        // constructor rejects until later neighbors are installed. Reproduce a
        // matching *model* prefix through ordinary commands, without loosening
        // that constructor or claiming to recover the live world's history.
        let mut runtime =
            if let Some(index) = self.build.iter().position(|s| s.expected == baseline) {
                let declared = literal_world(&self.initial)?;
                let mut replay =
                    new_piston_runtime(World::new(), region, limits).map_err(|e| e.to_string())?;
                replay.run_until_idle().map_err(|e| e.to_string())?;
                for step in &self.build[..=index] {
                    let requested = self
                        .initial
                        .blocks
                        .iter()
                        .find(|b| b.pos == step.position)
                        .ok_or("missing construction request")?
                        .clone();
                    let block = declared
                        .get(step.position)
                        .ok_or("missing construction block")?
                        .clone();
                    let (block, _) = snapshot::initialization_request(block, requested);
                    replay
                        .install_now(step.position, block)
                        .map_err(|e| e.to_string())?;
                    replay.run_until_idle().map_err(|e| e.to_string())?;
                    if electrical_snapshot(replay.view().world(), region)? != step.expected {
                        return Err("replayed construction prefix differs from its model".into());
                    }
                }
                replay
            } else {
                new_piston_runtime(literal, region, limits).map_err(|e| e.to_string())?
            };
        runtime.run_until_idle().map_err(|e| e.to_string())?;
        if electrical_snapshot(runtime.view().world(), region)? != baseline {
            return Err("observed reconstruction state does not remain stable in the model".into());
        }
        let mut steps = teardown(&mut runtime, region)?;
        steps.extend(self.build.iter().cloned());
        Ok(steps)
    }
}

fn teardown(
    runtime: &mut ElectricalPistonRuntime,
    region: Region,
) -> Result<Vec<ElectricalConstructionStep>, String> {
    let mut remove = Vec::new();
    while runtime.view().world().iter().next().is_some() {
        let pos = order::next_removal_position(runtime.view().world())?;
        let start = runtime.view().time().game_tick;
        runtime.remove_now(pos).map_err(|e| e.to_string())?;
        remove.push(settled_step(
            runtime,
            region,
            pos,
            "minecraft:air".into(),
            start,
        )?);
    }
    Ok(remove)
}

fn settled_step(
    runtime: &mut ElectricalPistonRuntime,
    region: Region,
    position: Pos,
    state: String,
    start: u64,
) -> Result<ElectricalConstructionStep, String> {
    // Finish the submitted command and all its synchronous callbacks first.
    // Merely observing zero elapsed ticks after run_until_idle would hide
    // scheduled block events at the same tick.
    if !runtime
        .step()
        .map_err(|e| format!("construction command at {position:?}: {e}"))?
    {
        return Err("construction command was not executed".into());
    }
    let immediate_idle = runtime.pending_count() == 0
        && runtime.at_input_boundary()
        && runtime.view().time().game_tick == start;
    runtime
        .run_until_idle()
        .map_err(|e| format!("construction settling at {position:?}: {e}"))?;
    let wait_ticks = runtime
        .view()
        .time()
        .game_tick
        .checked_sub(start)
        .and_then(|n| n.checked_add(4))
        .ok_or("construction time overflow")?;
    if wait_ticks > 1200 {
        return Err("construction settling exceeds the per-step live wait limit".into());
    }
    Ok(ElectricalConstructionStep {
        position,
        state,
        wait_ticks,
        expected: electrical_snapshot(runtime.view().world(), region)?,
        immediate_idle,
    })
}
