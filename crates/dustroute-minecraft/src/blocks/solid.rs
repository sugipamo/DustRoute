use super::{BlockBehaviorProfile, UpdateModel};
use crate::BlockProperties;

pub(super) const PROFILE: BlockBehaviorProfile = BlockBehaviorProfile {
    properties: BlockProperties {
        supports_components: true,
        receives_weak_power: true,
        receives_strong_power: true,
        repeater_reads_block_power: true,
        strong_power_drives_dust: true,
    },
    update_model: UpdateModel::Passive,
    order_sensitive: false,
};
