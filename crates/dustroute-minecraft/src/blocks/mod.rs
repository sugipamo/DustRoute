//! Per-block behavior metadata.
//!
//! These profiles are the stable dispatch surface for the future event-driven
//! simulator. They classify behavior; they do not yet claim to reproduce the
//! complete vanilla update engine.

use crate::{BlockKind, BlockProperties};

mod air;
mod comparator;
mod lever;
mod piston;
mod redstone_block;
mod redstone_torch;
mod redstone_wire;
mod repeater;
mod solid;
mod transparent;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum UpdateModel {
    Passive,
    ImmediateNeighborChain,
    ScheduledBlockTick,
    UserInteraction,
    BlockEvent,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct BlockBehaviorProfile {
    pub properties: BlockProperties,
    pub update_model: UpdateModel,
    /// Whether behavior can depend on ordering within or between game ticks.
    pub order_sensitive: bool,
}

#[must_use]
pub const fn behavior_profile(kind: BlockKind) -> BlockBehaviorProfile {
    match kind {
        BlockKind::Air => air::PROFILE,
        BlockKind::Solid => solid::PROFILE,
        BlockKind::Transparent => transparent::PROFILE,
        BlockKind::RedstoneWire => redstone_wire::PROFILE,
        BlockKind::RedstoneTorch => redstone_torch::PROFILE,
        BlockKind::Repeater => repeater::PROFILE,
        BlockKind::Comparator => comparator::PROFILE,
        BlockKind::Lever => lever::PROFILE,
        BlockKind::RedstoneBlock => redstone_block::PROFILE,
        BlockKind::Piston => piston::PROFILE,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn temporal_components_are_explicitly_classified() {
        assert_eq!(
            behavior_profile(BlockKind::Repeater).update_model,
            UpdateModel::ScheduledBlockTick
        );
        assert_eq!(
            behavior_profile(BlockKind::Piston).update_model,
            UpdateModel::BlockEvent
        );
        assert!(behavior_profile(BlockKind::RedstoneWire).order_sensitive);
    }
}
