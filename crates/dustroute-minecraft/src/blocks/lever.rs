use super::{TemporalProfile, UpdateModel};

pub(super) const PROFILE: TemporalProfile = TemporalProfile {
    update_model: UpdateModel::UserInteraction,
    order_sensitive: true,
};
