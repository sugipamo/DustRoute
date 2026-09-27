use super::{TemporalProfile, UpdateModel};

/// An observer detects a state transition at its front and emits a strong
/// pulse from its back. It is a full block and therefore does not require a
/// support block of its own.
pub(super) const PROFILE: TemporalProfile = TemporalProfile {
    update_model: UpdateModel::ScheduledBlockTick,
    order_sensitive: true,
};
