//! Explicitly unstable, low-level operations.
//!
//! These APIs expose protocol-adjacent behavior whose contracts may change
//! between `0.x` releases. Normal controllers should prefer the higher-level
//! methods directly on [`Bot`].

use crate::versions::java_1_16_1::{BlockFace, BlockPos, Bot, DiggingStatus, Result, VehiclePose};

/// A borrowed view of a [`Bot`] exposing low-level operations.
#[derive(Clone, Copy)]
pub struct UnstableBot<'a> {
    pub(crate) bot: &'a Bot,
}

impl UnstableBot<'_> {
    /// Directly changes the locally proposed position and sends it.
    ///
    /// This bypasses the normal control-state physics and can be rejected by
    /// the server. Prefer [`Bot::set_control`](crate::versions::java_1_16_1::Bot::set_control).
    pub async fn move_relative(&self, forward: f64, strafe: f64) -> Result<()> {
        self.bot.move_relative_unchecked(forward, strafe).await
    }

    /// Sends a raw player-digging action without waiting for acknowledgement.
    ///
    /// Prefer [`Bot::dig_block`](crate::versions::java_1_16_1::Bot::dig_block) when performing a
    /// complete block-breaking operation.
    pub async fn send_digging(
        &self,
        status: DiggingStatus,
        position: BlockPos,
        face: BlockFace,
    ) -> Result<()> {
        self.bot.send_digging_packet(status, position, face).await
    }

    /// Dispatches an explicit client-authoritative vehicle pose.
    /// Callers are responsible for vehicle physics and server correction handling.
    pub async fn send_vehicle_pose(&self, pose: VehiclePose) -> Result<()> {
        self.bot.send_vehicle_pose_unchecked(pose).await
    }
}
