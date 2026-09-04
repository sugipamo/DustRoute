use serde::{Deserialize, Serialize};

use crate::{PistonAction, PistonPlan, Pos};

#[derive(Clone, Copy, Debug, Deserialize, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize)]
pub struct EventId(pub u64);

/// Coarse scheduler phase used to order events that share a game tick.
///
/// The enum is deliberately separate from [`PhysicsEventKind`]: two events
/// with similar payloads may be delivered by different sources in a future
/// versioned scheduler. [`super::SchedulerProfile`] supplies the active phase
/// order; the declaration order is only the compatibility profile's default,
/// not a claim that packet observation exposes vanilla's complete scheduler.
#[derive(
    Clone, Copy, Debug, Default, Deserialize, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize,
)]
#[serde(rename_all = "snake_case")]
pub enum PhysicsEventPhase {
    #[default]
    External,
    NeighborUpdate,
    ScheduledTick,
    BlockEvent,
    BlockEntity,
    Observation,
}

#[derive(
    Clone, Copy, Debug, Default, Deserialize, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize,
)]
pub struct PhysicsTime {
    pub game_tick: u64,
    /// Coarse event phase within `game_tick`.  It is ordered before the
    /// deterministic insertion sequence below.
    #[serde(default)]
    pub phase: PhysicsEventPhase,
    /// Deterministic sequence within the same game tick and phase.
    /// Zero is valid and is not a duration.
    #[serde(default)]
    pub sub_tick_order: u64,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum BlockEventKind {
    PistonExtend,
    PistonRetract,
    Custom { event_type: u8, data: u8 },
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum PhysicsEventKind {
    /// Applies a typed external redstone source edge, then emits neighbor
    /// updates for adjacent pistons.  This is the input boundary for the
    /// redstone-driven piston runner; it does not model upstream propagation.
    RedstoneInput {
        powered: bool,
    },
    NeighborUpdate {
        source: Pos,
    },
    ScheduledBlockTick,
    BlockEvent {
        event: BlockEventKind,
    },
    /// Internal completion event emitted after a piston has entered its
    /// moving state. The plan is rebased against that start state and is
    /// revalidated atomically when this event executes.
    PistonComplete {
        action: PistonAction,
        plan: Box<PistonPlan>,
    },
    UserAction {
        action: String,
    },
}

impl PhysicsEventKind {
    /// Returns the default scheduler phase for this event kind. Callers that
    /// have stronger source evidence may use the queue's explicit phase API;
    /// the active [`super::SchedulerProfile`] decides when that phase runs.
    #[must_use]
    pub const fn default_phase(&self) -> PhysicsEventPhase {
        match self {
            Self::RedstoneInput { .. } => PhysicsEventPhase::External,
            Self::NeighborUpdate { .. } => PhysicsEventPhase::NeighborUpdate,
            Self::ScheduledBlockTick => PhysicsEventPhase::ScheduledTick,
            Self::BlockEvent { .. } => PhysicsEventPhase::BlockEvent,
            Self::PistonComplete { .. } => PhysicsEventPhase::BlockEntity,
            Self::UserAction { .. } => PhysicsEventPhase::External,
        }
    }
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum EventCause {
    External,
    Event { id: EventId },
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct PhysicsEvent {
    pub id: EventId,
    pub time: PhysicsTime,
    pub target: Pos,
    pub cause: EventCause,
    pub kind: PhysicsEventKind,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct QueuedEvent {
    /// Delay in game ticks from the parent event. Zero is valid. The active
    /// scheduler profile decides whether it stays in the same game tick or
    /// crosses to the next one; phase and `sub_tick_order` retain the causal
    /// order either way.
    pub delay_ticks: u64,
    pub target: Pos,
    /// Explicit phase for the child event.  The handler normally uses the
    /// phase implied by its kind, but keeping it on the queued edge allows a
    /// versioned scheduler to model a source-specific phase later.
    pub phase: PhysicsEventPhase,
    pub kind: PhysicsEventKind,
}
