//! Versioned byte ordering for one adapter's complete root-boundary state.
//! Normalization and admissible boundaries are owned by the caller. The named
//! record documents the compared fields; its typed tuple retains every compared field.
use super::super::Delivery;
use super::State;
use crate::time::runtime::RuntimeError;
use crate::time::runtime::history::RecentHistory;
use crate::time::runtime::{CarrierState, RuntimeLimits, RuntimeTime};
use crate::{Block, Pos, Region};
use serde::ser::SerializeTuple;
use serde::{Serialize, Serializer};
use std::collections::VecDeque;

pub(super) struct RootRecord<'a, P> {
    schema: &'static str,
    profile: &'static str,
    adapter: &'static str,
    region: Region,
    time: RuntimeTime,
    blocks: Vec<(&'a Pos, &'a Block)>,
    pending: Vec<(&'a RuntimeTime, &'a VecDeque<Delivery<P>>)>,
    carriers: Vec<(&'a Pos, &'a CarrierState)>,
    outputs: Vec<(&'a Pos, &'a u8)>,
    histories: Vec<(&'a (String, Pos), &'a RecentHistory)>,
    staged: Vec<(&'a Pos, &'a Block)>,
    next_id: u64,
    next_carrier: u64,
    limits: RuntimeLimits,
}
impl<'a, P: Serialize> RootRecord<'a, P> {
    pub(super) fn from_state(state: &'a State<P>, schema: &'static str) -> Self {
        Self {
            schema,
            profile: state.profile,
            adapter: state.adapter,
            region: state.region,
            time: state.time,
            blocks: state.world.iter().collect(),
            pending: state.pending.iter().collect(),
            carriers: state.carriers.iter().collect(),
            outputs: state.outputs.iter().collect(),
            histories: state.histories.iter().collect(),
            staged: state.staged_carriers.iter().collect(),
            next_id: state.next_id,
            next_carrier: state.next_carrier,
            limits: state.limits,
        }
    }
    pub(super) fn encode(&self) -> Result<Vec<u8>, RuntimeError> {
        dustroute_codec::canonical::encode(self).map_err(|e| RuntimeError::Invalid(e.to_string()))
    }
}
impl<P: Serialize> Serialize for RootRecord<'_, P> {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        let mut record = serializer.serialize_tuple(14)?;
        record.serialize_element(self.schema)?;
        record.serialize_element(self.profile)?;
        record.serialize_element(self.adapter)?;
        record.serialize_element(&self.region)?;
        record.serialize_element(&self.time)?;
        record.serialize_element(&self.blocks)?;
        record.serialize_element(&self.pending)?;
        record.serialize_element(&self.carriers)?;
        record.serialize_element(&self.outputs)?;
        record.serialize_element(&self.histories)?;
        record.serialize_element(&self.staged)?;
        record.serialize_element(&self.next_id)?;
        record.serialize_element(&self.next_carrier)?;
        record.serialize_element(&self.limits)?;
        record.end()
    }
}
