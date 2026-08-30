use super::{BlockBehaviorProfile, UpdateModel};
use crate::BlockProperties;

pub(super) const PROFILE: BlockBehaviorProfile = BlockBehaviorProfile {
    properties: BlockProperties::support_only(true),
    update_model: UpdateModel::BlockEvent,
    order_sensitive: true,
};
