use super::{TemporalProfile, UpdateModel};

pub(super) const PROFILE: TemporalProfile = TemporalProfile {
    update_model: UpdateModel::Passive,
    order_sensitive: false,
};
