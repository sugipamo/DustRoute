//! Reject unsupported property shapes before publishing an observed event.
use super::{ObservedBlockState, ObservedTransitionEvent};
use crate::observed_properties::{InvalidObservedProperty, PropertyInput};
use crate::world::Pos;
use serde::Deserialize;

#[derive(Deserialize)]
pub(super) struct State {
    name: String,
    properties: PropertyInput,
}
impl State {
    fn finish(
        self,
        position: Option<Pos>,
        slot: &'static str,
    ) -> Result<ObservedBlockState, InvalidObservedProperty> {
        Ok(ObservedBlockState {
            name: self.name,
            properties: self.properties.finish(position, slot)?,
        })
    }
}
impl TryFrom<State> for ObservedBlockState {
    type Error = InvalidObservedProperty;
    fn try_from(value: State) -> Result<Self, Self::Error> {
        value.finish(None, "state")
    }
}

#[derive(Deserialize)]
pub(super) struct Event {
    sequence: u64,
    kind: String,
    position: Pos,
    relative_game_tick: i64,
    sub_tick_order: u64,
    scheduler_phase: Option<String>,
    changed: bool,
    before: State,
    after: State,
}
impl TryFrom<Event> for ObservedTransitionEvent {
    type Error = InvalidObservedProperty;
    fn try_from(v: Event) -> Result<Self, Self::Error> {
        Ok(Self {
            sequence: v.sequence,
            kind: v.kind,
            position: v.position,
            relative_game_tick: v.relative_game_tick,
            sub_tick_order: v.sub_tick_order,
            scheduler_phase: v.scheduler_phase,
            changed: v.changed,
            before: v.before.finish(Some(v.position), "before")?,
            after: v.after.finish(Some(v.position), "after")?,
        })
    }
}
