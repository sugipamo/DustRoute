use std::collections::{BTreeMap, BTreeSet};
use std::error::Error;
use std::fmt::{Display, Formatter};

use serde::{Deserialize, Serialize};

use super::queue::QueuePopCheckpoint;
use super::{
    BlockEventKind, EventCause, EventExecutionStatus, EventRecord, EventTrace, PhysicsEvent,
    PhysicsEventKind, PhysicsEventPhase, PhysicsEventQueue, PhysicsTime, QueuedEvent,
    StateTransition, TraceStatus, TransitionElapsed, TransitionId, TransitionRecord,
    TransitionStep, TransitionTrace,
};
use crate::{
    Block, BlockKind, ChangeReason, DEFAULT_PISTON_MOTION_PROFILE, PistonAction, PistonError,
    PistonMotionProfile, PistonMotionProfileError, Pos, Region, ShapeId, World, WorldDelta,
    WorldDeltaError, plan_piston, plan_piston_in_region,
};

/// Default guard for zero-delay event chains. This is deliberately separate
/// from the total event budget: a circuit may process many game ticks while a
/// single same-tick feedback loop must still be bounded.
pub const DEFAULT_MAX_MICROSTEPS_PER_GAME_TICK: usize = 4096;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct WorldChange {
    pub position: Pos,
    pub after: Block,
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct EventOutcome {
    /// Legacy per-position output for signal handlers that do not yet expose
    /// a geometry transition.  Mechanical handlers should use `delta`.
    pub changes: Vec<WorldChange>,
    /// An all-or-nothing shape/state transition with before-state checks and
    /// dirty-region metadata.
    pub delta: Option<WorldDelta>,
    pub queued: Vec<QueuedEvent>,
}

/// A complete, in-memory execution snapshot for [`PhysicsEngine`].
///
/// `StateId` deliberately identifies only the observed World. This checkpoint
/// also retains pending scheduler work, logical time, counters, policy, and
/// trace cursors so restoring it can reproduce the same next step. The
/// checkpoint is intentionally opaque and is not a wire-format or a cycle key;
/// use [`ExecutionStateKey`] when comparing execution state without history.
/// State held inside a caller-provided event-handler closure is outside this
/// snapshot and must be made deterministic or checkpointed separately.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ExecutionCheckpoint {
    world: World,
    queue: PhysicsEventQueue,
    trace: Vec<StateTransition>,
    shape_transitions: Vec<ShapeTransition>,
    transition_trace: TransitionTrace,
    event_trace: EventTrace,
    next_transition_id: u64,
    last_transition_time: Option<PhysicsTime>,
    current_time: PhysicsTime,
    max_events: usize,
    processed_events: usize,
    max_microsteps_per_game_tick: usize,
    microsteps_this_game_tick: usize,
    piston_motion_profile: PistonMotionProfile,
    piston_planning_region: Option<Region>,
}

impl ExecutionCheckpoint {
    #[must_use]
    pub const fn time(&self) -> PhysicsTime {
        self.current_time
    }

    #[must_use]
    pub fn world(&self) -> &World {
        &self.world
    }

    #[must_use]
    pub fn state_id(&self) -> crate::StateId {
        self.world.state_id()
    }

    #[must_use]
    pub fn shape_id(&self) -> ShapeId {
        self.world.shape_id()
    }

    #[must_use]
    pub fn pending_event_count(&self) -> usize {
        self.queue.len()
    }

    /// Pending events are returned in the same order in which the scheduler
    /// would execute them. Event IDs are preserved for exact restoration.
    pub fn pending_events(&self) -> impl Iterator<Item = &PhysicsEvent> {
        self.queue.iter()
    }

    #[must_use]
    pub const fn processed_events(&self) -> usize {
        self.processed_events
    }

    #[must_use]
    pub const fn max_events(&self) -> usize {
        self.max_events
    }

    #[must_use]
    pub const fn max_microsteps_per_game_tick(&self) -> usize {
        self.max_microsteps_per_game_tick
    }

    #[must_use]
    pub const fn piston_motion_profile(&self) -> PistonMotionProfile {
        self.piston_motion_profile
    }

    #[must_use]
    pub const fn piston_planning_region(&self) -> Option<Region> {
        self.piston_planning_region
    }

    #[must_use]
    pub fn transition_trace(&self) -> &TransitionTrace {
        &self.transition_trace
    }

    #[must_use]
    pub fn event_trace(&self) -> &EventTrace {
        &self.event_trace
    }

    /// Returns a history-independent comparison view. Opaque event IDs and
    /// causal IDs are intentionally excluded because they identify trace
    /// records, not the future physical work. The pending vector still keeps
    /// scheduler order significant.
    #[must_use]
    pub fn state_key(&self) -> ExecutionStateKey {
        ExecutionStateKey::from_parts(
            self.world.state_id(),
            self.current_time,
            self.queue.iter().map(PendingEventKey::from).collect(),
            self.queue
                .next_sub_tick_orders_from(self.current_time.game_tick),
            self.piston_motion_profile,
            self.piston_planning_region,
        )
    }
}

/// A history-independent execution comparison key.
///
/// This is deliberately separate from `StateId`: two engines with the same
/// World but different pending work are not equivalent execution states. It
/// is also separate from `ExecutionCheckpoint`, whose trace and ID counters
/// are required for exact restoration but would prevent useful state reuse.
/// `world` is the existing content hash, so exact stale-state validation still
/// belongs to `WorldDelta::validate`; this key is a fast comparison aid.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ExecutionStateKey {
    pub world: crate::StateId,
    pub current_time: PhysicsTime,
    pub pending_events: Vec<PendingEventKey>,
    pub next_sub_tick_orders: BTreeMap<u64, u64>,
    pub piston_motion_profile: PistonMotionProfile,
    pub piston_planning_region: Option<Region>,
}

impl ExecutionStateKey {
    fn from_parts(
        world: crate::StateId,
        current_time: PhysicsTime,
        pending_events: Vec<PendingEventKey>,
        next_sub_tick_orders: BTreeMap<u64, u64>,
        piston_motion_profile: PistonMotionProfile,
        piston_planning_region: Option<Region>,
    ) -> Self {
        Self {
            world,
            current_time,
            pending_events,
            next_sub_tick_orders,
            piston_motion_profile,
            piston_planning_region,
        }
    }
}

/// A pending event projection used by [`ExecutionStateKey`]. IDs and parent
/// event IDs are omitted so a repeated physical state is not made unique only
/// by an ever-increasing trace counter. Queue order remains represented by the
/// vector position.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PendingEventKey {
    pub time: PhysicsTime,
    pub target: Pos,
    pub kind: PhysicsEventKind,
}

impl From<&PhysicsEvent> for PendingEventKey {
    fn from(event: &PhysicsEvent) -> Self {
        Self {
            time: event.time,
            target: event.target,
            kind: event.kind.clone(),
        }
    }
}

/// A legacy coordinate-level event output.  It remains available for the
/// existing signal handlers; `WorldDelta` is the preferred contract for any
/// operation that changes geometry.

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum PhysicsEngineError {
    EventLimitExceeded {
        limit: usize,
        processed: usize,
        next_time: Option<PhysicsTime>,
    },
    MicrostepLimitExceeded {
        game_tick: u64,
        limit: usize,
        processed: usize,
        next_time: Option<PhysicsTime>,
    },
    UnsupportedEvent {
        time: PhysicsTime,
        kind: PhysicsEventKind,
    },
    /// An event cannot execute in a phase that has already passed in the
    /// current game tick. Rejecting it preserves causal order instead of
    /// emitting a trace whose phase moves backwards.
    CausalOrderViolation {
        parent: PhysicsTime,
        child_phase: PhysicsEventPhase,
    },
    InvalidPistonMotionProfile(PistonMotionProfileError),
    Piston(Box<PistonError>),
    WorldDelta(Box<WorldDeltaError>),
    /// The handler returned mutually exclusive output forms or duplicate
    /// coordinate changes for one atomic event.
    InvalidOutcome,
}

impl Display for PhysicsEngineError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::EventLimitExceeded { limit, .. } => {
                write!(formatter, "Minecraft physics event limit {limit} exceeded")
            }
            Self::MicrostepLimitExceeded {
                game_tick, limit, ..
            } => write!(
                formatter,
                "Minecraft physics microstep limit {limit} exceeded at game tick {game_tick}"
            ),
            Self::UnsupportedEvent { time, .. } => write!(
                formatter,
                "event is not supported by this physics runner at game tick {} phase {:?}",
                time.game_tick, time.phase
            ),
            Self::CausalOrderViolation {
                parent,
                child_phase,
            } => write!(
                formatter,
                "event cannot move from phase {:?} back to phase {:?} at game tick {}",
                parent.phase, child_phase, parent.game_tick
            ),
            Self::InvalidPistonMotionProfile(error) => write!(formatter, "{error}"),
            Self::Piston(error) => write!(formatter, "piston event failed: {error}"),
            Self::WorldDelta(error) => write!(formatter, "world delta failed: {error}"),
            Self::InvalidOutcome => write!(
                formatter,
                "event outcome cannot contain both changes and a delta"
            ),
        }
    }
}

impl Error for PhysicsEngineError {}

impl From<PistonError> for PhysicsEngineError {
    fn from(value: PistonError) -> Self {
        Self::Piston(Box::new(value))
    }
}

impl From<PistonMotionProfileError> for PhysicsEngineError {
    fn from(value: PistonMotionProfileError) -> Self {
        Self::InvalidPistonMotionProfile(value)
    }
}

impl From<WorldDeltaError> for PhysicsEngineError {
    fn from(value: WorldDeltaError) -> Self {
        Self::WorldDelta(Box::new(value))
    }
}

/// A causal record for a geometry transition.  `from`/`to` are content-derived
/// shape identities; the event ID links the record to the scheduler trace.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct ShapeTransition {
    pub time: PhysicsTime,
    pub cause: super::EventId,
    pub from: ShapeId,
    pub to: ShapeId,
    pub delta: WorldDelta,
}

#[derive(Clone, Debug)]
struct TransitionDraft {
    trigger: super::EventId,
    time: PhysicsTime,
    from_state: crate::StateId,
    to_state: crate::StateId,
    from_shape: ShapeId,
    to_shape: ShapeId,
    changes: Vec<crate::BlockChange>,
    moves: Vec<crate::BlockMove>,
    cause: Option<crate::DeltaCause>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PhysicsEngine {
    world: World,
    queue: PhysicsEventQueue,
    trace: Vec<StateTransition>,
    shape_transitions: Vec<ShapeTransition>,
    transition_trace: TransitionTrace,
    event_trace: EventTrace,
    next_transition_id: u64,
    last_transition_time: Option<PhysicsTime>,
    current_time: PhysicsTime,
    max_events: usize,
    processed_events: usize,
    max_microsteps_per_game_tick: usize,
    microsteps_this_game_tick: usize,
    piston_motion_profile: PistonMotionProfile,
    piston_planning_region: Option<Region>,
}

impl PhysicsEngine {
    #[must_use]
    pub fn new(world: World, max_events: usize) -> Self {
        Self {
            world,
            queue: PhysicsEventQueue::default(),
            trace: Vec::new(),
            shape_transitions: Vec::new(),
            transition_trace: TransitionTrace::default(),
            event_trace: EventTrace::default(),
            next_transition_id: 0,
            last_transition_time: None,
            current_time: PhysicsTime::default(),
            max_events,
            processed_events: 0,
            max_microsteps_per_game_tick: DEFAULT_MAX_MICROSTEPS_PER_GAME_TICK,
            microsteps_this_game_tick: 0,
            piston_motion_profile: DEFAULT_PISTON_MOTION_PROFILE,
            piston_planning_region: None,
        }
    }

    /// Captures all mutable engine state needed to resume an execution
    /// exactly. The returned value owns its data and can safely outlive the
    /// engine that produced it.
    #[must_use]
    pub fn checkpoint(&self) -> ExecutionCheckpoint {
        ExecutionCheckpoint {
            world: self.world.clone(),
            queue: self.queue.clone(),
            trace: self.trace.clone(),
            shape_transitions: self.shape_transitions.clone(),
            transition_trace: self.transition_trace.clone(),
            event_trace: self.event_trace.clone(),
            next_transition_id: self.next_transition_id,
            last_transition_time: self.last_transition_time,
            current_time: self.current_time,
            max_events: self.max_events,
            processed_events: self.processed_events,
            max_microsteps_per_game_tick: self.max_microsteps_per_game_tick,
            microsteps_this_game_tick: self.microsteps_this_game_tick,
            piston_motion_profile: self.piston_motion_profile,
            piston_planning_region: self.piston_planning_region,
        }
    }

    /// Restores a checkpoint captured from a compatible engine. This is an
    /// in-memory operation and cannot fail because the checkpoint contains
    /// all fields that determine the engine's next step.
    pub fn restore(&mut self, checkpoint: &ExecutionCheckpoint) {
        self.world = checkpoint.world.clone();
        self.queue = checkpoint.queue.clone();
        self.trace = checkpoint.trace.clone();
        self.shape_transitions = checkpoint.shape_transitions.clone();
        self.transition_trace = checkpoint.transition_trace.clone();
        self.event_trace = checkpoint.event_trace.clone();
        self.next_transition_id = checkpoint.next_transition_id;
        self.last_transition_time = checkpoint.last_transition_time;
        self.current_time = checkpoint.current_time;
        self.max_events = checkpoint.max_events;
        self.processed_events = checkpoint.processed_events;
        self.max_microsteps_per_game_tick = checkpoint.max_microsteps_per_game_tick;
        self.microsteps_this_game_tick = checkpoint.microsteps_this_game_tick;
        self.piston_motion_profile = checkpoint.piston_motion_profile;
        self.piston_planning_region = checkpoint.piston_planning_region;
    }

    /// Returns a history-independent key for cycle detection or memoization.
    /// Unlike `state_id`, this includes pending scheduler work and logical
    /// ordering state. It intentionally omits trace-only IDs and history.
    #[must_use]
    pub fn execution_state_key(&self) -> ExecutionStateKey {
        ExecutionStateKey::from_parts(
            self.world.state_id(),
            self.current_time,
            self.queue.iter().map(PendingEventKey::from).collect(),
            self.queue
                .next_sub_tick_orders_from(self.current_time.game_tick),
            self.piston_motion_profile,
            self.piston_planning_region,
        )
    }

    /// Returns the motion profile used when a piston Block Event queues its
    /// stable completion. The initial-delay range is informational until a
    /// signal/update scheduler supplies the phase that triggered the event.
    #[must_use]
    pub const fn piston_motion_profile(&self) -> PistonMotionProfile {
        self.piston_motion_profile
    }

    /// Overrides the completion profile for a controlled simulation or a
    /// version-specific fixture. A zero movement interval is valid for future
    /// same-tick experiments, but remains outside the formal piston subset.
    #[must_use]
    pub fn with_piston_motion_profile(mut self, profile: PistonMotionProfile) -> Self {
        self.piston_motion_profile = profile;
        self
    }

    /// Supplies the complete static observation boundary used by built-in
    /// piston planning. Coordinates outside the region are `Unknown`, not
    /// Air. Leaving this unset preserves the unchecked synthetic-world API of
    /// [`plan_piston`].
    #[must_use]
    pub const fn with_piston_planning_region(mut self, known_region: Region) -> Self {
        self.piston_planning_region = Some(known_region);
        self
    }

    /// Sets the maximum number of events that may execute in one game tick.
    /// A value of zero intentionally rejects the first queued event and is
    /// useful for testing a caller's budget handling.
    #[must_use]
    pub const fn with_max_microsteps_per_game_tick(mut self, limit: usize) -> Self {
        self.max_microsteps_per_game_tick = limit;
        self
    }

    #[must_use]
    pub const fn time(&self) -> PhysicsTime {
        self.current_time
    }

    #[must_use]
    pub const fn world(&self) -> &World {
        &self.world
    }

    /// Returns the full observed-state identity at the current transition
    /// boundary. Pending scheduler work is intentionally not part of this
    /// identity; callers that compare executions should use
    /// [`Self::execution_state_key`] instead.
    #[must_use]
    pub fn state_id(&self) -> crate::StateId {
        self.world.state_id()
    }

    /// Number of events that remain pending after the last accepted step.
    #[must_use]
    pub fn pending_event_count(&self) -> usize {
        self.queue.len()
    }

    /// Number of events accepted by the engine, including successful no-op
    /// events but excluding rejected events that were requeued.
    #[must_use]
    pub const fn processed_events(&self) -> usize {
        self.processed_events
    }

    #[must_use]
    pub fn trace(&self) -> &[StateTransition] {
        &self.trace
    }

    #[must_use]
    pub fn shape_transitions(&self) -> &[ShapeTransition] {
        &self.shape_transitions
    }

    /// Returns the state-changing transition ledger. Unlike `trace()`, which
    /// retains one record per changed coordinate for compatibility, this
    /// ledger has one record per accepted state transition.
    #[must_use]
    pub fn transition_trace(&self) -> &TransitionTrace {
        &self.transition_trace
    }

    /// Returns the successfully handled event ledger, including events that
    /// produced no state transition. Rejected events are left pending and are
    /// intentionally absent until a later retry succeeds. Its status marks a
    /// rejected prefix as failed rather than complete.
    #[must_use]
    pub fn event_trace(&self) -> &EventTrace {
        &self.event_trace
    }

    /// Returns whether the current trace is complete, still running, or a
    /// failed prefix. A failed prefix must not be treated as a verified full
    /// execution even though its accepted records remain available.
    #[must_use]
    pub fn trace_status(&self) -> &TraceStatus {
        self.transition_trace.status()
    }

    #[must_use]
    pub fn shape_id(&self) -> ShapeId {
        self.world.shape_id()
    }

    pub fn schedule_external(
        &mut self,
        game_tick: u64,
        target: Pos,
        kind: PhysicsEventKind,
    ) -> super::EventId {
        self.schedule_external_in_phase(game_tick, target, kind.default_phase(), kind)
    }

    /// Schedules an event with explicit phase evidence. The default API uses
    /// the phase implied by the event kind; this variant is the seam for a
    /// versioned Minecraft scheduler or a live event source with stronger
    /// phase information.
    pub fn schedule_external_in_phase(
        &mut self,
        game_tick: u64,
        target: Pos,
        phase: PhysicsEventPhase,
        kind: PhysicsEventKind,
    ) -> super::EventId {
        self.mark_trace_in_progress();
        self.queue.schedule_in_phase(
            game_tick.max(self.current_time.game_tick),
            target,
            EventCause::External,
            // Keep the requested phase separate from the event kind. This is
            // intentional: the same payload can be delivered by different
            // sources in a future versioned scheduler.
            phase,
            kind,
        )
    }

    /// Checked variant for live or versioned schedulers that want phase
    /// regressions rejected at insertion time. The compatibility method above
    /// still accepts the event and `step_transition` rejects it before the
    /// handler runs, so an invalid event can never mutate the World.
    pub fn schedule_external_in_phase_checked(
        &mut self,
        game_tick: u64,
        target: Pos,
        phase: PhysicsEventPhase,
        kind: PhysicsEventKind,
    ) -> Result<super::EventId, PhysicsEngineError> {
        let effective_tick = game_tick.max(self.current_time.game_tick);
        if effective_tick == self.current_time.game_tick && phase < self.current_time.phase {
            let error = PhysicsEngineError::CausalOrderViolation {
                parent: self.current_time,
                child_phase: phase,
            };
            self.mark_trace_failed(&error);
            return Err(error);
        }
        Ok(self.schedule_external_in_phase(effective_tick, target, phase, kind))
    }

    /// Schedules a piston Block Event without applying it immediately. The
    /// event engine, rather than the redstone graph, owns the transition
    /// ordering and causal event ID.
    pub fn schedule_piston_action(
        &mut self,
        game_tick: u64,
        target: Pos,
        action: PistonAction,
    ) -> super::EventId {
        let event = match action {
            PistonAction::Extend => BlockEventKind::PistonExtend,
            PistonAction::Retract => BlockEventKind::PistonRetract,
        };
        self.schedule_external(game_tick, target, PhysicsEventKind::BlockEvent { event })
    }

    pub fn run_until_idle(
        &mut self,
        mut handler: impl FnMut(&PhysicsEvent, &World) -> EventOutcome,
    ) -> Result<(), PhysicsEngineError> {
        self.run_until_idle_checked(|event, world| Ok(handler(event, world)))
    }

    /// Variant of `run_until_idle` for behavior handlers that can reject an
    /// event before any WorldChange is applied. This is used by the built-in
    /// Piston handler so obstruction and stale-state errors fail closed.
    pub fn run_until_idle_checked(
        &mut self,
        mut handler: impl FnMut(&PhysicsEvent, &World) -> Result<EventOutcome, PhysicsEngineError>,
    ) -> Result<(), PhysicsEngineError> {
        while !self.queue.is_empty() {
            self.step_transition(&mut handler)?;
        }
        if !self.trace_status().is_failed() {
            self.mark_trace_complete();
        }
        Ok(())
    }

    /// Executes at most one pending event and returns the corresponding
    /// transition record. This is the transition-first boundary; the older
    /// `run_until_idle_checked` method is a convenience loop over this step.
    /// A successful event may return `transition = None` when it was a no-op.
    /// Rejected events are restored with their queue position, execution-order
    /// counter, and previous logical time; the accepted prefix remains in the
    /// ledgers and is marked as a failed trace.
    pub fn step_transition(
        &mut self,
        handler: &mut impl FnMut(&PhysicsEvent, &World) -> Result<EventOutcome, PhysicsEngineError>,
    ) -> Result<Option<TransitionStep>, PhysicsEngineError> {
        if self.queue.is_empty() {
            if !self.trace_status().is_failed() {
                self.mark_trace_complete();
            }
            return Ok(None);
        }
        if self.processed_events >= self.max_events {
            let error = PhysicsEngineError::EventLimitExceeded {
                limit: self.max_events,
                processed: self.processed_events,
                next_time: self.queue.next_time(),
            };
            self.mark_trace_failed(&error);
            return Err(error);
        }
        let next_time = self.queue.next_time().expect("queue is non-empty");
        if next_time.game_tick != self.current_time.game_tick {
            self.microsteps_this_game_tick = 0;
        }
        if self.microsteps_this_game_tick >= self.max_microsteps_per_game_tick {
            let error = PhysicsEngineError::MicrostepLimitExceeded {
                game_tick: next_time.game_tick,
                limit: self.max_microsteps_per_game_tick,
                processed: self.processed_events,
                next_time: Some(next_time),
            };
            self.mark_trace_failed(&error);
            return Err(error);
        }
        let previous_time = self.current_time;
        let previous_microsteps = self.microsteps_this_game_tick;
        let (event, pop_checkpoint) = self
            .queue
            .pop_with_checkpoint()
            .expect("queue is non-empty");
        if event.time.game_tick == previous_time.game_tick && event.time.phase < previous_time.phase
        {
            let error = PhysicsEngineError::CausalOrderViolation {
                parent: previous_time,
                child_phase: event.time.phase,
            };
            self.rollback_rejected_event(
                event,
                pop_checkpoint,
                previous_time,
                previous_microsteps,
                &error,
            );
            return Err(error);
        }
        self.current_time = event.time;
        let outcome = match handler(&event, &self.world) {
            Ok(outcome) => outcome,
            Err(error) => {
                self.rollback_rejected_event(
                    event,
                    pop_checkpoint,
                    previous_time,
                    previous_microsteps,
                    &error,
                );
                return Err(error);
            }
        };
        if let Some(queued) = outcome
            .queued
            .iter()
            .find(|queued| queued.delay_ticks == 0 && queued.phase < event.time.phase)
        {
            let child_phase = queued.phase;
            let error = PhysicsEngineError::CausalOrderViolation {
                parent: self.current_time,
                child_phase,
            };
            self.rollback_rejected_event(
                event,
                pop_checkpoint,
                previous_time,
                previous_microsteps,
                &error,
            );
            return Err(error);
        }
        if outcome.delta.is_some() && !outcome.changes.is_empty() {
            let error = PhysicsEngineError::InvalidOutcome;
            self.rollback_rejected_event(
                event,
                pop_checkpoint,
                previous_time,
                previous_microsteps,
                &error,
            );
            return Err(error);
        }

        let mut transition = None;
        if let Some(delta) = outcome.delta {
            let from_state = self.world.state_id();
            let from_shape = self.world.shape_id();
            if let Err(error) = delta.apply(&mut self.world) {
                let error = PhysicsEngineError::WorldDelta(Box::new(error));
                self.rollback_rejected_event(
                    event,
                    pop_checkpoint,
                    previous_time,
                    previous_microsteps,
                    &error,
                );
                return Err(error);
            }
            let to_state = self.world.state_id();
            let to_shape = self.world.shape_id();
            for change in &delta.changes {
                if change.before == change.after {
                    continue;
                }
                self.trace.push(StateTransition {
                    time: event.time,
                    position: change.position,
                    before: change.before.clone(),
                    after: change.after.clone(),
                    cause: event.id,
                    reason: change.reason.clone(),
                    delta_cause: Some(delta.cause.clone()),
                });
            }
            self.shape_transitions.push(ShapeTransition {
                time: event.time,
                cause: event.id,
                from: from_shape,
                to: to_shape,
                delta: delta.clone(),
            });
            if from_state != to_state || !delta.moves.is_empty() {
                transition = Some(self.append_transition(TransitionDraft {
                    trigger: event.id,
                    time: event.time,
                    from_state,
                    to_state,
                    from_shape,
                    to_shape,
                    changes: delta.changes.clone(),
                    moves: delta.moves.clone(),
                    cause: Some(delta.cause.clone()),
                }));
            }
        } else {
            let from_state = self.world.state_id();
            let from_shape = self.world.shape_id();
            let mut staged = self.world.clone();
            let mut changes = Vec::new();
            let mut positions = BTreeSet::new();
            for change in outcome.changes {
                if !positions.insert(change.position) {
                    let error = PhysicsEngineError::InvalidOutcome;
                    self.rollback_rejected_event(
                        event,
                        pop_checkpoint,
                        previous_time,
                        previous_microsteps,
                        &error,
                    );
                    return Err(error);
                }
                let before = self
                    .world
                    .get(change.position)
                    .cloned()
                    .unwrap_or_else(|| Block::new(BlockKind::Air));
                if before == change.after {
                    continue;
                }
                staged.set(change.position, change.after.clone());
                let block_change = crate::BlockChange {
                    position: change.position,
                    before: before.clone(),
                    after: change.after.clone(),
                    reason: ChangeReason::Unknown,
                };
                changes.push(block_change);
                self.trace.push(StateTransition {
                    time: event.time,
                    position: change.position,
                    before,
                    after: change.after,
                    cause: event.id,
                    reason: ChangeReason::Unknown,
                    delta_cause: None,
                });
            }
            self.world = staged;
            if !changes.is_empty() {
                let to_state = self.world.state_id();
                let to_shape = self.world.shape_id();
                transition = Some(self.append_transition(TransitionDraft {
                    trigger: event.id,
                    time: event.time,
                    from_state,
                    to_state,
                    from_shape,
                    to_shape,
                    changes,
                    moves: Vec::new(),
                    cause: None,
                }));
            }
        }

        for queued in outcome.queued {
            self.queue.schedule_in_phase(
                event.time.game_tick.saturating_add(queued.delay_ticks),
                queued.target,
                EventCause::Event { id: event.id },
                queued.phase,
                queued.kind,
            );
        }
        self.processed_events += 1;
        self.microsteps_this_game_tick += 1;
        let status = transition
            .as_ref()
            .map_or(EventExecutionStatus::NoTransition, |transition| {
                EventExecutionStatus::Transition { id: transition.id }
            });
        let event_record = EventRecord {
            event: event.clone(),
            status,
        };
        self.event_trace.records.push(event_record.clone());
        if self.queue.is_empty() {
            self.mark_trace_complete();
        } else {
            self.mark_trace_in_progress();
        }
        Ok(Some(TransitionStep {
            event: event_record,
            transition,
        }))
    }

    fn rollback_rejected_event(
        &mut self,
        event: PhysicsEvent,
        pop_checkpoint: QueuePopCheckpoint,
        previous_time: PhysicsTime,
        previous_microsteps: usize,
        error: &PhysicsEngineError,
    ) {
        self.queue.rollback_pop(event, pop_checkpoint);
        self.current_time = previous_time;
        self.microsteps_this_game_tick = previous_microsteps;
        self.mark_trace_failed(error);
    }

    fn mark_trace_in_progress(&mut self) {
        self.transition_trace.status = TraceStatus::InProgress;
        self.event_trace.status = TraceStatus::InProgress;
    }

    fn mark_trace_complete(&mut self) {
        self.transition_trace.status = TraceStatus::Complete;
        self.event_trace.status = TraceStatus::Complete;
    }

    fn mark_trace_failed(&mut self, error: &PhysicsEngineError) {
        let error = error.to_string();
        self.transition_trace.status = TraceStatus::Failed {
            error: error.clone(),
        };
        self.event_trace.status = TraceStatus::Failed { error };
    }

    fn append_transition(&mut self, draft: TransitionDraft) -> TransitionRecord {
        let id = TransitionId(self.next_transition_id);
        self.next_transition_id = self.next_transition_id.saturating_add(1);
        let elapsed_from_previous = self
            .last_transition_time
            .map(|previous| TransitionElapsed::between(previous, draft.time));
        let record = TransitionRecord {
            id,
            trigger: draft.trigger,
            time: draft.time,
            elapsed_from_previous,
            from_state: draft.from_state,
            to_state: draft.to_state,
            from_shape: draft.from_shape,
            to_shape: draft.to_shape,
            changes: draft.changes,
            moves: draft.moves,
            cause: draft.cause,
        };
        self.last_transition_time = Some(draft.time);
        self.transition_trace.records.push(record.clone());
        record
    }

    /// Runs the queued built-in piston events. A piston Block Event first
    /// records `Extending`/`Retracting` and queues a completion event after the
    /// current motion profile. The completion applies the stable movement
    /// delta atomically. Other event kinds are rejected without being lost;
    /// callers that need mixed behavior should use `run_until_idle_checked`
    /// with a handler for every event phase.
    pub fn run_piston_events(&mut self) -> Result<(), PhysicsEngineError> {
        if let Err(error) = self.piston_motion_profile.validate() {
            let error = PhysicsEngineError::from(error);
            self.mark_trace_failed(&error);
            return Err(error);
        }
        let unsupported_event = self
            .queue
            .iter()
            .find(|event| {
                !matches!(
                    event.kind,
                    PhysicsEventKind::BlockEvent {
                        event: BlockEventKind::PistonExtend | BlockEventKind::PistonRetract
                    } | PhysicsEventKind::PistonComplete { .. }
                )
            })
            .cloned();
        if let Some(event) = unsupported_event {
            let error = PhysicsEngineError::UnsupportedEvent {
                time: event.time,
                kind: event.kind.clone(),
            };
            self.mark_trace_failed(&error);
            return Err(error);
        }
        let movement_game_ticks = self
            .piston_motion_profile
            .stable_completion_delay_game_ticks();
        let planning_region = self.piston_planning_region;
        self.run_until_idle_checked(move |event, world| match &event.kind {
            PhysicsEventKind::BlockEvent {
                event: BlockEventKind::PistonExtend,
            } => {
                let plan = match planning_region {
                    Some(region) => {
                        plan_piston_in_region(world, region, event.target, PistonAction::Extend)?
                    }
                    None => plan_piston(world, event.target, PistonAction::Extend)?,
                };
                let start_delta = plan.start_delta();
                let mut started = world.clone();
                start_delta.apply(&mut started)?;
                let completion = plan.completion_plan(&started)?;
                Ok(EventOutcome {
                    changes: Vec::new(),
                    delta: Some(start_delta),
                    queued: vec![QueuedEvent {
                        delay_ticks: movement_game_ticks,
                        target: event.target,
                        phase: PhysicsEventPhase::BlockEntity,
                        kind: PhysicsEventKind::PistonComplete {
                            action: PistonAction::Extend,
                            plan: Box::new(completion),
                        },
                    }],
                })
            }
            PhysicsEventKind::BlockEvent {
                event: BlockEventKind::PistonRetract,
            } => {
                let plan = match planning_region {
                    Some(region) => {
                        plan_piston_in_region(world, region, event.target, PistonAction::Retract)?
                    }
                    None => plan_piston(world, event.target, PistonAction::Retract)?,
                };
                let start_delta = plan.start_delta();
                let mut started = world.clone();
                start_delta.apply(&mut started)?;
                let completion = plan.completion_plan(&started)?;
                Ok(EventOutcome {
                    changes: Vec::new(),
                    delta: Some(start_delta),
                    queued: vec![QueuedEvent {
                        delay_ticks: movement_game_ticks,
                        target: event.target,
                        phase: PhysicsEventPhase::BlockEntity,
                        kind: PhysicsEventKind::PistonComplete {
                            action: PistonAction::Retract,
                            plan: Box::new(completion),
                        },
                    }],
                })
            }
            PhysicsEventKind::PistonComplete { action, plan } => {
                if *action != plan.action {
                    return Err(PhysicsEngineError::InvalidOutcome);
                }
                Ok(EventOutcome {
                    changes: Vec::new(),
                    delta: Some(plan.world_delta().clone()),
                    queued: Vec::new(),
                })
            }
            _ => Err(PhysicsEngineError::UnsupportedEvent {
                time: event.time,
                kind: event.kind.clone(),
            }),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::time::PhysicsEventKind;
    use crate::{PistonState, piston_state};

    #[test]
    fn records_same_tick_transitions_in_causal_order() {
        let pos = Pos::new(1, 2, 3);
        let mut engine = PhysicsEngine::new(World::new(), 8);
        engine.schedule_external(
            10,
            pos,
            PhysicsEventKind::UserAction {
                action: "pulse".into(),
            },
        );
        engine
            .run_until_idle(|event, _| {
                let powered = event.time.sub_tick_order == 0;
                EventOutcome {
                    changes: vec![WorldChange {
                        position: pos,
                        after: Block::new(if powered {
                            BlockKind::RedstoneBlock
                        } else {
                            BlockKind::Air
                        }),
                    }],
                    delta: None,
                    queued: powered
                        .then_some(QueuedEvent {
                            delay_ticks: 0,
                            target: pos,
                            phase: PhysicsEventPhase::NeighborUpdate,
                            kind: PhysicsEventKind::NeighborUpdate { source: pos },
                        })
                        .into_iter()
                        .collect(),
                }
            })
            .unwrap();
        assert_eq!(engine.trace().len(), 2);
        assert_eq!(engine.trace()[0].time.game_tick, 10);
        assert_eq!(engine.trace()[0].time.sub_tick_order, 0);
        assert_eq!(engine.trace()[1].time.sub_tick_order, 1);
        assert_eq!(engine.world().kind_at(pos), BlockKind::Air);
    }

    #[test]
    fn step_transition_exposes_elapsed_time_and_noop_events_separately() {
        let pos = Pos::new(1, 2, 3);
        let mut engine = PhysicsEngine::new(World::new(), 8);
        engine.schedule_external(
            10,
            pos,
            PhysicsEventKind::UserAction {
                action: "pulse".into(),
            },
        );
        let mut handler = |event: &PhysicsEvent, _world: &World| {
            if matches!(
                &event.kind,
                PhysicsEventKind::UserAction { action } if action == "pulse"
            ) {
                return Ok(EventOutcome {
                    changes: vec![WorldChange {
                        position: pos,
                        after: Block::new(BlockKind::RedstoneBlock),
                    }],
                    delta: None,
                    queued: vec![
                        QueuedEvent {
                            delay_ticks: 0,
                            target: pos,
                            phase: PhysicsEventPhase::NeighborUpdate,
                            kind: PhysicsEventKind::NeighborUpdate { source: pos },
                        },
                        QueuedEvent {
                            delay_ticks: 0,
                            target: pos,
                            phase: PhysicsEventPhase::Observation,
                            kind: PhysicsEventKind::UserAction {
                                action: "observation".into(),
                            },
                        },
                    ],
                });
            }
            if matches!(event.kind, PhysicsEventKind::NeighborUpdate { .. }) {
                return Ok(EventOutcome {
                    changes: vec![WorldChange {
                        position: pos,
                        after: Block::new(BlockKind::Air),
                    }],
                    delta: None,
                    queued: Vec::new(),
                });
            }
            Ok(EventOutcome::default())
        };

        let first = engine.step_transition(&mut handler).unwrap().unwrap();
        let second = engine.step_transition(&mut handler).unwrap().unwrap();
        let third = engine.step_transition(&mut handler).unwrap().unwrap();
        assert!(engine.step_transition(&mut handler).unwrap().is_none());
        assert!(matches!(
            first.event.status,
            EventExecutionStatus::Transition {
                id: TransitionId(0)
            }
        ));
        assert!(matches!(
            second.event.status,
            EventExecutionStatus::Transition {
                id: TransitionId(1)
            }
        ));
        assert!(matches!(
            third.event.status,
            EventExecutionStatus::NoTransition
        ));
        assert_eq!(engine.transition_trace().len(), 2);
        assert_eq!(
            engine.transition_trace().records[0].elapsed_from_previous,
            None
        );
        assert_eq!(
            engine.transition_trace().records[1].elapsed_from_previous,
            Some(TransitionElapsed::Zero { order_delta: 1 })
        );
        assert_eq!(engine.event_trace().len(), 3);
        assert_eq!(
            engine.event_trace().records[2].event.id,
            third.event.event.id
        );
        assert_eq!(
            engine.transition_trace().records[0].from_state,
            World::new().state_id()
        );
    }

    #[test]
    fn stops_an_unbounded_update_chain() {
        let pos = Pos::new(0, 0, 0);
        let mut engine = PhysicsEngine::new(World::new(), 3);
        engine.schedule_external(0, pos, PhysicsEventKind::ScheduledBlockTick);
        let error = engine
            .run_until_idle(|_, _| EventOutcome {
                changes: Vec::new(),
                delta: None,
                queued: vec![QueuedEvent {
                    delay_ticks: 0,
                    target: pos,
                    phase: PhysicsEventPhase::ScheduledTick,
                    kind: PhysicsEventKind::ScheduledBlockTick,
                }],
            })
            .unwrap_err();
        assert!(matches!(
            error,
            PhysicsEngineError::EventLimitExceeded { processed: 3, .. }
        ));
    }

    #[test]
    fn piston_runner_rejects_without_consuming_non_piston_events() {
        let pos = Pos::new(0, 0, 0);
        let mut engine = PhysicsEngine::new(World::new(), 8);
        let event_id =
            engine.schedule_external(4, pos, PhysicsEventKind::NeighborUpdate { source: pos });
        let error = engine.run_piston_events().unwrap_err();
        assert!(matches!(
            error,
            PhysicsEngineError::UnsupportedEvent { kind: PhysicsEventKind::NeighborUpdate { source }, .. }
                if source == pos
        ));
        assert_eq!(engine.queue.len(), 1);
        assert_eq!(engine.queue.peek().map(|event| event.id), Some(event_id));
        assert_eq!(engine.processed_events, 0);
        assert!(engine.trace.is_empty());
    }

    #[test]
    fn piston_runner_preflights_a_mixed_queue_before_partial_application() {
        let piston_pos = Pos::new(0, 1, 0);
        let mut world = World::new();
        world.set(Pos::new(0, 0, 0), Block::new(BlockKind::Solid));
        let mut piston = Block::new(BlockKind::Piston);
        piston.facing = Some(crate::Facing::East);
        world.set(piston_pos, piston);
        world.set(Pos::new(1, 1, 0), Block::new(BlockKind::Solid));
        let mut engine = PhysicsEngine::new(world.clone(), 8);
        let piston_event = engine.schedule_piston_action(0, piston_pos, PistonAction::Extend);
        let other_event = engine.schedule_external(
            1,
            piston_pos,
            PhysicsEventKind::NeighborUpdate { source: piston_pos },
        );

        let error = engine.run_piston_events().unwrap_err();
        assert!(matches!(
            error,
            PhysicsEngineError::UnsupportedEvent {
                kind: PhysicsEventKind::NeighborUpdate { source },
                ..
            } if source == piston_pos
        ));
        assert_eq!(engine.world(), &world);
        assert_eq!(engine.queue.len(), 2);
        assert_eq!(
            engine
                .queue
                .iter()
                .map(|event| event.id)
                .collect::<Vec<_>>(),
            vec![piston_event, other_event]
        );
        assert!(engine.trace.is_empty());
    }

    #[test]
    fn zero_delay_child_cannot_move_back_to_an_earlier_phase() {
        let pos = Pos::new(0, 0, 0);
        let mut engine = PhysicsEngine::new(World::new(), 8);
        engine.schedule_external_in_phase(
            4,
            pos,
            PhysicsEventPhase::BlockEvent,
            PhysicsEventKind::UserAction {
                action: "phase-test".into(),
            },
        );
        let error = engine
            .run_until_idle(|_, _| EventOutcome {
                changes: Vec::new(),
                delta: None,
                queued: vec![QueuedEvent {
                    delay_ticks: 0,
                    target: pos,
                    phase: PhysicsEventPhase::NeighborUpdate,
                    kind: PhysicsEventKind::NeighborUpdate { source: pos },
                }],
            })
            .unwrap_err();
        assert!(matches!(
            error,
            PhysicsEngineError::CausalOrderViolation {
                parent: PhysicsTime {
                    game_tick: 4,
                    phase: PhysicsEventPhase::BlockEvent,
                    ..
                },
                child_phase: PhysicsEventPhase::NeighborUpdate,
            }
        ));
        assert_eq!(engine.queue.len(), 1);
        assert_eq!(engine.processed_events, 0);
        assert!(engine.trace.is_empty());
    }

    #[test]
    fn invalid_piston_profile_is_rejected_before_consuming_events() {
        let piston_pos = Pos::new(0, 1, 0);
        let mut world = World::new();
        world.set(Pos::new(0, 0, 0), Block::new(BlockKind::Solid));
        let mut piston = Block::new(BlockKind::Piston);
        piston.facing = Some(crate::Facing::East);
        world.set(piston_pos, piston);
        let profile = PistonMotionProfile {
            initial_delay_min_game_ticks: 2,
            initial_delay_max_game_ticks: 1,
            movement_game_ticks: 0,
        };
        let mut engine = PhysicsEngine::new(world, 8).with_piston_motion_profile(profile);
        let event_id = engine.schedule_piston_action(4, piston_pos, PistonAction::Extend);

        let error = engine.run_piston_events().unwrap_err();
        assert!(matches!(
            error,
            PhysicsEngineError::InvalidPistonMotionProfile(
                PistonMotionProfileError::InvalidInitialDelayRange {
                    minimum_game_ticks: 2,
                    maximum_game_ticks: 1
                }
            )
        ));
        assert_eq!(engine.queue.len(), 1);
        assert_eq!(engine.queue.peek().map(|event| event.id), Some(event_id));
        assert_eq!(engine.processed_events, 0);
    }

    #[test]
    fn bounded_piston_runner_rejects_unknown_space_without_mutation() {
        let piston_pos = Pos::new(0, 1, 0);
        let mut world = World::new();
        world.set(Pos::new(0, 0, 0), Block::new(BlockKind::Solid));
        let mut piston = Block::new(BlockKind::Piston);
        piston.facing = Some(crate::Facing::East);
        world.set(piston_pos, piston);
        world.set(Pos::new(1, 1, 0), Block::new(BlockKind::Solid));
        let known_region = Region::new(Pos::new(-1, 0, -1), Pos::new(1, 2, 1));
        let mut engine =
            PhysicsEngine::new(world.clone(), 8).with_piston_planning_region(known_region);
        let event_id = engine.schedule_piston_action(0, piston_pos, PistonAction::Extend);

        let error = engine.run_piston_events().unwrap_err();
        assert!(matches!(
            error,
            PhysicsEngineError::Piston(error)
                if matches!(error.as_ref(), PistonError::UnknownSpace { position }
                    if *position == Pos::new(2, 1, 0))
        ));
        assert_eq!(engine.world(), &world);
        assert_eq!(engine.queue.peek().map(|event| event.id), Some(event_id));
        assert!(engine.trace.is_empty());
    }

    #[test]
    fn checkpoint_round_trip_restores_world_queue_and_trace_cursors() {
        let pos = Pos::new(2, 0, 0);
        let mut engine = PhysicsEngine::new(World::new(), 8);
        engine.schedule_external(
            4,
            pos,
            PhysicsEventKind::UserAction {
                action: "set".to_owned(),
            },
        );
        let checkpoint = engine.checkpoint();
        assert_eq!(checkpoint.pending_event_count(), 1);
        assert_eq!(checkpoint.state_key(), engine.execution_state_key());

        let mut handler = |_: &PhysicsEvent, _: &World| {
            Ok(EventOutcome {
                changes: vec![WorldChange {
                    position: pos,
                    after: Block::new(BlockKind::RedstoneBlock),
                }],
                delta: None,
                queued: Vec::new(),
            })
        };
        engine.step_transition(&mut handler).unwrap();
        assert_ne!(engine.checkpoint(), checkpoint);

        engine.restore(&checkpoint);
        assert_eq!(engine.checkpoint(), checkpoint);
        assert_eq!(engine.execution_state_key(), checkpoint.state_key());
        assert_eq!(engine.pending_event_count(), 1);
        assert_eq!(engine.world().kind_at(pos), BlockKind::Air);
    }

    #[test]
    fn rejected_step_restores_scheduler_state_and_retry_order() {
        let pos = Pos::new(2, 0, 0);
        let mut engine = PhysicsEngine::new(World::new(), 8);
        engine.schedule_external(0, pos, PhysicsEventKind::ScheduledBlockTick);
        let mut warmup = |_: &PhysicsEvent, _: &World| Ok(EventOutcome::default());
        engine.step_transition(&mut warmup).unwrap();
        engine.schedule_external(
            4,
            pos,
            PhysicsEventKind::UserAction {
                action: "retry".to_owned(),
            },
        );
        let before_key = engine.execution_state_key();
        let before_time = engine.time();
        let mut attempts = 0;
        let mut handler = |_: &PhysicsEvent, _: &World| {
            attempts += 1;
            if attempts == 1 {
                return Err(PhysicsEngineError::InvalidOutcome);
            }
            Ok(EventOutcome {
                changes: vec![WorldChange {
                    position: pos,
                    after: Block::new(BlockKind::RedstoneBlock),
                }],
                delta: None,
                queued: Vec::new(),
            })
        };

        let error = engine.step_transition(&mut handler).unwrap_err();
        assert_eq!(error, PhysicsEngineError::InvalidOutcome);
        assert_eq!(engine.execution_state_key(), before_key);
        assert_eq!(engine.time(), before_time);
        assert_eq!(engine.pending_event_count(), 1);
        assert!(engine.trace().is_empty());
        assert!(engine.trace_status().is_failed());

        let retry = engine.step_transition(&mut handler).unwrap().unwrap();
        assert_eq!(retry.event.event.time.sub_tick_order, 0);
        assert_eq!(engine.world().kind_at(pos), BlockKind::RedstoneBlock);
        assert!(engine.trace_status().is_complete());
    }

    #[test]
    fn legacy_outcome_duplicate_changes_are_rejected_atomically() {
        let pos = Pos::new(2, 0, 0);
        let mut engine = PhysicsEngine::new(World::new(), 8);
        engine.schedule_external(1, pos, PhysicsEventKind::ScheduledBlockTick);
        let before = engine.execution_state_key();
        let mut handler = |_: &PhysicsEvent, _: &World| {
            Ok(EventOutcome {
                changes: vec![
                    WorldChange {
                        position: pos,
                        after: Block::new(BlockKind::RedstoneBlock),
                    },
                    WorldChange {
                        position: pos,
                        after: Block::new(BlockKind::Air),
                    },
                ],
                delta: None,
                queued: Vec::new(),
            })
        };
        assert_eq!(
            engine.step_transition(&mut handler).unwrap_err(),
            PhysicsEngineError::InvalidOutcome
        );
        assert_eq!(engine.execution_state_key(), before);
        assert_eq!(engine.world().kind_at(pos), BlockKind::Air);
        assert_eq!(engine.pending_event_count(), 1);
    }

    #[test]
    fn execution_keys_distinguish_same_world_with_different_pending_work() {
        let pos = Pos::new(0, 0, 0);
        let world = World::new();
        let first = PhysicsEngine::new(world.clone(), 8);
        let mut second = PhysicsEngine::new(world, 8);
        second.schedule_external(2, pos, PhysicsEventKind::ScheduledBlockTick);
        assert_ne!(first.execution_state_key(), second.execution_state_key());
        assert_eq!(first.state_id(), second.state_id());
    }

    #[test]
    fn checked_external_schedule_rejects_a_past_same_tick_phase() {
        let pos = Pos::new(0, 0, 0);
        let mut engine = PhysicsEngine::new(World::new(), 8);
        engine.schedule_external_in_phase(
            4,
            pos,
            PhysicsEventPhase::BlockEvent,
            PhysicsEventKind::UserAction {
                action: "phase".to_owned(),
            },
        );
        let mut handler = |_: &PhysicsEvent, _: &World| Ok(EventOutcome::default());
        engine.step_transition(&mut handler).unwrap();
        let key = engine.execution_state_key();
        let error = engine
            .schedule_external_in_phase_checked(
                4,
                pos,
                PhysicsEventPhase::NeighborUpdate,
                PhysicsEventKind::NeighborUpdate { source: pos },
            )
            .unwrap_err();
        assert!(matches!(
            error,
            PhysicsEngineError::CausalOrderViolation {
                parent: PhysicsTime {
                    game_tick: 4,
                    phase: PhysicsEventPhase::BlockEvent,
                    ..
                },
                child_phase: PhysicsEventPhase::NeighborUpdate,
            }
        ));
        assert_eq!(engine.execution_state_key(), key);
        assert!(engine.trace_status().is_failed());
        engine
            .run_until_idle_checked(|_: &PhysicsEvent, _: &World| Ok(EventOutcome::default()))
            .unwrap();
        assert!(engine.trace_status().is_failed());
    }

    #[test]
    fn unchecked_external_phase_regression_is_rejected_before_handler() {
        let pos = Pos::new(0, 0, 0);
        let mut engine = PhysicsEngine::new(World::new(), 8);
        engine.schedule_external_in_phase(
            4,
            pos,
            PhysicsEventPhase::BlockEvent,
            PhysicsEventKind::UserAction {
                action: "phase".to_owned(),
            },
        );
        let mut handler = |_: &PhysicsEvent, _: &World| Ok(EventOutcome::default());
        engine.step_transition(&mut handler).unwrap();
        engine.schedule_external_in_phase(
            4,
            pos,
            PhysicsEventPhase::NeighborUpdate,
            PhysicsEventKind::NeighborUpdate { source: pos },
        );
        let before_key = engine.execution_state_key();
        let error = engine.step_transition(&mut handler).unwrap_err();
        assert!(matches!(
            error,
            PhysicsEngineError::CausalOrderViolation {
                parent: PhysicsTime {
                    game_tick: 4,
                    phase: PhysicsEventPhase::BlockEvent,
                    ..
                },
                child_phase: PhysicsEventPhase::NeighborUpdate,
            }
        ));
        assert_eq!(engine.execution_state_key(), before_key);
        assert_eq!(engine.pending_event_count(), 1);
    }

    #[test]
    fn same_tick_microstep_budget_is_independent_from_total_event_budget() {
        let pos = Pos::new(0, 0, 0);
        let mut engine = PhysicsEngine::new(World::new(), 100).with_max_microsteps_per_game_tick(2);
        engine.schedule_external(4, pos, PhysicsEventKind::ScheduledBlockTick);
        let error = engine
            .run_until_idle(|_, _| EventOutcome {
                changes: Vec::new(),
                delta: None,
                queued: vec![QueuedEvent {
                    delay_ticks: 0,
                    target: pos,
                    phase: PhysicsEventPhase::ScheduledTick,
                    kind: PhysicsEventKind::ScheduledBlockTick,
                }],
            })
            .unwrap_err();
        assert!(matches!(
            error,
            PhysicsEngineError::MicrostepLimitExceeded {
                game_tick: 4,
                limit: 2,
                processed: 2,
                ..
            }
        ));
        assert_eq!(engine.queue.len(), 1);
        assert_eq!(engine.processed_events, 2);
    }

    #[test]
    fn runs_a_piston_block_event_as_ordered_start_and_completion() {
        let piston_pos = Pos::new(0, 1, 0);
        let mut world = World::new();
        world.set(Pos::new(0, 0, 0), Block::new(BlockKind::Solid));
        let mut piston = Block::new(BlockKind::Piston);
        piston.facing = Some(crate::Facing::East);
        world.set(piston_pos, piston);
        world.set(Pos::new(1, 1, 0), Block::new(BlockKind::Solid));
        let mut engine = PhysicsEngine::new(world, 8);
        let event_id = engine.schedule_piston_action(4, piston_pos, PistonAction::Extend);
        engine.run_piston_events().unwrap();
        assert_eq!(engine.world().kind_at(Pos::new(1, 1, 0)), BlockKind::Air);
        assert_eq!(engine.world().kind_at(Pos::new(2, 1, 0)), BlockKind::Solid);
        assert_eq!(
            piston_state(engine.world().get(piston_pos).unwrap()),
            PistonState::Extended
        );
        let completion_tick =
            4 + crate::DEFAULT_PISTON_MOTION_PROFILE.stable_completion_delay_game_ticks();
        assert_eq!(engine.time().game_tick, completion_tick);
        assert_eq!(engine.trace().len(), 4);
        assert_eq!(engine.trace()[0].cause, event_id);
        assert_eq!(engine.trace()[0].time.game_tick, 4);
        assert_eq!(
            piston_state(&engine.trace()[0].after),
            PistonState::Extending
        );
        assert!(engine.trace()[1..].iter().all(|transition| {
            transition.time.game_tick == completion_tick && transition.cause != event_id
        }));
        assert_eq!(engine.shape_transitions().len(), 2);
        let start = &engine.shape_transitions()[0];
        assert_eq!(start.cause, event_id);
        assert_ne!(start.from, start.to);
        assert!(start.delta.moves.is_empty());
        let completion = &engine.shape_transitions()[1];
        assert_ne!(completion.cause, event_id);
        assert_ne!(completion.from, completion.to);
        assert_eq!(completion.time.game_tick, completion_tick);
        assert_eq!(completion.delta.moves.len(), 1);
        assert!(engine.trace().iter().all(|transition| {
            matches!(
                &transition.delta_cause,
                Some(crate::DeltaCause::PistonExtend { piston }) if *piston == piston_pos
            )
        }));
        assert_eq!(engine.transition_trace().len(), 2);
        let transitions = &engine.transition_trace().records;
        assert_eq!(transitions[0].trigger, event_id);
        assert_eq!(transitions[0].time.game_tick, 4);
        assert_eq!(transitions[0].elapsed_from_previous, None);
        assert_ne!(transitions[0].from_state, transitions[0].to_state);
        assert_eq!(transitions[1].time.game_tick, completion_tick);
        assert_eq!(
            transitions[1].elapsed_from_previous,
            Some(TransitionElapsed::ExactGameTicks {
                game_ticks: crate::DEFAULT_PISTON_MOTION_PROFILE
                    .stable_completion_delay_game_ticks(),
            })
        );
        assert_ne!(transitions[1].from_state, transitions[1].to_state);
        assert_eq!(transitions[1].moves.len(), 1);
    }

    #[test]
    fn zero_motion_profile_preserves_same_tick_causal_order() {
        let piston_pos = Pos::new(0, 1, 0);
        let mut world = World::new();
        world.set(Pos::new(0, 0, 0), Block::new(BlockKind::Solid));
        let mut piston = Block::new(BlockKind::Piston);
        piston.facing = Some(crate::Facing::East);
        world.set(piston_pos, piston);
        world.set(Pos::new(1, 1, 0), Block::new(BlockKind::Solid));
        let profile = crate::PistonMotionProfile {
            initial_delay_min_game_ticks: 0,
            initial_delay_max_game_ticks: 0,
            movement_game_ticks: 0,
        };
        let mut engine = PhysicsEngine::new(world, 8).with_piston_motion_profile(profile);
        let event_id = engine.schedule_piston_action(4, piston_pos, PistonAction::Extend);
        engine.run_piston_events().unwrap();
        assert_eq!(engine.shape_transitions().len(), 2);
        assert_eq!(engine.shape_transitions()[0].time.game_tick, 4);
        assert_eq!(engine.shape_transitions()[1].time.game_tick, 4);
        assert_eq!(engine.shape_transitions()[0].time.sub_tick_order, 0);
        assert_eq!(engine.shape_transitions()[1].time.sub_tick_order, 1);
        assert_eq!(engine.shape_transitions()[0].cause, event_id);
        assert_ne!(engine.shape_transitions()[1].cause, event_id);
        assert_eq!(engine.world().kind_at(Pos::new(2, 1, 0)), BlockKind::Solid);
    }

    #[test]
    fn piston_event_failure_leaves_the_world_unchanged() {
        let piston_pos = Pos::new(0, 1, 0);
        let mut world = World::new();
        world.set(Pos::new(0, 0, 0), Block::new(BlockKind::Solid));
        let mut piston = Block::new(BlockKind::Piston);
        piston.facing = Some(crate::Facing::East);
        world.set(piston_pos, piston);
        world.set(Pos::new(1, 1, 0), Block::new(BlockKind::RedstoneWire));
        let mut engine = PhysicsEngine::new(world.clone(), 8);
        let event_id = engine.schedule_piston_action(0, piston_pos, PistonAction::Extend);
        let error = engine.run_piston_events().unwrap_err();
        assert!(matches!(
            error,
            PhysicsEngineError::Piston(error)
                if matches!(error.as_ref(), PistonError::UnsupportedMovingBlock { .. })
        ));
        assert_eq!(engine.world(), &world);
        assert_eq!(engine.queue.len(), 1);
        assert_eq!(engine.queue.peek().map(|event| event.id), Some(event_id));
        assert!(engine.trace().is_empty());
    }
}
