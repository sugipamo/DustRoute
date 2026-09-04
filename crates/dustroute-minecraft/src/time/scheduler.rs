//! Versioned ordering policy for the Minecraft event queue.
//!
//! A scheduler profile answers only *when* an already-described event is
//! eligible to run.  Block-specific activation, movement, and pulse
//! durations belong to the block behavior model instead.  Keeping these
//! responsibilities separate lets a measured Minecraft profile replace the
//! ordering policy without silently changing piston or repeater physics.

use std::fmt::{Display, Formatter};

use serde::{Deserialize, Serialize};

use super::PhysicsEventPhase;

/// Identity and provenance of an event-ordering policy.
#[derive(Clone, Copy, Debug, Default, Deserialize, Eq, Hash, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum SchedulerProfileId {
    /// DustRoute's deterministic ordering model.  It is useful for
    /// reproducible simulation, but is not a claim of complete vanilla order.
    #[default]
    DustRouteDeterministicV1,
    /// A version-labelled model for Java 1.21.11.  Until all phases are
    /// backed by live evidence this remains explicitly modelled.
    MinecraftJava1_21_11Modelled,
}

/// Strength of the evidence behind a scheduler profile or its phase order.
#[derive(Clone, Copy, Debug, Default, Deserialize, Eq, Hash, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum SchedulerEvidence {
    /// Reproducible DustRoute policy or a reasoned approximation.
    #[default]
    Modelled,
    /// Directly measured from a versioned server fixture.
    Observed,
    /// The source does not expose enough information to identify the order.
    Unknown,
}

impl SchedulerEvidence {
    #[must_use]
    pub const fn is_known(self) -> bool {
        !matches!(self, Self::Unknown)
    }
}

/// Ordering of events that share one phase.  The initial contract deliberately
/// exposes only the behavior we can guarantee: insertion order is stable.
#[derive(Clone, Copy, Debug, Default, Deserialize, Eq, Hash, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum SamePhaseOrder {
    #[default]
    Insertion,
}

/// Policy for a child event whose requested delay is zero.
#[derive(Clone, Copy, Debug, Default, Deserialize, Eq, Hash, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ZeroDelayPolicy {
    /// Keep the child in the parent's game tick.  Phase and insertion order
    /// still determine its position, so zero is not collapsed into a tick.
    #[default]
    SameGameTickNextOrder,
    /// Move a zero-delay child to the next game tick.  This is available for
    /// profiles that have evidence for such a scheduler boundary.
    NextGameTick,
}

/// Event-ordering policy used by [`super::PhysicsEngine`].
///
/// `SchedulerProfile` intentionally contains no block delay constants.  A
/// piston movement duration or repeater delay is a property of the block
/// model, while this type defines phase order, same-phase order, and how a
/// zero-delay edge is placed on the clock.
#[derive(Clone, Copy, Debug, Deserialize, Eq, Hash, PartialEq, Serialize)]
pub struct SchedulerProfile {
    pub id: SchedulerProfileId,
    /// Lower array index runs first.  Every phase must occur exactly once.
    pub phase_order: [PhysicsEventPhase; 6],
    pub same_phase_order: SamePhaseOrder,
    pub zero_delay: ZeroDelayPolicy,
    pub evidence: SchedulerEvidence,
}

impl Default for SchedulerProfile {
    fn default() -> Self {
        Self::dustroute_deterministic()
    }
}

impl SchedulerProfile {
    /// The reproducible profile used by existing callers.  Its order matches
    /// the historical `PhysicsEventPhase` declaration and therefore preserves
    /// queue behavior while making the policy explicit.
    #[must_use]
    pub const fn dustroute_deterministic() -> Self {
        Self {
            id: SchedulerProfileId::DustRouteDeterministicV1,
            phase_order: [
                PhysicsEventPhase::External,
                PhysicsEventPhase::NeighborUpdate,
                PhysicsEventPhase::ScheduledTick,
                PhysicsEventPhase::BlockEvent,
                PhysicsEventPhase::BlockEntity,
                PhysicsEventPhase::Observation,
            ],
            same_phase_order: SamePhaseOrder::Insertion,
            zero_delay: ZeroDelayPolicy::SameGameTickNextOrder,
            evidence: SchedulerEvidence::Modelled,
        }
    }

    /// A version-labelled Java profile placeholder.  It intentionally uses
    /// the deterministic order and remains `Modelled` until live 1.21.11
    /// fixtures establish stronger phase evidence.
    #[must_use]
    pub const fn minecraft_java_1_21_11_modelled() -> Self {
        Self {
            id: SchedulerProfileId::MinecraftJava1_21_11Modelled,
            ..Self::dustroute_deterministic()
        }
    }

    /// Returns the scheduler rank of a phase.  Invalid custom profiles return
    /// `usize::MAX`; callers should run [`Self::validate`] before execution.
    #[must_use]
    pub fn phase_rank(&self, phase: PhysicsEventPhase) -> usize {
        self.phase_order
            .iter()
            .position(|candidate| *candidate == phase)
            .unwrap_or(usize::MAX)
    }

    /// Computes the game tick at which a child event becomes eligible.
    #[must_use]
    pub const fn game_tick_after_delay(self, parent_game_tick: u64, delay_ticks: u64) -> u64 {
        match self.zero_delay {
            ZeroDelayPolicy::SameGameTickNextOrder => parent_game_tick.saturating_add(delay_ticks),
            ZeroDelayPolicy::NextGameTick => {
                parent_game_tick.saturating_add(if delay_ticks == 0 { 1 } else { delay_ticks })
            }
        }
    }

    /// Validates that the profile defines a total order over all phases.
    pub fn validate(self) -> Result<(), SchedulerProfileError> {
        for (index, phase) in self.phase_order.iter().copied().enumerate() {
            if self.phase_order[..index].contains(&phase) {
                return Err(SchedulerProfileError::DuplicatePhase { phase });
            }
        }
        for phase in [
            PhysicsEventPhase::External,
            PhysicsEventPhase::NeighborUpdate,
            PhysicsEventPhase::ScheduledTick,
            PhysicsEventPhase::BlockEvent,
            PhysicsEventPhase::BlockEntity,
            PhysicsEventPhase::Observation,
        ] {
            if !self.phase_order.contains(&phase) {
                return Err(SchedulerProfileError::MissingPhase { phase });
            }
        }
        Ok(())
    }
}

/// Invalid custom phase order supplied to a scheduler profile.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum SchedulerProfileError {
    DuplicatePhase { phase: PhysicsEventPhase },
    MissingPhase { phase: PhysicsEventPhase },
}

impl Display for SchedulerProfileError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::DuplicatePhase { phase } => {
                write!(formatter, "scheduler profile repeats phase {phase:?}")
            }
            Self::MissingPhase { phase } => {
                write!(formatter, "scheduler profile omits phase {phase:?}")
            }
        }
    }
}

impl std::error::Error for SchedulerProfileError {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_profile_matches_legacy_phase_order() {
        let profile = SchedulerProfile::default();
        assert_eq!(profile.phase_rank(PhysicsEventPhase::External), 0);
        assert_eq!(profile.phase_rank(PhysicsEventPhase::Observation), 5);
        assert_eq!(profile.same_phase_order, SamePhaseOrder::Insertion);
        assert!(profile.validate().is_ok());
    }

    #[test]
    fn zero_delay_policy_is_explicit_and_zero_capable() {
        let same_tick = SchedulerProfile::default();
        assert_eq!(same_tick.game_tick_after_delay(7, 0), 7);
        assert_eq!(same_tick.game_tick_after_delay(7, 2), 9);

        let next_tick = SchedulerProfile {
            zero_delay: ZeroDelayPolicy::NextGameTick,
            ..same_tick
        };
        assert_eq!(next_tick.game_tick_after_delay(7, 0), 8);
    }

    #[test]
    fn invalid_phase_order_is_rejected() {
        let invalid = SchedulerProfile {
            phase_order: [
                PhysicsEventPhase::External,
                PhysicsEventPhase::External,
                PhysicsEventPhase::ScheduledTick,
                PhysicsEventPhase::BlockEvent,
                PhysicsEventPhase::BlockEntity,
                PhysicsEventPhase::Observation,
            ],
            ..SchedulerProfile::default()
        };
        assert!(matches!(
            invalid.validate(),
            Err(SchedulerProfileError::DuplicatePhase {
                phase: PhysicsEventPhase::External
            })
        ));
    }
}
