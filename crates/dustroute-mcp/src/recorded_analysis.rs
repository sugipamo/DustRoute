//! Owned analysis summaries. Classification and samples are diagnostic data,
//! not an executable circuit, fresh observation, or placement/adoption proof.
use dustroute_ir::{
    FunctionalCandidate, FunctionalKind, GateId, RecognitionStatus, RecognizedGateKind,
};
use dustroute_physical::{ComponentId, Confidence};
use dustroute_translate::analysis::{BooleanFunction, FunctionalClassification, LogicalRole};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};

pub(crate) mod reports;

/// Acquisition facts in a diagnostic report; this never reconstructs a fresh scan.
#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(tag = "strategy", rename_all = "snake_case")]
pub(crate) enum RecordedExpansion {
    AdjacentComponentFloodFill {
        components_loaded: usize,
        component_limit: usize,
        limit_reached: bool,
        scanned_tiles: usize,
        scanned_block_positions: usize,
    },
    ExplicitWorkRegion {
        limit_reached: bool,
        scope: &'static str,
    },
    ExplicitSelectedRegion {
        #[serde(skip_serializing_if = "Option::is_none")]
        components_loaded: Option<usize>,
        component_limit: Option<usize>,
        limit_reached: bool,
    },
    #[cfg(test)]
    #[serde(untagged)]
    Unspecified {},
}
impl RecordedExpansion {
    pub(crate) fn components_loaded(&self) -> Option<usize> {
        match self {
            Self::AdjacentComponentFloodFill {
                components_loaded, ..
            } => Some(*components_loaded),
            Self::ExplicitSelectedRegion {
                components_loaded, ..
            } => *components_loaded,
            Self::ExplicitWorkRegion { .. } => None,
            #[cfg(test)]
            Self::Unspecified {} => None,
        }
    }
    pub(crate) fn limit_reached(&self) -> bool {
        match self {
            Self::AdjacentComponentFloodFill { limit_reached, .. }
            | Self::ExplicitSelectedRegion { limit_reached, .. }
            | Self::ExplicitWorkRegion { limit_reached, .. } => *limit_reached,
            #[cfg(test)]
            Self::Unspecified {} => false,
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(untagged)]
pub(crate) enum PrimaryCandidate {
    TruthTable {
        kind: FunctionalClassification,
        confidence: Confidence,
        status: RecognitionStatus,
        basis: String,
        input_count: usize,
        output_count: usize,
        output_functions: Vec<BooleanFunction>,
    },
    Pattern(FunctionalCandidate),
}
#[derive(Clone, Copy, Debug, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum ClassificationLevel {
    HigherFunction,
    LocalGateNetwork,
    PhysicalOnly,
}
#[derive(Clone, Copy, Debug, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum UncertaintyReason {
    ComponentLimitReached,
    UnresolvedPhysicalConnections,
    UnresolvedLocalComponents,
    NoRegisteredHigherLevelPatternMatched,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub(crate) struct CandidateSample {
    kind: FunctionalKind,
    confidence: Confidence,
    status: RecognitionStatus,
    covered_gate_count: usize,
    missing_features: Vec<String>,
    conflicts: Vec<String>,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub(crate) struct GateSample {
    id: GateId,
    kind: RecognizedGateKind,
    status: RecognitionStatus,
    confidence: Confidence,
    input_count: usize,
    output_count: usize,
    physical_component_count: usize,
    physical_components: BTreeSet<ComponentId>,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub(crate) struct CircuitIdentity {
    classification_level: ClassificationLevel,
    primary_candidate: Option<PrimaryCandidate>,
    higher_level_candidate_count: usize,
    higher_level_candidate_samples: Vec<CandidateSample>,
    higher_level_candidates_truncated: bool,
    local_gate_counts: BTreeMap<RecognizedGateKind, usize>,
    local_gate_count: usize,
    local_gate_samples: Vec<GateSample>,
    local_gates_truncated: bool,
    analysis_complete: bool,
    uncertainty_reasons: Vec<UncertaintyReason>,
    repair_candidate_count: usize,
    repair_available: bool,
    temporal_validity: dustroute_ir::TimingAssessment,
    interpretation: String,
}

pub(crate) fn circuit_identity(
    hierarchy: &dustroute_ir::HierarchicalIr,
    logical_role: Option<&LogicalRole>,
    analysis_complete: bool,
    repair_count: usize,
) -> CircuitIdentity {
    const MAX_CANDIDATE_SAMPLES: usize = 8;
    const MAX_GATE_SAMPLES: usize = 12;
    let mut local_gate_counts = BTreeMap::new();
    for gate in &hierarchy.cell_graph.value.cells.gates {
        *local_gate_counts.entry(gate.kind).or_default() += 1;
    }
    let mut candidates: Vec<_> = hierarchy
        .functional_graph
        .value
        .functions
        .candidates
        .iter()
        .collect();
    candidates.sort_by_key(|candidate| {
        let status = match candidate.status {
            RecognitionStatus::Complete => 3_u8,
            RecognitionStatus::Partial => 2,
            RecognitionStatus::BoundaryLimited => 1,
            RecognitionStatus::Conflicting => 0,
        };
        (
            std::cmp::Reverse(candidate.confidence),
            std::cmp::Reverse(status),
        )
    });
    let truth_table_candidate = logical_role.filter(|role| {
        !matches!(
            role.classification,
            FunctionalClassification::Unknown | FunctionalClassification::Unclassified
        )
    });
    let primary = truth_table_candidate
        .map(|role| PrimaryCandidate::TruthTable {
            kind: role.classification,
            confidence: Confidence::Certain,
            status: RecognitionStatus::Complete,
            basis: role.basis.clone(),
            input_count: role.input_count,
            output_count: role.output_count,
            output_functions: role.output_functions.clone(),
        })
        .or_else(|| {
            candidates
                .first()
                .map(|candidate| PrimaryCandidate::Pattern((**candidate).clone()))
        });
    let classification_level = if primary.is_some() {
        ClassificationLevel::HigherFunction
    } else if !local_gate_counts.is_empty() {
        ClassificationLevel::LocalGateNetwork
    } else {
        ClassificationLevel::PhysicalOnly
    };
    let mut uncertainty_reasons = vec![];
    if !analysis_complete {
        uncertainty_reasons.push(UncertaintyReason::ComponentLimitReached);
    }
    if !hierarchy.physical_graph.unresolved.is_empty() {
        uncertainty_reasons.push(UncertaintyReason::UnresolvedPhysicalConnections);
    }
    if !hierarchy.cell_graph.unresolved.is_empty() {
        uncertainty_reasons.push(UncertaintyReason::UnresolvedLocalComponents);
    }
    if primary.is_none() {
        uncertainty_reasons.push(UncertaintyReason::NoRegisteredHigherLevelPatternMatched);
    }
    let local_gates = &hierarchy.cell_graph.value.cells.gates;
    CircuitIdentity {
        classification_level,
        primary_candidate: primary,
        higher_level_candidate_count: candidates.len(),
        higher_level_candidate_samples: candidates.iter().take(MAX_CANDIDATE_SAMPLES)
            .map(|candidate| CandidateSample {
                kind: candidate.kind,
                confidence: candidate.confidence,
                status: candidate.status,
                covered_gate_count: candidate.covered_gates.len(),
                missing_features: candidate.missing_features.clone(),
                conflicts: candidate.conflicts.clone(),
            }).collect(),
        higher_level_candidates_truncated: candidates.len() > MAX_CANDIDATE_SAMPLES,
        local_gate_counts,
        local_gate_count: local_gates.len(),
        local_gate_samples: local_gates.iter().take(MAX_GATE_SAMPLES)
            .map(|gate| GateSample {
                id: gate.id,
                kind: gate.kind,
                status: gate.status,
                confidence: gate.confidence,
                input_count: gate.inputs.len(),
                output_count: gate.outputs.len(),
                physical_component_count: gate.physical_components.len(),
                physical_components: gate.physical_components.clone(),
            }).collect(),
        local_gates_truncated: local_gates.len() > MAX_GATE_SAMPLES,
        analysis_complete,
        uncertainty_reasons,
        repair_candidate_count: repair_count,
        repair_available: repair_count > 0,
        temporal_validity: hierarchy.temporal.timing.clone(),
        interpretation: "primary_candidate is a ranked registered pattern; sampled local gates and mixed_ir node references provide bounded drill-down evidence".into(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn recorded_identity_keeps_ranked_bounded_samples_and_truth_table_precedence() {
        let world = dustroute_translate::world::World::new();
        let bounds = dustroute_translate::world_reverse::RegionBounds::new(
            dustroute_physical::Pos::new(0, 0, 0),
            dustroute_physical::Pos::new(1, 1, 1),
        );
        let analysis = dustroute_translate::world_reverse::analyze_world_region(&world, bounds);
        let mut hierarchy = dustroute_ir::derive_hierarchy(&analysis.scene);
        // Deliberately unsorted mixed-status candidates exercise the ranking
        // and truncation contract, independently of pattern recognition.
        hierarchy.functional_graph.value.functions.candidates = (0..9)
            .map(|i| FunctionalCandidate {
                kind: FunctionalKind::HalfAdder,
                covered_gates: [GateId(i)].into(),
                confidence: if i < 7 {
                    Confidence::Medium
                } else {
                    Confidence::Certain
                },
                status: if i == 7 {
                    RecognitionStatus::Partial
                } else {
                    RecognitionStatus::Complete
                },
                missing_features: vec![],
                conflicts: vec![],
            })
            .collect();
        hierarchy.cell_graph.value.cells.gates = (0..13)
            .map(|i| dustroute_ir::RecognizedGate {
                id: GateId(i),
                kind: RecognizedGateKind::Not,
                status: RecognitionStatus::Complete,
                inputs: vec![],
                outputs: vec![],
                physical_components: [ComponentId(i)].into(),
                confidence: Confidence::High,
                evidence: vec![],
            })
            .collect();
        let record = circuit_identity(&hierarchy, None, false, 2);
        let bytes = dustroute_codec::storage::encode("identity-test.v1", &record, 65536).unwrap();
        let reopened: CircuitIdentity =
            dustroute_codec::storage::decode("identity-test.v1", &bytes, 65536).unwrap();
        assert_eq!(
            serde_json::to_value(&record).unwrap(),
            serde_json::to_value(&reopened).unwrap()
        );
        let Some(PrimaryCandidate::Pattern(candidate)) = reopened.primary_candidate else {
            panic!("ranked registered pattern must remain the primary candidate");
        };
        assert_eq!(candidate.covered_gates, [GateId(8)].into());
        assert_eq!(reopened.higher_level_candidate_samples.len(), 8);
        assert_eq!(reopened.local_gate_samples.len(), 12);
        assert!(reopened.higher_level_candidates_truncated && reopened.local_gates_truncated);
        assert_eq!(reopened.local_gate_counts[&RecognizedGateKind::Not], 13);
        assert!(reopened.repair_available);
        assert!(!reopened.analysis_complete);
        assert!(matches!(
            reopened.uncertainty_reasons[0],
            UncertaintyReason::ComponentLimitReached
        ));
        let public = serde_json::to_value(&record).unwrap();
        assert_eq!(public["local_gate_counts"]["not"], 13);
        assert_eq!(
            public["higher_level_candidate_samples"][0]["kind"],
            "half_adder"
        );

        let role = LogicalRole {
            classification: FunctionalClassification::Xor,
            output_functions: vec![BooleanFunction::Xor],
            input_count: 2,
            output_count: 1,
            basis: "truth table".into(),
            reason: None,
        };
        let record = circuit_identity(&hierarchy, Some(&role), true, 0);
        let bytes = dustroute_codec::storage::encode("identity-test.v1", &record, 65536).unwrap();
        let reopened: CircuitIdentity =
            dustroute_codec::storage::decode("identity-test.v1", &bytes, 65536).unwrap();
        assert!(matches!(
            reopened.primary_candidate,
            Some(PrimaryCandidate::TruthTable {
                kind: FunctionalClassification::Xor,
                confidence: Confidence::Certain,
                status: RecognitionStatus::Complete,
                ..
            })
        ));
        assert!(!reopened.repair_available);
    }
}
