use serde::{Deserialize, Serialize};

use super::{EventId, PhysicsTime};
use crate::{Block, Pos};

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct StateTransition {
    pub time: PhysicsTime,
    pub position: Pos,
    pub before: Block,
    pub after: Block,
    pub cause: EventId,
}
