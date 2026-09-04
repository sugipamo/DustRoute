//! Deterministic time and event foundations for Minecraft physics.

mod engine;
mod event;
mod queue;
mod scheduler;
mod trace;
mod transition;

pub use engine::{
    DEFAULT_MAX_MICROSTEPS_PER_GAME_TICK, EventOutcome, ExecutionCheckpoint, ExecutionStateKey,
    PendingEventKey, PhysicsEngine, PhysicsEngineError, ShapeTransition, WorldChange,
};
pub use event::{
    BlockEventKind, EventCause, EventId, PhysicsEvent, PhysicsEventKind, PhysicsEventPhase,
    PhysicsTime, QueuedEvent,
};
pub use queue::PhysicsEventQueue;
pub use scheduler::{
    SamePhaseOrder, SchedulerEvidence, SchedulerProfile, SchedulerProfileError, SchedulerProfileId,
    ZeroDelayPolicy,
};
pub use trace::StateTransition;
pub use transition::{
    EventExecutionStatus, EventRecord, EventTrace, TraceStatus, TransitionElapsed, TransitionId,
    TransitionRecord, TransitionStep, TransitionTrace,
};
