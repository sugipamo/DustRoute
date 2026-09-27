use super::{TemporalProfile, UpdateModel};

pub(super) const PROFILE: TemporalProfile = TemporalProfile {
    update_model: UpdateModel::ScheduledBlockTick,
    order_sensitive: true,
};
