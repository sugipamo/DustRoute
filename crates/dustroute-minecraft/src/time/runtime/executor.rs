use std::any::TypeId;
use std::collections::{BTreeMap, VecDeque};
use std::marker::PhantomData;
use std::sync::Arc;

use super::*;
use crate::time::TraceStatus;

mod queue;
mod root_behavior;
mod transaction;
use queue::{Delivery, Guard};
pub use root_behavior::RootBehaviorState;

#[derive(Clone, Debug, Eq, PartialEq)]
struct State<P> {
    profile: &'static str,
    adapter: &'static str,
    adapter_type: TypeId,
    // Read-only invocations and checkpoints share the complete world. A write
    // detaches it while the outer staged State still owns queue/history/IDs.
    world: Arc<World>,
    region: Region,
    time: RuntimeTime,
    pending: BTreeMap<RuntimeTime, VecDeque<Delivery<P>>>,
    input: Option<Invocation<P>>,
    stack: Vec<Invocation<P>>,
    carriers: BTreeMap<Pos, CarrierState>,
    staged_carriers: BTreeMap<Pos, Block>,
    outputs: Arc<BTreeMap<Pos, u8>>,
    histories: Arc<history::Histories>,
    next_id: u64,
    next_carrier: u64,
    processed: usize,
    limits: RuntimeLimits,
}

/// Exact comparison, not a world hash or a normalized cycle-detection key.
/// Includes clocks, queues, continuations, identities, budgets and failure.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RuntimeStateKey<P> {
    state: State<P>,
    status: TraceStatus,
}

/// In-memory continuation evidence. No deserialization or mutable world access.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RuntimeCheckpoint<P> {
    state: State<P>,
    status: TraceStatus,
    trace: Vec<RuntimeRecord<P>>,
}

/// The old phase scheduler is intentionally not used for synchronous calls.
/// One external-input boundary encloses a complete root call and its nested
/// callbacks/continuations. Microsteps remain observable and checkpointable.
pub struct SynchronousWorldRuntime<A: RuntimeAdapter> {
    state: State<A::Payload>,
    status: TraceStatus,
    trace: Vec<RuntimeRecord<A::Payload>>,
    adapter: PhantomData<A>,
}

impl<P: Clone + Eq> State<P> {
    fn view(&self) -> RuntimeView<'_> {
        RuntimeView {
            adapter_revision: self.adapter,
            world: &self.world,
            region: self.region,
            time: self.time,
            carriers: &self.carriers,
            staged_carriers: &self.staged_carriers,
            outputs: &self.outputs,
            histories: &self.histories,
            limits: self.limits,
            block_ticks: &self.pending,
        }
    }

    fn check_position(&self, position: Pos) -> Result<(), RuntimeError> {
        if !self.region.contains(position) {
            return Err(RuntimeError::UnknownSpace(position));
        }
        Ok(())
    }

    fn id(&mut self) -> Result<u64, RuntimeError> {
        let id = self.next_id;
        self.next_id = id.checked_add(1).ok_or(RuntimeError::ClockOverflow)?;
        Ok(id)
    }

    fn carrier_id(&mut self) -> Result<CarrierId, RuntimeError> {
        let id = self.next_carrier;
        self.next_carrier = id.checked_add(1).ok_or(RuntimeError::ClockOverflow)?;
        Ok(CarrierId(id))
    }

    fn pending_count(&self) -> usize {
        self.pending.values().map(VecDeque::len).sum::<usize>()
            + self.stack.len()
            + usize::from(self.input.is_some())
    }

    fn check_pending_limit(&self) -> Result<(), RuntimeError> {
        if self.pending_count() > self.limits.max_pending {
            return Err(RuntimeError::Limit("pending work"));
        }
        Ok(())
    }
}

impl<A: RuntimeAdapter> SynchronousWorldRuntime<A> {
    pub fn new(world: World, region: Region, limits: RuntimeLimits) -> Result<Self, RuntimeError> {
        if A::REVISION.trim().is_empty()
            || !region.contains(region.min)
            || !region.contains(region.max)
        {
            return Err(RuntimeError::Invalid(
                "invalid adapter or known region".into(),
            ));
        }
        for (pos, block) in world.iter() {
            if !region.contains(*pos) {
                return Err(RuntimeError::UnknownSpace(*pos));
            }
            if block.kind == BlockKind::MovingPiston
                || block.piston_entity.is_some()
                || block.piston_state.is_some_and(|s| !s.is_stable())
            {
                return Err(RuntimeError::Invalid(
                    "initial motion requires a runtime checkpoint".into(),
                ));
            }
        }
        if limits.max_microsteps == 0 || limits.max_pending == 0 {
            return Err(RuntimeError::Invalid(
                "runtime budgets must be nonzero".into(),
            ));
        }
        let runtime = Self {
            state: State {
                profile: PROFILE,
                adapter: A::REVISION,
                adapter_type: TypeId::of::<A>(),
                world: Arc::new(world),
                region,
                time: RuntimeTime {
                    game_tick: 0,
                    section: TickSection::External,
                },
                pending: BTreeMap::new(),
                input: None,
                stack: Vec::new(),
                carriers: BTreeMap::new(),
                staged_carriers: BTreeMap::new(),
                outputs: Arc::new(BTreeMap::new()),
                histories: Arc::new(BTreeMap::new()),
                next_id: 0,
                next_carrier: 0,
                processed: 0,
                limits,
            },
            status: TraceStatus::InProgress,
            trace: Vec::new(),
            adapter: PhantomData,
        };
        A::validate_initial(runtime.view())?;
        Ok(runtime)
    }

    pub fn from_checkpoint(
        checkpoint: &RuntimeCheckpoint<A::Payload>,
    ) -> Result<Self, RuntimeError> {
        Self::check_restore_identity(&checkpoint.state, "checkpoint")?;
        Ok(Self {
            state: checkpoint.state.clone(),
            status: checkpoint.status.clone(),
            trace: checkpoint.trace.clone(),
            adapter: PhantomData,
        })
    }

    fn check_restore_identity(state: &State<A::Payload>, source: &str) -> Result<(), RuntimeError> {
        if state.profile != PROFILE
            || state.adapter != A::REVISION
            || state.adapter_type != TypeId::of::<A>()
        {
            return Err(RuntimeError::Invalid(format!(
                "{source} execution profile/adapter mismatch"
            )));
        }
        Ok(())
    }

    pub fn checkpoint(&self) -> RuntimeCheckpoint<A::Payload> {
        RuntimeCheckpoint {
            state: self.state.clone(),
            status: self.status.clone(),
            trace: self.trace.clone(),
        }
    }

    pub fn state_key(&self) -> RuntimeStateKey<A::Payload> {
        RuntimeStateKey {
            state: self.state.clone(),
            status: self.status.clone(),
        }
    }

    pub fn view(&self) -> RuntimeView<'_> {
        self.state.view()
    }
    pub fn trace(&self) -> &[RuntimeRecord<A::Payload>] {
        &self.trace
    }
    pub fn status(&self) -> &TraceStatus {
        &self.status
    }
    pub fn pending_count(&self) -> usize {
        self.state.pending_count()
    }
    pub fn at_input_boundary(&self) -> bool {
        self.state.stack.is_empty() && self.state.input.is_none() && !self.status.is_failed()
    }

    fn ensure_running(&self) -> Result<(), RuntimeError> {
        if let TraceStatus::Failed { error } = &self.status {
            return Err(RuntimeError::Failed(error.clone()));
        }
        Ok(())
    }

    /// Adds future external work only at a root boundary. During a handler,
    /// new work is part of its transactional `RuntimeOutcome` instead.
    pub fn enqueue(
        &mut self,
        request: QueueRequest<A::Payload>,
    ) -> Result<Option<u64>, RuntimeError> {
        self.ensure_running()?;
        if !self.at_input_boundary() {
            return Err(RuntimeError::InputInsideCallback);
        }
        let mut staged = self.state.clone();
        let id = staged.enqueue(request, None)?;
        self.state = staged;
        if id.is_some() {
            self.status = TraceStatus::InProgress;
        }
        Ok(id)
    }

    /// An input between complete root operations inherits the current world
    /// section. It never travels backward to the global External section.
    pub fn input_now(&mut self, call: RuntimeCall<A::Payload>) -> Result<u64, RuntimeError> {
        self.ensure_running()?;
        if !self.at_input_boundary() {
            return Err(RuntimeError::InputInsideCallback);
        }
        let mut staged = self.state.clone();
        staged.check_position(call.target)?;
        let id = staged.id()?;
        staged.input = Some(Invocation {
            id,
            root: id,
            cause: None,
            time: staged.time,
            kind: InvocationKind::Input,
            depth: 0,
            call,
        });
        staged.check_pending_limit()?;
        self.state = staged;
        self.status = TraceStatus::InProgress;
        Ok(id)
    }

    /// Executes one staged operation. Rejection preserves its pending work,
    /// time, IDs and physical state; the accepted prefix is marked failed.
    pub fn microstep(&mut self) -> Result<Option<RuntimeRecord<A::Payload>>, RuntimeError> {
        self.ensure_running()?;
        let result = self.try_microstep();
        if let Err(error) = &result {
            self.status = TraceStatus::Failed {
                error: error.to_string(),
            };
        }
        result
    }

    /// Finishes exactly one root operation, including a chain restored in the
    /// middle of a callback. Inputs may be supplied after this boundary.
    pub fn step(&mut self) -> Result<bool, RuntimeError> {
        if self.microstep()?.is_none() {
            return Ok(false);
        }
        while !self.state.stack.is_empty() {
            self.microstep()?;
        }
        Ok(true)
    }

    pub fn run_until_idle(&mut self) -> Result<(), RuntimeError> {
        while self.step()? {}
        Ok(())
    }
}
