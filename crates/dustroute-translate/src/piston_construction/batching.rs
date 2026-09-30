//! Consecutive commands may share a live boundary only when each command root
//! leaves no queued work. Callback order and every modeled prefix are retained.
use super::ElectricalConstructionStep;
use crate::snapshot::MinecraftSnapshot;

// A bounded submission also bounds the unverified prefix on transport failure.
const MAX_BATCH_WRITES: usize = 32;

pub struct ElectricalConstructionBatch<'a> {
    steps: &'a [ElectricalConstructionStep],
    first: usize,
}
impl<'a> ElectricalConstructionBatch<'a> {
    pub fn steps(&self) -> &'a [ElectricalConstructionStep] {
        self.steps
    }
    /// Inclusive range in the original, one-based construction sequence.
    pub fn first_step(&self) -> usize {
        self.first + 1
    }
    pub fn last_step(&self) -> usize {
        self.first + self.steps.len()
    }
    pub fn expected(&self) -> &'a MinecraftSnapshot {
        &self.steps.last().expect("nonempty model batch").expected
    }
    pub fn wait_ticks(&self) -> u64 {
        self.steps.last().expect("nonempty model batch").wait_ticks
    }
}

/// Uses a private, non-deserializable per-command model result. This iterator
/// does not grant write permission; live callers still freshly replay plans
/// and verify the complete baseline and every completed batch.
pub fn construction_batches(
    steps: &[ElectricalConstructionStep],
) -> impl Iterator<Item = ElectricalConstructionBatch<'_>> {
    let mut next = 0;
    std::iter::from_fn(move || {
        let first = next;
        let head = steps.get(first)?;
        let len = if head.immediate_idle {
            steps[first..]
                .iter()
                .take(MAX_BATCH_WRITES)
                .take_while(|step| {
                    step.immediate_idle
                        && (step.state == "minecraft:air") == (head.state == "minecraft:air")
                })
                .count()
        } else {
            1
        };
        next += len;
        Some(ElectricalConstructionBatch {
            steps: &steps[first..next],
            first,
        })
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::native_state::NativeBlockState;
    use crate::piston_construction::{ElectricalConstruction, electrical_snapshot, snapshot};
    use crate::snapshot::MinecraftSnapshotBlock;
    use dustroute_minecraft::time::piston_runtime::new_piston_runtime;
    use dustroute_minecraft::{BlockKind, Facing, Pos, Region, World};

    fn replay(steps: &[ElectricalConstructionStep], region: Region) -> MinecraftSnapshot {
        let mut runtime = new_piston_runtime(World::new(), region, Default::default()).unwrap();
        runtime.run_until_idle().unwrap();
        for batch in construction_batches(steps) {
            for step in batch.steps() {
                let state: NativeBlockState = step.state.parse().unwrap();
                let request = MinecraftSnapshot {
                    min: region.min,
                    max: region.max,
                    blocks: vec![MinecraftSnapshotBlock {
                        pos: step.position,
                        name: state.name().into(),
                        properties: state.properties().clone(),
                    }],
                };
                let world = snapshot::literal_world(&request).unwrap();
                runtime
                    .install_now(step.position, world.get(step.position).unwrap().clone())
                    .unwrap();
                runtime.step().unwrap();
                if batch.steps().len() > 1 {
                    assert_eq!(runtime.pending_count(), 0, "a batched prefix must be idle");
                    assert_eq!(
                        electrical_snapshot(runtime.view().world(), region).unwrap(),
                        step.expected
                    );
                }
            }
            runtime.run_until_idle().unwrap();
            assert_eq!(
                &electrical_snapshot(runtime.view().world(), region).unwrap(),
                batch.expected()
            );
        }
        electrical_snapshot(runtime.view().world(), region).unwrap()
    }

    #[test]
    fn idle_commands_keep_all_prefixes_and_bound_batch_size() {
        let region = Region::new(Pos::new(-1, -1, -1), Pos::new(36, 2, 2));
        let mut world = World::new();
        for x in 0..35 {
            world.place(BlockKind::Solid, Pos::new(x, 0, 0));
        }
        let plan = ElectricalConstruction::new(&world, region, Default::default()).unwrap();
        let batches = construction_batches(plan.build_steps()).collect::<Vec<_>>();
        assert_eq!(
            batches.iter().map(|b| b.steps().len()).collect::<Vec<_>>(),
            [32, 3]
        );
        assert_eq!((batches[1].first_step(), batches[1].last_step()), (33, 35));
        assert_eq!(replay(plan.build_steps(), region), *plan.settled());
        assert_eq!(construction_batches(plan.remove_steps()).count(), 2);
    }

    #[test]
    fn saved_and_forged_steps_cannot_grant_batching() {
        let region = Region::new(Pos::new(-1, -1, -1), Pos::new(3, 2, 2));
        let mut world = World::new();
        for x in 0..2 {
            world.place(BlockKind::Solid, Pos::new(x, 0, 0));
        }
        let plan = ElectricalConstruction::new(&world, region, Default::default()).unwrap();
        assert_eq!(construction_batches(plan.build_steps()).count(), 1);
        let roundtrip = [plan.build_steps(), plan.remove_steps()].concat();
        assert_eq!(
            construction_batches(&roundtrip)
                .map(|batch| batch.steps().len())
                .collect::<Vec<_>>(),
            [2, 2],
            "installation and removal must retain their separate live boundary"
        );
        let mut saved = serde_json::to_value(plan.build_steps()).unwrap();
        for step in saved.as_array_mut().unwrap() {
            assert!(step.get("immediate_idle").is_none());
            step["immediate_idle"] = serde_json::json!(true);
        }
        let decoded: Vec<ElectricalConstructionStep> = serde_json::from_value(saved).unwrap();
        assert_eq!(construction_batches(&decoded).count(), 2);
        assert!(decoded.iter().all(|step| !step.immediate_idle));
    }

    #[test]
    fn powered_piston_work_keeps_its_own_settling_boundary() {
        let region = Region::new(Pos::new(-3, -3, -3), Pos::new(5, 3, 3));
        let mut world = World::new();
        let piston = world.place(BlockKind::Piston, Pos::default());
        piston.facing = Some(Facing::East);
        world.place(BlockKind::Solid, Pos::new(1, 0, 0));
        world.place(BlockKind::RedstoneBlock, Pos::new(0, 0, 1));
        let plan = ElectricalConstruction::new(&world, region, Default::default()).unwrap();
        let power = plan
            .build_steps()
            .iter()
            .find(|s| s.position == Pos::new(0, 0, 1))
            .unwrap();
        assert!(
            !power.immediate_idle,
            "same-tick block events must not be grouped"
        );
        assert!(power.wait_ticks > 4);
        let batch = construction_batches(plan.build_steps())
            .find(|b| b.steps().iter().any(|s| s.position == power.position))
            .unwrap();
        assert_eq!(batch.steps().len(), 1);
        assert_eq!(replay(plan.build_steps(), region), *plan.settled());
    }

    #[test]
    fn observer_initialization_keeps_order_and_does_not_hide_a_pulse() {
        let region = Region::new(Pos::new(-3, -3, -3), Pos::new(3, 3, 3));
        let mut world = World::new();
        for x in 0..2 {
            let observer = world.place(BlockKind::Observer, Pos::new(x, 0, 0));
            observer.facing = Some(Facing::West);
            observer.powered = Some(false);
        }
        world
            .place(BlockKind::RedstoneLamp, Pos::new(2, 0, 0))
            .powered = Some(false);
        let plan = ElectricalConstruction::new(&world, region, Default::default()).unwrap();
        assert_eq!(
            plan.build_steps()
                .iter()
                .map(|s| s.position.x)
                .collect::<Vec<_>>(),
            [2, 1, 0]
        );
        assert_eq!(replay(plan.build_steps(), region), *plan.settled());
    }
}
