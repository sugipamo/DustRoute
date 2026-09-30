//! Differential command construction against a declared stationary baseline.
//! This models an empty queue; live snapshots cannot prove that assumption.
use super::{ElectricalConstructionStep, electrical_snapshot, order, settled_step, snapshot};
use crate::snapshot::{MinecraftSnapshot, index_literal_snapshot};
use dustroute_minecraft::time::piston_runtime::new_piston_runtime;
use dustroute_minecraft::time::runtime::RuntimeLimits;
use dustroute_minecraft::{Pos, Region};
use std::collections::BTreeSet;

#[derive(Clone, Debug)]
pub struct ElectricalModification {
    before: MinecraftSnapshot,
    after: MinecraftSnapshot,
    forward: Vec<ElectricalConstructionStep>,
    undo: Vec<ElectricalConstructionStep>,
}

impl ElectricalModification {
    /// Every original block outside the explicit diff is retained. Removal and
    /// insertion callbacks are the same as ordinary electrical construction.
    pub fn new(
        before: &MinecraftSnapshot,
        after: &MinecraftSnapshot,
        limits: RuntimeLimits,
    ) -> Result<Self, String> {
        if before.min != after.min || before.max != after.max {
            return Err("modification requires identical fully observed bounds".into());
        }
        let old = index_literal_snapshot(before)?;
        let new = index_literal_snapshot(after)?;
        if old.len() > 4096 || new.len() > 4096 {
            return Err("modification exceeds 4096 non-air blocks".into());
        }
        let changed = old
            .keys()
            .chain(new.keys())
            .copied()
            .filter(|p| old.get(p) != new.get(p))
            .collect::<BTreeSet<_>>();
        if changed.is_empty() || changed.len() > 64 {
            return Err("modification requires 1..64 changed positions".into());
        }
        let region = Region::new(before.min, before.max);
        let old_world = snapshot::literal_world(before)?;
        let new_world = snapshot::literal_world(after)?;
        let before = electrical_snapshot(&old_world, region)?;
        let after = electrical_snapshot(&new_world, region)?;
        if index_literal_snapshot(&before)? != old || index_literal_snapshot(&after)? != new {
            return Err("modification needs lossless, complete native block states".into());
        }
        let forward = steps(&before, &after, &changed, limits)?;
        let undo = steps(&after, &before, &changed, limits)?;
        Ok(Self {
            before,
            after,
            forward,
            undo,
        })
    }
    pub fn before(&self) -> &MinecraftSnapshot {
        &self.before
    }
    pub fn after(&self) -> &MinecraftSnapshot {
        &self.after
    }
    pub fn steps(&self, undo: bool) -> &[ElectricalConstructionStep] {
        if undo { &self.undo } else { &self.forward }
    }
}

fn steps(
    before: &MinecraftSnapshot,
    after: &MinecraftSnapshot,
    changed: &BTreeSet<Pos>,
    limits: RuntimeLimits,
) -> Result<Vec<ElectricalConstructionStep>, String> {
    let region = Region::new(before.min, before.max);
    let world = snapshot::literal_world(before)?;
    let desired = snapshot::literal_world(after)?;
    let mut runtime = new_piston_runtime(world, region, limits).map_err(|e| e.to_string())?;
    runtime.run_until_idle().map_err(|e| e.to_string())?;
    if electrical_snapshot(runtime.view().world(), region)? != *before {
        return Err(
            "modification baseline is not stationary under the declared empty-queue model".into(),
        );
    }
    let mut result = Vec::new();
    let mut remove = changed
        .iter()
        .copied()
        .filter(|p| runtime.view().world().get(*p).is_some())
        .collect::<BTreeSet<_>>();
    while !remove.is_empty() {
        remove.retain(|p| runtime.view().world().get(*p).is_some());
        if remove.is_empty() {
            break;
        }
        let pos =
            order::next_removal_position_matching(runtime.view().world(), |p| remove.contains(&p))?;
        let start = runtime.view().time().game_tick;
        runtime.remove_now(pos).map_err(|e| e.to_string())?;
        result.push(settled_step(
            &mut runtime,
            region,
            pos,
            "minecraft:air".into(),
            start,
        )?);
        remove.remove(&pos);
    }
    let mut remaining = order::ordered_blocks(&desired);
    remaining.retain(|(pos, _)| changed.contains(pos));
    let predecessors = order::observer_predecessors(&remaining, &desired)?;
    let mut installed = runtime
        .view()
        .world()
        .iter()
        .map(|(p, _)| *p)
        .collect::<BTreeSet<_>>();
    while !remaining.is_empty() {
        let index = order::next_build_index(
            &remaining,
            &predecessors,
            &installed,
            runtime.view().world(),
        )?;
        let (pos, block) = remaining.remove(index);
        let requested = after
            .blocks
            .iter()
            .find(|b| b.pos == pos)
            .ok_or("missing modification state")?
            .clone();
        let (block, state) = snapshot::initialization_request(block, requested);
        let start = runtime.view().time().game_tick;
        runtime.install_now(pos, block).map_err(|e| e.to_string())?;
        result.push(settled_step(&mut runtime, region, pos, state, start)?);
        installed.insert(pos);
    }
    if electrical_snapshot(runtime.view().world(), region)? != *after {
        return Err(
            "modification callbacks do not produce the complete declared target state".into(),
        );
    }
    Ok(result)
}
