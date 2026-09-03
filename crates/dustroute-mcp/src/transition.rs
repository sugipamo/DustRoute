use std::collections::{BTreeMap, BTreeSet};

use dustroute_ir::{
    BehaviorEvent, BehaviorTrace, EventCause, EventKind, EventSource, TraceStatus, TraceTimeUnit,
    TransitionPhase,
};
use dustroute_physical::{ComponentId, PhysicalScene};
use dustroute_translate::{MinecraftSnapshot, ScenarioEvent, ScenarioTrace};
use serde::{Deserialize, Serialize};

use crate::{BlockUpdateEvent, UpdateRecording};

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum TransitionSafety {
    Ready,
    PreviewOnly,
    Rejected,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum TransitionSafetyReason {
    DangerousBlock { name: String },
    TemporalDeviceRequiresReview { name: String },
    UnsupportedRedstoneDevice { name: String },
    NoLeverInput,
    MultipleLeverInputs { count: usize },
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct TransitionSafetyAssessment {
    pub safety: TransitionSafety,
    pub reasons: Vec<TransitionSafetyReason>,
}

#[must_use]
pub fn assess_transition_safety(snapshot: &MinecraftSnapshot) -> TransitionSafetyAssessment {
    let names = snapshot
        .blocks
        .iter()
        .map(|block| block.name.as_str())
        .collect::<BTreeSet<_>>();
    let mut reasons = Vec::new();
    for name in &names {
        if matches!(
            *name,
            "minecraft:tnt"
                | "minecraft:fire"
                | "minecraft:soul_fire"
                | "minecraft:lava"
                | "minecraft:water"
        ) {
            reasons.push(TransitionSafetyReason::DangerousBlock {
                name: (*name).to_owned(),
            });
        } else if matches!(
            *name,
            "minecraft:piston"
                | "minecraft:sticky_piston"
                | "minecraft:observer"
                | "minecraft:dispenser"
                | "minecraft:dropper"
                | "minecraft:hopper"
        ) {
            reasons.push(TransitionSafetyReason::TemporalDeviceRequiresReview {
                name: (*name).to_owned(),
            });
        } else if name.ends_with("_button")
            || name.ends_with("_pressure_plate")
            || matches!(
                *name,
                "minecraft:sculk_sensor"
                    | "minecraft:calibrated_sculk_sensor"
                    | "minecraft:tripwire_hook"
            )
        {
            reasons.push(TransitionSafetyReason::UnsupportedRedstoneDevice {
                name: (*name).to_owned(),
            });
        }
    }
    let lever_count = snapshot
        .blocks
        .iter()
        .filter(|block| block.name == "minecraft:lever")
        .count();
    match lever_count {
        0 => reasons.push(TransitionSafetyReason::NoLeverInput),
        1 => {}
        count => reasons.push(TransitionSafetyReason::MultipleLeverInputs { count }),
    }
    let safety = if reasons
        .iter()
        .any(|reason| matches!(reason, TransitionSafetyReason::DangerousBlock { .. }))
        || lever_count == 0
    {
        TransitionSafety::Rejected
    } else if reasons.iter().any(|reason| {
        matches!(
            reason,
            TransitionSafetyReason::TemporalDeviceRequiresReview { .. }
                | TransitionSafetyReason::UnsupportedRedstoneDevice { .. }
        )
    }) {
        TransitionSafety::PreviewOnly
    } else {
        TransitionSafety::Ready
    };
    TransitionSafetyAssessment { safety, reasons }
}

/// Converts bridge game-tick observations to the redstone-tick scenario
/// timeline. The physics-tick observer can straddle a server tick boundary, so
/// deltas are rounded to the nearest following redstone tick.
#[must_use]
pub fn scenario_trace_from_recording(
    recording: &UpdateRecording,
    observe: &BTreeSet<dustroute_physical::Pos>,
    duration_redstone_ticks: u64,
) -> ScenarioTrace {
    scenario_trace_from_recording_with_initial(recording, observe, duration_redstone_ticks, None)
}

/// Seeds unchanged components from the snapshot captured immediately before
/// the packet update recording began.
#[must_use]
pub fn scenario_trace_from_recording_with_initial(
    recording: &UpdateRecording,
    observe: &BTreeSet<dustroute_physical::Pos>,
    duration_redstone_ticks: u64,
    initial: Option<&dustroute_translate::MinecraftSnapshot>,
) -> ScenarioTrace {
    let mut trace = ScenarioTrace {
        duration_redstone_ticks,
        duration_game_ticks: Some(
            recording
                .stopped_game_tick
                .saturating_sub(recording.started_game_tick),
        ),
        time_unit: TraceTimeUnit::RedstoneTick,
        ..ScenarioTrace::default()
    };
    let mut last = BTreeMap::new();
    let mut initial_sub_tick_order = 0;
    for position in observe {
        let snapshot_state = initial
            .and_then(|snapshot| snapshot.blocks.iter().find(|block| block.pos == *position))
            .map(|block| {
                let strength = block
                    .properties
                    .get("power")
                    .and_then(|value| value.parse::<u8>().ok())
                    .unwrap_or_else(|| {
                        u8::from(
                            block
                                .properties
                                .get("powered")
                                .or_else(|| block.properties.get("lit"))
                                .and_then(|value| value.parse::<bool>().ok())
                                .unwrap_or(false),
                        ) * 15
                    });
                (strength, strength > 0)
            });
        let initial_event = recording
            .events
            .iter()
            .find(|event| event.pos == *position)
            .cloned();
        let event_state = initial_event
            .as_ref()
            .and_then(|event| event.before.as_ref())
            .map(|before| {
                (
                    strength_state(before),
                    powered_state(before).unwrap_or(false),
                )
            });
        if let Some(value) = event_state.or(snapshot_state) {
            let (event_kind, cause, source, cause_sequence, game_tick, phase) = initial_event
                .as_ref()
                .map(|event| {
                    (
                        event.event_kind,
                        event.cause,
                        event.source,
                        event.cause_sequence,
                        Some(event.game_tick),
                        event.phase,
                    )
                })
                .unwrap_or((
                    EventKind::StateTransition,
                    EventCause::InitialSnapshot,
                    EventSource::InitialSnapshot,
                    None,
                    Some(recording.started_game_tick),
                    TransitionPhase::Unknown,
                ));
            last.insert(*position, value);
            trace.events.push(ScenarioEvent {
                redstone_tick: 0,
                sub_tick_order: initial_sub_tick_order,
                game_tick,
                phase,
                event_kind,
                cause,
                source,
                cause_sequence,
                sequence: trace.events.len() as u64,
                position: *position,
                strength: value.0,
                powered: value.1,
            });
            initial_sub_tick_order += 1;
        }
    }
    let mut updates = recording
        .events
        .iter()
        .filter(|event| {
            observe.contains(&event.pos)
                && event
                    .game_tick
                    .saturating_sub(recording.started_game_tick)
                    .div_ceil(2)
                    <= duration_redstone_ticks
        })
        .collect::<Vec<_>>();
    updates.sort_by_key(|event| {
        (
            event.game_tick,
            event.phase,
            event.sub_tick_order,
            event.sequence,
        )
    });
    for update in updates {
        let Some(after) = update.after.as_ref() else {
            continue;
        };
        let value = (strength_state(after), powered_state(after).unwrap_or(false));
        if last.get(&update.pos).copied() == Some(value) {
            continue;
        }
        let delta = update.game_tick.saturating_sub(recording.started_game_tick);
        let redstone_tick = delta.div_ceil(2);
        if redstone_tick == 0
            && let Some(initial) = trace
                .events
                .iter_mut()
                .find(|event| event.position == update.pos && event.redstone_tick == 0)
        {
            initial.game_tick = Some(update.game_tick);
            initial.phase = update.phase;
            initial.strength = value.0;
            initial.powered = value.1;
            last.insert(update.pos, value);
            continue;
        }
        trace.events.push(ScenarioEvent {
            redstone_tick,
            sub_tick_order: update.sub_tick_order,
            game_tick: Some(update.game_tick),
            phase: update.phase,
            event_kind: update.event_kind,
            cause: update.cause,
            source: update.source,
            cause_sequence: update.cause_sequence,
            sequence: trace.events.len() as u64,
            position: update.pos,
            strength: value.0,
            powered: value.1,
        });
        last.insert(update.pos, value);
    }
    for position in observe {
        let (strength, powered) = last.get(position).copied().unwrap_or((0, false));
        trace.final_strengths.insert(*position, strength);
        trace.final_powered.insert(*position, powered);
    }
    trace.status = if recording.truncated {
        TraceStatus::Failed {
            error: "live update recording was truncated before the requested boundary".to_owned(),
        }
    } else {
        TraceStatus::Complete
    };
    trace
}

#[must_use]
pub fn behavior_trace_from_recording(
    recording: &UpdateRecording,
    scene: &PhysicalScene,
    label: impl Into<String>,
) -> BehaviorTrace {
    let component_at = scene
        .components
        .iter()
        .map(|component| (component.pos, component.id))
        .collect::<BTreeMap<_, _>>();
    let mut by_component = BTreeMap::<ComponentId, Vec<&BlockUpdateEvent>>::new();
    for event in &recording.events {
        if let Some(component) = component_at.get(&event.pos) {
            by_component.entry(*component).or_default().push(event);
        }
    }
    for updates in by_component.values_mut() {
        updates.sort_by_key(|event| {
            (
                event.game_tick,
                event.phase,
                event.sub_tick_order,
                event.sequence,
            )
        });
    }
    let mut events = Vec::new();
    for (component, updates) in by_component {
        let Some(first) = updates.first() else {
            continue;
        };
        if let Some(powered) = first.before.as_ref().and_then(powered_state) {
            events.push(BehaviorEvent {
                tick: first.game_tick.saturating_sub(recording.started_game_tick),
                sub_tick_order: first.sub_tick_order,
                game_tick: Some(first.game_tick),
                phase: first.phase,
                event_kind: first.event_kind,
                cause: first.cause,
                source: first.source,
                cause_sequence: first.cause_sequence,
                component,
                powered,
            });
        }
        let mut previous = first.before.as_ref().and_then(powered_state);
        for update in updates {
            let Some(powered) = update.after.as_ref().and_then(powered_state) else {
                continue;
            };
            if previous == Some(powered) {
                continue;
            }
            events.push(BehaviorEvent {
                tick: update.game_tick.saturating_sub(recording.started_game_tick),
                sub_tick_order: update.sub_tick_order,
                game_tick: Some(update.game_tick),
                phase: update.phase,
                event_kind: update.event_kind,
                cause: update.cause,
                source: update.source,
                cause_sequence: update.cause_sequence,
                component,
                powered,
            });
            previous = Some(powered);
        }
    }
    events.sort_by_key(|event| {
        (
            event.game_tick.unwrap_or(event.tick),
            event.phase,
            event.sub_tick_order,
            event.component,
        )
    });
    BehaviorTrace {
        label: label.into(),
        time_unit: TraceTimeUnit::GameTick,
        events,
        stable: !recording.truncated,
        status: if recording.truncated {
            TraceStatus::Failed {
                error: "live update recording was truncated before the requested boundary"
                    .to_owned(),
            }
        } else {
            TraceStatus::Complete
        },
    }
}

fn powered_state(state: &crate::ObservedBlockState) -> Option<bool> {
    for key in ["powered", "lit", "extended", "enabled"] {
        if let Some(value) = state.properties.get(key) {
            return value.parse().ok();
        }
    }
    state
        .properties
        .get("power")
        .and_then(|value| value.parse::<u8>().ok())
        .map(|power| power > 0)
}

fn strength_state(state: &crate::ObservedBlockState) -> u8 {
    state
        .properties
        .get("power")
        .and_then(|value| value.parse::<u8>().ok())
        .unwrap_or_else(|| {
            if matches!(
                state.name.as_str(),
                "minecraft:repeater"
                    | "minecraft:comparator"
                    | "minecraft:lever"
                    | "minecraft:redstone_torch"
                    | "minecraft:redstone_wall_torch"
                    | "minecraft:observer"
            ) || state.name.ends_with("_button")
                || state.name.ends_with("_pressure_plate")
            {
                powered_state(state).map_or(0, |powered| if powered { 15 } else { 0 })
            } else {
                0
            }
        })
}

#[cfg(test)]
mod tests {
    use dustroute_physical::{Block, BlockKind, Observation, Pos, SceneBounds, VerifiedTopology};
    use dustroute_translate::{
        MinecraftSnapshotBlock, Scenario, ScenarioDifference, compare_scenario_traces, run_scenario,
    };

    use super::*;
    use crate::{ObservedBlockState, UpdateRecording};

    #[test]
    fn rejects_danger_and_requires_review_for_temporal_devices() {
        let snapshot = MinecraftSnapshot {
            min: Pos::new(0, 0, 0),
            max: Pos::new(2, 0, 0),
            blocks: vec![
                MinecraftSnapshotBlock {
                    pos: Pos::new(0, 0, 0),
                    name: "minecraft:lever".to_owned(),
                    properties: BTreeMap::new(),
                },
                MinecraftSnapshotBlock {
                    pos: Pos::new(1, 0, 0),
                    name: "minecraft:piston".to_owned(),
                    properties: BTreeMap::new(),
                },
            ],
        };
        assert_eq!(
            assess_transition_safety(&snapshot).safety,
            TransitionSafety::PreviewOnly
        );
        let mut dangerous = snapshot;
        dangerous.blocks.push(MinecraftSnapshotBlock {
            pos: Pos::new(2, 0, 0),
            name: "minecraft:tnt".to_owned(),
            properties: BTreeMap::new(),
        });
        assert_eq!(
            assess_transition_safety(&dangerous).safety,
            TransitionSafety::Rejected
        );
    }

    #[test]
    fn converts_game_tick_updates_to_a_traceable_pulse() {
        let topology = VerifiedTopology::from_parts(
            vec![dustroute_physical::PhysicalComponent {
                id: ComponentId(0),
                pos: Pos::new(0, 1, 0),
                block: Block::new(BlockKind::RedstoneWire),
            }],
            [],
        );
        let scene = PhysicalScene::from_unvalidated_topology(
            Observation::complete(
                "test",
                SceneBounds::new(Pos::new(0, 0, 0), Pos::new(1, 2, 1)),
            ),
            &topology,
        );
        let state = |power: &str| ObservedBlockState {
            name: "minecraft:redstone_wire".to_owned(),
            properties: BTreeMap::from([("power".to_owned(), power.to_owned())]),
        };
        let recording = UpdateRecording {
            recording_id: "r".to_owned(),
            started_game_tick: 100,
            stopped_game_tick: 104,
            seen_events: 2,
            truncated: false,
            events: vec![
                BlockUpdateEvent {
                    sequence: 1,
                    game_tick: 101,
                    sub_tick_order: 0,
                    phase: TransitionPhase::Unknown,
                    event_kind: EventKind::StateTransition,
                    cause: EventCause::PacketObservation,
                    source: EventSource::LiveMineflayer,
                    cause_sequence: None,
                    pos: Pos::new(0, 1, 0),
                    before: Some(state("0")),
                    after: Some(state("15")),
                },
                BlockUpdateEvent {
                    sequence: 2,
                    game_tick: 102,
                    sub_tick_order: 0,
                    phase: TransitionPhase::Unknown,
                    event_kind: EventKind::StateTransition,
                    cause: EventCause::PacketObservation,
                    source: EventSource::LiveMineflayer,
                    cause_sequence: None,
                    pos: Pos::new(0, 1, 0),
                    before: Some(state("15")),
                    after: Some(state("0")),
                },
            ],
        };
        let trace = behavior_trace_from_recording(&recording, &scene, "lever on");
        assert_eq!(trace.time_unit, TraceTimeUnit::GameTick);
        let pulses = dustroute_ir::observe_pulses(&trace);
        assert_eq!(pulses[0].width_ticks, 1);
    }

    #[test]
    fn normalizes_physics_tick_observations_for_scenario_comparison() {
        let pos = Pos::new(1, 1, 0);
        let state = |powered: &str| ObservedBlockState {
            name: "minecraft:repeater".to_owned(),
            properties: BTreeMap::from([("powered".to_owned(), powered.to_owned())]),
        };
        let recording = UpdateRecording {
            recording_id: "live".to_owned(),
            started_game_tick: 100,
            stopped_game_tick: 112,
            seen_events: 1,
            truncated: false,
            events: vec![BlockUpdateEvent {
                sequence: 4,
                game_tick: 103,
                sub_tick_order: 0,
                phase: TransitionPhase::Unknown,
                event_kind: EventKind::StateTransition,
                cause: EventCause::PacketObservation,
                source: EventSource::LiveMineflayer,
                cause_sequence: None,
                pos,
                before: Some(state("false")),
                after: Some(state("true")),
            }],
        };
        let trace = scenario_trace_from_recording(&recording, &BTreeSet::from([pos]), 2);
        assert_eq!(trace.events[0].redstone_tick, 0);
        assert_eq!(trace.events[1].redstone_tick, 2);
        assert_eq!(trace.events[0].game_tick, Some(103));
        assert_eq!(trace.events[1].game_tick, Some(103));
        assert_eq!(trace.events[1].sequence, 1);
        assert_eq!(trace.duration_game_ticks, Some(12));
        assert_eq!(trace.status, TraceStatus::Complete);
        assert_eq!(trace.final_strengths[&pos], 15);
    }

    #[test]
    fn truncated_live_recording_cannot_claim_a_complete_transition_trace() {
        let pos = Pos::new(1, 1, 0);
        let mut recording = UpdateRecording {
            recording_id: "truncated".to_owned(),
            started_game_tick: 40,
            stopped_game_tick: 42,
            seen_events: 2,
            truncated: true,
            events: vec![BlockUpdateEvent {
                sequence: 1,
                game_tick: 40,
                sub_tick_order: 0,
                phase: TransitionPhase::Unknown,
                event_kind: EventKind::StateTransition,
                cause: EventCause::PacketObservation,
                source: EventSource::LiveMineflayer,
                cause_sequence: None,
                pos,
                before: None,
                after: Some(ObservedBlockState {
                    name: "minecraft:redstone_wire".to_owned(),
                    properties: BTreeMap::from([("power".to_owned(), "15".to_owned())]),
                }),
            }],
        };
        let trace = scenario_trace_from_recording(&recording, &BTreeSet::from([pos]), 1);
        assert!(trace.status.is_failed());
        assert!(!trace.status.is_complete());
        recording.truncated = false;
        let complete = scenario_trace_from_recording(&recording, &BTreeSet::from([pos]), 1);
        assert_eq!(complete.status, TraceStatus::Complete);
    }

    #[test]
    fn tick_zero_activation_replaces_the_pre_action_baseline() {
        let pos = Pos::new(1, 1, 0);
        let state = |powered: &str| ObservedBlockState {
            name: "minecraft:lever".to_owned(),
            properties: BTreeMap::from([("powered".to_owned(), powered.to_owned())]),
        };
        let recording = UpdateRecording {
            recording_id: "live".to_owned(),
            started_game_tick: 100,
            stopped_game_tick: 104,
            seen_events: 1,
            truncated: false,
            events: vec![BlockUpdateEvent {
                sequence: 1,
                game_tick: 100,
                sub_tick_order: 0,
                phase: TransitionPhase::Unknown,
                event_kind: EventKind::StateTransition,
                cause: EventCause::PacketObservation,
                source: EventSource::LiveMineflayer,
                cause_sequence: None,
                pos,
                before: Some(state("false")),
                after: Some(state("true")),
            }],
        };

        let trace = scenario_trace_from_recording(&recording, &BTreeSet::from([pos]), 2);
        assert_eq!(trace.events.len(), 1);
        assert_eq!(trace.events[0].redstone_tick, 0);
        assert_eq!(trace.events[0].game_tick, Some(100));
        assert!(trace.events[0].powered);
    }

    #[test]
    fn captured_java_scenario_matches_state_and_timing_and_classifies_order() {
        let scenario: Scenario = serde_json::from_str(include_str!(
            "../tests/fixtures/repeater_lamp_scenario.json"
        ))
        .unwrap();
        let recording: UpdateRecording = serde_json::from_str(include_str!(
            "../tests/fixtures/repeater_lamp_live_recording.json"
        ))
        .unwrap();
        let simulated = run_scenario(&scenario).unwrap();
        let live = scenario_trace_from_recording(
            &recording,
            &scenario.observe,
            scenario.duration_redstone_ticks,
        );
        let differences = compare_scenario_traces(&simulated.trace, &live);
        assert!(
            differences
                .iter()
                .all(|difference| matches!(difference, ScenarioDifference::EventOrder { .. }))
        );
        assert!(!differences.is_empty());
        assert_eq!(simulated.trace.final_strengths, live.final_strengths);
        assert_eq!(simulated.trace.final_powered, live.final_powered);
    }

    #[test]
    fn captured_wall_torch_scenario_matches_simulated_delay() {
        let scenario: Scenario =
            serde_json::from_str(include_str!("../tests/fixtures/wall_torch_scenario.json"))
                .unwrap();
        let recording: UpdateRecording = serde_json::from_str(include_str!(
            "../tests/fixtures/wall_torch_live_recording.json"
        ))
        .unwrap();
        let simulated = run_scenario(&scenario).unwrap();
        let live = scenario_trace_from_recording(
            &recording,
            &scenario.observe,
            scenario.duration_redstone_ticks,
        );
        assert!(compare_scenario_traces(&simulated.trace, &live).is_empty());
    }

    #[test]
    fn captured_repeater_lock_preserves_unchanged_powered_side_input() {
        let scenario: Scenario = serde_json::from_str(include_str!(
            "../tests/fixtures/repeater_lock_scenario.json"
        ))
        .unwrap();
        let recording: UpdateRecording = serde_json::from_str(include_str!(
            "../tests/fixtures/repeater_lock_live_recording.json"
        ))
        .unwrap();
        let simulated = run_scenario(&scenario).unwrap();
        let live = scenario_trace_from_recording_with_initial(
            &recording,
            &scenario.observe,
            scenario.duration_redstone_ticks,
            Some(&scenario.initial),
        );
        let differences = compare_scenario_traces(&simulated.trace, &live);
        assert!(live.final_powered[&Pos::new(2, 1, 1)]);
        assert_eq!(live.final_strengths[&Pos::new(2, 1, 2)], 15);
        assert_eq!(simulated.trace.final_powered, live.final_powered);
        assert_eq!(simulated.trace.final_strengths, live.final_strengths);
        assert!(differences.is_empty());
    }
}
