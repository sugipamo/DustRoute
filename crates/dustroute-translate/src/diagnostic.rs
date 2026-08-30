//! Stable, presentation-neutral circuit diagnostics for MCP and other clients.

use std::collections::{BTreeMap, BTreeSet};

use dustroute_physical::{
    BlockKind, CapabilityLevel, CapabilityStage, ComponentId, PhysicalScene, Pos,
};
use serde::{Deserialize, Serialize};

use crate::{
    DriveFailure, RequiredInputStatus, SignalSourceKind, analyze_signal_liveness,
    rank_liveness_findings,
};

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum DiagnosticHealth {
    Healthy,
    AwaitingExternalInput,
    Degraded,
    Incomplete,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum CircuitDiagnosticStatus {
    Healthy,
    AwaitingExternalInput,
    ProbableFault,
    Unsupported,
    IncompleteObservation,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum DiagnosticConfidence {
    High,
    Medium,
    Low,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum RecommendedActionKind {
    None,
    ExpandObservation,
    InspectFault,
    ProvideExternalInput,
    ReviewUnsupportedBehavior,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct DiagnosticFinding {
    pub status: CircuitDiagnosticStatus,
    pub confidence: DiagnosticConfidence,
    pub position: Option<Pos>,
    pub component: Option<ComponentId>,
    pub block: Option<BlockKind>,
    pub reason: String,
    pub evidence: Vec<String>,
    pub related_components: BTreeSet<ComponentId>,
    pub inferred: bool,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct RecommendedAction {
    pub kind: RecommendedActionKind,
    pub position: Option<Pos>,
    pub reason: String,
    pub requires_confirmation: bool,
}

#[derive(Clone, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
pub struct DiagnosticCounts {
    pub healthy: usize,
    pub awaiting_external_input: usize,
    pub probable_faults: usize,
    pub unsupported: usize,
    pub incomplete_observation: usize,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct CircuitDiagnosticReport {
    pub health: DiagnosticHealth,
    pub observation_complete: bool,
    pub focus: Option<Pos>,
    pub counts: DiagnosticCounts,
    pub source_counts: BTreeMap<String, usize>,
    pub findings: Vec<DiagnosticFinding>,
    pub recommended_next_action: RecommendedAction,
}

/// Derives a compact diagnostic contract without discarding physical evidence.
/// `analysis_complete` must also account for caller-side discovery limits.
#[must_use]
pub fn diagnose_scene(
    scene: &PhysicalScene,
    focus: Option<Pos>,
    analysis_complete: bool,
) -> CircuitDiagnosticReport {
    let liveness = analyze_signal_liveness(scene);
    let observation_complete = analysis_complete && scene.observation.is_complete();
    let mut findings = Vec::new();

    if !observation_complete {
        findings.push(DiagnosticFinding {
            status: CircuitDiagnosticStatus::IncompleteObservation,
            confidence: DiagnosticConfidence::High,
            position: focus,
            component: focus.and_then(|position| scene.component_at(position).map(|item| item.id)),
            block: focus
                .and_then(|position| scene.component_at(position).map(|item| item.block.kind)),
            reason: "the observed circuit continues beyond the available scan".to_owned(),
            evidence: vec!["analysis_complete=false".to_owned()],
            related_components: BTreeSet::new(),
            inferred: false,
        });
    }

    for assessment in &liveness.required_input_assessments {
        match assessment.status {
            RequiredInputStatus::DrivenByKnownSource => {}
            RequiredInputStatus::AwaitingExternalInput => findings.push(DiagnosticFinding {
                status: CircuitDiagnosticStatus::AwaitingExternalInput,
                confidence: DiagnosticConfidence::Medium,
                position: Some(assessment.position),
                component: Some(assessment.device),
                block: Some(assessment.block),
                reason: "required input is reachable from an inferred primary input".to_owned(),
                evidence: vec!["inferred_primary_input_path".to_owned()],
                related_components: assessment.inferred_primary_inputs.clone(),
                inferred: true,
            }),
            RequiredInputStatus::Disconnected | RequiredInputStatus::NoKnownSource => {
                let (confidence, reason, evidence) = match assessment.status {
                    RequiredInputStatus::Disconnected => (
                        DiagnosticConfidence::High,
                        "required directional input has no physical connection",
                        "disconnected_required_input",
                    ),
                    RequiredInputStatus::NoKnownSource => (
                        DiagnosticConfidence::Medium,
                        "connected input has no reachable known or inferred signal source",
                        "no_reachable_driver",
                    ),
                    _ => unreachable!(),
                };
                findings.push(DiagnosticFinding {
                    status: CircuitDiagnosticStatus::ProbableFault,
                    confidence,
                    position: Some(assessment.position),
                    component: Some(assessment.device),
                    block: Some(assessment.block),
                    reason: reason.to_owned(),
                    evidence: vec![evidence.to_owned()],
                    related_components: assessment.immediate_sources.clone(),
                    inferred: false,
                });
            }
        }
    }

    for issue in scene
        .capability_report()
        .issues
        .into_iter()
        .filter(|issue| issue.level == CapabilityLevel::Unsupported)
    {
        findings.push(DiagnosticFinding {
            status: CircuitDiagnosticStatus::Unsupported,
            confidence: DiagnosticConfidence::High,
            position: Some(issue.position),
            component: Some(issue.component),
            block: Some(issue.kind),
            reason: format!(
                "{:?} support is {:?} for this block",
                issue.stage, issue.level
            )
            .to_lowercase(),
            evidence: vec![capability_evidence(issue.stage, issue.level)],
            related_components: BTreeSet::new(),
            inferred: false,
        });
    }

    findings.sort_by_key(|finding| {
        let distance = focus
            .zip(finding.position)
            .map(|(focus, position)| manhattan(focus, position))
            .unwrap_or(u32::MAX);
        (status_priority(finding.status), distance, finding.position)
    });

    let healthy = liveness
        .required_input_assessments
        .iter()
        .filter(|assessment| assessment.status == RequiredInputStatus::DrivenByKnownSource)
        .count();
    let counts = DiagnosticCounts {
        healthy,
        awaiting_external_input: count_status(
            &findings,
            CircuitDiagnosticStatus::AwaitingExternalInput,
        ),
        probable_faults: count_status(&findings, CircuitDiagnosticStatus::ProbableFault),
        unsupported: count_status(&findings, CircuitDiagnosticStatus::Unsupported),
        incomplete_observation: count_status(
            &findings,
            CircuitDiagnosticStatus::IncompleteObservation,
        ),
    };
    let health = if !observation_complete {
        DiagnosticHealth::Incomplete
    } else if counts.probable_faults > 0 || counts.unsupported > 0 {
        DiagnosticHealth::Degraded
    } else if counts.awaiting_external_input > 0 {
        DiagnosticHealth::AwaitingExternalInput
    } else {
        DiagnosticHealth::Healthy
    };
    let source_counts =
        liveness
            .sources
            .iter()
            .fold(BTreeMap::<String, usize>::new(), |mut counts, source| {
                *counts
                    .entry(source_kind_name(source.kind).to_owned())
                    .or_default() += 1;
                counts
            });
    let recommended_next_action =
        recommend_action(scene, focus, observation_complete, &liveness, &findings);

    CircuitDiagnosticReport {
        health,
        observation_complete,
        focus,
        counts,
        source_counts,
        findings,
        recommended_next_action,
    }
}

fn recommend_action(
    scene: &PhysicalScene,
    focus: Option<Pos>,
    observation_complete: bool,
    liveness: &crate::SignalLivenessReport,
    findings: &[DiagnosticFinding],
) -> RecommendedAction {
    if !observation_complete {
        return RecommendedAction {
            kind: RecommendedActionKind::ExpandObservation,
            position: focus,
            reason: "load more connected components before making a higher-level claim".to_owned(),
            requires_confirmation: false,
        };
    }
    if let Some(ranked) =
        focus
            .map(|focus| rank_liveness_findings(scene, liveness, focus))
            .and_then(|ranked| ranked.into_iter().next())
            .or_else(|| {
                liveness.undriven_inputs.first().cloned().map(|finding| {
                    crate::RankedLivenessFinding {
                        finding,
                        manhattan_distance_from_focus: 0,
                        downstream_component_count: 0,
                        nearby_gap_candidate_count: 0,
                        suspicion_score: 0,
                    }
                })
            })
    {
        return RecommendedAction {
            kind: RecommendedActionKind::InspectFault,
            position: Some(ranked.finding.position),
            reason: match ranked.finding.failure {
                DriveFailure::DisconnectedRequiredInput => {
                    "inspect the disconnected required input, then request a repair preview"
                }
                DriveFailure::NoReachableDriver => {
                    "inspect the upstream path before deciding whether a repair is appropriate"
                }
            }
            .to_owned(),
            requires_confirmation: false,
        };
    }
    if let Some(finding) = findings
        .iter()
        .find(|finding| finding.status == CircuitDiagnosticStatus::Unsupported)
    {
        return RecommendedAction {
            kind: RecommendedActionKind::ReviewUnsupportedBehavior,
            position: finding.position,
            reason: "review unsupported semantics before relying on the logical interpretation"
                .to_owned(),
            requires_confirmation: false,
        };
    }
    if let Some(finding) = findings
        .iter()
        .find(|finding| finding.status == CircuitDiagnosticStatus::AwaitingExternalInput)
    {
        return RecommendedAction {
            kind: RecommendedActionKind::ProvideExternalInput,
            position: finding.position,
            reason: "identify or operate the intended external input before diagnosing a fault"
                .to_owned(),
            requires_confirmation: false,
        };
    }
    RecommendedAction {
        kind: RecommendedActionKind::None,
        position: focus,
        reason: "no actionable fault was found in the observed circuit".to_owned(),
        requires_confirmation: false,
    }
}

fn count_status(findings: &[DiagnosticFinding], status: CircuitDiagnosticStatus) -> usize {
    findings
        .iter()
        .filter(|finding| finding.status == status)
        .count()
}

const fn status_priority(status: CircuitDiagnosticStatus) -> u8 {
    match status {
        CircuitDiagnosticStatus::IncompleteObservation => 0,
        CircuitDiagnosticStatus::ProbableFault => 1,
        CircuitDiagnosticStatus::Unsupported => 2,
        CircuitDiagnosticStatus::AwaitingExternalInput => 3,
        CircuitDiagnosticStatus::Healthy => 4,
    }
}

const fn source_kind_name(kind: SignalSourceKind) -> &'static str {
    match kind {
        SignalSourceKind::ControllableInput => "controllable_input",
        SignalSourceKind::IntrinsicSource => "intrinsic_source",
        SignalSourceKind::ObservationBoundary => "observation_boundary",
        SignalSourceKind::InferredPrimaryInput => "inferred_primary_input",
    }
}

fn capability_evidence(stage: CapabilityStage, level: CapabilityLevel) -> String {
    format!("capability:{stage:?}:{level:?}").to_lowercase()
}

fn manhattan(left: Pos, right: Pos) -> u32 {
    left.x.abs_diff(right.x) + left.y.abs_diff(right.y) + left.z.abs_diff(right.z)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Block, Facing, RegionBounds, World, analyze_world_region};

    #[test]
    fn separates_an_external_input_from_a_probable_fault() {
        let mut world = World::new();
        for x in 0..=4 {
            world.set(Pos::new(x, 0, 0), Block::new(BlockKind::Solid));
        }
        world.set(Pos::new(1, 1, 0), Block::new(BlockKind::RedstoneWire));
        let mut externally_driven = Block::new(BlockKind::Repeater);
        externally_driven.facing = Some(Facing::East);
        world.set(Pos::new(2, 1, 0), externally_driven);
        let mut disconnected = Block::new(BlockKind::Repeater);
        disconnected.facing = Some(Facing::East);
        world.set(Pos::new(4, 1, 0), disconnected);
        let analysis = analyze_world_region(
            &world,
            RegionBounds::new(Pos::new(-1, -1, -1), Pos::new(5, 2, 1)),
        );
        let report = diagnose_scene(&analysis.scene, Some(Pos::new(4, 1, 0)), true);

        assert_eq!(report.counts.awaiting_external_input, 1);
        assert_eq!(report.counts.probable_faults, 1);
        assert_eq!(report.health, DiagnosticHealth::Degraded);
        assert_eq!(
            report.recommended_next_action.kind,
            RecommendedActionKind::InspectFault
        );
        assert_eq!(
            report.recommended_next_action.position,
            Some(Pos::new(4, 1, 0))
        );
    }

    #[test]
    fn incomplete_observation_takes_priority_over_repair_advice() {
        let world = World::new();
        let analysis = analyze_world_region(
            &world,
            RegionBounds::new(Pos::new(0, 0, 0), Pos::new(1, 1, 1)),
        );
        let report = diagnose_scene(&analysis.scene, Some(Pos::new(0, 0, 0)), false);
        assert_eq!(report.health, DiagnosticHealth::Incomplete);
        assert_eq!(
            report.recommended_next_action.kind,
            RecommendedActionKind::ExpandObservation
        );
    }
}
