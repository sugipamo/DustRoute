//! Explicit Boolean observations of fixed locations. Predicates declare what
//! to observe; they do not supply physics, replace a type, or certify behavior.
use dustroute_minecraft::time::runtime::{HalfProgress, LocationObservation};
use dustroute_minecraft::{BlockKind, PistonState};
use serde::{Deserialize, Serialize};

#[derive(
    Clone, Debug, Deserialize, Eq, Ord, PartialEq, PartialOrd, Serialize, schemars::JsonSchema,
)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum LocationPredicate {
    /// True for every known non-Air block, including a moving piston carrier.
    Present,
    /// Tests the actual block at the coordinate, not the block it carries.
    BlockKind { block_kind: BlockKind },
    /// The explicitly stored Boolean state of a supported kind. Missing state
    /// is unknown, never silently false. This is not an electrical power query.
    Powered {
        block_kind: BlockKind,
        powered: bool,
    },
    /// Only the actual Piston block has this property. A moving carrier is a
    /// distinct block, even when its payload is a retracted piston body.
    PistonState { state: PistonState },
    /// Runtime progress, not the historical 0/1 block metadata marker.
    MotionProgress { progress: HalfProgress },
}

impl LocationPredicate {
    pub fn validate(&self) -> Result<(), &'static str> {
        if let Self::Powered { block_kind, .. } = self {
            if !matches!(
                block_kind,
                BlockKind::Lever
                    | BlockKind::Repeater
                    | BlockKind::RedstoneTorch
                    | BlockKind::RedstoneLamp
            ) {
                return Err(
                    "powered observation needs a kind with a supported Boolean block state",
                );
            }
        }
        Ok(())
    }

    pub fn evaluate(&self, sample: &LocationObservation) -> Result<bool, String> {
        self.validate().map_err(str::to_owned)?;
        let block = sample.block();
        match *self {
            Self::Present => Ok(block.kind != BlockKind::Air),
            Self::BlockKind { block_kind } => Ok(block.kind == block_kind),
            Self::Powered {
                block_kind,
                powered,
            } => {
                if block.kind != block_kind {
                    return Ok(false);
                }
                block
                    .powered
                    .map(|actual| actual == powered)
                    .ok_or_else(|| "missing powered block state".into())
            }
            Self::PistonState { state } => {
                if block.kind != BlockKind::Piston {
                    return Ok(false);
                }
                block
                    .piston_state
                    .map(|actual| actual == state)
                    .ok_or_else(|| "missing piston block state".into())
            }
            Self::MotionProgress { progress } => {
                if block.kind != BlockKind::MovingPiston {
                    return Ok(false);
                }
                sample
                    .carrier()
                    .map(|c| c.history.progress == progress)
                    .ok_or_else(|| "missing runtime motion history".into())
            }
        }
    }
}

/// A named physical port plus an explicit observation. The source port provides
/// its coordinate; optimization can relocate that port in a new candidate.
#[derive(
    Clone, Debug, Deserialize, Eq, Ord, PartialEq, PartialOrd, Serialize, schemars::JsonSchema,
)]
#[serde(tag = "observation", rename_all = "snake_case", deny_unknown_fields)]
pub enum ObservedPort {
    /// The existing electrical meaning of Wire, BlockPower or DeviceOutput.
    Signal { port: String },
    /// Only a BlockState port. Known false and unknown remain distinct.
    Location {
        port: String,
        predicate: LocationPredicate,
    },
}

impl ObservedPort {
    pub fn port(&self) -> &str {
        match self {
            Self::Signal { port } | Self::Location { port, .. } => port,
        }
    }
}
