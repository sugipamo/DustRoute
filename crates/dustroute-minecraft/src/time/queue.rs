use std::collections::{BTreeMap, VecDeque};

use super::{EventCause, EventId, PhysicsEvent, PhysicsEventKind, PhysicsTime};
use crate::Pos;

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct PhysicsEventQueue {
    events: BTreeMap<u64, VecDeque<PhysicsEvent>>,
    next_event_id: u64,
    next_sub_tick_order: BTreeMap<u64, u64>,
}

impl PhysicsEventQueue {
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.events.is_empty()
    }

    #[must_use]
    pub fn len(&self) -> usize {
        self.events.values().map(VecDeque::len).sum()
    }

    #[must_use]
    pub fn next_time(&self) -> Option<PhysicsTime> {
        self.events
            .first_key_value()
            .and_then(|(_, events)| events.front())
            .map(|event| event.time)
    }

    pub fn schedule(
        &mut self,
        game_tick: u64,
        target: Pos,
        cause: EventCause,
        kind: PhysicsEventKind,
    ) -> EventId {
        let id = EventId(self.next_event_id);
        self.next_event_id += 1;
        let sub_tick_order = self.next_sub_tick_order.entry(game_tick).or_default();
        let time = PhysicsTime {
            game_tick,
            sub_tick_order: *sub_tick_order,
        };
        *sub_tick_order += 1;
        self.events
            .entry(game_tick)
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
        let game_tick = *self.events.first_key_value()?.0;
        let queue = self.events.get_mut(&game_tick).expect("key exists");
        let event = queue.pop_front().expect("non-empty tick queue");
        if queue.is_empty() {
            self.events.remove(&game_tick);
        }
        Some(event)
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
        assert_eq!(events[0].time.sub_tick_order, 0);
        assert_eq!(events[1].time.sub_tick_order, 1);
    }
}
