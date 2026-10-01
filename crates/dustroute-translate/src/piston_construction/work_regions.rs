//! Bounded work partitions share one complete physical context. A partition
//! is a candidate sequence, not an executable or behavioral certificate.
use super::{ElectricalModification, order, snapshot};
use crate::snapshot::{MinecraftSnapshot, index_literal_snapshot};
use dustroute_library::world_edit::WorldEditScope;
use dustroute_minecraft::time::runtime::RuntimeLimits;
use dustroute_minecraft::{Pos, Region};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct ElectricalWorkRegion {
    pub region: Region,
    pub changed_positions: Vec<Pos>,
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
        if regions.is_empty() || regions.len() > 64 {
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
            for other in &regions[..i] {
                if r.min.x <= other.max.x
                    && other.min.x <= r.max.x
                    && r.min.y <= other.max.y
                    && other.min.y <= r.max.y
                    && r.min.z <= other.max.z
                    && other.min.z <= r.max.z
                {
                    return Err("work regions must be disjoint".into());
                }
            }
        }
        let old = index_literal_snapshot(before)?;
        let new = index_literal_snapshot(after)?;
        if old.len() > 4096 || new.len() > 4096 {
            return Err("work job exceeds 4096 non-air context blocks".into());
        }
        let changed = old
            .keys()
            .chain(new.keys())
            .copied()
            .filter(|p| old.get(p) != new.get(p))
            .collect::<BTreeSet<_>>();
        if changed.is_empty() || changed.len() > 4096 {
            return Err("work job requires 1..4096 changed positions".into());
        }
        let mut owners = BTreeMap::new();
        let mut groups = regions
            .iter()
            .map(|r| ElectricalWorkRegion {
                region: *r,
                changed_positions: vec![],
            })
            .collect::<Vec<_>>();
        for p in changed {
            if !scope.allows_change(p) {
                return Err(format!("job write at {p:?} is outside editable space"));
            }
            let i = regions
                .iter()
                .position(|r| r.contains(p))
                .ok_or_else(|| format!("job change at {p:?} is not covered by a work region"))?;
            groups[i].changed_positions.push(p);
            owners.insert(p, i);
        }
        if groups.iter().any(|g| g.changed_positions.len() > 64) {
            return Err(
                "each work region permits at most 64 changed positions; partition it further"
                    .into(),
            );
        }
        let mut edges = vec![BTreeSet::new(); regions.len()];
        let mut edge = |first: Pos, next: Pos| {
            if let (Some(a), Some(b)) = (owners.get(&first), owners.get(&next)) {
                if a != b {
                    edges[*a].insert(*b);
                }
            }
        };
        let desired = snapshot::literal_world(after)?;
        let original = snapshot::literal_world(before)?;
        // New support before dependents; old dependents before support removal.
        for (world, removing) in [(&desired, false), (&original, true)] {
            for (p, block) in world.iter() {
                if let Some(d) = block.support_offset {
                    let support =
                        p.x.checked_add(d.x)
                            .zip(p.y.checked_add(d.y))
                            .zip(p.z.checked_add(d.z))
                            .map(|((x, y), z)| Pos::new(x, y, z))
                            .ok_or("support coordinate overflow")?;
                    if removing {
                        edge(*p, support);
                    } else {
                        edge(support, *p);
                    }
                }
            }
        }
        for (observer, watched) in
            order::observer_predecessors(&order::ordered_blocks(&desired), &desired)?
        {
            edge(watched, observer);
        }
        let mut remaining = (0..regions.len()).collect::<BTreeSet<_>>();
        let mut ordered = vec![];
        while !remaining.is_empty() {
            let next = remaining.iter().copied().find(|i| {
                !remaining.iter().any(|other| edges[*other].contains(i))
            }).ok_or("work region support/watch dependency cycle; combine dependent changes in one region")?;
            remaining.remove(&next);
            if !groups[next].changed_positions.is_empty() {
                ordered.push(groups[next].clone());
            }
        }
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
    pub fn regions(&self) -> &[ElectricalWorkRegion] {
        &self.regions
    }
    /// Materialize one verified-prefix candidate, never all per-command states.
    pub fn snapshot_at(&self, completed: usize) -> Result<MinecraftSnapshot, String> {
        if completed > self.regions.len() {
            return Err("invalid completed region count".into());
        }
        let mut state = index_literal_snapshot(&self.before)?;
        let target = index_literal_snapshot(&self.after)?;
        for r in &self.regions[..completed] {
            for p in &r.changed_positions {
                state.remove(p);
                if let Some(block) = target.get(p) {
                    state.insert(*p, block.clone());
                }
            }
        }
        Ok(MinecraftSnapshot {
            min: self.before.min,
            max: self.before.max,
            blocks: state.into_values().collect(),
        })
    }
    /// Fresh common-runtime proof in the whole context, including inverse and
    /// transient protected states. Later regions are deliberately not certified.
    pub fn prove_region(
        &self,
        index: usize,
        undo: bool,
        limits: RuntimeLimits,
    ) -> Result<ElectricalModification, String> {
        if index >= self.regions.len() {
            return Err("work region index out of bounds".into());
        }
        let before = self.snapshot_at(index)?;
        let after = self.snapshot_at(index + 1)?;
        if undo {
            ElectricalModification::new_scoped(&after, &before, self.scope.clone(), limits)
        } else {
            ElectricalModification::new_scoped(&before, &after, self.scope.clone(), limits)
        }
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
        assert_eq!(
            job.prove_region(0, false, Default::default())
                .unwrap()
                .steps(false)
                .len(),
            1
        );
        let proof = job.prove_region(1, false, Default::default()).unwrap();
        assert_eq!(*proof.after(), after);
        assert_eq!(
            *job.prove_region(1, true, Default::default())
                .unwrap()
                .after(),
            job.snapshot_at(1).unwrap()
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
        job.prove_region(0, false, Default::default()).unwrap();
        job.prove_region(1, false, Default::default()).unwrap();
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
            job.prove_region(0, false, Default::default())
                .unwrap_err()
                .contains("protected state changed")
        );
        let after = snapshot(vec![
            block(source, "redstone_block", &[]),
            block(lamp, "redstone_lamp", &[("lit", "true")]),
        ]);
        let job = plan(&before, &after, vec![region(source), region(lamp)]).unwrap();
        assert!(
            job.prove_region(0, false, Default::default())
                .unwrap_err()
                .contains("complete declared target")
        );
        // The explicitly coupled partition passes using the same physical runtime.
        let coupled = plan(&before, &after, vec![Region::new(source, lamp)]).unwrap();
        coupled.prove_region(0, false, Default::default()).unwrap();
    }

    #[test]
    fn conflicting_replacement_dependencies_require_a_different_partition() {
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
        assert!(
            plan(&before, &after, vec![region(lever), region(support)])
                .unwrap_err()
                .contains("dependency cycle")
        );
        plan(&before, &after, vec![Region::new(support, lever)])
            .unwrap()
            .prove_region(0, false, Default::default())
            .unwrap();
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
        for index in 0..job.regions().len() {
            let proof = job.prove_region(index, false, Default::default()).unwrap();
            assert_eq!(proof.steps(false).len(), 64);
            assert_eq!(*proof.after(), job.snapshot_at(index + 1).unwrap());
        }
        assert_eq!(job.snapshot_at(32).unwrap(), after);
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
        assert!(
            plan(
                &before,
                &large,
                vec![Region::new(Pos::default(), Pos::new(31, 0, 2))]
            )
            .unwrap_err()
            .contains("at most 64")
        );
    }
}
