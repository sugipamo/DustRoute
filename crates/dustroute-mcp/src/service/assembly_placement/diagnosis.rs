//! Read-only diagnosis of a registered design, regardless of who changed it.
use super::*;
use dustroute_translate::diagnostic::difference::{DifferenceKind, differences};
use dustroute_translate::diagnostic::report::{Diagnosis, RepairStatus};
use dustroute_translate::snapshot::MinecraftSnapshot;
use std::collections::BTreeMap;

#[derive(serde::Serialize)]
struct AssemblyDiagnosis {
    #[serde(flatten)]
    diagnosis: Diagnosis,
    #[serde(flatten)]
    details: DiagnosisOutcome,
}

#[derive(serde::Serialize)]
#[serde(tag = "status", rename_all = "snake_case")]
enum DiagnosisOutcome {
    ObservationUnavailable { reason: String, cause: &'static str },
    ReferenceUnverified(ComparisonDetails),
    MatchesReference(ComparisonDetails),
    DifferencesFound(ComparisonDetails),
}
impl DiagnosisOutcome {
    fn comparison_mut(&mut self) -> Option<&mut ComparisonDetails> {
        match self {
            Self::ObservationUnavailable { .. } => None,
            Self::ReferenceUnverified(details)
            | Self::MatchesReference(details)
            | Self::DifferencesFound(details) => Some(details),
        }
    }
}

#[derive(serde::Serialize)]
#[serde(tag = "mode", rename_all = "snake_case")]
enum ComparisonReference {
    RemovedInstance,
    ObservedInputs {
        input_order: Vec<Pos>,
        limit: &'static str,
    },
    DeclaredInitial {
        reason: String,
        limit: &'static str,
    },
    SavedInitialUnverified {
        reason: String,
        limit: &'static str,
    },
}

#[derive(serde::Serialize)]
struct DifferenceSummary {
    differing_positions: usize,
    by_kind: BTreeMap<DifferenceKind, usize>,
}

#[derive(serde::Serialize)]
struct ReconstructionOffer {
    steps: usize,
    target: &'static str,
    affected_blocks: usize,
    next_action: &'static str,
    review_scope: &'static str,
}

#[derive(serde::Serialize)]
struct ComparisonDetails {
    reference: ComparisonReference,
    reference_snapshot: MinecraftSnapshot,
    summary: DifferenceSummary,
    repair_details: Option<ReconstructionOffer>,
    cause: &'static str,
    ownership: &'static str,
    server_readiness_proven: bool,
    interpretation: &'static str,
}

impl AssemblyService<'_> {
    pub(super) async fn diagnose_instance(
        &self,
        record: &PlacedAssembly,
        proof: &Result<ValidatedAssemblyPlacement, String>,
        observation: &observation::InstanceObservation,
    ) -> Value {
        let snapshot = match observation::stable_baseline(observation) {
            Ok(snapshot) => snapshot,
            Err(cause) => {
                let mut report = with_design(unavailable(&cause.message), record, proof);
                report["observation_failure"] = serde_json::json!(cause);
                return report;
            }
        };
        let saved = record.expected.clone();
        let state = record.state;
        let inspected_proof = proof.clone();
        let result = tokio::task::spawn_blocking(move || {
            inspect_snapshot(state, saved, inspected_proof, snapshot)
        })
        .await
        .map_err(|e| e.to_string())
        .and_then(|r| r);
        let mut report = match result {
            Ok(report) => report,
            Err(reason) => return with_design(unavailable(&reason), record, proof),
        };
        if let Some(details) = report.details.comparison_mut()
            && let Some(offer) = &details.repair_details
        {
            if let Err(error) = self.policy.validate_placement_size(offer.steps) {
                report
                    .diagnosis
                    .assess_repair(RepairStatus::Blocked, Some(error.to_string()));
                details.repair_details = None;
            }
        }
        with_design(report, record, proof)
    }
}

fn with_design(
    mut report: AssemblyDiagnosis,
    record: &PlacedAssembly,
    proof: &Result<ValidatedAssemblyPlacement, String>,
) -> Value {
    report.diagnosis.attach_design(
        record.assembly_id.clone(),
        proof.as_ref().ok().map(|p| p.design_index()),
        proof.is_ok(),
    );
    serde_json::to_value(report).expect("serializable diagnosis")
}

fn unavailable(reason: &str) -> AssemblyDiagnosis {
    let mut diagnosis = Diagnosis::design_comparison(vec![]);
    diagnosis.assess_repair(RepairStatus::NotAssessed, Some(reason.into()));
    AssemblyDiagnosis {
        diagnosis,
        details: DiagnosisOutcome::ObservationUnavailable {
            reason: reason.into(),
            cause: "not_inferred",
        },
    }
}

fn inspect_snapshot(
    state: InstanceState,
    saved: MinecraftSnapshot,
    proof: Result<ValidatedAssemblyPlacement, String>,
    snapshot: MinecraftSnapshot,
) -> Result<AssemblyDiagnosis, String> {
    // Raw differences remain available when revalidation or runtime admission
    // fails. A saved reference must be explicitly labelled as unverified.
    let (reference, reference_info) = if state == InstanceState::Removed {
        (
            MinecraftSnapshot {
                blocks: vec![],
                ..saved.clone()
            },
            ComparisonReference::RemovedInstance,
        )
    } else if let Ok(proof) = &proof {
        match proof.operating_reference(&snapshot) {
            Ok(reference) => (
                reference,
                ComparisonReference::ObservedInputs {
                    input_order: proof.context().input_levers.clone(),
                    limit: "one settled reference from the initial design and observed lever levels in declared order; other histories may have other valid states",
                },
            ),
            Err(reason) => (
                proof.settled().clone(),
                ComparisonReference::DeclaredInitial {
                    reason,
                    limit: "input state could not be used; findings are relative to the initial construction result",
                },
            ),
        }
    } else {
        (
            saved,
            ComparisonReference::SavedInitialUnverified {
                reason: proof.as_ref().unwrap_err().clone(),
                limit: "historical reference only; fresh source/target review failed",
            },
        )
    };
    let findings = differences(&snapshot, &reference)?;
    let mut counts: BTreeMap<_, usize> = [
        DifferenceKind::Missing,
        DifferenceKind::Unexpected,
        DifferenceKind::DifferentBlock,
        DifferenceKind::Orientation,
        DifferenceKind::Configuration,
        DifferenceKind::State,
    ]
    .into_iter()
    .map(|kind| (kind, 0))
    .collect();
    for finding in &findings {
        for kind in &finding.kinds {
            *counts.get_mut(kind).expect("known difference kind") += 1;
        }
    }
    let differing_positions = findings.len();
    let mut diagnosis = Diagnosis::design_comparison(findings);
    let (status, reason, repair_details) = match &proof {
        Err(reason) => (RepairStatus::Blocked, Some(reason.clone()), None),
        Ok(_) if state == InstanceState::Removed => {
            (RepairStatus::RequiresNewPlacement, None, None)
        }
        Ok(_) if differing_positions == 0 => (RepairStatus::NotNeededForReferenceMatch, None, None),
        Ok(proof) => match proof.reconstruction_steps(&snapshot) {
            Ok(steps) => (
                RepairStatus::PlanAvailable,
                None,
                Some(ReconstructionOffer {
                    steps: steps.len(),
                    target: "declared_initial_construction_result",
                    affected_blocks: crate::revision::blocks(&snapshot)?.len(),
                    next_action: "plan_reconstruction",
                    review_scope: "all affected blocks, including matching blocks; ownership and modification intent are not inferred",
                }),
            ),
            Err(reason) => (RepairStatus::Blocked, Some(reason), None),
        },
    };
    diagnosis.assess_repair(status, reason);
    let details = ComparisonDetails {
        reference: reference_info,
        reference_snapshot: reference,
        summary: DifferenceSummary {
            differing_positions,
            by_kind: counts,
        },
        repair_details,
        cause: "not_inferred",
        ownership: "not_proven",
        server_readiness_proven: false,
        interpretation: "literal differences from the selected reference; damage, intentional edits, and consequences of another fault are not automatically distinguished",
    };
    Ok(AssemblyDiagnosis {
        diagnosis,
        details: if proof.is_err() {
            DiagnosisOutcome::ReferenceUnverified(details)
        } else if differing_positions == 0 {
            DiagnosisOutcome::MatchesReference(details)
        } else {
            DiagnosisOutcome::DifferencesFound(details)
        },
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unreviewable_source_still_reports_literal_damage_without_repair_permission() {
        let saved: MinecraftSnapshot = serde_json::from_value(json!({
            "min":{"x":0,"y":0,"z":0},"max":{"x":1,"y":0,"z":0},
            "blocks":[{"pos":{"x":0,"y":0,"z":0},"name":"minecraft:stone"}]
        }))
        .unwrap();
        let actual = MinecraftSnapshot {
            blocks: vec![],
            ..saved.clone()
        };
        let result = inspect_snapshot(
            InstanceState::Applied,
            saved,
            Err("stale context".into()),
            actual,
        )
        .unwrap();
        let result = serde_json::to_value(result).unwrap();
        assert_eq!(result["status"], "reference_unverified");
        assert_eq!(result["reference"]["mode"], "saved_initial_unverified");
        assert_eq!(result["summary"]["by_kind"]["missing"], 1);
        assert_eq!(result["repair"]["status"], "blocked");
        assert_eq!(result["world_writes"], false);
    }
}
