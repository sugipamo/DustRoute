use serde::{Deserialize, Serialize};

use crate::Pos;

#[derive(Clone, Copy, Debug, Deserialize, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize)]
pub struct EventId(pub u64);

#[derive(
    Clone, Copy, Debug, Default, Deserialize, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize,
)]
pub struct PhysicsTime {
    pub game_tick: u64,
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
    NeighborUpdate { source: Pos },
    ScheduledBlockTick,
    BlockEvent { event: BlockEventKind },
    UserAction { action: String },
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
    pub delay_ticks: u64,
    pub target: Pos,
    pub kind: PhysicsEventKind,
}
