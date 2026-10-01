use std::any::TypeId;
use std::collections::{BTreeMap, BTreeSet, VecDeque};
use std::marker::PhantomData;
use std::sync::Arc;

use super::*;
use crate::time::TraceStatus;

mod root_behavior;
pub use root_behavior::RootBehaviorState;

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
enum Guard {
    None,
    Block(BlockIdentity),
    Carrier(CarrierId),
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
struct Delivery<P> {
    invocation: Invocation<P>,
    guard: Guard,
    #[serde(skip_serializing_if = "Option::is_none")]
    priority: Option<u8>,
}

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
    outputs: BTreeMap<Pos, u8>,
    histories: history::Histories,
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

impl<P> BlockTickQuery for BTreeMap<RuntimeTime, VecDeque<Delivery<P>>> {
    fn contains(
        &self,
        position: Pos,
        block: &BlockIdentity,
        time: RuntimeTime,
        ticking: bool,
    ) -> bool {
        self.iter().any(|(due, queue)| {
            let ready = due.game_tick == time.game_tick && time.section >= TickSection::BlockTicks;
            let selected = if ticking {
                ready && time.section == TickSection::BlockTicks
            } else {
                !ready
            };
            selected
                && queue.iter().any(|pending| {
                    pending.priority.is_some()
                        && pending.guard == Guard::Block(block.clone())
                        && pending.invocation.call.target == position
                })
        })
    }
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

    fn enqueue(
        &mut self,
        request: QueueRequest<P>,
        cause: Option<u64>,
    ) -> Result<Option<u64>, RuntimeError> {
        let (time, kind, guard, call, priority) = match request {
            QueueRequest::External { game_tick, call } => (
                RuntimeTime {
                    game_tick,
                    section: TickSection::External,
                },
                InvocationKind::External,
                Guard::None,
                call,
                None,
            ),
            QueueRequest::AfterWorldTick { game_tick, call } => (
                RuntimeTime {
                    game_tick,
                    section: TickSection::AfterWorldTick,
                },
                InvocationKind::External,
                Guard::None,
                call,
                None,
            ),
            QueueRequest::ScheduledTick { game_tick, call } => (
                RuntimeTime {
                    game_tick,
                    section: TickSection::BlockTicks,
                },
                InvocationKind::ScheduledTick,
                Guard::None,
                call,
                None,
            ),
            QueueRequest::PrioritizedBlockTick {
                game_tick,
                priority,
                block,
                call,
            } => (
                RuntimeTime {
                    game_tick,
                    section: TickSection::BlockTicks,
                },
                InvocationKind::ScheduledTick,
                Guard::Block(block),
                call,
                Some(priority),
            ),
            QueueRequest::BlockEvent { block, call } => (
                self.time.next_block_event()?,
                InvocationKind::BlockEvent,
                Guard::Block(block),
                call,
                None,
            ),
            QueueRequest::CarrierTick {
                game_tick,
                carrier,
                call,
            } => (
                RuntimeTime {
                    game_tick,
                    section: TickSection::BlockEntities,
                },
                InvocationKind::CarrierTick,
                Guard::Carrier(carrier),
                call,
                None,
            ),
        };
        self.check_position(call.target)?;
        if time < self.time {
            return Err(RuntimeError::PastDelivery {
                current: self.time,
                requested: time,
            });
        }
        if priority.is_some()
            && time.game_tick == self.time.game_tick
            && self.time.section >= TickSection::BlockTicks
        {
            return Err(RuntimeError::Invalid(
                "native ticks requested from the ready batch require a future game tick".into(),
            ));
        }
        if priority.is_some()
            && let Guard::Block(block) = &guard
            && self.view().block_tick_queued(call.target, block)
        {
            // ChunkTickScheduler deduplicates pending ticks; callbacks already
            // collected into the current ready batch no longer count as queued.
            return Ok(None);
        }
        // Like the source block-event set, duplicate pending requests retain
        // the first insertion. A delivered request can be requested again.
        if kind == InvocationKind::BlockEvent
            && self.pending.get(&time).is_some_and(|queue| {
                queue.iter().any(|pending| {
                    pending.invocation.kind == kind
                        && pending.guard == guard
                        && pending.invocation.call == call
                })
            })
        {
            return Ok(None);
        }
        let id = self.id()?;
        let invocation = Invocation {
            id,
            root: id,
            cause,
            time,
            kind,
            depth: 0,
            call,
        };
        let queue = self.pending.entry(time).or_default();
        let index = priority
            .and_then(|priority| {
                queue
                    .iter()
                    .position(|pending| pending.priority.is_none_or(|old| old > priority))
            })
            .unwrap_or(queue.len());
        queue.insert(
            index,
            Delivery {
                invocation,
                guard,
                priority,
            },
        );
        self.check_pending_limit()?;
        Ok(Some(id))
    }

    fn take_delivery(&mut self) -> Option<Delivery<P>> {
        if let Some(invocation) = self.input.take() {
            return Some(Delivery {
                invocation,
                guard: Guard::None,
                priority: None,
            });
        }
        let time = *self.pending.first_key_value()?.0;
        let queue = self.pending.get_mut(&time).expect("existing key");
        let delivery = queue.pop_front().expect("nonempty queue");
        if queue.is_empty() {
            self.pending.remove(&time);
        }
        self.time = time;
        Some(delivery)
    }

    fn delivery_result(&self, delivery: &Delivery<P>) -> Result<DeliveryResult, RuntimeError> {
        let pos = delivery.invocation.call.target;
        Ok(match &delivery.guard {
            Guard::None => DeliveryResult::Executed,
            Guard::Block(identity) => {
                if BlockIdentity::of(&self.view().block(pos)?) == *identity {
                    DeliveryResult::Executed
                } else {
                    DeliveryResult::BlockReplaced
                }
            }
            Guard::Carrier(id) => {
                if self.carriers.get(&pos).is_some_and(|c| c.id == *id) {
                    DeliveryResult::Executed
                } else {
                    DeliveryResult::CarrierRetired
                }
            }
        })
    }

    fn apply_carriers(&mut self, effects: Vec<CarrierEffect>) -> Result<(), RuntimeError> {
        let mut touched = BTreeSet::new();
        for effect in effects {
            let position = effect.position();
            self.check_position(position)?;
            if !touched.insert(position) {
                return Err(RuntimeError::CarrierConflict(position));
            }
            let existing = self.carriers.get(&position).copied();
            let (after, history) = match effect {
                CarrierEffect::Stage { block, .. } => {
                    if existing.is_some()
                        || self.staged_carriers.contains_key(&position)
                        || block.kind != BlockKind::MovingPiston
                        || block.piston_entity.is_none()
                    {
                        return Err(RuntimeError::CarrierConflict(position));
                    }
                    self.staged_carriers.insert(position, *block);
                    (None, None)
                }
                CarrierEffect::Install { history, .. } => {
                    if existing.is_some() {
                        return Err(RuntimeError::CarrierConflict(position));
                    }
                    if let Some(block) = self.staged_carriers.remove(&position)
                        && self.world.get(position) != Some(&block)
                    {
                        return Err(RuntimeError::CarrierConflict(position));
                    }
                    (Some(self.carrier_id()?), Some(history))
                }
                CarrierEffect::Update {
                    expected, history, ..
                } => {
                    if existing != Some(expected) {
                        return Err(RuntimeError::CarrierConflict(position));
                    }
                    (Some(expected.id), Some(history))
                }
                CarrierEffect::Retire { expected, .. } => {
                    if existing != Some(expected) {
                        return Err(RuntimeError::CarrierConflict(position));
                    }
                    (None, None)
                }
                CarrierEffect::Replace {
                    expected, history, ..
                } => {
                    if existing != Some(expected) {
                        return Err(RuntimeError::CarrierConflict(position));
                    }
                    (Some(self.carrier_id()?), Some(history))
                }
            };
            if let (Some(id), Some(history)) = (after, history) {
                if history.last_progress > history.progress
                    || history.saved_world_time > self.time.game_tick
                {
                    return Err(RuntimeError::Invalid("inconsistent motion history".into()));
                }
                self.carriers.insert(position, CarrierState { id, history });
            } else {
                self.carriers.remove(&position);
            }
        }
        Ok(())
    }

    fn validate_carriers(
        &self,
        before: &Self,
        delta: Option<&WorldDelta>,
    ) -> Result<(), RuntimeError> {
        // The previous state was validated and only WorldDelta can write the
        // world. Include removed metadata too: retiring a carrier without
        // removing its moving block must still be rejected.
        let positions = delta
            .into_iter()
            .flat_map(WorldDelta::changed_positions)
            .chain(before.carriers.keys().copied())
            .chain(self.carriers.keys().copied())
            .chain(before.staged_carriers.keys().copied())
            .chain(self.staged_carriers.keys().copied())
            .collect::<BTreeSet<_>>();
        for position in positions {
            let Some(block) = self.world.get(position) else {
                continue;
            };
            if block.kind == BlockKind::MovingPiston
                && (block.piston_entity.is_none() || !self.carriers.contains_key(&position))
                && !self.staged_carriers.contains_key(&position)
            {
                return Err(RuntimeError::CarrierConflict(position));
            }
        }
        for (position, planned) in &self.staged_carriers {
            let mut bare = planned.clone();
            bare.piston_entity = None;
            if self.world.get(*position) != Some(&bare)
                || self.carriers.contains_key(position)
                || self.stack.is_empty()
            {
                return Err(RuntimeError::CarrierConflict(*position));
            }
        }
        for (position, carrier) in &self.carriers {
            if !self
                .world
                .get(*position)
                .is_some_and(|b| b.kind == BlockKind::MovingPiston && b.piston_entity.is_some())
            {
                return Err(RuntimeError::CarrierConflict(*position));
            }
            // Changing a payload/body identity requires a new lifetime token.
            if before
                .carriers
                .get(position)
                .is_some_and(|c| c.id == carrier.id)
                && before.world.get(*position) != self.world.get(*position)
            {
                return Err(RuntimeError::CarrierConflict(*position));
            }
        }
        Ok(())
    }

    fn apply_outcome(
        &mut self,
        parent: &Invocation<P>,
        outcome: RuntimeOutcome<P>,
    ) -> Result<(), RuntimeError> {
        if let Some(delta) = outcome.delta {
            for pos in delta
                .changed_positions()
                .chain(delta.moves.iter().flat_map(|m| [m.from, m.to]))
            {
                self.check_position(pos)?;
            }
            let identities: Vec<_> = delta
                .changed_positions()
                .chain(delta.moves.iter().flat_map(|m| [m.from, m.to]))
                .map(|pos| (pos, self.world.get(pos).map(BlockIdentity::of)))
                .collect();
            self.apply_world_delta(&delta)?;
            for (pos, before) in identities {
                if before != self.world.get(pos).map(BlockIdentity::of) {
                    self.outputs.remove(&pos);
                }
            }
        }
        for effect in outcome.outputs {
            self.check_position(effect.position)?;
            if effect.value > 15
                || self.world.get(effect.position).map(BlockIdentity::of) != Some(effect.block)
            {
                return Err(RuntimeError::Invalid(
                    "invalid stored output assignment".into(),
                ));
            }
            if effect.value == 0 {
                self.outputs.remove(&effect.position);
            } else {
                self.outputs.insert(effect.position, effect.value);
            }
        }
        history::apply(
            &mut self.histories,
            outcome.histories,
            self.region,
            self.time.game_tick,
        )?;
        self.apply_carriers(outcome.carriers)?;
        for queued in outcome.queued {
            self.enqueue(queued, Some(parent.id))?;
        }
        if let Some(payload) = outcome.continuation {
            let id = self.id()?;
            self.stack.push(Invocation {
                id,
                root: parent.root,
                cause: Some(parent.id),
                time: parent.time,
                kind: InvocationKind::Continuation,
                depth: parent.depth,
                call: RuntimeCall {
                    target: parent.call.target,
                    payload,
                },
            });
        }
        // Allocate in request order and push in reverse so each child's entire
        // nested chain executes before the next sibling and parent continuation.
        let mut calls = Vec::with_capacity(outcome.callbacks.len());
        for call in outcome.callbacks {
            self.check_position(call.target)?;
            let depth = parent
                .depth
                .checked_add(1)
                .ok_or(RuntimeError::Limit("call depth"))?;
            if depth > self.limits.max_call_depth {
                return Err(RuntimeError::Limit("call depth"));
            }
            calls.push(Invocation {
                id: self.id()?,
                root: parent.root,
                cause: Some(parent.id),
                time: parent.time,
                kind: InvocationKind::Callback,
                depth,
                call,
            });
        }
        self.stack.extend(calls.into_iter().rev());
        self.check_pending_limit()
    }

    /// The sole world-write boundary in a staged invocation. An empty delta
    /// still validates its shape/moves but needs no detached world allocation.
    fn apply_world_delta(&mut self, delta: &WorldDelta) -> Result<(), RuntimeError> {
        let result = if delta.changes.is_empty() {
            delta.validate(&self.world)
        } else {
            delta.apply(Arc::make_mut(&mut self.world))
        };
        result.map_err(|e| RuntimeError::Invalid(e.to_string()))
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
                outputs: BTreeMap::new(),
                histories: BTreeMap::new(),
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

    fn try_microstep(&mut self) -> Result<Option<RuntimeRecord<A::Payload>>, RuntimeError> {
        if self.pending_count() == 0 {
            if !self.state.carriers.is_empty() || !self.state.staged_carriers.is_empty() {
                return Err(RuntimeError::UnfinishedMotion);
            }
            self.status = TraceStatus::Complete;
            return Ok(None);
        }
        if self.state.processed >= self.state.limits.max_microsteps {
            return Err(RuntimeError::Limit("microsteps"));
        }
        let mut staged = self.state.clone();
        let (invocation, result) = if let Some(frame) = staged.stack.pop() {
            (frame, DeliveryResult::Executed)
        } else {
            let delivery = staged.take_delivery().expect("pending root");
            let result = staged.delivery_result(&delivery)?;
            (delivery.invocation, result)
        };
        let outcome = if result == DeliveryResult::Executed {
            A::handle(&invocation, staged.view())?
        } else {
            RuntimeOutcome::default()
        };
        let delta = outcome.delta.clone();
        staged.apply_outcome(&invocation, outcome)?;
        staged.validate_carriers(&self.state, delta.as_ref())?;
        staged.processed += 1;
        let positions: BTreeSet<_> = self
            .state
            .carriers
            .keys()
            .chain(staged.carriers.keys())
            .copied()
            .collect();
        let carrier_changes = positions
            .into_iter()
            .filter_map(|pos| {
                let before = self.state.carriers.get(&pos).copied();
                let after = staged.carriers.get(&pos).copied();
                (before != after).then_some((pos, before, after))
            })
            .collect();
        let output_changes = self
            .state
            .outputs
            .keys()
            .chain(staged.outputs.keys())
            .copied()
            .collect::<BTreeSet<_>>()
            .into_iter()
            .filter_map(|pos| {
                let before = self.state.outputs.get(&pos).copied().unwrap_or(0);
                let after = staged.outputs.get(&pos).copied().unwrap_or(0);
                (before != after).then_some((pos, before, after))
            })
            .collect();
        let history_changes = self
            .state
            .histories
            .keys()
            .chain(staged.histories.keys())
            .cloned()
            .collect::<BTreeSet<_>>()
            .into_iter()
            .filter_map(|key| {
                let before = self.state.histories.get(&key).cloned();
                let after = staged.histories.get(&key).cloned();
                (before != after).then_some(HistoryChange {
                    key: key.0,
                    position: key.1,
                    before,
                    after,
                })
            })
            .collect();
        let record = RuntimeRecord {
            invocation,
            result,
            delta,
            carrier_changes,
            output_changes,
            history_changes,
        };
        self.state = staged;
        self.trace.push(record.clone());
        self.status = TraceStatus::InProgress;
        Ok(Some(record))
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
