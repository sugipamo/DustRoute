//! Read-only watches. Fresh position is an observation, not construction authority.
use super::super::operations::Operations;
use super::*;
use crate::{Error, ErrorKind, Result};

/// One live observer/world/entity instance; cannot be restored from JSON.
#[derive(Clone, Debug, Serialize)]
pub struct PlayerMotionWatch {
    observer_connection_id: u64,
    generation: u64,
    entity_id: i32,
    uuid: [u8; 16],
    spawn_receive_sequence: u64,
    after_receive_sequence: u64,
}
/// Freshness result; none of these states certifies stopped motion or grants mutation.
#[derive(Clone, Debug, Serialize)]
#[serde(tag = "status", rename_all = "snake_case")]
pub enum PlayerMotionStatus {
    /// No position-containing packet has arrived after registration.
    Pending,
    /// A later packet supplied position for the same observed player instance.
    PositionUpdated {
        /// Position, quantization bounds and independent receipt ordinals.
        player: ObservedPlayer,
    },
    /// Observer world or target lifetime changed.
    RequiresInspection {
        /// Why this watch cannot establish a same-instance observation.
        reason: String,
    },
}
impl Operations {
    /// Register an exact spawned player UUID on this observer. Read-only; not a
    /// server-time fence. A different spawn or world cannot satisfy the watch.
    pub async fn watch_player_motion(&self, uuid: [u8; 16]) -> Result<PlayerMotionWatch> {
        let state = self.bot.session.state.lock().await;
        self.ready(&state)?;
        let player = state
            .players
            .observations()
            .into_iter()
            .find(|p| p.uuid == uuid)
            .ok_or_else(|| {
                Error::new(
                    ErrorKind::State,
                    anyhow::anyhow!("exact player UUID is not observed"),
                )
            })?;
        Ok(PlayerMotionWatch {
            observer_connection_id: self.bot.session.id,
            generation: state.loading.generation,
            entity_id: player.entity_id,
            uuid,
            spawn_receive_sequence: player.motion.spawn_receive_sequence,
            after_receive_sequence: state.sequence,
        })
    }
    /// Observe without sending. Rotation, pose, attributes, velocity alone and
    /// cached matching coordinates cannot establish fresh position. No prediction
    /// or received zero velocity is promoted to an arrival/settle acknowledgement.
    pub async fn observe_player_motion(
        &self,
        watch: &PlayerMotionWatch,
    ) -> Result<PlayerMotionStatus> {
        let state = self.bot.session.state.lock().await;
        self.ready(&state)?;
        if watch.observer_connection_id != self.bot.session.id {
            return Err(Error::new(
                ErrorKind::State,
                anyhow::anyhow!("motion watch belongs to another observer connection"),
            ));
        }
        Ok(evaluate(&state, watch))
    }
}
pub(in super::super) fn evaluate(
    state: &super::super::State,
    watch: &PlayerMotionWatch,
) -> PlayerMotionStatus {
    if state.loading.generation != watch.generation {
        return PlayerMotionStatus::RequiresInspection {
            reason: "observer world generation changed".into(),
        };
    }
    let Some(player) = state.players.observations().into_iter().find(|p| {
        p.uuid == watch.uuid
            && p.entity_id == watch.entity_id
            && p.motion.spawn_receive_sequence == watch.spawn_receive_sequence
    }) else {
        return PlayerMotionStatus::RequiresInspection {
            reason: "target player instance disappeared or changed".into(),
        };
    };
    if player.motion.position_receive_sequence <= watch.after_receive_sequence {
        return PlayerMotionStatus::Pending;
    }
    PlayerMotionStatus::PositionUpdated { player }
}
#[cfg(test)]
mod tests {
    use super::super::tests::{UUID, spawn, tracker};
    use super::*;
    #[test]
    fn watch_needs_new_position_and_exact_world_spawn_not_new_viewpoint() {
        let mut state = super::super::super::State {
            players: tracker(),
            sequence: 2,
            ..Default::default()
        };
        let watch = PlayerMotionWatch {
            observer_connection_id: 1,
            generation: 0,
            entity_id: 42,
            uuid: UUID,
            spawn_receive_sequence: 2,
            after_receive_sequence: 2,
        };
        assert!(matches!(
            evaluate(&state, &watch),
            PlayerMotionStatus::Pending
        ));
        state
            .players
            .receive(ids::play_clientbound::ENTITY_HEAD_ROTATION, &[42, 0], 3)
            .unwrap();
        assert!(matches!(
            evaluate(&state, &watch),
            PlayerMotionStatus::Pending
        ));
        state
            .players
            .receive(
                ids::play_clientbound::REL_ENTITY_MOVE,
                &[42, 0, 1, 0, 0, 0, 0, 1],
                4,
            )
            .unwrap();
        assert!(matches!(
            evaluate(&state, &watch),
            PlayerMotionStatus::PositionUpdated { .. }
        ));
        state
            .players
            .receive(ids::play_clientbound::SPAWN_ENTITY, &spawn(), 5)
            .unwrap();
        assert!(matches!(
            evaluate(&state, &watch),
            PlayerMotionStatus::RequiresInspection { .. }
        ));
        state.loading.reset(6);
        assert!(matches!(
            evaluate(&state, &watch),
            PlayerMotionStatus::RequiresInspection { .. }
        ));
    }
}
