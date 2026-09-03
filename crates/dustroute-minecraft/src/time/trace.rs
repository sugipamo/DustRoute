use serde::{Deserialize, Serialize};

use super::{EventId, PhysicsTime};
use crate::{Block, BlockChange, ChangeReason, DeltaCause, Pos};

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct StateTransition {
    pub time: PhysicsTime,
    pub position: Pos,
    pub before: Block,
    pub after: Block,
    pub cause: EventId,
    /// Coordinate-level reason retained for causal explanations.  Older
    /// traces deserialize as `unknown`.
    #[serde(default)]
    pub reason: ChangeReason,
    /// Shape-transition cause, when this state change came from an atomic
    /// geometry delta rather than a legacy signal handler.
    #[serde(default)]
    pub delta_cause: Option<DeltaCause>,
}

impl StateTransition {
    #[must_use]
    pub fn from_change(time: PhysicsTime, cause: EventId, change: &BlockChange) -> Self {
        Self {
            time,
            position: change.position,
            before: change.before.clone(),
            after: change.after.clone(),
            cause,
            reason: change.reason.clone(),
            delta_cause: None,
        }
    }
}
