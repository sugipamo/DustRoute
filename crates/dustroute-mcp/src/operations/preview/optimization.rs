//! Model-only optimization reports. No recorded candidate restores a proof,
//! executable plan, fresh observation, or permission to mutate a world.
use crate::operations::mutation::Success;
use dustroute_library::ComponentId;
use dustroute_optimize::{
    ContextualVerificationState, ContractCheck, ContractCheckState, LogicalContractMode,
    MacroSteadyStateReport, MacroTransitionReport, OptimizationContract,
    OptimizationContractAssessment, PhasedPhysicalScore, PhysicalOptimizationPhase,
    PhysicalOptimizationSearchBudget, PhysicalOptimizationSearchStats, PhysicalOptimizationStop,
    PhysicalWireOptimizationError, PhysicalWireOptimizationFailure, TimingContractMode,
};
use dustroute_physical::{PhysicalPatch, Pos, TemporalRequirement};
use dustroute_translate::cells::RotationY;
use dustroute_translate::world_reverse::{RegionBounds, TruthTableComparison};
use serde::{Serialize, Serializer, ser::SerializeStruct};
use uuid::Uuid;

/// Keep the native contract rather than serializing and rereading its labels.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct OptimizationContractView(pub OptimizationContract);
impl Serialize for OptimizationContractView {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        #[derive(Serialize)]
        struct Logical {
            mode: &'static str,
        }
        #[derive(Serialize)]
        struct Timing {
            mode: &'static str,
            maximum_added_redstone_ticks: usize,
            settle_deadline_redstone_ticks: usize,
        }
        #[derive(Serialize)]
        struct Pulse {
            allow_new_pulses: bool,
            allow_removed_pulses: bool,
            maximum_width_delta_redstone_ticks: usize,
        }
        #[derive(Serialize)]
        struct Analog {
            preserve_strength: bool,
        }
        #[derive(Serialize)]
        struct Boundary {
            preserve_blocks: bool,
            preserve_facing: bool,
            preserve_driver_positions: bool,
        }
        #[derive(Serialize)]
        struct Mutation {
            focus_only: bool,
            allow_temporary_expansion: bool,
            maximum_changed_blocks: usize,
            automatic_apply: bool,
        }
        #[derive(Serialize)]
        struct Display {
            logical: Logical,
            timing: Timing,
            pulse: Pulse,
            analog: Analog,
            boundary: Boundary,
            mutation: Mutation,
        }
        let contract = self.0;
        Display {
            logical: Logical {
                mode: match contract.logical {
                    LogicalContractMode::ExactTruthTable => "exact_truth_table",
                },
            },
            timing: Timing {
                mode: match contract.timing.mode {
                    TimingContractMode::ExactTrace => "exact_trace",
                    TimingContractMode::ExactTransitions => "exact_transitions",
                    TimingContractMode::BoundedDelay => "bounded_delay",
                    TimingContractMode::SettledValueOnly => "settled_value_only",
                    TimingContractMode::PreserveOrder => "preserve_order",
                },
                maximum_added_redstone_ticks: contract.timing.maximum_added_redstone_ticks,
                settle_deadline_redstone_ticks: contract.timing.settle_deadline_redstone_ticks,
            },
            pulse: Pulse {
                allow_new_pulses: contract.pulse.allow_new_pulses,
                allow_removed_pulses: contract.pulse.allow_removed_pulses,
                maximum_width_delta_redstone_ticks: contract
                    .pulse
                    .maximum_width_delta_redstone_ticks,
            },
            analog: Analog {
                preserve_strength: contract.analog.preserve_strength,
            },
            boundary: Boundary {
                preserve_blocks: contract.boundary.preserve_blocks,
                preserve_facing: contract.boundary.preserve_facing,
                preserve_driver_positions: contract.boundary.preserve_driver_positions,
            },
            mutation: Mutation {
                focus_only: contract.mutation.focus_only,
                allow_temporary_expansion: contract.mutation.allow_temporary_expansion,
                maximum_changed_blocks: contract.mutation.maximum_changed_blocks,
                automatic_apply: contract.mutation.automatic_apply,
            },
        }
        .serialize(serializer)
    }
}
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct OptimizationAssessmentView(pub OptimizationContractAssessment);
impl Serialize for OptimizationAssessmentView {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        struct Check<'a>(&'a ContractCheck);
        impl Serialize for Check<'_> {
            fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
                let mut record = serializer.serialize_struct("ContractCheck", 3)?;
                record.serialize_field(
                    "state",
                    &match self.0.state {
                        ContractCheckState::Passed => "passed",
                        ContractCheckState::Failed => "failed",
                        ContractCheckState::Unavailable => "unavailable",
                    },
                )?;
                record.serialize_field("reason_codes", &self.0.reason_codes)?;
                record.serialize_field("reasons", &self.0.reasons)?;
                record.end()
            }
        }
        let mut record = serializer.serialize_struct("ContractAssessment", 7)?;
        record.serialize_field("satisfied", &self.0.satisfied())?;
        record.serialize_field("logical", &Check(&self.0.logical))?;
        record.serialize_field("timing", &Check(&self.0.timing))?;
        record.serialize_field("pulse", &Check(&self.0.pulse))?;
        record.serialize_field("analog", &Check(&self.0.analog))?;
        record.serialize_field("boundary", &Check(&self.0.boundary))?;
        record.serialize_field("mutation", &Check(&self.0.mutation))?;
        record.end()
    }
}
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct VerificationState(pub ContextualVerificationState);
impl Serialize for VerificationState {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(match self.0 {
            ContextualVerificationState::Pending => "pending",
            ContextualVerificationState::Passed => "passed",
            ContextualVerificationState::Failed => "failed",
        })
    }
}
#[derive(Clone, Debug, PartialEq)]
pub(crate) enum BoundaryStrength {
    Passed,
    Failed(String),
}
impl From<Result<(), String>> for BoundaryStrength {
    fn from(result: Result<(), String>) -> Self {
        match result {
            Ok(()) => Self::Passed,
            Err(reason) => Self::Failed(reason),
        }
    }
}
impl Serialize for BoundaryStrength {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        let mut record = serializer.serialize_struct(
            "BoundaryStrength",
            if matches!(self, Self::Passed) { 1 } else { 2 },
        )?;
        match self {
            Self::Passed => record.serialize_field("state", "passed")?,
            Self::Failed(reason) => {
                record.serialize_field("state", "failed")?;
                record.serialize_field("reason", reason)?;
            }
        }
        record.end()
    }
}
#[derive(Clone, Debug, PartialEq)]
pub(crate) enum WireSemantic {
    Equivalent(TruthTableComparison),
    Unavailable,
}
impl Serialize for WireSemantic {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        let mut record = serializer.serialize_struct(
            "WireSemantic",
            if matches!(self, Self::Unavailable) {
                2
            } else {
                3
            },
        )?;
        match self {
            Self::Equivalent(comparison) => {
                record.serialize_field("available", &true)?;
                record.serialize_field("equivalent", &true)?;
                record.serialize_field("comparison", comparison)?;
            }
            Self::Unavailable => {
                record.serialize_field("available", &false)?;
                record.serialize_field("reason", "truth-table inference was unavailable; the plan remains preview-only physical-path optimization")?;
            }
        }
        record.end()
    }
}

/// Exactly the old history subset; no operation ID, progress or execution proof.
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct MacroOptimizationCandidate {
    circuit_id: Uuid,
    component_id: ComponentId,
    patch: PhysicalPatch,
    contract: OptimizationContractView,
    contract_assessment: OptimizationAssessmentView,
}
impl MacroOptimizationCandidate {
    pub(crate) fn new(
        circuit_id: Uuid,
        component_id: ComponentId,
        patch: PhysicalPatch,
        contract: OptimizationContract,
        assessment: OptimizationContractAssessment,
    ) -> Self {
        Self {
            circuit_id,
            component_id,
            patch,
            contract: OptimizationContractView(contract),
            contract_assessment: OptimizationAssessmentView(assessment),
        }
    }
}
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct WireOptimizationCandidate {
    circuit_id: Uuid,
    focus: RegionBounds,
    patch: PhysicalPatch,
    semantic_verification: WireSemantic,
    contract: OptimizationContractView,
    contract_assessment: OptimizationAssessmentView,
}
impl WireOptimizationCandidate {
    pub(crate) fn new(
        circuit_id: Uuid,
        focus: RegionBounds,
        patch: PhysicalPatch,
        semantic: WireSemantic,
        contract: OptimizationContract,
        assessment: OptimizationContractAssessment,
    ) -> Self {
        Self {
            circuit_id,
            focus,
            patch,
            semantic_verification: semantic,
            contract: OptimizationContractView(contract),
            contract_assessment: OptimizationAssessmentView(assessment),
        }
    }
}

#[derive(Debug, Serialize)]
pub(crate) struct MacroOptimizationPreview {
    pub schema_version: &'static str,
    pub ok: Success,
    pub operation_id: Uuid,
    pub optimization_kind: &'static str,
    pub name: String,
    #[serde(flatten)]
    pub candidate: MacroOptimizationCandidate,
    pub placement: MacroPlacement,
    pub metrics: MacroMetrics,
    pub verification: MacroVerification,
    pub next_step: &'static str,
}
#[derive(Debug, Serialize)]
pub(crate) struct MacroPlacement {
    pub origin: Pos,
    pub rotation_y: RotationY,
    pub route_length: usize,
}
#[derive(Debug, Serialize)]
pub(crate) struct MacroMetrics {
    pub changed_blocks: usize,
    pub added_supports: usize,
    pub inserted_repeaters: usize,
}
#[derive(Debug, Serialize)]
pub(crate) struct MacroVerification {
    pub structural: VerificationState,
    pub steady_state: VerificationState,
    pub transition_cases: Option<usize>,
    pub transition_differences: Option<usize>,
    pub boundary_strength: BoundaryStrength,
}

/// Input strings are interpreted once by the MCP handler, before this workflow.
#[derive(Clone, Copy, Debug, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum WireObjective {
    WireLength,
    DensityThenWireLength,
}
#[derive(Debug, Serialize)]
pub(crate) struct WireOptimizationPreview {
    pub schema_version: &'static str,
    pub ok: Success,
    pub circuit_id: Uuid,
    pub operation_id: Uuid,
    pub objective: WireObjective,
    pub contract: OptimizationContractView,
    pub contract_assessment: OptimizationAssessmentView,
    pub focus: RegionBounds,
    pub outside_focus_fixed: bool,
    pub preserved_boundary_blocks: usize,
    pub fixed_endpoints: [Pos; 2],
    pub metrics: WireMetrics,
    pub search: WireSearch,
    pub phase_trace: Vec<WirePhase>,
    pub planning_policy: WirePlanningPolicy,
    pub patch: PhysicalPatch,
    pub verification: WireVerification,
    pub next_step: &'static str,
}
#[derive(Debug, Serialize)]
pub(crate) struct WireMetrics {
    pub wire_blocks_before: usize,
    pub wire_blocks_after: usize,
    pub path_length_before: usize,
    pub path_length_after: usize,
    pub changed_blocks: usize,
}
#[derive(Clone, Debug, PartialEq, Serialize)]
pub(crate) struct WireSearch {
    budget: WireBudget,
    candidate_budget_scope: &'static str,
    expansions: usize,
    candidates: usize,
    truncated: bool,
    stop_reason: Option<PhysicalOptimizationStop>,
}
#[derive(Clone, Debug, PartialEq, Serialize)]
struct WireBudget {
    max_expansions: usize,
    max_candidates: usize,
    max_millis: u64,
}
impl WireSearch {
    pub(crate) fn new(
        budget: PhysicalOptimizationSearchBudget,
        stats: PhysicalOptimizationSearchStats,
    ) -> Self {
        Self {
            budget: WireBudget {
                max_expansions: budget.max_expansions,
                max_candidates: budget.max_candidates,
                max_millis: budget.max_millis,
            },
            candidate_budget_scope: "strength_preserving_paths_per_segment",
            expansions: stats.expansions,
            candidates: stats.candidates,
            truncated: stats.truncated,
            stop_reason: stats.stop_reason,
        }
    }
}
/// A failed read-only search has no operation ID or executable partial patch.
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct WireSearchRefusal {
    ok: bool,
    schema_version: &'static str,
    error: &'static str,
    error_code: crate::api::McpErrorCode,
    retryable: bool,
    cause: PhysicalWireOptimizationError,
    search: WireSearch,
    writes_minecraft: bool,
    impossibility_proven: bool,
    next_action: &'static str,
    next_step: &'static str,
}
impl From<PhysicalWireOptimizationFailure> for WireSearchRefusal {
    fn from(failure: PhysicalWireOptimizationFailure) -> Self {
        Self {
            ok: false,
            schema_version: crate::api::ERROR_SCHEMA_V1,
            error: "no acceptable wire optimization candidate was found in this search",
            error_code: if failure.search.truncated {
                crate::api::McpErrorCode::ResourceLimit
            } else {
                crate::api::McpErrorCode::InvalidState
            },
            retryable: false,
            cause: failure.reason,
            search: WireSearch::new(failure.budget, failure.search),
            writes_minecraft: false,
            impossibility_proven: false,
            next_action: "review_search_failure",
            next_step: "inspect search limits and candidate refusal; revise the request or budget before a new search; this result does not prove physical impossibility",
        }
    }
}
#[derive(Debug, Serialize)]
pub(crate) struct WirePhase {
    name: &'static str,
    accepted: bool,
    before: PhaseScore,
    after: PhaseScore,
    connector_growth: usize,
}
#[derive(Debug, Serialize)]
struct PhaseScore {
    bounding_volume: usize,
    occupied_blocks: usize,
    connector_length: usize,
}
impl From<PhasedPhysicalScore> for PhaseScore {
    fn from(score: PhasedPhysicalScore) -> Self {
        Self {
            bounding_volume: score.bounding_volume,
            occupied_blocks: score.occupied_blocks,
            connector_length: score.connector_length,
        }
    }
}
impl From<&PhysicalOptimizationPhase> for WirePhase {
    fn from(phase: &PhysicalOptimizationPhase) -> Self {
        Self {
            name: phase.name,
            accepted: phase.accepted,
            before: phase.before.into(),
            after: phase.after.into(),
            connector_growth: phase.connector_growth,
        }
    }
}
#[derive(Debug, Serialize)]
pub(crate) struct WirePlanningPolicy {
    pub temporary_connector_growth_is_internal_only: bool,
    pub temporary_connector_growth_budget: usize,
    pub final_global_improvement_required: bool,
}
#[derive(Debug, Default, Serialize)]
pub(crate) struct WireTruthFailures {
    original: Option<crate::recorded_analysis::reports::TruthErrorView>,
    candidate: Option<crate::recorded_analysis::reports::TruthErrorView>,
}
impl WireTruthFailures {
    pub(crate) fn new(
        original: &Result<
            dustroute_translate::world_reverse::InferredTruthTable,
            dustroute_translate::world_reverse::TruthTableError,
        >,
        candidate: &Result<
            dustroute_translate::world_reverse::InferredTruthTable,
            dustroute_translate::world_reverse::TruthTableError,
        >,
    ) -> Self {
        use crate::recorded_analysis::reports::TruthErrorView;
        Self {
            original: original.as_ref().err().cloned().map(TruthErrorView),
            candidate: candidate.as_ref().err().cloned().map(TruthErrorView),
        }
    }
}
#[derive(Debug, Serialize)]
pub(crate) struct WireVerification {
    pub truth_table_failures: WireTruthFailures,
    pub diagnostics_not_worse: bool,
    pub temporal_requirement_preserved: bool,
    pub temporal_requirement: TemporalRequirement,
    pub semantic: WireSemantic,
    pub steady_state: Option<WireSteadyState>,
    pub transitions: Option<WireTransitions>,
    pub boundary_strength: Option<BoundaryStrength>,
}
#[derive(Debug, Serialize)]
pub(crate) struct WireSteadyState {
    state: VerificationState,
    differing_assignments: Vec<Vec<bool>>,
    reason: Option<String>,
}
impl From<&MacroSteadyStateReport> for WireSteadyState {
    fn from(report: &MacroSteadyStateReport) -> Self {
        Self {
            state: VerificationState(report.state),
            differing_assignments: report.differing_assignments.clone(),
            reason: report.reason.clone(),
        }
    }
}
#[derive(Debug, Serialize)]
pub(crate) struct WireTransitions {
    reason_code: Option<dustroute_optimize::TransitionUnavailableReason>,
    state: VerificationState,
    case_count: usize,
    differing_cases: usize,
    reason: Option<String>,
}
impl From<&MacroTransitionReport> for WireTransitions {
    fn from(report: &MacroTransitionReport) -> Self {
        Self {
            reason_code: report.unavailable_reason,
            state: VerificationState(report.state),
            case_count: report.cases.len(),
            differing_cases: report.differing_cases,
            reason: report.reason.clone(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::operations::OperationResult;
    use dustroute_physical::PhysicalPatchReason;
    use serde_json::json;

    fn passed_assessment() -> OptimizationContractAssessment {
        let passed = ContractCheck {
            state: ContractCheckState::Passed,
            reason_codes: vec![],
            reasons: vec![],
        };
        OptimizationContractAssessment {
            logical: passed.clone(),
            timing: passed.clone(),
            pulse: passed.clone(),
            analog: passed.clone(),
            boundary: passed.clone(),
            mutation: passed,
        }
    }

    #[test]
    fn every_contract_mode_and_category_retains_its_native_value() {
        for (mode, label) in [
            (TimingContractMode::ExactTrace, "exact_trace"),
            (TimingContractMode::ExactTransitions, "exact_transitions"),
            (TimingContractMode::BoundedDelay, "bounded_delay"),
            (TimingContractMode::SettledValueOnly, "settled_value_only"),
            (TimingContractMode::PreserveOrder, "preserve_order"),
        ] {
            let mut contract = OptimizationContract::default();
            contract.timing.mode = mode;
            contract.timing.maximum_added_redstone_ticks = 7;
            contract.timing.settle_deadline_redstone_ticks = 35;
            contract.pulse.allow_new_pulses = true;
            contract.pulse.maximum_width_delta_redstone_ticks = 2;
            contract.analog.preserve_strength = true;
            contract.boundary.preserve_facing = false;
            contract.mutation.allow_temporary_expansion = false;
            contract.mutation.maximum_changed_blocks = 123;
            let view = OptimizationContractView(contract);
            let wire = serde_json::to_value(view).unwrap();
            assert_eq!(
                wire,
                json!({
                    "logical":{"mode":"exact_truth_table"},
                    "timing":{"mode":label,"maximum_added_redstone_ticks":7,"settle_deadline_redstone_ticks":35},
                    "pulse":{"allow_new_pulses":true,"allow_removed_pulses":false,"maximum_width_delta_redstone_ticks":2},
                    "analog":{"preserve_strength":true},
                    "boundary":{"preserve_blocks":true,"preserve_facing":false,"preserve_driver_positions":true},
                    "mutation":{"focus_only":true,"allow_temporary_expansion":false,"maximum_changed_blocks":123,"automatic_apply":false}
                })
            );
            assert_eq!(view.0, contract);
        }
        for (state, label) in [
            (ContractCheckState::Passed, "passed"),
            (ContractCheckState::Failed, "failed"),
            (ContractCheckState::Unavailable, "unavailable"),
        ] {
            let mut assessment = passed_assessment();
            assessment.logical = ContractCheck {
                state,
                reason_codes: vec!["logical_evidence".into()],
                reasons: vec!["the original evidence".into()],
            };
            let view = OptimizationAssessmentView(assessment.clone());
            let wire = serde_json::to_value(&view).unwrap();
            assert_eq!(wire["satisfied"], state == ContractCheckState::Passed);
            assert_eq!(wire["logical"]["state"], label);
            assert_eq!(wire["logical"]["reason_codes"], json!(["logical_evidence"]));
            assert_eq!(wire["logical"]["reasons"], json!(["the original evidence"]));
            for category in ["timing", "pulse", "analog", "boundary", "mutation"] {
                assert_eq!(
                    wire[category],
                    json!({"state":"passed","reason_codes":[],"reasons":[]})
                );
            }
            assert_eq!(view.0, assessment);
        }
    }

    #[test]
    fn unavailable_checks_are_distinct_from_known_zero_and_failed_evidence() {
        let verification = WireVerification {
            truth_table_failures: WireTruthFailures::default(),
            diagnostics_not_worse: true,
            temporal_requirement_preserved: true,
            temporal_requirement: TemporalRequirement::SteadyStateSafe,
            semantic: WireSemantic::Unavailable,
            steady_state: None,
            transitions: None,
            boundary_strength: None,
        };
        let wire = serde_json::to_value(verification).unwrap();
        assert_eq!(wire["semantic"]["available"], false);
        assert!(wire["semantic"].get("equivalent").is_none());
        assert!(wire["semantic"].get("comparison").is_none());
        for field in ["steady_state", "transitions", "boundary_strength"] {
            assert!(wire.as_object().unwrap().contains_key(field));
            assert!(wire[field].is_null());
        }
        let known_zero = MacroTransitionReport {
            unavailable_reason: None,
            state: ContextualVerificationState::Passed,
            cases: vec![],
            differing_cases: 0,
            reason: None,
        };
        let wire = serde_json::to_value(WireTransitions::from(&known_zero)).unwrap();
        assert_eq!(
            wire,
            json!({"state":"passed","case_count":0,"differing_cases":0,"reason":null,"reason_code":null})
        );
        assert_eq!(
            serde_json::to_value(BoundaryStrength::Passed).unwrap(),
            json!({"state":"passed"})
        );
        assert_eq!(
            serde_json::to_value(BoundaryStrength::Failed("original cause".into())).unwrap(),
            json!({"state":"failed","reason":"original cause"})
        );
        for state in [
            ContextualVerificationState::Pending,
            ContextualVerificationState::Passed,
            ContextualVerificationState::Failed,
        ] {
            assert_eq!(
                serde_json::to_value(VerificationState(state)).unwrap(),
                format!("{state:?}").to_lowercase()
            );
        }
    }

    #[test]
    fn macro_preview_retains_failed_evidence_without_authorizing_history() {
        let circuit_id = Uuid::new_v4();
        let operation_id = Uuid::new_v4();
        let mut assessment = passed_assessment();
        assessment.timing = ContractCheck {
            state: ContractCheckState::Failed,
            reason_codes: vec!["timing_difference".into()],
            reasons: vec!["settled output matches but transitions differ".into()],
        };
        let candidate = MacroOptimizationCandidate::new(
            circuit_id,
            ComponentId::new("example.macro.v1").unwrap(),
            PhysicalPatch {
                reason: PhysicalPatchReason::OptimizePlacement,
                affected_fragments: vec![],
                confidence_percent: 100,
                explanation: "synthetic projection, not an observed plan".into(),
                changes: vec![],
            },
            OptimizationContract::default(),
            assessment,
        );
        for (cases, differences) in [(Some(16), Some(12)), (None, None)] {
            let preview = MacroOptimizationPreview {
                schema_version: "dustroute.optimization.v1",
                ok: Success,
                operation_id,
                optimization_kind: "macro_replacement",
                name: "synthetic macro".into(),
                candidate: candidate.clone(),
                placement: MacroPlacement {
                    origin: Pos::new(3, 2, -1),
                    rotation_y: RotationY::R90,
                    route_length: 27,
                },
                metrics: MacroMetrics {
                    changed_blocks: 9,
                    added_supports: 4,
                    inserted_repeaters: 2,
                },
                verification: MacroVerification {
                    structural: VerificationState(ContextualVerificationState::Passed),
                    steady_state: VerificationState(ContextualVerificationState::Passed),
                    transition_cases: cases,
                    transition_differences: differences,
                    boundary_strength: BoundaryStrength::Failed("strength differs".into()),
                },
                next_step: "do not invoke; inspect the failed or unavailable contract categories",
            };
            let response = serde_json::to_value(preview).unwrap();
            assert_eq!(response.as_object().unwrap().len(), 14);
            assert_eq!(response["ok"], true);
            assert_eq!(response["operation_id"], operation_id.to_string());
            assert_eq!(response["optimization_kind"], "macro_replacement");
            assert!(response.get("candidate").is_none());
            assert!(response.get("execution_progress").is_none());
            assert_eq!(response["contract_assessment"]["satisfied"], false);
            assert_eq!(
                response["placement"],
                json!({
                    "origin":{"x":3,"y":2,"z":-1},"rotation_y":"r90","route_length":27
                })
            );
            assert_eq!(
                response["metrics"],
                json!({
                    "changed_blocks":9,"added_supports":4,"inserted_repeaters":2
                })
            );
            assert_eq!(
                response["verification"],
                json!({
                    "structural":"passed","steady_state":"passed",
                    "transition_cases":cases,"transition_differences":differences,
                    "boundary_strength":{"state":"failed","reason":"strength differs"}
                })
            );
            let history = OperationResult::from(candidate.clone());
            assert!(!history.failed());
            assert!(!history.consumed());
            assert!(history.progress().is_none());
            let history = serde_json::to_value(history).unwrap();
            assert_eq!(history.as_object().unwrap().len(), 5);
            for field in [
                "circuit_id",
                "component_id",
                "patch",
                "contract",
                "contract_assessment",
            ] {
                assert_eq!(history[field], response[field]);
            }
        }
    }

    #[test]
    fn optimization_history_keeps_only_the_original_subset_and_no_execution_facts() {
        let circuit_id = Uuid::new_v4();
        let component_id = ComponentId::new("example.macro.v1").unwrap();
        let focus = RegionBounds::new(Pos::default(), Pos::new(2, 1, 2));
        let patch = PhysicalPatch {
            reason: PhysicalPatchReason::OptimizePlacement,
            affected_fragments: vec![],
            confidence_percent: 100,
            explanation: "virtual candidate".into(),
            changes: vec![],
        };
        let contract = OptimizationContract::default();
        let assessment = passed_assessment();
        let macro_candidate = MacroOptimizationCandidate::new(
            circuit_id,
            component_id.clone(),
            patch.clone(),
            contract,
            assessment.clone(),
        );
        let wire_candidate = WireOptimizationCandidate::new(
            circuit_id,
            focus,
            patch.clone(),
            WireSemantic::Unavailable,
            contract,
            assessment.clone(),
        );
        let expected_macro = json!({"circuit_id":circuit_id,"component_id":component_id,"patch":patch,"contract":OptimizationContractView(contract),"contract_assessment":OptimizationAssessmentView(assessment.clone())});
        let expected_wire = json!({"circuit_id":circuit_id,"focus":focus,"patch":patch,"semantic_verification":WireSemantic::Unavailable,"contract":OptimizationContractView(contract),"contract_assessment":OptimizationAssessmentView(assessment)});
        for (history, expected) in [
            (OperationResult::from(macro_candidate), expected_macro),
            (OperationResult::from(wire_candidate), expected_wire),
        ] {
            assert!(!history.failed());
            assert!(history.progress().is_none());
            assert!(!history.consumed());
            let wire = serde_json::to_value(history).unwrap();
            assert_eq!(wire, expected);
            assert!(wire.get("ok").is_none());
            assert!(wire.get("operation_id").is_none());
            assert!(wire.get("execution_progress").is_none());
        }
    }
}
