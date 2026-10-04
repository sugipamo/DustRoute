//! Input staging retains only rejected kinds so the observation owner can
//! diagnose its position. No invalid state is exposed to normalization.
use super::{
    InstrumentedBlockState, InstrumentedStateEvent, NeighborUpdateObservation, PistonStateKind,
    PistonStateObservation,
};
use crate::observed_properties::{InvalidObservedProperty, PropertyInput};
use crate::world::Pos;
use serde::Deserialize;

#[derive(Deserialize)]
pub(super) struct State {
    name: String,
    #[serde(default)]
    properties: PropertyInput,
}
impl State {
    fn finish(
        self,
        position: Option<Pos>,
        slot: &'static str,
    ) -> Result<InstrumentedBlockState, InvalidObservedProperty> {
        Ok(InstrumentedBlockState {
            name: self.name,
            properties: self.properties.finish(position, slot)?,
        })
    }
}
impl TryFrom<State> for InstrumentedBlockState {
    type Error = InvalidObservedProperty;
    fn try_from(v: State) -> Result<Self, Self::Error> {
        v.finish(None, "state")
    }
}

#[derive(Deserialize)]
pub(super) struct Event {
    sequence: u64,
    game_tick: i64,
    sub_tick_order: u64,
    position: Pos,
    source: String,
    before: State,
    after: State,
    changed: bool,
    #[serde(default)]
    ordered_tick_sequence: Option<u64>,
}
impl TryFrom<Event> for InstrumentedStateEvent {
    type Error = InvalidObservedProperty;
    fn try_from(v: Event) -> Result<Self, Self::Error> {
        Ok(Self {
            sequence: v.sequence,
            game_tick: v.game_tick,
            sub_tick_order: v.sub_tick_order,
            position: v.position,
            source: v.source,
            before: v.before.finish(Some(v.position), "before")?,
            after: v.after.finish(Some(v.position), "after")?,
            changed: v.changed,
            ordered_tick_sequence: v.ordered_tick_sequence,
        })
    }
}

#[derive(Deserialize)]
pub(super) struct Piston {
    sequence: u64,
    game_tick: i64,
    position: Pos,
    state_kind: PistonStateKind,
    body: State,
    head: Option<State>,
    moving_block: Option<State>,
    block_entity_present: bool,
    block_entity_extending: Option<bool>,
    #[serde(default)]
    ordered_tick_sequence: Option<u64>,
}
impl TryFrom<Piston> for PistonStateObservation {
    type Error = InvalidObservedProperty;
    fn try_from(v: Piston) -> Result<Self, Self::Error> {
        Ok(Self {
            sequence: v.sequence,
            game_tick: v.game_tick,
            position: v.position,
            state_kind: v.state_kind,
            body: v.body.finish(Some(v.position), "body")?,
            head: v
                .head
                .map(|s| s.finish(Some(v.position), "head"))
                .transpose()?,
            moving_block: v
                .moving_block
                .map(|s| s.finish(Some(v.position), "moving_block"))
                .transpose()?,
            block_entity_present: v.block_entity_present,
            block_entity_extending: v.block_entity_extending,
            ordered_tick_sequence: v.ordered_tick_sequence,
        })
    }
}

#[derive(Deserialize)]
pub(super) struct Neighbor {
    sequence: u64,
    game_tick: i64,
    sub_tick_order: u64,
    position: Pos,
    target: State,
    source_block: String,
    orientation: Option<String>,
    notify: bool,
    #[serde(default)]
    ordered_tick_sequence: Option<u64>,
}
impl TryFrom<Neighbor> for NeighborUpdateObservation {
    type Error = InvalidObservedProperty;
    fn try_from(v: Neighbor) -> Result<Self, Self::Error> {
        Ok(Self {
            sequence: v.sequence,
            game_tick: v.game_tick,
            sub_tick_order: v.sub_tick_order,
            position: v.position,
            target: v.target.finish(Some(v.position), "target")?,
            source_block: v.source_block,
            orientation: v.orientation,
            notify: v.notify,
            ordered_tick_sequence: v.ordered_tick_sequence,
        })
    }
}
