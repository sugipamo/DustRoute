use std::collections::{BTreeMap, VecDeque};

use super::{
    EventCause, EventId, PhysicsEvent, PhysicsEventKind, PhysicsEventPhase, PhysicsTime,
    SchedulerProfile,
};
use crate::Pos;

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct PhysicsEventQueue {
    events: BTreeMap<u64, BTreeMap<PhysicsEventPhase, VecDeque<PhysicsEvent>>>,
    next_event_id: u64,
    next_sub_tick_order: BTreeMap<u64, u64>,
}

/// The scheduler bookkeeping needed to undo one queue pop.
///
/// Assigning a `sub_tick_order` is part of executing an event, but a rejected
/// handler must not consume that order.  Keeping this token private prevents
/// callers from mutating queue internals while allowing the physics engine to
/// make one event step transactional.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct QueuePopCheckpoint {
    game_tick: u64,
    previous_sub_tick_order: Option<u64>,
}

impl PhysicsEventQueue {
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.events.is_empty()
    }

    #[must_use]
    pub fn len(&self) -> usize {
        self.events
            .values()
            .flat_map(BTreeMap::values)
            .map(VecDeque::len)
            .sum()
    }

    #[must_use]
    pub fn next_time(&self) -> Option<PhysicsTime> {
        self.next_time_with_profile(&SchedulerProfile::default())
    }

    /// Returns the next event according to a versioned phase order.
    pub(crate) fn next_time_with_profile(&self, profile: &SchedulerProfile) -> Option<PhysicsTime> {
        self.events.first_key_value().and_then(|(_, phases)| {
            phases
                .iter()
                .min_by_key(|(phase, _)| profile.phase_rank(**phase))
                .and_then(|(_, events)| events.front())
                .map(|event| event.time)
        })
    }

    #[must_use]
    pub fn peek(&self) -> Option<&PhysicsEvent> {
        self.peek_with_profile(&SchedulerProfile::default())
    }

    /// Returns the next pending event according to a versioned phase order.
    #[must_use]
    pub fn peek_with_profile(&self, profile: &SchedulerProfile) -> Option<&PhysicsEvent> {
        self.events
            .first_key_value()
            .and_then(|(_, phases)| {
                phases
                    .iter()
                    .min_by_key(|(phase, _)| profile.phase_rank(**phase))
            })
            .and_then(|(_, events)| events.front())
    }

    /// Iterates over all pending events in the same order in which `pop`
    /// would deliver them.  The iterator is intentionally read-only so a
    /// runner can preflight a specialized handler without consuming or
    /// reordering the queue.
    pub fn iter(&self) -> impl Iterator<Item = &PhysicsEvent> {
        self.ordered_events(&SchedulerProfile::default())
            .into_iter()
    }

    /// Returns pending events in the order selected by `profile`. Stable sort
    /// preserves insertion order for events sharing one phase.
    pub(crate) fn ordered_events<'a>(
        &'a self,
        profile: &SchedulerProfile,
    ) -> Vec<&'a PhysicsEvent> {
        let mut events = self
            .events
            .values()
            .flat_map(BTreeMap::values)
            .flat_map(VecDeque::iter)
            .collect::<Vec<_>>();
        events.sort_by_key(|event| (event.time.game_tick, profile.phase_rank(event.time.phase)));
        events
    }

    /// Returns insertion counters for the current and future game ticks.
    /// Counters for completed ticks are execution history and are omitted from
    /// the comparison key; callers should not mutate scheduler bookkeeping.
    pub(crate) fn next_sub_tick_orders_from(&self, minimum_game_tick: u64) -> BTreeMap<u64, u64> {
        self.next_sub_tick_order
            .range(minimum_game_tick..)
            .map(|(tick, order)| (*tick, *order))
            .collect()
    }

    pub fn schedule(
        &mut self,
        game_tick: u64,
        target: Pos,
        cause: EventCause,
        kind: PhysicsEventKind,
    ) -> EventId {
        self.schedule_in_phase(game_tick, target, cause, kind.default_phase(), kind)
    }

    pub fn schedule_in_phase(
        &mut self,
        game_tick: u64,
        target: Pos,
        cause: EventCause,
        phase: PhysicsEventPhase,
        kind: PhysicsEventKind,
    ) -> EventId {
        let id = EventId(self.next_event_id);
        self.next_event_id += 1;
        let time = PhysicsTime {
            game_tick,
            phase,
            // The execution order is assigned by `pop`, after phase ordering
            // is known.  A scheduled event has no execution order yet.
            sub_tick_order: 0,
        };
        self.events
            .entry(game_tick)
            .or_default()
            .entry(phase)
            .or_default()
            .push_back(PhysicsEvent {
                id,
                time,
                target,
                cause,
                kind,
            });
        id
    }

    pub fn pop(&mut self) -> Option<PhysicsEvent> {
        self.pop_with_profile(&SchedulerProfile::default())
    }

    /// Pops the next event according to a versioned phase order.
    pub(crate) fn pop_with_profile(&mut self, profile: &SchedulerProfile) -> Option<PhysicsEvent> {
        self.pop_with_checkpoint_in_profile(profile)
            .map(|(event, _)| event)
    }

    pub(crate) fn pop_with_checkpoint_in_profile(
        &mut self,
        profile: &SchedulerProfile,
    ) -> Option<(PhysicsEvent, QueuePopCheckpoint)> {
        let game_tick = *self.events.first_key_value()?.0;
        let phases = self.events.get_mut(&game_tick).expect("key exists");
        let phase = *phases
            .iter()
            .min_by_key(|(phase, _)| profile.phase_rank(**phase))?
            .0;
        let queue = phases.get_mut(&phase).expect("phase exists");
        let mut event = queue.pop_front().expect("non-empty phase queue");
        if queue.is_empty() {
            phases.remove(&phase);
        }
        if phases.is_empty() {
            self.events.remove(&game_tick);
        }
        let previous_sub_tick_order = self.next_sub_tick_order.get(&game_tick).copied();
        let sub_tick_order = self.next_sub_tick_order.entry(game_tick).or_default();
        event.time = PhysicsTime {
            game_tick,
            phase,
            sub_tick_order: *sub_tick_order,
        };
        *sub_tick_order += 1;
        Some((
            event,
            QueuePopCheckpoint {
                game_tick,
                previous_sub_tick_order,
            },
        ))
    }

    /// Reverses a pop, including the per-tick order counter.  The event is
    /// restored at the front of its original phase queue and keeps its ID.
    pub(crate) fn rollback_pop(&mut self, event: PhysicsEvent, checkpoint: QueuePopCheckpoint) {
        self.push_front(event);
        match checkpoint.previous_sub_tick_order {
            Some(order) => {
                self.next_sub_tick_order.insert(checkpoint.game_tick, order);
            }
            None => {
                self.next_sub_tick_order.remove(&checkpoint.game_tick);
            }
        }
    }

    /// Puts an event back at the front without allocating a new event ID.
    /// This is used when a handler rejects an event; a failed run must not
    /// silently discard the work item that caused the failure.
    pub fn push_front(&mut self, mut event: PhysicsEvent) {
        let phase = event.time.phase;
        event.time.sub_tick_order = 0;
        self.events
            .entry(event.time.game_tick)
            .or_default()
            .entry(phase)
            .or_default()
            .push_front(event);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn orders_by_tick_then_insertion_order() {
        let mut queue = PhysicsEventQueue::default();
        let kind = PhysicsEventKind::ScheduledBlockTick;
        queue.schedule(4, Pos::new(4, 0, 0), EventCause::External, kind.clone());
        queue.schedule(2, Pos::new(2, 0, 0), EventCause::External, kind.clone());
        queue.schedule(2, Pos::new(3, 0, 0), EventCause::External, kind);
        let events = [
            queue.pop().unwrap(),
            queue.pop().unwrap(),
            queue.pop().unwrap(),
        ];
        assert_eq!(events.each_ref().map(|event| event.target.x), [2, 3, 4]);
        assert!(
            events
                .iter()
                .all(|event| { event.time.phase == PhysicsEventPhase::ScheduledTick })
        );
        assert_eq!(events[0].time.sub_tick_order, 0);
        assert_eq!(events[1].time.sub_tick_order, 1);
    }

    #[test]
    fn orders_phases_before_same_phase_insertion_order() {
        let mut queue = PhysicsEventQueue::default();
        queue.schedule_in_phase(
            2,
            Pos::new(2, 0, 0),
            EventCause::External,
            PhysicsEventPhase::BlockEvent,
            PhysicsEventKind::BlockEvent {
                event: super::super::BlockEventKind::Custom {
                    event_type: 1,
                    data: 0,
                },
            },
        );
        queue.schedule_in_phase(
            2,
            Pos::new(0, 0, 0),
            EventCause::External,
            PhysicsEventPhase::NeighborUpdate,
            PhysicsEventKind::NeighborUpdate {
                source: Pos::new(0, 0, 0),
            },
        );
        queue.schedule_in_phase(
            2,
            Pos::new(1, 0, 0),
            EventCause::External,
            PhysicsEventPhase::BlockEvent,
            PhysicsEventKind::BlockEvent {
                event: super::super::BlockEventKind::Custom {
                    event_type: 2,
                    data: 0,
                },
            },
        );

        let events = [
            queue.pop().unwrap(),
            queue.pop().unwrap(),
            queue.pop().unwrap(),
        ];
        assert_eq!(events.each_ref().map(|event| event.target.x), [0, 2, 1]);
        assert_eq!(events[0].time.phase, PhysicsEventPhase::NeighborUpdate);
        assert_eq!(events[1].time.phase, PhysicsEventPhase::BlockEvent);
        assert_eq!(events[2].time.phase, PhysicsEventPhase::BlockEvent);
        assert_eq!(
            events.each_ref().map(|event| event.time.sub_tick_order),
            [0, 1, 2]
        );
    }

    #[test]
    fn custom_profile_controls_same_tick_phase_order() {
        let mut queue = PhysicsEventQueue::default();
        let profile = SchedulerProfile {
            phase_order: [
                PhysicsEventPhase::BlockEvent,
                PhysicsEventPhase::External,
                PhysicsEventPhase::NeighborUpdate,
                PhysicsEventPhase::ScheduledTick,
                PhysicsEventPhase::BlockEntity,
                PhysicsEventPhase::Observation,
            ],
            ..SchedulerProfile::default()
        };
        queue.schedule_in_phase(
            2,
            Pos::new(1, 0, 0),
            EventCause::External,
            PhysicsEventPhase::External,
            PhysicsEventKind::UserAction {
                action: "external".to_owned(),
            },
        );
        queue.schedule_in_phase(
            2,
            Pos::new(2, 0, 0),
            EventCause::External,
            PhysicsEventPhase::BlockEvent,
            PhysicsEventKind::BlockEvent {
                event: super::super::BlockEventKind::Custom {
                    event_type: 1,
                    data: 0,
                },
            },
        );

        let first = queue.pop_with_profile(&profile).unwrap();
        let second = queue.pop_with_profile(&profile).unwrap();
        assert_eq!(first.time.phase, PhysicsEventPhase::BlockEvent);
        assert_eq!(second.time.phase, PhysicsEventPhase::External);
    }

    #[test]
    fn requeued_event_keeps_its_id_and_phase() {
        let mut queue = PhysicsEventQueue::default();
        let id = queue.schedule(
            4,
            Pos::new(4, 0, 0),
            EventCause::External,
            PhysicsEventKind::ScheduledBlockTick,
        );
        let event = queue.pop().unwrap();
        assert_eq!(event.id, id);
        queue.push_front(event);
        let restored = queue.pop().unwrap();
        assert_eq!(restored.id, id);
        assert_eq!(restored.time.phase, PhysicsEventPhase::ScheduledTick);
    }
}
