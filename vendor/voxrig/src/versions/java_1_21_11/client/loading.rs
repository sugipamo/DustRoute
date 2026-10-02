//! Native player-loading notification, separate from position readiness.
use super::*;
use serde::Serialize;

/// One PLAYER_LOADED attempt, retained before sending. Dispatch is not an ack.
#[derive(Clone, Debug, Serialize)]
pub struct LoadingAttempt {
    /// Receive ordinal starting this login/respawn/configuration generation.
    pub generation: u64,
    /// Receive boundary whose position and terrain allowed the notification.
    pub after_sequence: u64,
    /// Complete frame dispatched through the shared guarded sender.
    pub dispatched: bool,
}
/// Received loading progress and notification history for this connection.
#[derive(Clone, Debug, Default, Serialize)]
pub struct InteractionLoading {
    /// Last login/respawn/configuration receive boundary, not a server tick.
    pub generation: u64,
    /// Native INITIAL_CHUNKS_COMING receipt for this generation.
    pub initial_chunks_sequence: Option<u64>,
    /// Current notification attempt. Missing means required baselines are pending.
    pub attempt: Option<LoadingAttempt>,
    /// Last previous-generation attempt, retained when the context resets.
    pub previous_attempt: Option<LoadingAttempt>,
}
impl InteractionLoading {
    pub(super) fn reset(&mut self, sequence: u64) {
        if let Some(previous) = self.attempt.take() {
            self.previous_attempt = Some(previous);
        }
        self.generation = sequence;
        self.initial_chunks_sequence = None;
    }
    /// True means a complete notification frame was sent for this generation.
    /// It does not certify acceptance of any subsequent world operation.
    pub fn notification_dispatched(&self) -> bool {
        self.attempt
            .as_ref()
            .is_some_and(|a| a.generation == self.generation && a.dispatched)
    }
    #[cfg(test)]
    pub(super) fn completed_fixture() -> Self {
        Self {
            generation: 1,
            initial_chunks_sequence: Some(2),
            attempt: Some(LoadingAttempt {
                generation: 1,
                after_sequence: 3,
                dispatched: true,
            }),
            previous_attempt: None,
        }
    }
}
impl Session {
    // Called after ordinary automatic responses. The loading gate blocks user
    // operations until this send completes, including teleport confirmation first.
    pub(super) async fn complete_loading(&self, state: &mut State) -> Result<()> {
        if state.phase != Phase::Play
            || !state.ready
            || state.loading.initial_chunks_sequence.is_none()
            || state.loading.attempt.is_some()
        {
            return Ok(());
        }
        let Some(position) = state.position else {
            return Ok(());
        };
        // Headless replacement for native local rendering readiness: the entire
        // own chunk must actually be received. No timeout or inferred air bypass.
        let cell = position.map(|v| v.floor() as i32);
        if state.world.block(cell).is_none() {
            return Ok(());
        }
        state.loading.attempt = Some(LoadingAttempt {
            generation: state.loading.generation,
            after_sequence: state.sequence,
            dispatched: false,
        });
        self.send(ids::play_serverbound::PLAYER_LOADED, &[]).await?;
        state
            .loading
            .attempt
            .as_mut()
            .expect("attempt stored before I/O")
            .dispatched = true;
        Ok(())
    }
}

#[cfg(test)]
mod tests;
