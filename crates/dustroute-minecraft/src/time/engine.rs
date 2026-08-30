use std::error::Error;
use std::fmt::{Display, Formatter};

use super::{
    EventCause, PhysicsEvent, PhysicsEventQueue, PhysicsTime, QueuedEvent, StateTransition,
};
use crate::{Block, BlockKind, Pos, World};

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct WorldChange {
    pub position: Pos,
    pub after: Block,
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct EventOutcome {
    pub changes: Vec<WorldChange>,
    pub queued: Vec<QueuedEvent>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum PhysicsEngineError {
    EventLimitExceeded {
        limit: usize,
        processed: usize,
        next_time: Option<PhysicsTime>,
    },
}

impl Display for PhysicsEngineError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::EventLimitExceeded { limit, .. } => {
                write!(formatter, "Minecraft physics event limit {limit} exceeded")
            }
        }
    }
}

impl Error for PhysicsEngineError {}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PhysicsEngine {
    world: World,
    queue: PhysicsEventQueue,
    trace: Vec<StateTransition>,
    current_time: PhysicsTime,
    max_events: usize,
    processed_events: usize,
}

impl PhysicsEngine {
    #[must_use]
    pub fn new(world: World, max_events: usize) -> Self {
        Self {
            world,
            queue: PhysicsEventQueue::default(),
            trace: Vec::new(),
            current_time: PhysicsTime::default(),
            max_events,
            processed_events: 0,
        }
    }

    #[must_use]
    pub const fn time(&self) -> PhysicsTime {
        self.current_time
    }

    #[must_use]
    pub const fn world(&self) -> &World {
        &self.world
    }

    #[must_use]
    pub fn trace(&self) -> &[StateTransition] {
        &self.trace
    }

    pub fn schedule_external(
        &mut self,
        game_tick: u64,
        target: Pos,
        kind: super::PhysicsEventKind,
    ) -> super::EventId {
        self.queue.schedule(
            game_tick.max(self.current_time.game_tick),
            target,
            EventCause::External,
            kind,
        )
    }

    pub fn run_until_idle(
        &mut self,
        mut handler: impl FnMut(&PhysicsEvent, &World) -> EventOutcome,
    ) -> Result<(), PhysicsEngineError> {
        while !self.queue.is_empty() {
            if self.processed_events >= self.max_events {
                return Err(PhysicsEngineError::EventLimitExceeded {
                    limit: self.max_events,
                    processed: self.processed_events,
                    next_time: self.queue.next_time(),
                });
            }
            let event = self.queue.pop().expect("queue is non-empty");
            self.current_time = event.time;
            let outcome = handler(&event, &self.world);
            for change in outcome.changes {
                let before = self
                    .world
                    .get(change.position)
                    .cloned()
                    .unwrap_or_else(|| Block::new(BlockKind::Air));
                if before == change.after {
                    continue;
                }
                self.world.set(change.position, change.after.clone());
                self.trace.push(StateTransition {
                    time: event.time,
                    position: change.position,
                    before,
                    after: change.after,
                    cause: event.id,
                });
            }
            for queued in outcome.queued {
                self.queue.schedule(
                    event.time.game_tick.saturating_add(queued.delay_ticks),
                    queued.target,
                    EventCause::Event { id: event.id },
                    queued.kind,
                );
            }
            self.processed_events += 1;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::time::PhysicsEventKind;

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
                    queued: powered
                        .then_some(QueuedEvent {
                            delay_ticks: 0,
                            target: pos,
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
    fn stops_an_unbounded_update_chain() {
        let pos = Pos::new(0, 0, 0);
        let mut engine = PhysicsEngine::new(World::new(), 3);
        engine.schedule_external(0, pos, PhysicsEventKind::ScheduledBlockTick);
        let error = engine
            .run_until_idle(|_, _| EventOutcome {
                changes: Vec::new(),
                queued: vec![QueuedEvent {
                    delay_ticks: 0,
                    target: pos,
                    kind: PhysicsEventKind::ScheduledBlockTick,
                }],
            })
            .unwrap_err();
        assert!(matches!(
            error,
            PhysicsEngineError::EventLimitExceeded { processed: 3, .. }
        ));
    }
}
