use super::{BlockBehaviorProfile, UpdateModel};
use crate::BlockProperties;

/// An observer detects a state transition at its front and emits a strong
/// pulse from its back. It is a full block and therefore does not require a
/// support block of its own.
pub(super) const PROFILE: BlockBehaviorProfile = BlockBehaviorProfile {
    properties: BlockProperties::support_only(true),
    update_model: UpdateModel::ScheduledBlockTick,
    order_sensitive: true,
};
