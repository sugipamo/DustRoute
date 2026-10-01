//! Bounded work partitions share one complete physical context and queue.
//! Stage boundaries are derived lazily, never projected from final properties.
use super::{ElectricalModification, limits, snapshot};
mod partition;
use crate::snapshot::{MinecraftSnapshot, MinecraftSnapshotBlock, index_literal_snapshot};
use dustroute_library::world_edit::WorldEditScope;
use dustroute_minecraft::time::runtime::RuntimeLimits;
use dustroute_minecraft::{Pos, Region};
use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct ElectricalWorkRegion {
    /// Display bounds. Only parts and changed_positions define stage membership.
    pub region: Region,
    pub parts: Vec<Region>,
    pub changed_positions: Vec<Pos>,
}
/// Compact historical state boundary, never executable command authority.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct ElectricalBoundary {
    pub changes: Vec<MinecraftSnapshotBlock>,
}
impl ElectricalBoundary {
    pub fn between(before: &MinecraftSnapshot, after: &MinecraftSnapshot) -> Result<Self, String> {
        if before.min != after.min || before.max != after.max {
            return Err("boundary context bounds changed".into());
        }
        let old = index_literal_snapshot(before)?;
        let new = index_literal_snapshot(after)?;
        let positions = old
            .keys()
            .chain(new.keys())
            .copied()
            .filter(|p| old.get(p) != new.get(p))
            .collect::<BTreeSet<_>>();
        Ok(Self {
            changes: positions
                .into_iter()
                .map(|p| new.get(&p).cloned().unwrap_or_else(|| air(p)))
                .collect(),
        })
    }
    pub fn apply(
        &self,
        before: &MinecraftSnapshot,
        scope: &WorldEditScope,
    ) -> Result<MinecraftSnapshot, String> {
        let mut state = index_literal_snapshot(before)?;
        let known = Region::new(before.min, before.max);
        let mut seen = BTreeSet::new();
        for block in &self.changes {
            if !known.contains(block.pos)
                || !scope.allows_change(block.pos)
                || !seen.insert(block.pos)
            {
                return Err(
                    "boundary update escapes editable context or repeats a coordinate".into(),
                );
            }
            state.remove(&block.pos);
            if block.name == "minecraft:air" {
                if !block.properties.is_empty() {
                    return Err("Air boundary update has properties".into());
                }
            } else {
                state.insert(block.pos, block.clone());
            }
        }
        if state.len() > limits::MAX_CONTEXT_BLOCKS {
            return Err("boundary exceeds 4096 non-air blocks".into());
        }
        Ok(MinecraftSnapshot {
            min: before.min,
            max: before.max,
            blocks: state.into_values().collect(),
        })
    }
}
#[derive(Clone, Debug)]
pub struct ElectricalWorkPlan {
    before: MinecraftSnapshot,
    after: MinecraftSnapshot,
    scope: WorldEditScope,
    regions: Vec<ElectricalWorkRegion>,
}
impl ElectricalWorkPlan {
    pub fn new(
        before: &MinecraftSnapshot,
        after: &MinecraftSnapshot,
        regions: Vec<Region>,
        scope: WorldEditScope,
    ) -> Result<Self, String> {
        if before.min != after.min || before.max != after.max {
            return Err("work job requires identical complete context bounds".into());
        }
        let known = Region::new(before.min, before.max);
        scope.validate(known)?;
        if regions.is_empty() || regions.len() > limits::MAX_WORK_STAGES {
            return Err("work job requires 1..64 work regions".into());
        }
        for (i, r) in regions.iter().enumerate() {
            if r.min.x > r.max.x
                || r.min.y > r.max.y
                || r.min.z > r.max.z
                || !known.contains(r.min)
                || !known.contains(r.max)
            {
                return Err(format!("work region {i} escapes the complete context"));
            }
            if regions[..i].iter().any(|other| intersects(r, other)) {
                return Err("work regions must be disjoint".into());
            }
        }
        let old = index_literal_snapshot(before)?;
        let new = index_literal_snapshot(after)?;
        if old.len() > limits::MAX_CONTEXT_BLOCKS || new.len() > limits::MAX_CONTEXT_BLOCKS {
            return Err("work job exceeds 4096 non-air context blocks".into());
        }
        let changed = old
            .keys()
            .chain(new.keys())
            .copied()
            .filter(|p| old.get(p) != new.get(p))
            .collect::<BTreeSet<_>>();
        if changed.is_empty() || changed.len() > limits::MAX_JOB_CHANGES {
            return Err("work job requires 1..4096 changed positions".into());
        }
        for p in &changed {
            if !scope.allows_change(*p) {
                return Err(format!("job write at {p:?} is outside editable space"));
            }
            if !regions.iter().any(|r| r.contains(*p)) {
                return Err(format!(
                    "job change at {p:?} is not covered by a work region"
                ));
            }
        }
        let desired = snapshot::literal_world(after)?;
        let original = snapshot::literal_world(before)?;
        let ordered = partition::plan(regions, &changed, &original, &desired)?;
        Ok(Self {
            before: MinecraftSnapshot {
                min: before.min,
                max: before.max,
                blocks: old.into_values().collect(),
            },
            after: MinecraftSnapshot {
                min: after.min,
                max: after.max,
                blocks: new.into_values().collect(),
            },
            scope,
            regions: ordered,
        })
    }
    /// Restore immutable intention only; current stages still need fresh proof.
    pub fn from_regions(
        before: &MinecraftSnapshot,
        after: &MinecraftSnapshot,
        regions: &[ElectricalWorkRegion],
        scope: WorldEditScope,
    ) -> Result<Self, String> {
        let plan = Self::new(
            before,
            after,
            regions.iter().flat_map(|r| r.parts.clone()).collect(),
            scope,
        )?;
        if plan.regions != regions {
            return Err("saved job partition differs from its intention".into());
        }
        Ok(plan)
    }
    pub fn before(&self) -> &MinecraftSnapshot {
        &self.before
    }
    pub fn target(&self) -> &MinecraftSnapshot {
        &self.after
    }
    pub fn regions(&self) -> &[ElectricalWorkRegion] {
        &self.regions
    }
    pub fn requests(
        &self,
        index: usize,
        target: &MinecraftSnapshot,
    ) -> Result<Vec<MinecraftSnapshotBlock>, String> {
        let group = self
            .regions
            .get(index)
            .ok_or("work region index out of bounds")?;
        if target.min != self.before.min || target.max != self.before.max {
            return Err("stage context bounds changed".into());
        }
        let state = index_literal_snapshot(target)?;
        Ok(group
            .changed_positions
            .iter()
            .map(|p| state.get(p).cloned().unwrap_or_else(|| air(*p)))
            .collect())
    }
    pub fn prove_region(
        &self,
        index: usize,
        before: &MinecraftSnapshot,
        limits: RuntimeLimits,
    ) -> Result<ElectricalModification, String> {
        let requests = self.requests(index, &self.after)?;
        let proof = match ElectricalModification::derive_scoped(
            before,
            requests.clone(),
            self.scope.clone(),
            limits,
        ) {
            Ok(proof) => proof,
            Err(original) => {
                let temporary = self.temporary_requests(index)?;
                if temporary == requests {
                    return Err(original.into());
                }
                ElectricalModification::derive_scoped(before,temporary,self.scope.clone(),limits)
                    .map_err(|e|format!("declared output initialization failed: {original}; temporary output initialization failed: {e}"))?
            }
        };
        if index + 1 == self.regions.len() && proof.after() != &self.after {
            return Err("last stage does not produce the complete immutable job target; revise the partition or explicit intermediate design".into());
        }
        Ok(proof)
    }
    /// One bounded alternate initialization, using existing device bindings.
    /// Controllable inputs, identities, geometry and scope are never changed.
    pub fn temporary_requests(&self, index: usize) -> Result<Vec<MinecraftSnapshotBlock>, String> {
        let world = snapshot::literal_world(&self.after)?;
        let mut requests = self.requests(index, &self.after)?;
        for request in &mut requests {
            let Some(block) = world.get(request.pos) else {
                continue;
            };
            if let Some(program) = dustroute_minecraft::device_program::program(block) {
                let definition = program.definition();
                if !definition
                    .handlers
                    .contains_key(&dustroute_minecraft::device_program::Callback::Use)
                    && definition.history.is_none()
                {
                    let property = definition.primary_power.name();
                    if request
                        .properties
                        .get(property)
                        .is_some_and(|value| value == "true")
                    {
                        request.properties.insert(property.into(), "false".into());
                    }
                }
            }
            if block.kind == dustroute_minecraft::BlockKind::RedstoneWire
                && request.properties.contains_key("power")
            {
                request.properties.insert("power".into(), "0".into());
            }
        }
        Ok(requests)
    }
    pub fn prove_undo(
        &self,
        index: usize,
        before: &MinecraftSnapshot,
        target: &MinecraftSnapshot,
        limits: RuntimeLimits,
    ) -> Result<ElectricalModification, String> {
        let proof = ElectricalModification::derive_scoped(
            before,
            self.requests(index, target)?,
            self.scope.clone(),
            limits,
        )?;
        if proof.after() != target {
            return Err("inverse stage does not restore its verified boundary".into());
        }
        Ok(proof)
    }
}
fn intersects(a: &Region, b: &Region) -> bool {
    a.min.x <= b.max.x
        && b.min.x <= a.max.x
        && a.min.y <= b.max.y
        && b.min.y <= a.max.y
        && a.min.z <= b.max.z
        && b.min.z <= a.max.z
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
    use crate::snapshot::MinecraftSnapshotBlock;

    fn block(p: Pos, name: &str, properties: &[(&str, &str)]) -> MinecraftSnapshotBlock {
        MinecraftSnapshotBlock {
            pos: p,
            name: format!("minecraft:{name}"),
            properties: properties
                .iter()
                .map(|(k, v)| ((*k).into(), (*v).into()))
                .collect(),
        }
    }
    fn region(p: Pos) -> Region {
        Region::new(p, p)
    }
    fn snapshot(blocks: Vec<MinecraftSnapshotBlock>) -> MinecraftSnapshot {
        MinecraftSnapshot {
            min: Pos::new(-2, -2, -2),
            max: Pos::new(40, 4, 70),
            blocks,
        }
    }
    fn plan(
        before: &MinecraftSnapshot,
        after: &MinecraftSnapshot,
        regions: Vec<Region>,
    ) -> Result<ElectricalWorkPlan, String> {
        ElectricalWorkPlan::new(
            before,
            after,
            regions.clone(),
            WorldEditScope {
                editable: regions,
                protected: vec![],
            },
        )
    }

    #[test]
    fn region_order_preserves_cross_region_support_and_fresh_inverse() {
        let support = Pos::default();
        let lever = Pos::new(0, 1, 0);
        let before = snapshot(vec![]);
        let after = snapshot(vec![
            block(support, "stone", &[]),
            block(
                lever,
                "lever",
                &[("face", "floor"), ("facing", "north"), ("powered", "false")],
            ),
        ]);
        let job = plan(&before, &after, vec![region(lever), region(support)]).unwrap();
        assert_eq!(job.regions()[0].region, region(support));
        let first = job.prove_region(0, &before, Default::default()).unwrap();
        assert_eq!(first.steps(false).len(), 1);
        let proof = job
            .prove_region(1, first.after(), Default::default())
            .unwrap();
        assert_eq!(*proof.after(), after);
        assert_eq!(
            *job.prove_undo(1, proof.after(), first.after(), Default::default())
                .unwrap()
                .after(),
            *first.after()
        );
    }

    #[test]
    fn occupied_observer_watch_is_built_in_the_predecessor_region() {
        let observer = Pos::default();
        let watch = Pos::new(1, 0, 0);
        let before = snapshot(vec![]);
        let after = snapshot(vec![
            block(
                observer,
                "observer",
                &[("facing", "east"), ("powered", "false")],
            ),
            block(watch, "stone", &[]),
        ]);
        let job = plan(&before, &after, vec![region(observer), region(watch)]).unwrap();
        assert_eq!(job.regions()[0].region, region(watch));
        let first = job.prove_region(0, &before, Default::default()).unwrap();
        job.prove_region(1, first.after(), Default::default())
            .unwrap();
    }

    #[test]
    fn notifications_and_power_are_never_cut_at_work_region_boundaries() {
        let source = Pos::default();
        let lamp = Pos::new(1, 0, 0);
        let before = snapshot(vec![block(lamp, "redstone_lamp", &[("lit", "false")])]);
        let protected_target = snapshot(vec![
            block(source, "redstone_block", &[]),
            block(lamp, "redstone_lamp", &[("lit", "false")]),
        ]);
        let job = plan(&before, &protected_target, vec![region(source)]).unwrap();
        assert!(
            job.prove_region(0, &before, Default::default())
                .unwrap_err()
                .contains("protected state changed")
        );
        let after = snapshot(vec![
            block(source, "redstone_block", &[]),
            block(lamp, "redstone_lamp", &[("lit", "true")]),
        ]);
        let job = plan(&before, &after, vec![region(source), region(lamp)]).unwrap();
        let first = job.prove_region(0, &before, Default::default()).unwrap();
        assert_eq!(*first.after(), after);
        assert_eq!(first.requests().len(), 1);
        assert!(first.steps(false).iter().all(|s| s.position == source));
        let next = job
            .prove_region(1, first.after(), Default::default())
            .unwrap();
        assert!(next.steps(false).is_empty());
        assert_eq!(*next.after(), after);
        assert_eq!(
            *job.prove_undo(0, first.after(), &before, Default::default())
                .unwrap()
                .after(),
            before
        );
        // Caller order is only a preference among candidates. The shared
        // construction table prioritizes power before its requested lit device.
        let reversed = plan(&before, &after, vec![region(lamp), region(source)]).unwrap();
        assert_eq!(reversed.regions()[0].changed_positions, vec![source]);
        assert_eq!(
            *reversed
                .prove_region(0, &before, Default::default())
                .unwrap()
                .after(),
            after
        );
        // The explicitly coupled partition passes using the same physical runtime.
        let coupled = plan(&before, &after, vec![Region::new(source, lamp)]).unwrap();
        coupled
            .prove_region(0, &before, Default::default())
            .unwrap();
    }

    #[test]
    fn powered_support_uses_an_off_intermediate_then_natural_power_and_inverse() {
        let lamp = Pos::default();
        let lever = Pos::new(0, 1, 0);
        let before = snapshot(vec![]);
        let after = snapshot(vec![
            block(lamp, "redstone_lamp", &[("lit", "true")]),
            block(
                lever,
                "lever",
                &[("face", "floor"), ("facing", "north"), ("powered", "true")],
            ),
        ]);
        let job = plan(&before, &after, vec![region(lever), region(lamp)]).unwrap();
        assert_eq!(job.regions()[0].changed_positions, vec![lamp]);
        let first = job.prove_region(0, &before, Default::default()).unwrap();
        assert_eq!(first.requests()[0].properties["lit"], "false");
        assert_eq!(first.after().blocks[0].properties["lit"], "false");
        let second = job
            .prove_region(1, first.after(), Default::default())
            .unwrap();
        assert_eq!(second.requests()[0].properties["powered"], "true");
        assert_eq!(second.requests().len(), 1);
        assert_eq!(*second.after(), after);
        assert_eq!(
            ElectricalBoundary::between(first.after(), second.after())
                .unwrap()
                .changes
                .len(),
            2
        );
        assert_eq!(*second.reprove(Default::default()).unwrap().after(), after);
        let inverse = job
            .prove_undo(1, second.after(), first.after(), Default::default())
            .unwrap();
        assert!(inverse.steps(false).iter().all(|s| s.position == lever));
        assert_eq!(
            *job.prove_undo(0, inverse.after(), &before, Default::default())
                .unwrap()
                .after(),
            before
        );
    }

    #[test]
    fn automatic_changes_do_not_become_extra_command_positions() {
        let sources = (0..8)
            .flat_map(|x| (0..4).map(move |z| Pos::new(3 * x, 0, 3 * z)))
            .collect::<Vec<_>>();
        let lamps = sources
            .iter()
            .flat_map(|p| {
                [
                    Pos::new(p.x - 1, 0, p.z),
                    Pos::new(p.x + 1, 0, p.z),
                    Pos::new(p.x, 0, p.z - 1),
                    Pos::new(p.x, 0, p.z + 1),
                ]
            })
            .collect::<Vec<_>>();
        let before = snapshot(
            lamps
                .iter()
                .map(|p| block(*p, "redstone_lamp", &[("lit", "false")]))
                .collect(),
        );
        let mut target = lamps
            .iter()
            .map(|p| block(*p, "redstone_lamp", &[("lit", "true")]))
            .collect::<Vec<_>>();
        target.extend(sources.iter().map(|p| block(*p, "redstone_block", &[])));
        target.sort_by_key(|b| b.pos);
        let after = snapshot(target);
        let proof = ElectricalModification::derive_scoped(
            &before,
            sources
                .iter()
                .map(|p| block(*p, "redstone_block", &[]))
                .collect(),
            WorldEditScope::entire(Region::new(before.min, before.max)),
            Default::default(),
        )
        .unwrap();
        assert_eq!(proof.requests().len(), 32);
        assert_eq!(*proof.after(), after);
        assert_eq!(
            ElectricalBoundary::between(&before, proof.after())
                .unwrap()
                .changes
                .len(),
            160
        );
        assert_eq!(
            index_literal_snapshot(&proof.steps(true).last().unwrap().expected.materialize())
                .unwrap(),
            index_literal_snapshot(&before).unwrap()
        );
        assert!(
            proof
                .steps(false)
                .iter()
                .chain(proof.steps(true))
                .all(|s| sources.contains(&s.position))
        );
    }

    #[test]
    fn conflicting_replacement_dependencies_are_combined_without_expanding_scope() {
        let support = Pos::default();
        let lever = Pos::new(0, 1, 0);
        let before = snapshot(vec![
            block(support, "stone", &[]),
            block(
                lever,
                "lever",
                &[("face", "floor"), ("facing", "north"), ("powered", "false")],
            ),
        ]);
        let after = snapshot(vec![
            block(support, "smooth_quartz", &[]),
            block(
                lever,
                "lever",
                &[("face", "floor"), ("facing", "east"), ("powered", "false")],
            ),
        ]);
        let combined = plan(&before, &after, vec![region(lever), region(support)]).unwrap();
        assert_eq!(combined.regions().len(), 1);
        assert_eq!(combined.regions()[0].parts.len(), 2);
        combined
            .prove_region(0, &before, Default::default())
            .unwrap();
        plan(&before, &after, vec![Region::new(support, lever)])
            .unwrap()
            .prove_region(0, &before, Default::default())
            .unwrap();
        // The same dependency cycle cannot make a merged stage exceed its
        // command budget, even when one member contains independent geometry.
        let mut oversized = after.clone();
        oversized
            .blocks
            .extend((1..64).map(|z| block(Pos::new(0, 0, z), "stone", &[])));
        assert!(
            plan(
                &before,
                &oversized,
                vec![Region::new(support, Pos::new(0, 0, 63)), region(lever)],
            )
            .unwrap_err()
            .contains("cycle exceeds 64 changes")
        );
    }

    #[test]
    fn thousands_of_blocks_keep_only_the_current_regions_command_proof() {
        let before = snapshot(vec![]);
        let after = snapshot(
            (0..32)
                .flat_map(|x| (0..64).map(move |z| block(Pos::new(x, 0, z), "stone", &[])))
                .collect(),
        );
        let regions = (0..32)
            .map(|i| Region::new(Pos::new(i, 0, 0), Pos::new(i, 0, 63)))
            .collect();
        let job = plan(&before, &after, regions).unwrap();
        assert_eq!(job.regions().len(), 32);
        let mut current = before.clone();
        for index in 0..job.regions().len() {
            let proof = job
                .prove_region(index, &current, Default::default())
                .unwrap();
            assert_eq!(proof.steps(false).len(), 64);
            let delta = ElectricalBoundary::between(&current, proof.after()).unwrap();
            assert_eq!(delta.changes.len(), 64);
            current = delta.apply(&current, &job.scope).unwrap();
            assert_eq!(current, *proof.after());
        }
        assert_eq!(current, after);
        let automatic = plan(
            &before,
            &after,
            vec![Region::new(Pos::default(), Pos::new(31, 0, 63))],
        )
        .unwrap();
        assert_eq!(automatic.regions().len(), 32);
        assert!(
            automatic
                .regions()
                .iter()
                .all(|r| r.changed_positions.len() <= 64)
        );
    }

    #[test]
    fn partition_limits_missing_changes_and_overlapping_regions_are_rejected() {
        let before = snapshot(vec![]);
        let after = snapshot(vec![block(Pos::default(), "stone", &[])]);
        assert!(
            plan(&before, &after, vec![region(Pos::new(1, 0, 0))])
                .unwrap_err()
                .contains("outside editable")
        );
        assert!(
            plan(
                &before,
                &after,
                vec![region(Pos::default()), region(Pos::default())]
            )
            .unwrap_err()
            .contains("disjoint")
        );
        let large = snapshot(
            (0..65)
                .map(|x| block(Pos::new(x % 32, 0, x / 32), "stone", &[]))
                .collect(),
        );
        let subdivided = plan(
            &before,
            &large,
            vec![Region::new(Pos::default(), Pos::new(31, 0, 2))],
        )
        .unwrap();
        assert_eq!(subdivided.regions().len(), 2);
        assert!(
            subdivided
                .regions()
                .iter()
                .all(|r| r.changed_positions.len() <= 64)
        );
    }
}
