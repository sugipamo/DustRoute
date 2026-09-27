use super::{TemporalProfile, UpdateModel};

pub(super) const PROFILE: TemporalProfile = TemporalProfile {
    update_model: UpdateModel::ImmediateNeighborChain,
    order_sensitive: true,
};
