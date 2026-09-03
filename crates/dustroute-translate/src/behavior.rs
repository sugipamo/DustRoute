use std::collections::BTreeMap;

use dustroute_ir::{
    BehaviorEvent, BehaviorTrace, EventCause, EventKind, EventSource, TemporalAnalysis,
    TraceTimeUnit, TransitionTrace,
};
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
        let mut sub_tick_order = 0;
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
                // Device-specific causal labels are assigned below from the
                // physical block at the component position.
                let (event_kind, cause) = if state.tick == 0 && !previous.contains_key(component) {
                    (EventKind::StateTransition, EventCause::InitialSnapshot)
                } else {
                    match physical
                        .components
                        .iter()
                        .find(|item| item.id == *component)
                        .map(|item| item.block.kind)
                    {
                        Some(dustroute_physical::BlockKind::Observer) => {
                            if powered && previous.get(component) != Some(&true) {
                                (EventKind::PulseStart, EventCause::ObserverFrontStateChange)
                            } else if !powered && previous.get(component) == Some(&true) {
                                (EventKind::PulseEnd, EventCause::ObserverFrontStateChange)
                            } else {
                                (
                                    EventKind::StateTransition,
                                    EventCause::ObserverFrontStateChange,
                                )
                            }
                        }
                        Some(dustroute_physical::BlockKind::Repeater) => {
                            (EventKind::SignalPropagation, EventCause::RepeaterDelay)
                        }
                        _ => (
                            EventKind::SignalPropagation,
                            EventCause::SimulatorPropagation,
                        ),
                    }
                };
                let source = if cause == EventCause::InitialSnapshot {
                    EventSource::InitialSnapshot
                } else {
                    EventSource::Simulator
                };
                events.push(BehaviorEvent {
                    tick: state.tick,
                    sub_tick_order,
                    event_kind,
                    cause,
                    source,
                    cause_sequence: None,
                    component: *component,
                    powered,
                });
                previous.insert(*component, powered);
                sub_tick_order += 1;
                changed_on_last_tick = true;
            }
        }
    }
    Ok(BehaviorTrace {
        label: label.into(),
        time_unit: TraceTimeUnit::RedstoneTick,
        events,
        stable: !changed_on_last_tick,
    })
}

/// Simulates the same compatibility timeline and exposes its state-changing
/// edges as a transition-first IR. `ticks` remains the bounded simulation
/// horizon; it is not used as the identity of a transition.
pub fn simulate_transition_trace(
    world: &World,
    physical: &PhysicalScene,
    temporal: &TemporalAnalysis,
    ticks: usize,
    label: impl Into<String>,
) -> Result<TransitionTrace, String> {
    let behavior = simulate_behavior_trace(world, physical, temporal, ticks, label)?;
    Ok(behavior.transition_trace())
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

    #[test]
    fn exposes_same_tick_component_changes_as_zero_elapsed_transitions() {
        let mut world = World::new();
        world.fill(
            Pos::new(0, 0, 0),
            Pos::new(4, 0, 1),
            Block::new(BlockKind::Solid),
        );
        world.set(Pos::new(0, 1, 0), Block::new(BlockKind::RedstoneBlock));
        world.place(BlockKind::RedstoneWire, Pos::new(1, 1, 0));
        world.place(BlockKind::RedstoneWire, Pos::new(1, 1, 1));
        let repeater_a = world.place(BlockKind::Repeater, Pos::new(2, 1, 0));
        repeater_a.facing = Some(crate::Facing::East);
        let repeater_b = world.place(BlockKind::Repeater, Pos::new(2, 1, 1));
        repeater_b.facing = Some(crate::Facing::East);
        update_wire_shapes(&mut world);
        let analysis = analyze_world_region(
            &world,
            RegionBounds::new(Pos::new(0, 0, 0), Pos::new(4, 2, 1)),
        );
        let temporal = TemporalAnalysis::from_scene(&analysis.scene);
        let trace =
            simulate_transition_trace(&world, &analysis.scene, &temporal, 1, "same tick").unwrap();
        assert!(trace.len() >= 2);
        assert!(trace.transitions.windows(2).any(|pair| {
            pair[0].time.tick == pair[1].time.tick
                && pair[1]
                    .elapsed_from_previous
                    .as_ref()
                    .is_some_and(|elapsed| elapsed.is_zero())
        }));
    }
}
