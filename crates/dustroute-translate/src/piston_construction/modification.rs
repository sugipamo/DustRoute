//! Differential command construction against a declared stationary baseline.
//! This models an empty queue; live snapshots cannot prove that assumption.
use super::{ConstructionBudget, ConstructionError, limits};
use super::{
    ConstructionObserver, ElectricalConstructionStep, ExpectedObserver, electrical_snapshot,
    observed_root, order, settled_step_observed, snapshot,
};
use crate::snapshot::{
    LiteralSnapshotIndex, MinecraftSnapshot, MinecraftSnapshotBlock, index_literal_snapshot,
};
use dustroute_library::world_edit::WorldEditScope;
use dustroute_minecraft::time::piston_runtime::new_piston_runtime;
use dustroute_minecraft::time::runtime::RuntimeLimits;
use dustroute_minecraft::{Pos, Region, World};
use std::collections::BTreeSet;

#[derive(Clone, Debug)]
pub struct ElectricalModification {
    before: MinecraftSnapshot,
    after: MinecraftSnapshot,
    forward: Vec<ElectricalConstructionStep>,
    undo: Vec<ElectricalConstructionStep>,
    scope: WorldEditScope,
    requests: Vec<MinecraftSnapshotBlock>,
}

impl ElectricalModification {
    /// Every original block outside the explicit diff is retained. Removal and
    /// insertion callbacks are the same as ordinary electrical construction.
    pub fn new(
        before: &MinecraftSnapshot,
        after: &MinecraftSnapshot,
        limits: RuntimeLimits,
    ) -> Result<Self, ConstructionError> {
        Self::new_scoped(
            before,
            after,
            WorldEditScope::entire(Region::new(before.min, before.max)),
            limits,
        )
    }
    pub fn new_scoped(
        before: &MinecraftSnapshot,
        after: &MinecraftSnapshot,
        scope: WorldEditScope,
        limits: RuntimeLimits,
    ) -> Result<Self, ConstructionError> {
        if before.min != after.min || before.max != after.max {
            return Err("modification requires identical fully observed bounds".into());
        }
        let old = LiteralSnapshotIndex::new(before)?;
        let new = LiteralSnapshotIndex::new(after)?;
        let changed = old.changed_positions(&new)?;
        if changed.is_empty() || changed.len() > limits::MAX_STAGE_CHANGES {
            return Err("modification requires 1..64 changed positions".into());
        }
        let requests = changed
            .iter()
            .map(|p| new.get(*p).cloned().unwrap_or_else(|| air(*p)))
            .collect::<Vec<_>>();
        let proof = Self::derive_indexed(before, &old, requests, scope, limits)?;
        if !LiteralSnapshotIndex::new(proof.after())?
            .changed_positions(&new)?
            .is_empty()
        {
            return Err(
                "modification callbacks do not produce the complete declared target state".into(),
            );
        }
        Ok(proof)
    }

    /// Submit only these explicitly requested coordinates. Physical callbacks
    /// derive the complete settled target, including changes elsewhere inside
    /// editable space. The same requested coordinates must also restore the
    /// entire original baseline under fresh inverse simulation.
    pub fn derive_scoped(
        before: &MinecraftSnapshot,
        requests: Vec<MinecraftSnapshotBlock>,
        scope: WorldEditScope,
        limits: RuntimeLimits,
    ) -> Result<Self, ConstructionError> {
        let old = LiteralSnapshotIndex::new(before)?;
        Self::derive_indexed(before, &old, requests, scope, limits)
    }

    fn derive_indexed(
        before: &MinecraftSnapshot,
        old: &LiteralSnapshotIndex<'_>,
        requests: Vec<MinecraftSnapshotBlock>,
        scope: WorldEditScope,
        limits: RuntimeLimits,
    ) -> Result<Self, ConstructionError> {
        check_budget(
            ConstructionBudget::ContextBlocks,
            old.len(),
            limits::MAX_CONTEXT_BLOCKS,
        )?;
        check_budget(
            ConstructionBudget::StageChanges,
            requests.len(),
            limits::MAX_STAGE_CHANGES,
        )?;
        let region = Region::new(before.min, before.max);
        scope.validate(region)?;
        let mut desired = old.to_owned_blocks();
        let mut seen = BTreeSet::new();
        let mut effective = vec![];
        for request in requests {
            if !region.contains(request.pos) || !seen.insert(request.pos) {
                return Err(
                    "requested positions must be distinct and inside the complete context".into(),
                );
            }
            if !scope.allows_change(request.pos) {
                return Err(ConstructionError::InvalidInput(format!(
                    "declared write at {:?} is outside editable space or is protected",
                    request.pos
                )));
            }
            let is_air = request.name == "minecraft:air";
            if is_air && !request.properties.is_empty() {
                return Err("Air request cannot carry properties".into());
            }
            if (is_air && !desired.contains_key(&request.pos))
                || desired.get(&request.pos) == Some(&request)
            {
                continue;
            }
            desired.remove(&request.pos);
            if !is_air {
                desired.insert(request.pos, request.clone());
            }
            effective.push(request);
        }
        check_budget(
            ConstructionBudget::ContextBlocks,
            desired.len(),
            limits::MAX_CONTEXT_BLOCKS,
        )?;
        let changed = effective.iter().map(|b| b.pos).collect::<BTreeSet<_>>();
        let requested = MinecraftSnapshot {
            min: before.min,
            max: before.max,
            blocks: desired.into_values().collect(),
        };
        let world = snapshot::literal_world(before)?;
        let request_world = snapshot::literal_world(&requested)?;
        let before = electrical_snapshot(&world, region)?;
        if !LiteralSnapshotIndex::new(&before)?
            .changed_positions(old)?
            .is_empty()
            || index_literal_snapshot(&electrical_snapshot(&request_world, region)?)?
                != index_literal_snapshot(&requested)?
        {
            return Err("modification needs lossless, complete native block states".into());
        }
        let (forward, after) = steps(&before, &requested, &changed, &scope, limits)?;
        check_budget(
            ConstructionBudget::ContextBlocks,
            after.blocks.len(),
            limits::MAX_CONTEXT_BLOCKS,
        )?;
        let (undo, restored) = steps(&after, &before, &changed, &scope, limits)?;
        if restored != before {
            return Err("derived region cannot restore the complete baseline; combine its dependent changes or author an explicit repair design".into());
        }
        Ok(Self {
            before,
            after,
            forward,
            undo,
            scope,
            requests: effective,
        })
    }

    /// Saved endpoint differences do not identify explicit writes. Reprove the
    /// same sealed requests, rather than treating automatic changes as commands.
    pub fn reprove(&self, limits: RuntimeLimits) -> Result<Self, ConstructionError> {
        let proof = Self::derive_scoped(
            &self.before,
            self.requests.clone(),
            self.scope.clone(),
            limits,
        )?;
        if proof.after != self.after {
            return Err("fresh derived target differs from the preview".into());
        }
        Ok(proof)
    }
    pub fn requests(&self) -> &[MinecraftSnapshotBlock] {
        &self.requests
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
    pub fn scope(&self) -> &WorldEditScope {
        &self.scope
    }
}

fn check_budget(
    resource: ConstructionBudget,
    actual: usize,
    maximum: usize,
) -> Result<(), ConstructionError> {
    if actual > maximum {
        Err(ConstructionError::Budget {
            resource,
            actual,
            maximum,
        })
    } else {
        Ok(())
    }
}

fn steps(
    before: &MinecraftSnapshot,
    after: &MinecraftSnapshot,
    changed: &BTreeSet<Pos>,
    scope: &WorldEditScope,
    limits: RuntimeLimits,
) -> Result<(Vec<ElectricalConstructionStep>, MinecraftSnapshot), String> {
    let region = Region::new(before.min, before.max);
    let world = snapshot::literal_world(before)?;
    let desired = snapshot::literal_world(after)?;
    let baseline = world.clone();
    let mut runtime = new_piston_runtime(world, region, limits).map_err(|e| e.to_string())?;
    let mut protect = ExpectedObserver::new(ScopeGuard {
        before: &baseline,
        scope,
        known: region,
    });
    protect.initial(runtime.view())?;
    while observed_root(&mut runtime, &mut protect)? {}
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
        result.push(settled_step_observed(
            &mut runtime,
            region,
            pos,
            "minecraft:air".into(),
            start,
            &mut protect,
        )?);
        remove.remove(&pos);
    }
    let requested_index = LiteralSnapshotIndex::new(after)?;
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
        let requested = requested_index
            .get(pos)
            .ok_or("missing modification state")?
            .clone();
        let (block, state) = snapshot::initialization_request(block, requested);
        let start = runtime.view().time().game_tick;
        runtime.install_now(pos, block).map_err(|e| e.to_string())?;
        result.push(settled_step_observed(
            &mut runtime,
            region,
            pos,
            state,
            start,
            &mut protect,
        )?);
        installed.insert(pos);
    }
    Ok((result, electrical_snapshot(runtime.view().world(), region)?))
}

struct ScopeGuard<'a> {
    before: &'a World,
    scope: &'a WorldEditScope,
    known: Region,
}
impl ScopeGuard<'_> {
    fn check(
        &self,
        position: Pos,
        view: dustroute_minecraft::time::runtime::RuntimeView<'_>,
    ) -> Result<(), String> {
        let actual = view.world();
        if !self.known.contains(position) {
            return Err(format!(
                "modeled state leaves observed space at {position:?}"
            ));
        }
        if !self.scope.allows_change(position) && self.before.get(position) != actual.get(position)
        {
            return Err(format!(
                "protected state changed at {position:?}, runtime {:?}; expected {:?}, actual {:?}",
                view.time(),
                self.before.get(position),
                actual.get(position)
            ));
        }
        Ok(())
    }
}
impl ConstructionObserver for ScopeGuard<'_> {
    fn initial(
        &mut self,
        view: dustroute_minecraft::time::runtime::RuntimeView<'_>,
    ) -> Result<(), String> {
        for (position, _) in self.before.iter().chain(view.world().iter()) {
            self.check(*position, view)?;
        }
        Ok(())
    }
    fn committed<P>(
        &mut self,
        view: dustroute_minecraft::time::runtime::RuntimeView<'_>,
        record: &dustroute_minecraft::time::runtime::RuntimeRecord<P>,
    ) -> Result<(), String> {
        // Runtime handlers receive a read-only World. Every world mutation is
        // a validated delta; include removals and formerly Air positions.
        for position in record.world_changed_positions() {
            self.check(position, view)?;
        }
        Ok(())
    }
}

fn air(pos: Pos) -> MinecraftSnapshotBlock {
    MinecraftSnapshotBlock {
        pos,
        name: "minecraft:air".into(),
        properties: Default::default(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use dustroute_minecraft::{Block, BlockKind};

    #[test]
    fn admission_errors_expose_budget_counts_and_literal_coordinates() {
        let before = MinecraftSnapshot {
            min: Pos::default(),
            max: Pos::new(64, 0, 0),
            blocks: vec![],
        };
        let requests = (0..65).map(|x| air(Pos::new(x, 0, 0))).collect();
        assert!(matches!(
            ElectricalModification::derive_scoped(
                &before,
                requests,
                WorldEditScope::entire(Region::new(before.min, before.max)),
                Default::default()
            ),
            Err(ConstructionError::Budget {
                resource: ConstructionBudget::StageChanges,
                actual: 65,
                maximum: 64
            })
        ));
        let mut bad = before.clone();
        bad.blocks = vec![air(Pos::default()), air(Pos::default())];
        assert!(matches!(
            ElectricalModification::derive_scoped(
                &bad,
                vec![],
                WorldEditScope::entire(Region::new(before.min, before.max)),
                Default::default()
            ),
            Err(ConstructionError::Literal(
                crate::snapshot::LiteralSnapshotError::InvalidCoordinate {
                    duplicate: true,
                    ..
                }
            ))
        ));
    }

    #[test]
    fn incremental_guard_matches_full_scan_until_first_violation_and_sees_air_changes() {
        let protected = Pos::new(0, 0, 0);
        let known = Region::around(protected, 2);
        let scope = WorldEditScope::entire(Region::around(Pos::new(2, 0, 0), 0));
        for initially_present in [false, true] {
            let mut baseline = World::new();
            if initially_present {
                baseline.set(protected, Block::new(BlockKind::Solid));
            }
            let mut runtime =
                new_piston_runtime(baseline.clone(), known, Default::default()).unwrap();
            runtime.run_until_idle().unwrap();
            assert_eq!(runtime.view().world(), &baseline);
            let mut guard = ScopeGuard {
                before: &baseline,
                scope: &scope,
                known,
            };
            guard.initial(runtime.view()).unwrap();
            if initially_present {
                runtime.remove_now(protected).unwrap();
            } else {
                runtime
                    .install_now(protected, Block::new(BlockKind::Solid))
                    .unwrap();
            }
            let mut saw_violation = false;
            while let Some(record) = runtime.microstep().unwrap() {
                let incremental = guard.committed(runtime.view(), &record);
                let full = guard.initial(runtime.view());
                assert_eq!(incremental, full);
                if full.is_err() {
                    assert_eq!(
                        record.world_changed_positions().collect::<Vec<_>>(),
                        [protected]
                    );
                    saw_violation = true;
                    break;
                }
            }
            assert!(saw_violation);
            // A later restoration must not conceal the earlier violation.
            runtime.run_until_idle().unwrap();
            if initially_present {
                runtime
                    .install_now(protected, Block::new(BlockKind::Solid))
                    .unwrap();
            } else {
                runtime.remove_now(protected).unwrap();
            }
            runtime.run_until_idle().unwrap();
            assert_eq!(runtime.view().world(), &baseline);
            guard.initial(runtime.view()).unwrap();
        }
    }
}
