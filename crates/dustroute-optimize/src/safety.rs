use dustroute_physical::{PhysicalPatch, Pos, TemporalAssessment, TemporalRequirement, World};
use dustroute_translate::physical::PlacementCircuit;

use crate::{
    BehavioralEquivalence, BehavioralVerificationConfig, OptimizationRealizationError,
    OptimizationVerification, RealizedOptimization, optimization_patch,
    verify_realized_optimization,
};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct TemporalCapabilities {
    pub maximum_verified: TemporalRequirement,
}

impl TemporalCapabilities {
    /// Current safe capability: steady-state circuits and circuits whose final
    /// result only requires ordered immediate updates. Scheduled ticks and
    /// block events remain preview-only.
    #[must_use]
    pub const fn current() -> Self {
        Self {
            maximum_verified: TemporalRequirement::OrderedUpdates,
        }
    }

    #[must_use]
    pub const fn supports(self, requirement: TemporalRequirement) -> bool {
        !matches!(requirement, TemporalRequirement::Unsupported)
            && requirement as u8 <= self.maximum_verified as u8
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum OptimizationSafetyReason {
    LogicalTopologyChanged,
    BehavioralMismatch,
    BehavioralVerificationUnavailable(String),
    InvalidPhysicalSupport,
    IncompleteObservation,
    InsufficientBehavioralEvidence,
    UnsupportedObservedPhysics {
        blocks: Vec<Pos>,
    },
    TemporalModelUnavailable {
        required: TemporalRequirement,
        available: TemporalRequirement,
    },
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum OptimizationSafety {
    Verified {
        temporal: TemporalAssessment,
    },
    PreviewOnly {
        temporal: TemporalAssessment,
        reasons: Vec<OptimizationSafetyReason>,
    },
    Rejected {
        temporal: TemporalAssessment,
        reasons: Vec<OptimizationSafetyReason>,
    },
}

impl OptimizationSafety {
    #[must_use]
    pub const fn permits_automatic_apply(&self) -> bool {
        matches!(self, Self::Verified { .. })
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct GuardedOptimizationCandidate {
    patch: PhysicalPatch,
    verification: OptimizationVerification,
    safety: OptimizationSafety,
}

impl GuardedOptimizationCandidate {
    #[must_use]
    pub const fn preview_patch(&self) -> &PhysicalPatch {
        &self.patch
    }

    #[must_use]
    pub fn automatic_patch(&self) -> Option<&PhysicalPatch> {
        self.safety.permits_automatic_apply().then_some(&self.patch)
    }

    #[must_use]
    pub const fn verification(&self) -> &OptimizationVerification {
        &self.verification
    }

    #[must_use]
    pub const fn safety(&self) -> &OptimizationSafety {
        &self.safety
    }
}

pub fn prepare_guarded_optimization(
    observed: &World,
    original: &PlacementCircuit,
    realized: &RealizedOptimization,
    verification_config: BehavioralVerificationConfig,
    capabilities: TemporalCapabilities,
) -> Result<GuardedOptimizationCandidate, OptimizationRealizationError> {
    let original_world = original.build_world()?;
    let verification =
        verify_realized_optimization(&original_world, original, realized, verification_config);
    let safety = assess_optimization_safety(&verification, capabilities);
    let patch = optimization_patch(observed, original, realized)?;
    Ok(GuardedOptimizationCandidate {
        patch,
        verification,
        safety,
    })
}

#[must_use]
pub fn assess_optimization_safety(
    verification: &OptimizationVerification,
    capabilities: TemporalCapabilities,
) -> OptimizationSafety {
    let temporal = verification.optimized_analysis.scene.temporal_assessment();
    let mut rejected = Vec::new();
    if !verification.topology_preserved {
        rejected.push(OptimizationSafetyReason::LogicalTopologyChanged);
    }
    if matches!(verification.behavior, BehavioralEquivalence::Mismatch(_)) {
        rejected.push(OptimizationSafetyReason::BehavioralMismatch);
    }
    if matches!(
        verification.behavior,
        BehavioralEquivalence::Verified(ref comparison)
            if !comparison.comparable
                || comparison.expected_inputs == 0
                || comparison.expected_outputs == 0
                || comparison.actual_inputs == 0
                || comparison.actual_outputs == 0
    ) {
        rejected.push(OptimizationSafetyReason::InsufficientBehavioralEvidence);
    }
    let unsupported_blocks = verification
        .original_analysis
        .unsupported
        .keys()
        .chain(verification.optimized_analysis.unsupported.keys())
        .copied()
        .collect::<std::collections::BTreeSet<_>>();
    if !unsupported_blocks.is_empty() {
        rejected.push(OptimizationSafetyReason::UnsupportedObservedPhysics {
            blocks: unsupported_blocks.into_iter().collect(),
        });
    }
    if !verification
        .optimized_analysis
        .diagnostics
        .invalid_supports
        .is_empty()
    {
        rejected.push(OptimizationSafetyReason::InvalidPhysicalSupport);
    }
    if temporal.requirement == TemporalRequirement::Unsupported {
        rejected.push(OptimizationSafetyReason::IncompleteObservation);
    }
    if !rejected.is_empty() {
        return OptimizationSafety::Rejected {
            temporal,
            reasons: rejected,
        };
    }

    let mut preview = Vec::new();
    if let BehavioralEquivalence::Unavailable(reason) = &verification.behavior {
        preview.push(OptimizationSafetyReason::BehavioralVerificationUnavailable(
            reason.clone(),
        ));
    }
    if !capabilities.supports(temporal.requirement) {
        preview.push(OptimizationSafetyReason::TemporalModelUnavailable {
            required: temporal.requirement,
            available: capabilities.maximum_verified,
        });
    }
    if preview.is_empty() {
        OptimizationSafety::Verified { temporal }
    } else {
        OptimizationSafety::PreviewOnly {
            temporal,
            reasons: preview,
        }
    }
}

#[cfg(test)]
mod tests {
    use dustroute_physical::{Block, BlockKind, FrontierReason, ObservationFrontier, Pos, World};
    use dustroute_translate::{RegionBounds, TruthTableComparison, analyze_world_region};

    use super::*;

    fn verification(kind: BlockKind) -> OptimizationVerification {
        let mut world = World::new();
        world.set(Pos::new(0, 0, 0), Block::new(BlockKind::Solid));
        world.place(kind, Pos::new(0, 1, 0));
        let analysis = analyze_world_region(
            &world,
            RegionBounds::new(Pos::new(-2, -1, -2), Pos::new(2, 3, 2)),
        );
        OptimizationVerification {
            topology_preserved: true,
            original_analysis: analysis.clone(),
            optimized_analysis: analysis,
            behavior: BehavioralEquivalence::Verified(TruthTableComparison {
                comparable: true,
                expected_inputs: 1,
                actual_inputs: 1,
                expected_outputs: 1,
                actual_outputs: 1,
                differing_rows: 0,
                differing_bits: 0,
                terminal_count_delta: 0,
                fitness_penalty: 0,
            }),
        }
    }

    #[test]
    fn ordered_circuit_can_still_be_automatically_applied() {
        let verification = verification(BlockKind::RedstoneWire);
        let safety = assess_optimization_safety(&verification, TemporalCapabilities::current());
        assert!(safety.permits_automatic_apply());
    }

    #[test]
    fn scheduled_and_block_event_circuits_are_preview_only() {
        let repeater = assess_optimization_safety(
            &verification(BlockKind::Repeater),
            TemporalCapabilities::current(),
        );
        assert!(matches!(repeater, OptimizationSafety::PreviewOnly { .. }));
        assert!(!repeater.permits_automatic_apply());

        let piston = assess_optimization_safety(
            &verification(BlockKind::Piston),
            TemporalCapabilities::current(),
        );
        assert!(matches!(piston, OptimizationSafety::Rejected { .. }));
        assert!(!piston.permits_automatic_apply());
    }

    #[test]
    fn mismatch_and_incomplete_observation_are_rejected() {
        let mut mismatch = verification(BlockKind::RedstoneWire);
        mismatch.behavior = BehavioralEquivalence::Mismatch(TruthTableComparison {
            comparable: true,
            expected_inputs: 1,
            actual_inputs: 1,
            expected_outputs: 1,
            actual_outputs: 1,
            differing_rows: 1,
            differing_bits: 1,
            terminal_count_delta: 0,
            fitness_penalty: 1,
        });
        assert!(matches!(
            assess_optimization_safety(&mismatch, TemporalCapabilities::current()),
            OptimizationSafety::Rejected { .. }
        ));

        let mut incomplete = verification(BlockKind::RedstoneWire);
        incomplete
            .optimized_analysis
            .scene
            .observation
            .frontier
            .push(ObservationFrontier {
                position: Pos::new(0, 1, 0),
                direction: dustroute_physical::Facing::East,
                reason: FrontierReason::ScanLimitReached,
            });
        assert!(matches!(
            assess_optimization_safety(&incomplete, TemporalCapabilities::current()),
            OptimizationSafety::Rejected { .. }
        ));
    }

    #[test]
    fn preview_only_candidate_hides_the_automatic_patch() {
        let verification = verification(BlockKind::Repeater);
        let temporal = verification.optimized_analysis.scene.temporal_assessment();
        let candidate = GuardedOptimizationCandidate {
            patch: PhysicalPatch {
                reason: dustroute_physical::PhysicalPatchReason::OptimizePlacement,
                affected_fragments: Vec::new(),
                confidence_percent: 100,
                explanation: "test".into(),
                changes: Vec::new(),
            },
            verification,
            safety: OptimizationSafety::PreviewOnly {
                temporal,
                reasons: vec![OptimizationSafetyReason::TemporalModelUnavailable {
                    required: TemporalRequirement::ScheduledTicks,
                    available: TemporalRequirement::OrderedUpdates,
                }],
            },
        };
        assert!(candidate.automatic_patch().is_none());
        assert_eq!(
            candidate.preview_patch().reason,
            dustroute_physical::PhysicalPatchReason::OptimizePlacement
        );
    }
}
