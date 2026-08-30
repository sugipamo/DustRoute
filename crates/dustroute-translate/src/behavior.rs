use std::collections::BTreeMap;

use dustroute_ir::{BehaviorEvent, BehaviorTrace, TemporalAnalysis};
use dustroute_physical::{ComponentId, PhysicalScene, World};

use crate::RedstoneTickSimulator;

pub fn simulate_behavior_trace(
    world: &World,
    physical: &PhysicalScene,
    temporal: &TemporalAnalysis,
    ticks: usize,
    label: impl Into<String>,
) -> Result<BehaviorTrace, String> {
    let positions: BTreeMap<ComponentId, _> = physical
        .components
        .iter()
        .map(|component| (component.id, component.pos))
        .collect();
    let devices = temporal
        .behavior
        .devices
        .iter()
        .map(|device| device.component)
        .collect::<Vec<_>>();
    let mut simulator =
        RedstoneTickSimulator::new(world.clone()).map_err(|error| error.to_string())?;
    let mut previous = BTreeMap::new();
    let mut events = Vec::new();
    let mut changed_on_last_tick = false;
    for tick in 0..=ticks {
        let state = if tick == 0 {
            simulator.snapshot()
        } else {
            simulator
                .advance_tick()
                .map_err(|error| error.to_string())?
        };
        changed_on_last_tick = false;
        for component in &devices {
            let Some(pos) = positions.get(component) else {
                continue;
            };
            let powered = state.powered(*pos);
            if previous.get(component).copied() != Some(powered) {
                events.push(BehaviorEvent {
                    tick: state.tick,
                    component: *component,
                    powered,
                });
                previous.insert(*component, powered);
                changed_on_last_tick = true;
            }
        }
    }
    Ok(BehaviorTrace {
        label: label.into(),
        events,
        stable: !changed_on_last_tick,
    })
}

#[cfg(test)]
mod tests {
    use dustroute_minecraft::{Block, BlockKind, Facing, Pos};

    use crate::{RegionBounds, analyze_world_region, update_wire_shapes};

    use super::*;

    #[test]
    fn captures_repeater_transition_at_its_delay() {
        let mut world = World::new();
        world.fill(
            Pos::new(0, 0, 0),
            Pos::new(3, 0, 0),
            Block::new(BlockKind::Solid),
        );
        world.set(Pos::new(0, 1, 0), Block::new(BlockKind::RedstoneBlock));
        world.place(BlockKind::RedstoneWire, Pos::new(1, 1, 0));
        let repeater = world.place(BlockKind::Repeater, Pos::new(2, 1, 0));
        repeater.facing = Some(Facing::East);
        repeater.delay = Some(2);
        world.place(BlockKind::RedstoneWire, Pos::new(3, 1, 0));
        update_wire_shapes(&mut world);
        let analysis = analyze_world_region(
            &world,
            RegionBounds::new(Pos::new(0, 0, 0), Pos::new(3, 2, 0)),
        );
        let temporal = TemporalAnalysis::from_scene(&analysis.scene);
        let trace = simulate_behavior_trace(&world, &analysis.scene, &temporal, 3, "powered input")
            .unwrap();
        assert!(
            trace
                .events
                .iter()
                .any(|event| event.tick == 2 && event.powered)
        );
        assert!(trace.stable);
    }
}
