//! Read-only diagnosis of a registered design, regardless of who changed it.
use super::*;
use dustroute_translate::MinecraftSnapshot;
use dustroute_translate::diagnostic::difference::{DifferenceKind, differences};
use dustroute_translate::diagnostic::report::{Diagnosis, RepairStatus};
use std::collections::BTreeMap;

#[derive(serde::Serialize)]
struct AssemblyDiagnosis {
    #[serde(flatten)]
    diagnosis: Diagnosis,
    #[serde(flatten)]
    details: Value,
}

impl DustRouteMcp {
    pub(super) async fn diagnose_instance(
        &self,
        record: &PlacedAssembly,
        proof: &Result<ValidatedAssemblyPlacement, String>,
        observation: &Value,
    ) -> Value {
        let snapshot = match observation::stable_baseline(observation) {
            Ok(snapshot) => snapshot,
            Err(reason) => return with_design(unavailable(&reason), record, proof),
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
        if let Some(count) = report.details["repair_details"]["steps"].as_u64() {
            if let Err(error) = self.policy.validate_placement_size(count as usize) {
                report
                    .diagnosis
                    .assess_repair(RepairStatus::Blocked, Some(error.to_string()));
                report.details["repair_details"] = Value::Null;
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
        details: json!({"status":"observation_unavailable",
        "reason":reason,"cause":"not_inferred"}),
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
            json!({"mode":"removed_instance"}),
        )
    } else if let Ok(proof) = &proof {
        match proof.operating_reference(&snapshot) {
            Ok(reference) => (
                reference,
                json!({"mode":"observed_inputs","input_order":proof.context().input_levers,
                "limit":"one settled reference from the initial design and observed lever levels in declared order; other histories may have other valid states"}),
            ),
            Err(reason) => (
                proof.settled().clone(),
                json!({"mode":"declared_initial","reason":reason,
                "limit":"input state could not be used; findings are relative to the initial construction result"}),
            ),
        }
    } else {
        (
            saved,
            json!({"mode":"saved_initial_unverified","reason":proof.as_ref().unwrap_err(),
            "limit":"historical reference only; fresh source/target review failed"}),
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
        Err(reason) => (RepairStatus::Blocked, Some(reason.clone()), Value::Null),
        Ok(_) if state == InstanceState::Removed => {
            (RepairStatus::RequiresNewPlacement, None, Value::Null)
        }
        Ok(_) if differing_positions == 0 => {
            (RepairStatus::NotNeededForReferenceMatch, None, Value::Null)
        }
        Ok(proof) => match proof.reconstruction_steps(&snapshot) {
            Ok(steps) => (
                RepairStatus::PlanAvailable,
                None,
                json!({"steps":steps.len(),
                "target":"declared_initial_construction_result",
                "affected_blocks":crate::revision::blocks(&snapshot)?.len(),
                "next_action":"plan_reconstruction",
                "review_scope":"all affected blocks, including matching blocks; ownership and modification intent are not inferred"}),
            ),
            Err(reason) => (RepairStatus::Blocked, Some(reason), Value::Null),
        },
    };
    diagnosis.assess_repair(status, reason);
    Ok(AssemblyDiagnosis {
        diagnosis,
        details: json!({
            "status":if proof.is_err(){"reference_unverified"}else if differing_positions == 0{"matches_reference"}else{"differences_found"},
            "reference":reference_info,"reference_snapshot":reference,
            "summary":{"differing_positions":differing_positions,"by_kind":counts},
            "repair_details":repair_details,
            "cause":"not_inferred","ownership":"not_proven",
            "server_readiness_proven":false,
            "interpretation":"literal differences from the selected reference; damage, intentional edits, and consequences of another fault are not automatically distinguished"
        }),
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
