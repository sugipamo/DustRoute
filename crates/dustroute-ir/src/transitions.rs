//! Transition-first behavior traces.
//!
//! `BehaviorTrace` is kept as the compatibility representation used by the
//! older timing and pulse helpers.  This module gives consumers an explicit
//! state-changing edge with an opaque ID and elapsed time.  The conversion is
//! intentionally lossless for the evidence currently present in
//! `BehaviorEvent`; it does not invent a scheduler cause or a missing
//! before-value.

use std::collections::BTreeMap;

use dustroute_physical::ComponentId;
use serde::{Deserialize, Serialize};

use crate::{
    BehaviorEvent, BehaviorTrace, EventCause, EventKind, EventSource, TraceStatus, TraceTimeUnit,
};

/// Stable identity within one transition trace.
#[derive(
    Clone, Copy, Debug, Default, Deserialize, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize,
)]
pub struct TransitionId(pub u64);

/// Scheduler phase associated with a logical transition.
///
/// `Unknown` is intentional for packet observations that preserve game-tick
/// and packet order but cannot identify the vanilla scheduler phase. The
/// remaining variants mirror the coarse phase vocabulary of the Minecraft
/// physics boundary without making this cross-layer IR depend on that crate.
#[derive(
    Clone, Copy, Debug, Default, Deserialize, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize,
)]
#[serde(rename_all = "snake_case")]
pub enum TransitionPhase {
    #[default]
    Unknown,
    External,
    NeighborUpdate,
    ScheduledTick,
    BlockEvent,
    BlockEntity,
    Observation,
}

impl TransitionPhase {
    #[must_use]
    pub const fn is_unknown(&self) -> bool {
        matches!(self, Self::Unknown)
    }
}

/// Position of an observed transition in the trace's declared time unit.
#[derive(Clone, Copy, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
pub struct TransitionTime {
    pub tick: u64,
    #[serde(default)]
    pub sub_tick_order: u64,
    /// Exact Minecraft game tick when the source provides it. `tick` keeps
    /// the compatibility unit declared by the containing trace.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub game_tick: Option<u64>,
    /// Coarse scheduler phase. Unknown is omitted from legacy JSON.
    #[serde(default, skip_serializing_if = "TransitionPhase::is_unknown")]
    pub phase: TransitionPhase,
}

/// Elapsed time between two observed transitions.
///
/// A same-tick transition is not collapsed into an ordinary zero delay: its
/// order delta remains available to consumers that need to reason about
/// pulses or update chains.  `ExactTicks` uses the `TraceTimeUnit` declared on
/// the containing trace (redstone tick or game tick).
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum TransitionElapsed {
    SameTick {
        order_delta: u64,
    },
    ExactTicks {
        ticks: u64,
    },
    TickRange {
        minimum_ticks: u64,
        maximum_ticks: u64,
    },
    Unavailable {
        reason: String,
    },
}

impl TransitionElapsed {
    #[must_use]
    pub fn between(previous: TransitionTime, current: TransitionTime) -> Self {
        if previous.tick == current.tick {
            Self::SameTick {
                order_delta: current
                    .sub_tick_order
                    .saturating_sub(previous.sub_tick_order),
            }
        } else {
            Self::ExactTicks {
                ticks: current.tick.saturating_sub(previous.tick),
            }
        }
    }

    #[must_use]
    pub const fn is_zero(&self) -> bool {
        matches!(self, Self::SameTick { .. })
    }
}

/// Exact elapsed ordering in the Minecraft game-tick coordinate, independent
/// of the trace's compatibility tick unit.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum LogicalElapsed {
    SameGameTick {
        from_phase: TransitionPhase,
        to_phase: TransitionPhase,
        order_delta: u64,
    },
    ExactGameTicks {
        game_ticks: u64,
    },
}

impl LogicalElapsed {
    #[must_use]
    pub fn between(previous: TransitionTime, current: TransitionTime) -> Option<Self> {
        let (Some(previous_tick), Some(current_tick)) = (previous.game_tick, current.game_tick)
        else {
            return None;
        };
        if previous_tick == current_tick {
            Some(Self::SameGameTick {
                from_phase: previous.phase,
                to_phase: current.phase,
                order_delta: current
                    .sub_tick_order
                    .saturating_sub(previous.sub_tick_order),
            })
        } else {
            Some(Self::ExactGameTicks {
                game_ticks: current_tick.saturating_sub(previous_tick),
            })
        }
    }
}

/// One state-changing observation in a transition-first trace.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct TransitionRecord {
    pub id: TransitionId,
    pub time: TransitionTime,
    /// `None` for the first observed transition.  The before-value is also
    /// optional because a live boundary may start after the prior state.
    #[serde(default)]
    pub elapsed_from_previous: Option<TransitionElapsed>,
    /// Lossless elapsed ordering when the source supplied exact game-tick
    /// coordinates. This is optional for legacy redstone-tick traces.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub logical_elapsed_from_previous: Option<LogicalElapsed>,
    pub component: ComponentId,
    #[serde(default)]
    pub from_powered: Option<bool>,
    pub powered: bool,
    #[serde(default)]
    pub event_kind: EventKind,
    #[serde(default)]
    pub cause: EventCause,
    #[serde(default)]
    pub source: EventSource,
    #[serde(default)]
    pub cause_sequence: Option<u64>,
}

/// Ordered state-changing edges for one behavior observation.
#[derive(Clone, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
pub struct TransitionTrace {
    pub label: String,
    #[serde(default)]
    pub time_unit: TraceTimeUnit,
    pub transitions: Vec<TransitionRecord>,
    pub stable: bool,
    /// Explicit completion state. `stable` remains for compatibility; callers
    /// must use this status for fail-closed checks.
    #[serde(default)]
    pub status: TraceStatus,
}

impl TransitionTrace {
    /// Projects the compatibility event trace into explicit transition
    /// records. Events are retained in source order; no scheduler order is
    /// inferred when the source did not provide one.
    #[must_use]
    pub fn from_behavior_trace(trace: &BehaviorTrace) -> Self {
        let mut previous_by_component = BTreeMap::<ComponentId, bool>::new();
        let mut previous_time = None;
        let transitions = trace
            .events
            .iter()
            .enumerate()
            .map(|(index, event)| {
                let time = TransitionTime {
                    tick: event.tick,
                    sub_tick_order: event.sub_tick_order,
                    game_tick: event.game_tick,
                    phase: event.phase,
                };
                let record = TransitionRecord {
                    id: TransitionId(index as u64),
                    time,
                    elapsed_from_previous: previous_time
                        .map(|previous| TransitionElapsed::between(previous, time)),
                    logical_elapsed_from_previous: previous_time
                        .and_then(|previous| LogicalElapsed::between(previous, time)),
                    component: event.component,
                    from_powered: previous_by_component.insert(event.component, event.powered),
                    powered: event.powered,
                    event_kind: event.event_kind,
                    cause: event.cause,
                    source: event.source,
                    cause_sequence: event.cause_sequence,
                };
                previous_time = Some(time);
                record
            })
            .collect();
        Self {
            label: trace.label.clone(),
            time_unit: trace.time_unit,
            transitions,
            stable: trace.stable,
            status: trace.status.clone(),
        }
    }

    /// Projects back to the legacy representation. This is an adapter, not a
    /// scheduler simulation: records that lack a before-value remain ordinary
    /// state observations in the resulting event trace.
    #[must_use]
    pub fn to_behavior_trace(&self) -> BehaviorTrace {
        BehaviorTrace {
            label: self.label.clone(),
            time_unit: self.time_unit,
            events: self
                .transitions
                .iter()
                .map(|transition| BehaviorEvent {
                    tick: transition.time.tick,
                    sub_tick_order: transition.time.sub_tick_order,
                    game_tick: transition.time.game_tick,
                    phase: transition.time.phase,
                    event_kind: transition.event_kind,
                    cause: transition.cause,
                    source: transition.source,
                    cause_sequence: transition.cause_sequence,
                    component: transition.component,
                    powered: transition.powered,
                })
                .collect(),
            stable: self.stable,
            status: self.status.clone(),
        }
    }

    #[must_use]
    pub fn len(&self) -> usize {
        self.transitions.len()
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.transitions.is_empty()
    }

    pub fn iter(&self) -> impl Iterator<Item = &TransitionRecord> {
        self.transitions.iter()
    }

    #[must_use]
    pub const fn status(&self) -> &TraceStatus {
        &self.status
    }
}

impl BehaviorTrace {
    /// Returns the transition-first view without changing the legacy event
    /// representation or its serde shape.
    #[must_use]
    pub fn transition_trace(&self) -> TransitionTrace {
        TransitionTrace::from_behavior_trace(self)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn conversion_retains_before_values_and_same_tick_order() {
        let trace = BehaviorTrace {
            label: "same tick pulse".to_owned(),
            time_unit: TraceTimeUnit::GameTick,
            events: vec![
                BehaviorEvent {
                    tick: 3,
                    sub_tick_order: 0,
                    game_tick: Some(20),
                    phase: TransitionPhase::BlockEvent,
                    event_kind: EventKind::PulseStart,
                    cause: EventCause::ObserverFrontStateChange,
                    source: EventSource::Simulator,
                    cause_sequence: Some(7),
                    component: ComponentId(2),
                    powered: true,
                },
                BehaviorEvent {
                    tick: 3,
                    sub_tick_order: 1,
                    game_tick: Some(20),
                    phase: TransitionPhase::BlockEvent,
                    event_kind: EventKind::PulseEnd,
                    cause: EventCause::ObserverFrontStateChange,
                    source: EventSource::Simulator,
                    cause_sequence: Some(8),
                    component: ComponentId(2),
                    powered: false,
                },
            ],
            stable: true,
            status: TraceStatus::Complete,
        };
        let transitions = trace.transition_trace();
        assert_eq!(transitions.len(), 2);
        assert_eq!(transitions.transitions[0].id, TransitionId(0));
        assert_eq!(transitions.transitions[0].from_powered, None);
        assert_eq!(transitions.transitions[1].from_powered, Some(true));
        assert_eq!(
            transitions.transitions[1].elapsed_from_previous,
            Some(TransitionElapsed::SameTick { order_delta: 1 })
        );
        assert_eq!(
            transitions.transitions[1].logical_elapsed_from_previous,
            Some(LogicalElapsed::SameGameTick {
                from_phase: TransitionPhase::BlockEvent,
                to_phase: TransitionPhase::BlockEvent,
                order_delta: 1,
            })
        );
        assert_eq!(transitions.to_behavior_trace(), trace);
    }

    #[test]
    fn elapsed_ticks_are_in_the_trace_declared_unit() {
        let trace = BehaviorTrace {
            label: "delayed".to_owned(),
            time_unit: TraceTimeUnit::RedstoneTick,
            events: vec![
                BehaviorEvent {
                    tick: 1,
                    sub_tick_order: 0,
                    game_tick: None,
                    phase: TransitionPhase::Unknown,
                    event_kind: EventKind::StateTransition,
                    cause: EventCause::Unknown,
                    source: EventSource::Unknown,
                    cause_sequence: None,
                    component: ComponentId(1),
                    powered: false,
                },
                BehaviorEvent {
                    tick: 4,
                    sub_tick_order: 0,
                    game_tick: None,
                    phase: TransitionPhase::Unknown,
                    event_kind: EventKind::SignalPropagation,
                    cause: EventCause::RepeaterDelay,
                    source: EventSource::Simulator,
                    cause_sequence: None,
                    component: ComponentId(1),
                    powered: true,
                },
            ],
            stable: true,
            status: TraceStatus::Complete,
        };
        assert_eq!(
            trace.transition_trace().transitions[1].elapsed_from_previous,
            Some(TransitionElapsed::ExactTicks { ticks: 3 })
        );
    }

    #[test]
    fn preserves_exact_game_tick_elapsed_in_a_redstone_tick_view() {
        let trace = BehaviorTrace {
            label: "live game tick evidence".to_owned(),
            time_unit: TraceTimeUnit::RedstoneTick,
            events: vec![
                BehaviorEvent {
                    tick: 0,
                    sub_tick_order: 0,
                    game_tick: Some(100),
                    phase: TransitionPhase::Observation,
                    event_kind: EventKind::StateTransition,
                    cause: EventCause::PacketObservation,
                    source: EventSource::LiveMineflayer,
                    cause_sequence: None,
                    component: ComponentId(1),
                    powered: false,
                },
                BehaviorEvent {
                    tick: 1,
                    sub_tick_order: 0,
                    game_tick: Some(103),
                    phase: TransitionPhase::Observation,
                    event_kind: EventKind::StateTransition,
                    cause: EventCause::PacketObservation,
                    source: EventSource::LiveMineflayer,
                    cause_sequence: None,
                    component: ComponentId(1),
                    powered: true,
                },
            ],
            stable: true,
            status: TraceStatus::Complete,
        };
        let transitions = trace.transition_trace();
        assert_eq!(
            transitions.transitions[1].elapsed_from_previous,
            Some(TransitionElapsed::ExactTicks { ticks: 1 })
        );
        assert_eq!(
            transitions.transitions[1].logical_elapsed_from_previous,
            Some(LogicalElapsed::ExactGameTicks { game_ticks: 3 })
        );
        let json = serde_json::to_value(&transitions).unwrap();
        assert_eq!(json["transitions"][1]["time"]["game_tick"], 103);
        assert_eq!(
            json["transitions"][1]["logical_elapsed_from_previous"]["game_ticks"],
            3
        );
    }
}
