//! Delivery admission, deduplication and ordering. Only a staged State is
//! mutated; the runtime owner commits it after admission or execution succeeds.
use super::*;

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub(super) enum Guard {
    None,
    Block(BlockIdentity),
    Carrier(CarrierId),
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub(super) struct Delivery<P> {
    pub(super) invocation: Invocation<P>,
    pub(super) guard: Guard,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(super) priority: Option<u8>,
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
    pub(super) fn enqueue(
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

    pub(super) fn take_delivery(&mut self) -> Option<Delivery<P>> {
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

    pub(super) fn delivery_result(
        &self,
        delivery: &Delivery<P>,
    ) -> Result<DeliveryResult, RuntimeError> {
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
}
