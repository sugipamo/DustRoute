//! Built-in placement metadata; it grants neither execution nor observation authority.
use crate::operations::mutation::{Success, UnrecordedFailure};
use dustroute_app::{PlacementAssembly, PlacementPlan};
use dustroute_library::assembly::Assembly;
use dustroute_library::blueprint::{AssemblyRevisionId, BlueprintRevisionId};
use dustroute_optimize::{BehavioralEquivalence, OptimizationSafety};
use dustroute_physical::{Pos, WorldValidationError};
use dustroute_translate::world_reverse::RegionBounds;
use serde::{Serialize, Serializer};
use std::collections::{BTreeMap, BTreeSet};
use std::fmt::Debug;
use uuid::Uuid;

/// Preserve the existing human-readable diagnostic at serialization time;
/// the workflow retains the native value and never parses this text.
struct DebugText<'a, T>(&'a T);
impl<T: Debug> Serialize for DebugText<'_, T> {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(&format!("{:?}", self.0))
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
enum SafetyLabel {
    Verified,
    PreviewOnly,
    Rejected,
}
impl From<&OptimizationSafety> for SafetyLabel {
    fn from(value: &OptimizationSafety) -> Self {
        match value {
            OptimizationSafety::Verified { .. } => Self::Verified,
            OptimizationSafety::PreviewOnly { .. } => Self::PreviewOnly,
            OptimizationSafety::Rejected { .. } => Self::Rejected,
        }
    }
}
#[derive(Clone, Debug, PartialEq, Serialize)]
pub(crate) struct OptimizationPhase {
    pub accepted_mutations: usize,
    pub initial_score: f64,
    pub final_score: f64,
}
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct BuiltinOptimization {
    pub safety: OptimizationSafety,
    pub topology_preserved: bool,
    pub phases: Vec<OptimizationPhase>,
}
impl Serialize for BuiltinOptimization {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        #[derive(Serialize)]
        struct Display<'a> {
            strategy: &'static str,
            safety: SafetyLabel,
            safety_details: DebugText<'a, OptimizationSafety>,
            topology_preserved: bool,
            phases: &'a [OptimizationPhase],
        }
        Display {
            strategy: "directional_x_toward_minimum_then_global",
            safety: (&self.safety).into(),
            safety_details: DebugText(&self.safety),
            topology_preserved: self.topology_preserved,
            phases: &self.phases,
        }
        .serialize(serializer)
    }
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) enum BuiltinPlanningFailure {
    Refused(UnrecordedFailure),
    OptimizationRejected {
        safety: OptimizationSafety,
        topology_preserved: bool,
        behavior: Box<BehavioralEquivalence>,
    },
    InvalidWorld(WorldValidationError),
}
impl From<UnrecordedFailure> for BuiltinPlanningFailure {
    fn from(value: UnrecordedFailure) -> Self {
        Self::Refused(value)
    }
}
impl From<crate::failure::FailureCause> for BuiltinPlanningFailure {
    fn from(value: crate::failure::FailureCause) -> Self {
        Self::Refused(value.into())
    }
}
impl From<String> for BuiltinPlanningFailure {
    fn from(value: String) -> Self {
        Self::Refused(value.into())
    }
}
impl From<&str> for BuiltinPlanningFailure {
    fn from(value: &str) -> Self {
        Self::Refused(value.into())
    }
}
impl Serialize for BuiltinPlanningFailure {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        #[derive(Serialize)]
        struct Rejected<'a> {
            ok: bool,
            error: String,
            optimization: RejectedDetails<'a>,
        }
        #[derive(Serialize)]
        struct RejectedDetails<'a> {
            safety: SafetyLabel,
            topology_preserved: bool,
            behavior: DebugText<'a, BehavioralEquivalence>,
        }
        #[derive(Serialize)]
        struct Invalid<'a> {
            ok: bool,
            error: String,
            validation: &'a WorldValidationError,
        }
        match self {
            Self::Refused(refusal) => refusal.serialize(serializer),
            Self::OptimizationRejected {
                safety,
                topology_preserved,
                behavior,
            } => Rejected {
                ok: false,
                error: format!("optimized placement was rejected: {safety:?}"),
                optimization: RejectedDetails {
                    safety: SafetyLabel::Rejected,
                    topology_preserved: *topology_preserved,
                    behavior: DebugText(behavior),
                },
            }
            .serialize(serializer),
            Self::InvalidWorld(validation) => Invalid {
                ok: false,
                error: validation.to_string(),
                validation,
            }
            .serialize(serializer),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize)]
struct BuiltinAssemblySummary {
    assembly_revision_id: AssemblyRevisionId,
    coordinate_origin: Pos,
    source_revision_ids: BTreeSet<BlueprintRevisionId>,
    source_instances: usize,
    connections: usize,
    scope: &'static str,
}
impl BuiltinAssemblySummary {
    fn new(id: AssemblyRevisionId, coordinate_origin: Pos, assembly: &Assembly) -> Self {
        Self {
            assembly_revision_id: id,
            coordinate_origin,
            source_revision_ids: assembly
                .instances
                .iter()
                .map(|instance| instance.revision.clone())
                .collect(),
            source_instances: assembly.instances.len(),
            connections: assembly.connections.len(),
            scope: "proposed composed circuit state; full data in show_operation",
        }
    }
}
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct BuiltinPlacementPreview {
    ok: Success,
    read_only: bool,
    operation_id: Uuid,
    origin: Pos,
    bounds: RegionBounds,
    changed_blocks: usize,
    collision_count: usize,
    collision_samples: Vec<Pos>,
    materials: BTreeMap<String, usize>,
    undo_change_count: usize,
    optimization: Option<BuiltinOptimization>,
    assembly_state: BuiltinAssemblySummary,
    next_step: &'static str,
}
impl BuiltinPlacementPreview {
    pub(crate) fn new(
        plan: &PlacementPlan,
        assembly: &PlacementAssembly,
        bounds: RegionBounds,
        optimization: Option<BuiltinOptimization>,
        read_only: bool,
    ) -> Self {
        Self {
            ok: Success,
            read_only,
            operation_id: plan.operation_id,
            origin: plan.origin,
            bounds,
            changed_blocks: plan.changes.len(),
            collision_count: plan.collision_count,
            collision_samples: plan
                .changes
                .iter()
                .filter(|change| change.collision)
                .take(32)
                .map(|change| change.pos)
                .collect(),
            materials: plan.materials.clone(),
            undo_change_count: plan.undo.changes.len(),
            optimization,
            assembly_state: BuiltinAssemblySummary::new(
                assembly.revision.id.clone(),
                assembly.coordinate_origin,
                &assembly.revision.assembly,
            ),
            next_step: if read_only {
                "review this plan; writes are disabled by policy"
            } else {
                "call show_operation, obtain explicit player confirmation, then call invoke_operation with confirm=true"
            },
        }
    }
}

#[derive(Debug, Serialize)]
pub(crate) struct PlacementPlanDisplay {
    pub ok: Success,
    pub read_only: bool,
    pub plan: PlacementPlan,
}

#[cfg(test)]
mod tests {
    use super::*;
    use dustroute_optimize::OptimizationSafetyReason;
    use dustroute_physical::{TemporalAssessment, TemporalRequirement};

    #[test]
    fn optimization_labels_and_legacy_diagnostics_do_not_create_execution_facts() {
        let temporal = TemporalAssessment {
            requirement: TemporalRequirement::SteadyStateSafe,
            reasons: vec![],
        };
        for (safety, label) in [
            (
                OptimizationSafety::Verified {
                    temporal: temporal.clone(),
                },
                "verified",
            ),
            (
                OptimizationSafety::PreviewOnly {
                    temporal: temporal.clone(),
                    reasons: vec![OptimizationSafetyReason::IncompleteObservation],
                },
                "preview_only",
            ),
            (
                OptimizationSafety::Rejected {
                    temporal: temporal.clone(),
                    reasons: vec![OptimizationSafetyReason::InvalidPhysicalSupport],
                },
                "rejected",
            ),
        ] {
            let metadata = BuiltinOptimization {
                safety: safety.clone(),
                topology_preserved: true,
                phases: vec![OptimizationPhase {
                    accepted_mutations: 3,
                    initial_score: 7.5,
                    final_score: 6.25,
                }],
            };
            let wire = serde_json::to_value(&metadata).unwrap();
            assert_eq!(wire["safety"], label);
            assert_eq!(wire["safety_details"], format!("{safety:?}"));
            assert_eq!(wire["phases"][0]["accepted_mutations"], 3);
            assert_eq!(wire["phases"][0]["initial_score"], 7.5);
            assert_eq!(wire["phases"][0]["final_score"], 6.25);
            assert_eq!(metadata.safety, safety);
            assert!(wire.get("execution_progress").is_none());
        }
        let rejected = BuiltinPlanningFailure::OptimizationRejected {
            safety: OptimizationSafety::Rejected {
                temporal,
                reasons: vec![OptimizationSafetyReason::InvalidPhysicalSupport],
            },
            topology_preserved: false,
            behavior: Box::new(BehavioralEquivalence::Unavailable(
                "oracle unavailable".into(),
            )),
        };
        let wire = serde_json::to_value(rejected).unwrap();
        assert_eq!(wire["ok"], false);
        assert_eq!(wire["optimization"]["safety"], "rejected");
        assert_eq!(
            wire["optimization"]["behavior"],
            "Unavailable(\"oracle unavailable\")"
        );
        assert!(wire.get("failure").is_none());
        assert!(wire.get("execution_progress").is_none());
        assert!(wire.get("operation_id").is_none());
    }
}
