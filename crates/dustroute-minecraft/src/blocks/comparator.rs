use super::{BlockBehaviorProfile, UpdateModel};
use crate::BlockProperties;

pub(super) const PROFILE: BlockBehaviorProfile = BlockBehaviorProfile {
    properties: BlockProperties::support_only(false),
    update_model: UpdateModel::ScheduledBlockTick,
    order_sensitive: true,
};
