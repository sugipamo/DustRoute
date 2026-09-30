//! A current location sample, separate from source geometry or type evidence.
use super::{CarrierState, RuntimeError, RuntimeTime, RuntimeView};
use crate::{Block, BlockKind, ObservationClassification, Pos};
use serde::Serialize;

/// Observation of one fixed coordinate. Moving payloads are not projected to
/// their eventual state. Construction requires the known runtime boundary;
/// this record cannot be deserialized into validation or execution authority.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct LocationObservation {
    position: Pos,
    time: RuntimeTime,
    block: Block,
    carrier: Option<CarrierState>,
}

impl LocationObservation {
    pub const fn position(&self) -> Pos {
        self.position
    }
    pub const fn time(&self) -> RuntimeTime {
        self.time
    }
    pub const fn block(&self) -> &Block {
        &self.block
    }
    pub const fn carrier(&self) -> Option<CarrierState> {
        self.carrier
    }
}

impl RuntimeView<'_> {
    pub fn observe_location(self, position: Pos) -> Result<LocationObservation, RuntimeError> {
        let block = self.block(position)?;
        if block.observation_classification == ObservationClassification::Coarse {
            return Err(RuntimeError::Invalid("coarse location evidence".into()));
        }
        let carrier = self.carrier(position).copied();
        if block.kind == BlockKind::MovingPiston {
            if !self.carrier_staged(position)
                && (carrier.is_none() || block.piston_entity.is_none())
            {
                return Err(RuntimeError::CarrierConflict(position));
            }
        } else if carrier.is_some() || block.piston_entity.is_some() {
            return Err(RuntimeError::CarrierConflict(position));
        } else if block.requires_live_observation()
            && !crate::physical::environment::is_water(&block)
        {
            return Err(RuntimeError::Invalid(
                "location needs unsupported live evidence".into(),
            ));
        }
        Ok(LocationObservation {
            position,
            time: self.time(),
            block,
            carrier,
        })
    }
}
