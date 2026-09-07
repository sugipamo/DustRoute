//! First-class state transitions for the Minecraft physics boundary.
//!
//! The scheduler still uses ticks internally, but consumers should reason
//! about an ordered transition from one observed state to another.  Events
//! are retained separately because an event may be processed without a state
//! change, or may be rejected and remain pending.

use serde::{Deserialize, Serialize};

use super::{EventId, PhysicsEvent, PhysicsTime};
use crate::{BlockChange, BlockMove, DeltaCause, ShapeId, StateId};

/// Stable identity of one accepted state transition.
#[derive(
    Clone, Copy, Debug, Default, Deserialize, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize,
)]
pub struct TransitionId(pub u64);

/// Elapsed scheduler time between two accepted transitions.
///
/// `Zero` means that the game tick did not advance; the order delta and the
/// source transition's [`PhysicsTime`] retain the within-tick causality. A
/// range or unavailable value is used when a live boundary cannot determine a
/// single delay without inventing precision.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum TransitionElapsed {
    Zero {
        order_delta: u64,
    },
    ExactGameTicks {
        game_ticks: u64,
    },
    GameTickRange {
        minimum_game_ticks: u64,
        maximum_game_ticks: u64,
    },
    Unavailable {
        reason: String,
    },
}

impl TransitionElapsed {
    /// Derives a deterministic elapsed value from two executed event times.
    /// Callers that only have a partial live trace should construct a range or
    /// `Unavailable` value instead of rounding it into a scalar tick count.
    #[must_use]
    pub fn between(previous: PhysicsTime, current: PhysicsTime) -> Self {
        if previous.game_tick == current.game_tick {
            Self::Zero {
                order_delta: current
                    .sub_tick_order
                    .saturating_sub(previous.sub_tick_order),
            }
        } else {
            Self::ExactGameTicks {
                game_ticks: current.game_tick.saturating_sub(previous.game_tick),
            }
        }
    }

    #[must_use]
    pub const fn is_zero(&self) -> bool {
        matches!(self, Self::Zero { .. })
    }
}

/// Result of one event execution. A successful event can have no transition;
/// this distinction prevents no-op events from disappearing from causal
/// diagnostics while keeping transition analysis focused on actual changes.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum EventExecutionStatus {
    NoTransition,
    Transition { id: TransitionId },
}

/// Completion state of an append-only execution trace.
///
/// A transition prefix is useful diagnostics, but it must not be presented as
/// a complete run after a rejected event or a budget error.  The status is
/// deliberately separate from the records so callers can fail closed without
/// discarding the evidence that was already accepted.
#[derive(Clone, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum TraceStatus {
    /// The engine has work in progress, or the caller has not attempted to
    /// drain a newly-created engine yet.
    #[default]
    InProgress,
    Complete,
    Failed {
        error: String,
    },
}

impl TraceStatus {
    #[must_use]
    pub const fn is_complete(&self) -> bool {
        matches!(self, Self::Complete)
    }

    #[must_use]
    pub const fn is_failed(&self) -> bool {
        matches!(self, Self::Failed { .. })
    }
}

/// An event that completed successfully in the transition engine.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct EventRecord {
    pub event: PhysicsEvent,
    pub status: EventExecutionStatus,
}

/// One state-changing edge in the physics transition graph.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct TransitionRecord {
    pub id: TransitionId,
    pub trigger: EventId,
    pub time: PhysicsTime,
    /// `None` for the first accepted transition in a run; subsequent records
    /// express elapsed time from the previous state-changing transition.
    #[serde(default)]
    pub elapsed_from_previous: Option<TransitionElapsed>,
    pub from_state: StateId,
    pub to_state: StateId,
    /// Geometry-only identities remain available for incremental topology
    /// caches. They may be equal for a signal-only transition.
    pub from_shape: ShapeId,
    pub to_shape: ShapeId,
    pub changes: Vec<BlockChange>,
    pub moves: Vec<BlockMove>,
    #[serde(default)]
    pub cause: Option<DeltaCause>,
}

/// Append-only transition ledger.
#[derive(Clone, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
pub struct TransitionTrace {
    pub records: Vec<TransitionRecord>,
    /// Whether this ledger represents a complete drain or only a prefix.
    #[serde(default)]
    pub status: TraceStatus,
}

impl TransitionTrace {
    #[must_use]
    pub fn len(&self) -> usize {
        self.records.len()
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.records.is_empty()
    }

    pub fn iter(&self) -> impl Iterator<Item = &TransitionRecord> {
        self.records.iter()
    }

    #[must_use]
    pub const fn status(&self) -> &TraceStatus {
        &self.status
    }
}

/// Append-only record of all successfully handled events, including events
/// that did not change state.
#[derive(Clone, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
pub struct EventTrace {
    pub records: Vec<EventRecord>,
    /// Whether this ledger represents a complete drain or only a prefix.
    #[serde(default)]
    pub status: TraceStatus,
}

impl EventTrace {
    #[must_use]
    pub fn len(&self) -> usize {
        self.records.len()
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.records.is_empty()
    }

    pub fn iter(&self) -> impl Iterator<Item = &EventRecord> {
        self.records.iter()
    }

    #[must_use]
    pub const fn status(&self) -> &TraceStatus {
        &self.status
    }
}

/// Return value of one transition-engine step. The event is always present;
/// `transition` is `None` when the handler completed without changing state.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct TransitionStep {
    pub event: EventRecord,
    pub transition: Option<TransitionRecord>,
}
