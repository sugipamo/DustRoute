//! Durable diagnosis, deliberately unable to recreate native operation authority.
use super::diagnostic::{DiagnosticPayload, RecordedExecutionPlan};
use super::{ExecutionError, Result, SurvivalErrorCode};
use crate::storage::{self, Durability};
use fs2::FileExt;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::{
    fs::{self, File, OpenOptions},
    io::Read,
    path::{Path, PathBuf},
};

const MAX_BYTES: usize = 16 * 1024 * 1024;
const RESERVE: usize = 256 * 1024;

/// Named lifecycle stages; opaque diagnostic payloads never decide a transition.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ExecutionPhase {
    Cancelled,
    CheckpointRefused,
    Completed,
    FreshReconnect,
    IdleCheckpoint,
    InventoryReceived,
    InventorySwap,
    MiningAim,
    MiningFinishSend,
    MiningOutcome,
    MiningSelectEmpty,
    MiningStartSend,
    MiningStarted,
    MovementPredicted,
    MovementPredictionMismatch,
    MovementSend,
    Observed,
    PlacementAim,
    PlacementObserved,
    PlacementSend,
    ReconnectBoundaryVerified,
    Recovered,
    Removed,
    RetireSource,
    RetryImprovement,
    SelectMaterial,
    StepCompleted,
    Stopped,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum JournalSchema {
    #[serde(rename = "dustroute.survival-execution.v1")]
    V1,
    #[serde(other)]
    Unknown,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum OperationOutcome {
    NotStarted,
    Uncertain,
    Pending,
    Observed,
    /// Fully dispatched, model-based motion; no received endpoint is implied.
    Predicted,
}

/// Saved readiness is historical evidence, never authority after reopening.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Continuation {
    Revalidate,
    AwaitMining,
    Replan,
    NeedsInspection,
    Completed,
    Cancelled,
    Checkpoint,
}

#[derive(Clone, Debug, Serialize)]
pub struct ExecutionEvent {
    pub step: usize,
    pub phase: ExecutionPhase,
    pub outcome: OperationOutcome,
    pub continuation: Continuation,
    pub evidence: ExecutionEvidence,
}

/// Payloads used for validation have Rust types; other native receipts are opaque.
#[derive(Clone, Debug, Serialize)]
#[serde(untagged)]
pub enum ExecutionEvidence {
    Checkpoint(Box<super::checkpoint::SafeCheckpoint>),
    Diagnostic(DiagnosticPayload),
}
impl From<Value> for ExecutionEvidence {
    fn from(value: Value) -> Self {
        Self::Diagnostic(value.into())
    }
}
impl From<super::checkpoint::SafeCheckpoint> for ExecutionEvidence {
    fn from(value: super::checkpoint::SafeCheckpoint) -> Self {
        Self::Checkpoint(Box::new(value))
    }
}
impl<'de> Deserialize<'de> for ExecutionEvent {
    fn deserialize<D: serde::Deserializer<'de>>(decoder: D) -> std::result::Result<Self, D::Error> {
        // Wire decoding is the only place an opaque checkpoint becomes typed data.
        #[derive(Deserialize)]
        #[serde(deny_unknown_fields)]
        struct Wire {
            step: usize,
            phase: ExecutionPhase,
            outcome: OperationOutcome,
            continuation: Continuation,
            evidence: Value,
        }
        let wire = Wire::deserialize(decoder)?;
        let evidence = match wire.phase {
            ExecutionPhase::IdleCheckpoint => ExecutionEvidence::Checkpoint(Box::new(
                serde_json::from_value(wire.evidence).map_err(serde::de::Error::custom)?,
            )),
            _ => ExecutionEvidence::Diagnostic(wire.evidence.into()),
        };
        Ok(Self {
            step: wire.step,
            phase: wire.phase,
            outcome: wire.outcome,
            continuation: wire.continuation,
            evidence,
        })
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ExecutionRecord {
    pub schema: JournalSchema,
    pub id: uuid::Uuid,
    /// Diagnostic plan, not a deserializable HypotheticalConstructionPlan.
    pub plan: RecordedExecutionPlan,
    pub completed_steps: usize,
    pub outcome: OperationOutcome,
    pub continuation: Continuation,
    pub reconnects: usize,
    pub events: Vec<ExecutionEvent>,
}

/// Opening a record never resumes a live executor, including a previously ready one.
#[derive(Debug, Serialize)]
pub struct ExecutionDiagnosis {
    pub record: ExecutionRecord,
    pub continuation: Continuation,
    pub explanation: &'static str,
}

pub fn diagnose(directory: &Path) -> Result<ExecutionDiagnosis> {
    let mut bytes = Vec::new();
    File::open(directory.join("record.json"))?
        .take((MAX_BYTES + 1) as u64)
        .read_to_end(&mut bytes)?;
    if bytes.len() > MAX_BYTES {
        return Err(ExecutionError::new(
            SurvivalErrorCode::JournalTooLarge,
            "record exceeds bound",
        ));
    }
    let record: ExecutionRecord = serde_json::from_slice(&bytes)?;
    if record.schema != JournalSchema::V1 {
        return Err(ExecutionError::new(
            SurvivalErrorCode::JournalSchema,
            "unknown survival journal",
        ));
    }
    Ok(ExecutionDiagnosis {
        record,
        continuation: Continuation::NeedsInspection,
        explanation: "Historical evidence only. Reobserve world/inventory and retire any old operation; native authority is never restored from this file.",
    })
}

pub(super) struct Journal {
    _lock: File,
    path: PathBuf,
    pub record: ExecutionRecord,
}
impl Journal {
    pub fn create(directory: &Path, plan: RecordedExecutionPlan) -> Result<Self> {
        let mut builder = fs::DirBuilder::new();
        builder.recursive(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::DirBuilderExt;
            builder.mode(0o700);
        }
        builder.create(directory)?;
        let mut options = OpenOptions::new();
        options.read(true).write(true).create(true).truncate(false);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            options.mode(0o600);
        }
        let lock = options.open(directory.join("executor.lock"))?;
        lock.try_lock_exclusive()
            .map_err(|e| ExecutionError::new(SurvivalErrorCode::ExecutionBusy, e))?;
        let path = directory.join("record.json");
        if path.try_exists()? {
            return Err(ExecutionError::new(
                SurvivalErrorCode::ExecutionExists,
                "existing record requires diagnosis; it cannot be overwritten or resumed",
            ));
        }
        let this = Self {
            _lock: lock,
            path,
            record: ExecutionRecord {
                schema: JournalSchema::V1,
                id: uuid::Uuid::new_v4(),
                plan,
                completed_steps: 0,
                outcome: OperationOutcome::NotStarted,
                continuation: Continuation::Revalidate,
                reconnects: 0,
                events: Vec::new(),
            },
        };
        this.save(true)?;
        Ok(this)
    }
    fn save(&self, reserve: bool) -> Result<()> {
        let bytes = serde_json::to_vec(&self.record)?;
        if bytes.len() > MAX_BYTES - if reserve { RESERVE } else { 0 } {
            return Err(ExecutionError::new(
                SurvivalErrorCode::JournalTooLarge,
                "no durable room for another action/outcome",
            ));
        }
        storage::replace(&self.path, &bytes, Durability::FileAndDirectory)?;
        Ok(())
    }
    pub fn event(
        &mut self,
        phase: ExecutionPhase,
        outcome: OperationOutcome,
        continuation: Continuation,
        evidence: impl Into<ExecutionEvidence>,
    ) -> Result<()> {
        self.append(phase, outcome, continuation, evidence.into());
        self.save(false)
    }
    fn append(
        &mut self,
        phase: ExecutionPhase,
        outcome: OperationOutcome,
        continuation: Continuation,
        evidence: ExecutionEvidence,
    ) {
        self.record.outcome = outcome.clone();
        self.record.continuation = continuation.clone();
        self.record.events.push(ExecutionEvent {
            step: self.record.completed_steps,
            phase,
            outcome,
            continuation,
            evidence,
        });
    }
    /// Must succeed before *each* mutating call, including inventory/look/reconnect.
    pub fn intend(&mut self, phase: ExecutionPhase, evidence: Value) -> Result<()> {
        self.append(
            phase,
            OperationOutcome::Uncertain,
            Continuation::NeedsInspection,
            evidence.into(),
        );
        self.save(true)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    struct Directory(PathBuf);
    impl Directory {
        fn new() -> Self {
            Self(std::env::temp_dir().join(format!(
                "dustroute-survival-journal-{}",
                uuid::Uuid::new_v4()
            )))
        }
    }
    impl Drop for Directory {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }
    #[test]
    #[ignore = "read-only fresh-process check of an explicitly selected retained live journal"]
    fn retained_live_record_is_diagnosis_only_in_a_fresh_process() {
        let directory = PathBuf::from(std::env::var("DUSTROUTE_SURVIVAL_REOPEN_JOURNAL").unwrap());
        let before = fs::read(directory.join("record.json")).unwrap();
        let diagnosis = diagnose(&directory).unwrap();
        assert!(!diagnosis.record.events.is_empty());
        assert_eq!(diagnosis.continuation, Continuation::NeedsInspection);
        assert!(
            matches!(Journal::create(&directory,RecordedExecutionPlan::default()),Err(e) if e.code == SurvivalErrorCode::ExecutionExists)
        );
        assert_eq!(fs::read(directory.join("record.json")).unwrap(), before);
        println!(
            "reopened {}: historical {:?}, effective diagnosis {:?}; no native client created",
            diagnosis.record.id, diagnosis.record.continuation, diagnosis.continuation
        );
    }
    #[test]
    fn restart_retains_uncertain_intent_without_replaying_or_replacing_it() {
        let d = Directory::new();
        let mut j = Journal::create(&d.0, RecordedExecutionPlan::default()).unwrap();
        j.intend(ExecutionPhase::MiningStartSend, json!({"target":[1,2,3]}))
            .unwrap();
        let id = j.record.id;
        drop(j);
        let diagnosis = diagnose(&d.0).unwrap();
        assert_eq!(diagnosis.record.id, id);
        assert_eq!(diagnosis.record.outcome, OperationOutcome::Uncertain);
        assert_eq!(diagnosis.continuation, Continuation::NeedsInspection);
        assert_eq!(
            json!(diagnosis.record.events[0].evidence)["target"],
            json!([1, 2, 3])
        );
        assert!(
            matches!(Journal::create(&d.0,RecordedExecutionPlan::default()),Err(e) if e.code == SurvivalErrorCode::ExecutionExists)
        );
    }
    #[test]
    fn only_one_writer_and_even_historical_readiness_cannot_resume() {
        let d = Directory::new();
        let j = Journal::create(&d.0, RecordedExecutionPlan::default()).unwrap();
        assert!(
            matches!(Journal::create(&d.0,RecordedExecutionPlan::default()),Err(e) if e.code == SurvivalErrorCode::ExecutionBusy)
        );
        let r = diagnose(&d.0).unwrap();
        assert_eq!(r.record.continuation, Continuation::Revalidate);
        assert_eq!(r.continuation, Continuation::NeedsInspection);
        drop(j);
    }
    #[test]
    fn failed_outcome_save_keeps_the_pre_dispatch_intent_on_disk() {
        let d = Directory::new();
        let mut j = Journal::create(&d.0, RecordedExecutionPlan::default()).unwrap();
        j.intend(ExecutionPhase::PlacementSend, json!({"target":[1,2,3]}))
            .unwrap();
        j.path = d.0.join("missing-directory/record.json");
        assert!(
            j.event(
                ExecutionPhase::Observed,
                OperationOutcome::Observed,
                Continuation::Revalidate,
                json!({})
            )
            .is_err()
        );
        let r = diagnose(&d.0).unwrap();
        assert_eq!(r.record.outcome, OperationOutcome::Uncertain);
        assert_eq!(r.record.events.len(), 1);
    }
    #[test]
    fn result_and_continuation_are_independent_and_cancellation_is_retained() {
        let d = Directory::new();
        let mut j = Journal::create(&d.0, RecordedExecutionPlan::default()).unwrap();
        j.event(
            ExecutionPhase::Removed,
            OperationOutcome::Observed,
            Continuation::NeedsInspection,
            json!({"retirement_pending":true}),
        )
        .unwrap();
        let r = diagnose(&d.0).unwrap().record;
        assert_eq!(r.outcome, OperationOutcome::Observed);
        assert_eq!(r.continuation, Continuation::NeedsInspection);
        j.event(
            ExecutionPhase::Cancelled,
            OperationOutcome::Observed,
            Continuation::Cancelled,
            json!({}),
        )
        .unwrap();
        drop(j);
        assert_eq!(
            diagnose(&d.0).unwrap().record.continuation,
            Continuation::Cancelled
        );
    }
    #[test]
    fn unknown_or_oversized_records_are_not_accepted() {
        let d = Directory::new();
        let mut j = Journal::create(&d.0, RecordedExecutionPlan::default()).unwrap();
        j.record.schema = JournalSchema::Unknown;
        j.save(false).unwrap();
        assert_eq!(
            diagnose(&d.0).unwrap_err().code,
            SurvivalErrorCode::JournalSchema
        );
        let f = File::create(d.0.join("record.json")).unwrap();
        f.set_len((MAX_BYTES + 1) as u64).unwrap();
        assert_eq!(
            diagnose(&d.0).unwrap_err().code,
            SurvivalErrorCode::JournalTooLarge
        );
    }

    #[test]
    fn unknown_phase_and_untyped_checkpoint_cannot_supply_a_saved_boundary() {
        let d = Directory::new();
        let mut journal = Journal::create(&d.0, RecordedExecutionPlan::default()).unwrap();
        journal
            .intend(ExecutionPhase::MiningStartSend, json!({"target":[1,2,3]}))
            .unwrap();
        let mut wire = json!(journal.record);
        drop(journal);
        for phase in ["future_observed", "idle_checkpoint"] {
            wire["events"][0]["phase"] = json!(phase);
            fs::write(d.0.join("record.json"), serde_json::to_vec(&wire).unwrap()).unwrap();
            assert_eq!(
                diagnose(&d.0).unwrap_err().code,
                SurvivalErrorCode::JournalEncoding
            );
            assert_eq!(
                fs::read(d.0.join("record.json")).unwrap(),
                serde_json::to_vec(&wire).unwrap()
            );
        }
    }
}
