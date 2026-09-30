//! Structured review evidence. Serializable history is never validation authority.
use crate::promotion::{CheckKind, CheckStatus, PromotionReport};
use dustroute_library::assembly::AssemblyPortRef;
use dustroute_library::blueprint::{BlueprintRevisionId, InstancePath, TypeRevisionId};
use dustroute_minecraft::time::runtime::{RuntimeTime, TickSection};
use dustroute_minecraft::{Block, BlockKind, Pos};
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum CheckExpectation {
    Known,
    BlockKind {
        block_kind: BlockKind,
    },
    Exact {
        block: Box<Block>,
    },
    Interface {
        port_kind: dustroute_library::blueprint::BlueprintPortKind,
    },
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(tag = "stage", rename_all = "snake_case")]
pub enum ReviewObservation {
    InitialAssembly,
    CommittedRuntimeState { time: ReviewTime },
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct ReviewTime {
    pub game_tick: u64,
    pub section: ReviewSection,
}
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ReviewSection {
    External,
    BlockTicks,
    BlockEvents,
    BlockEntities,
    AfterWorldTick,
}
impl From<RuntimeTime> for ReviewTime {
    fn from(time: RuntimeTime) -> Self {
        Self {
            game_tick: time.game_tick,
            section: match time.section {
                TickSection::External => ReviewSection::External,
                TickSection::BlockTicks => ReviewSection::BlockTicks,
                TickSection::BlockEvents => ReviewSection::BlockEvents,
                TickSection::BlockEntities => ReviewSection::BlockEntities,
                TickSection::AfterWorldTick => ReviewSection::AfterWorldTick,
            },
        }
    }
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct ReviewInput {
    pub position: Pos,
    /// Missing observation is unknown, never OFF.
    pub powered: Option<bool>,
}

#[derive(Clone, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
pub struct CheckEvidence {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub type_revision: Option<TypeRevisionId>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub port: Option<AssemblyPortRef>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub position: Option<Pos>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub expected: Option<CheckExpectation>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub actual: Option<Box<Block>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub observation: Option<ReviewObservation>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub inputs: Vec<ReviewInput>,
    /// Native verifier report, including its available counterexample and graph
    /// closure/budget evidence. Kept as JSON data across verifier report kinds;
    /// never parsed from the human-readable detail or deserialized as a runtime.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub behavior: Option<serde_json::Value>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub binding: Option<dustroute_library::blueprint::BehaviorBinding>,
}

#[derive(Clone, Debug, Serialize)]
pub struct ReviewFinding {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub instance: Option<InstancePath>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub revision: Option<BlueprintRevisionId>,
    pub kind: CheckKind,
    pub status: CheckStatus,
    pub detail: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub evidence: Option<Box<CheckEvidence>>,
}

#[derive(Clone, Debug, Serialize)]
pub struct ReviewDiagnostics {
    pub schema_version: &'static str,
    pub status: CheckStatus,
    pub failed_checks: usize,
    pub undetermined_checks: usize,
    pub total_findings: usize,
    pub omitted_findings: usize,
    pub findings: Vec<ReviewFinding>,
    pub live_world_verified: bool,
}

impl PromotionReport {
    /// Whole report remains in RecordedReview. The bounded repair-oriented view
    /// puts known failures first and explicitly records omitted findings.
    pub fn diagnostics(&self, limit: usize) -> ReviewDiagnostics {
        let mut findings = Vec::new();
        for (path, occurrence) in &self.occurrences {
            for check in &occurrence.checks {
                if check.status != CheckStatus::Passed {
                    findings.push(ReviewFinding {
                        instance: Some(path.clone()),
                        revision: Some(occurrence.revision.clone()),
                        kind: check.kind,
                        status: check.status,
                        detail: check.detail.clone(),
                        evidence: check.evidence.clone(),
                    });
                }
            }
        }
        // Arrangement and behavior may also be indexed on occurrences. Avoid
        // presenting the same physical finding twice under a global heading.
        for check in self.arrangement.iter().chain(&self.behavior) {
            if check.status == CheckStatus::Passed
                || findings.iter().any(|f| {
                    f.kind == check.kind
                        && f.status == check.status
                        && f.detail == check.detail
                        && f.evidence == check.evidence
                })
            {
                continue;
            }
            findings.push(ReviewFinding {
                instance: None,
                revision: None,
                kind: check.kind,
                status: check.status,
                detail: check.detail.clone(),
                evidence: check.evidence.clone(),
            });
        }
        findings.sort_by_key(|f| {
            if f.status == CheckStatus::Failed {
                0
            } else {
                1
            }
        });
        let failed_checks = findings
            .iter()
            .filter(|f| f.status == CheckStatus::Failed)
            .count();
        let undetermined_checks = findings.len() - failed_checks;
        let total_findings = findings.len();
        findings.truncate(limit);
        ReviewDiagnostics {
            schema_version: "dustroute.review-diagnostics.v1",
            status: self.status(),
            failed_checks,
            undetermined_checks,
            total_findings,
            omitted_findings: total_findings - findings.len(),
            findings,
            live_world_verified: false,
        }
    }
}
