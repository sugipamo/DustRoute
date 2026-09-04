use std::collections::BTreeMap;

use dustroute_ir::{
    BehaviorTrace, EventCause, EventKind, EventSource, LogicalElapsed, TemporalAnalysis,
    TraceStatus, TraceTimeUnit, TransitionElapsed, TransitionId, TransitionPhase, TransitionRecord,
    TransitionTime, TransitionTrace,
};
use dustroute_minecraft::time::PhysicsEventPhase;
use dustroute_physical::{BlockKind, ComponentId, PhysicalScene, Pos, World};

use crate::{RedstoneTickSimulator, SimulationEventKind, TickState};

/// Compatibility projection of the canonical transition simulation.
///
/// The translate layer now constructs a [`TransitionTrace`] first.  The
/// historical `BehaviorTrace` remains available to timing and pulse callers,
/// but it is an explicit view of the transition result rather than the
/// simulation's source of truth.
pub fn simulate_behavior_trace(
    world: &World,
    physical: &PhysicalScene,
    temporal: &TemporalAnalysis,
    ticks: usize,
    label: impl Into<String>,
) -> Result<BehaviorTrace, String> {
    Ok(simulate_transition_trace(world, physical, temporal, ticks, label)?.to_behavior_trace())
}

/// Simulates the compatibility timeline and emits its state-changing edges as
/// the canonical transition-first IR.
///
/// `ticks` remains the bounded compatibility-scheduler horizon; it is not the
/// identity of a transition. Each compatibility tick is drained through
/// [`RedstoneTickSimulator::step_event`], so the returned trace can contain
/// several same-game-tick component edges. Only actual component edges enter
/// the returned `TransitionTrace`; time-only scheduler events remain
/// available from the simulator's step API as explicit `NoOp` values.
pub fn simulate_transition_trace(
    world: &World,
    physical: &PhysicalScene,
    temporal: &TemporalAnalysis,
    ticks: usize,
    label: impl Into<String>,
) -> Result<TransitionTrace, String> {
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
    let mut previous_time = None;
    let mut transitions = Vec::new();
    let mut changed_on_last_tick = false;
    // Trace order is allocated independently from scheduler-event order: one
    // event may expose multiple component edges, and each edge needs a unique
    // same-game-tick coordinate.
    let mut next_sub_tick_order = BTreeMap::<u64, u64>::new();

    for step_index in 0..=ticks {
        let steps = if step_index == 0 {
            vec![None]
        } else {
            simulator
                .advance_tick_events()
                .map_err(|error| error.to_string())?
                .into_iter()
                .map(Some)
                .collect()
        };
        changed_on_last_tick = false;

        for transition in steps {
            let state = transition
                .as_ref()
                .map_or_else(|| simulator.snapshot(), |step| step.to.clone());
            let game_tick = transition.as_ref().map_or(0, |step| step.time.game_tick);
            let phase = transition
                .as_ref()
                .map_or(TransitionPhase::Unknown, |step| {
                    transition_phase(step.time.phase)
                });
            for component in &devices {
                let Some(pos) = positions.get(component) else {
                    continue;
                };
                let block_kind = physical
                    .components
                    .iter()
                    .find(|item| item.id == *component)
                    .map(|item| item.block.kind);
                let powered = component_powered(&state, *pos, block_kind);
                let changed = transition.as_ref().map_or_else(
                    || previous.get(component).copied() != Some(powered),
                    |step| {
                        component_powered(&step.from, *pos, block_kind)
                            != component_powered(&step.to, *pos, block_kind)
                    },
                );
                if !changed {
                    continue;
                }

                // Device-specific causal labels are assigned from the
                // physical block and scheduler event. The old value is
                // retained directly in the transition record instead of
                // being recovered by a later BehaviorTrace conversion.
                let from_powered = previous.insert(*component, powered);
                let scheduler_kind = transition.as_ref().map(|step| step.event_kind);
                let (event_kind, cause) = if state.tick == 0 && from_powered.is_none() {
                    (EventKind::StateTransition, EventCause::InitialSnapshot)
                } else {
                    match scheduler_kind {
                        Some(SimulationEventKind::ObserverPulseStart) => {
                            (EventKind::PulseStart, EventCause::ObserverFrontStateChange)
                        }
                        Some(SimulationEventKind::ObserverPulseEnd) => {
                            (EventKind::PulseEnd, EventCause::ObserverFrontStateChange)
                        }
                        Some(SimulationEventKind::RepeaterUpdate) => {
                            (EventKind::SignalPropagation, EventCause::RepeaterDelay)
                        }
                        _ => match block_kind {
                            Some(BlockKind::Observer) => (
                                EventKind::StateTransition,
                                EventCause::ObserverFrontStateChange,
                            ),
                            _ => (
                                EventKind::SignalPropagation,
                                EventCause::SimulatorPropagation,
                            ),
                        },
                    }
                };
                let source = if cause == EventCause::InitialSnapshot {
                    EventSource::InitialSnapshot
                } else {
                    EventSource::Simulator
                };
                let sub_tick_order = next_sub_tick_order.entry(game_tick).or_default();
                let time = TransitionTime {
                    tick: state.tick,
                    sub_tick_order: *sub_tick_order,
                    // The initial snapshot is anchored at game tick zero.
                    // Its phase remains unknown because it is an observation
                    // boundary, not a processed event.
                    game_tick: Some(game_tick),
                    phase,
                };
                *sub_tick_order = sub_tick_order.saturating_add(1);
                let record = TransitionRecord {
                    id: TransitionId(transitions.len() as u64),
                    time,
                    elapsed_from_previous: previous_time
                        .map(|previous| TransitionElapsed::between(previous, time)),
                    logical_elapsed_from_previous: previous_time
                        .and_then(|previous| LogicalElapsed::between(previous, time)),
                    component: *component,
                    from_powered,
                    powered,
                    event_kind,
                    cause,
                    source,
                    cause_sequence: None,
                };
                previous_time = Some(time);
                transitions.push(record);
                changed_on_last_tick = true;
            }
        }
    }

    Ok(TransitionTrace {
        label: label.into(),
        time_unit: TraceTimeUnit::RedstoneTick,
        transitions,
        stable: !changed_on_last_tick,
        status: if changed_on_last_tick {
            TraceStatus::InProgress
        } else {
            TraceStatus::Complete
        },
    })
}

fn component_powered(state: &TickState, pos: Pos, kind: Option<BlockKind>) -> bool {
    match kind {
        Some(BlockKind::Repeater) => state.repeater_powered.get(&pos).copied().unwrap_or(false),
        Some(BlockKind::Comparator) => state.comparator_output.get(&pos).copied().unwrap_or(0) > 0,
        Some(BlockKind::Observer) => state.observer_powered.get(&pos).copied().unwrap_or(false),
        Some(BlockKind::RedstoneTorch) => state.torch_lit.get(&pos).copied().unwrap_or(false),
        Some(BlockKind::RedstoneLamp) => state.lamp_lit.get(&pos).copied().unwrap_or(false),
        _ => state.powered(pos),
    }
}

fn transition_phase(phase: PhysicsEventPhase) -> TransitionPhase {
    match phase {
        PhysicsEventPhase::External => TransitionPhase::External,
        PhysicsEventPhase::NeighborUpdate => TransitionPhase::NeighborUpdate,
        PhysicsEventPhase::ScheduledTick => TransitionPhase::ScheduledTick,
        PhysicsEventPhase::BlockEvent => TransitionPhase::BlockEvent,
        PhysicsEventPhase::BlockEntity => TransitionPhase::BlockEntity,
        PhysicsEventPhase::Observation => TransitionPhase::Observation,
    }
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

        let transitions =
            simulate_transition_trace(&world, &analysis.scene, &temporal, 3, "powered input")
                .unwrap();
        assert!(
            transitions
                .transitions
                .iter()
                .any(|transition| { transition.time.tick == 2 && transition.powered })
        );
        assert!(transitions.len() >= 2);
        assert!(
            transitions
                .transitions
                .iter()
                .all(|transition| transition.time.tick != 1)
        );
        assert_eq!(
            transitions.transitions[1].elapsed_from_previous,
            Some(TransitionElapsed::ExactTicks { ticks: 2 })
        );
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
        let compatibility =
            simulate_behavior_trace(&world, &analysis.scene, &temporal, 1, "same tick").unwrap();
        assert_eq!(trace.to_behavior_trace(), compatibility);
    }
}
