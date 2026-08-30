//! Deterministic time and event foundations for Minecraft physics.

mod engine;
mod event;
mod queue;
mod trace;

pub use engine::{EventOutcome, PhysicsEngine, PhysicsEngineError, WorldChange};
pub use event::{
    BlockEventKind, EventCause, EventId, PhysicsEvent, PhysicsEventKind, PhysicsTime, QueuedEvent,
};
pub use queue::PhysicsEventQueue;
pub use trace::StateTransition;
