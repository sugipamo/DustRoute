//! Runtime motion facts, separate from historical block metadata and law data.
use serde::{Deserialize, Serialize};

use crate::Pos;

/// Exact half-step progress values reached by the audited block-entity law.
/// No floating point interpolation or collision/entity behavior is implied.
#[derive(
    Clone, Copy, Debug, Deserialize, Eq, Ord, PartialEq, PartialOrd, Serialize, schemars::JsonSchema,
)]
pub enum HalfProgress {
    Zero,
    Half,
    Full,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
pub struct MotionHistory {
    pub progress: HalfProgress,
    pub last_progress: HalfProgress,
    pub saved_world_time: u64,
}

impl MotionHistory {
    /// Source constructor state, not recovered from an observed 0/1 marker.
    pub const fn fresh() -> Self {
        Self {
            progress: HalfProgress::Zero,
            last_progress: HalfProgress::Zero,
            saved_world_time: 0,
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd, Serialize)]
pub struct CarrierId(pub(crate) u64);

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
pub struct CarrierState {
    pub id: CarrierId,
    pub history: MotionHistory,
}

/// Effects are staged with the world delta. Tokens prevent an old pending tick
/// from updating a different carrier installed later at the same location.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum CarrierEffect {
    /// A moving block has been written, but its block entity is not yet
    /// registered. The captured entity belongs to the pending continuation,
    /// not to block queries during the intervening synchronous callbacks.
    Stage {
        position: Pos,
        block: Box<crate::Block>,
    },
    Install {
        position: Pos,
        history: MotionHistory,
    },
    Update {
        position: Pos,
        expected: CarrierState,
        history: MotionHistory,
    },
    Retire {
        position: Pos,
        expected: CarrierState,
    },
    Replace {
        position: Pos,
        expected: CarrierState,
        history: MotionHistory,
    },
}

impl CarrierEffect {
    pub(super) const fn position(&self) -> Pos {
        match self {
            Self::Stage { position, .. }
            | Self::Install { position, .. }
            | Self::Update { position, .. }
            | Self::Retire { position, .. }
            | Self::Replace { position, .. } => *position,
        }
    }
}
