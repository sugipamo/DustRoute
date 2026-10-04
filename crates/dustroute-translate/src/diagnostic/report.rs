//! Shared findings and repair handoff. Reports are evidence, never capabilities.
use std::collections::{BTreeMap, BTreeSet};

use dustroute_library::assembly::AssemblyView;
use dustroute_library::blueprint::{AssemblyRevisionId, BlueprintOccurrence};
use serde::{Deserialize, Serialize};

use super::ConnectivityFinding;
use super::difference::SnapshotDifference;
use crate::world::Pos;

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum DiagnosticMethod {
    Connectivity,
    DesignComparison,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(tag = "basis", rename_all = "snake_case")]
pub enum FindingEvidence {
    Connectivity(ConnectivityFinding),
    DesignComparison(SnapshotDifference),
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct Finding {
    /// Local to this report; never reuse as a world or component identity.
    pub finding_id: String,
    #[serde(flatten)]
    pub evidence: FindingEvidence,
    pub design_occurrences: Vec<BlueprintOccurrence>,
}
impl Finding {
    pub fn position(&self) -> Option<Pos> {
        match &self.evidence {
            FindingEvidence::Connectivity(f) => f.position,
            FindingEvidence::DesignComparison(f) => Some(f.position),
        }
    }
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum RepairStrategy {
    PartialPatch,
    TeardownAndRebuild,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum RepairStatus {
    NotAssessed,
    PlanAvailable,
    NoCandidate,
    Blocked,
    NotNeededForReferenceMatch,
    RequiresNewPlacement,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct RepairAssessment {
    pub status: RepairStatus,
    pub strategy: RepairStrategy,
    pub reason: Option<String>,
    /// Findings to inspect with the plan; not proof that every finding is fixed.
    pub finding_ids: Vec<String>,
    pub review_required: bool,
    pub permission_granted: bool,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct DesignReference {
    pub assembly_revision_id: AssemblyRevisionId,
    pub fresh_review_passed: bool,
    pub occurrences: Vec<BlueprintOccurrence>,
    pub membership_basis: String,
}

/// Indexes declarations, not moved-payload ownership or a runtime history.
#[derive(Clone, Debug, Default)]
pub struct DesignIndex {
    occurrences: Vec<BlueprintOccurrence>,
    membership: BTreeMap<Pos, Vec<BlueprintOccurrence>>,
}
impl DesignIndex {
    pub fn from_view(view: &AssemblyView) -> Self {
        Self {
            occurrences: view.occurrences.values().cloned().collect(),
            membership: view
                .membership
                .iter()
                .map(|(p, paths)| {
                    (
                        *p,
                        paths
                            .iter()
                            .filter_map(|path| view.occurrences.get(path).cloned())
                            .collect(),
                    )
                })
                .collect(),
        }
    }
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct Diagnosis {
    pub schema: String,
    pub method: DiagnosticMethod,
    pub findings: Vec<Finding>,
    pub repair: RepairAssessment,
    pub design: Option<DesignReference>,
    pub world_writes: bool,
    pub functional_test_performed: bool,
}
impl Diagnosis {
    pub fn connectivity(findings: Vec<ConnectivityFinding>) -> Self {
        Self::new(
            DiagnosticMethod::Connectivity,
            RepairStrategy::PartialPatch,
            findings
                .into_iter()
                .map(FindingEvidence::Connectivity)
                .collect(),
        )
    }
    pub fn design_comparison(findings: Vec<SnapshotDifference>) -> Self {
        Self::new(
            DiagnosticMethod::DesignComparison,
            RepairStrategy::TeardownAndRebuild,
            findings
                .into_iter()
                .map(FindingEvidence::DesignComparison)
                .collect(),
        )
    }
    fn new(
        method: DiagnosticMethod,
        strategy: RepairStrategy,
        evidence: Vec<FindingEvidence>,
    ) -> Self {
        Self {
            schema: "dustroute.diagnosis.v1".into(),
            method,
            findings: evidence
                .into_iter()
                .enumerate()
                .map(|(i, evidence)| Finding {
                    finding_id: format!("finding-{i}"),
                    evidence,
                    design_occurrences: vec![],
                })
                .collect(),
            repair: RepairAssessment {
                status: RepairStatus::NotAssessed,
                strategy,
                reason: None,
                finding_ids: vec![],
                review_required: true,
                permission_granted: false,
            },
            design: None,
            world_writes: false,
            functional_test_performed: false,
        }
    }
    pub fn assess_repair(&mut self, status: RepairStatus, reason: Option<String>) {
        self.repair.status = status;
        self.repair.reason = reason;
        self.repair.finding_ids = self.findings.iter().map(|f| f.finding_id.clone()).collect();
    }
    pub fn findings_at(&self, positions: impl IntoIterator<Item = Pos>) -> Vec<String> {
        let positions: BTreeSet<_> = positions.into_iter().collect();
        self.findings
            .iter()
            .filter(|f| f.position().is_some_and(|p| positions.contains(&p)))
            .map(|f| f.finding_id.clone())
            .collect()
    }
    pub fn findings_for_repair(
        &self,
        proposal: &dustroute_physical::RepairProposal,
        scene: &dustroute_physical::PhysicalScene,
    ) -> Vec<String> {
        use dustroute_physical::GapEvidence;
        let mut positions: BTreeSet<_> = proposal.patch.changes.iter().map(|c| c.pos).collect();
        let mut components = BTreeSet::new();
        for evidence in &proposal.evidence {
            match evidence {
                GapEvidence::Nearby { left, right, .. } => {
                    components.extend([*left, *right]);
                }
                GapEvidence::MissingInlineBlock { position } => {
                    positions.insert(*position);
                }
                GapEvidence::InvalidSupport {
                    component,
                    expected_support,
                } => {
                    components.insert(*component);
                    positions.insert(*expected_support);
                }
                GapEvidence::DirectionMismatch { component, toward } => {
                    components.extend([*component, *toward]);
                }
                GapEvidence::SuspectedUnexpectedConnection { component } => {
                    components.insert(*component);
                }
            }
        }
        positions.extend(
            scene
                .components
                .iter()
                .filter(|c| components.contains(&c.id))
                .map(|c| c.pos),
        );
        self.findings_at(positions)
    }
    pub fn attach_design(
        &mut self,
        id: AssemblyRevisionId,
        index: Option<&DesignIndex>,
        fresh_review_passed: bool,
    ) {
        for finding in &mut self.findings {
            finding.design_occurrences = index
                .and_then(|i| finding.position().and_then(|p| i.membership.get(&p)))
                .cloned()
                .unwrap_or_default();
        }
        self.design = Some(DesignReference {
            assembly_revision_id: id,
            fresh_review_passed,
            occurrences: index.map(|i| i.occurrences.clone()).unwrap_or_default(),
            membership_basis:
                "declared source coordinates; not runtime payload tracking or ownership".into(),
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::diagnostic::difference::differences;
    use crate::{cells::RotationY, snapshot::MinecraftSnapshot, snapshot::MinecraftSnapshotBlock};
    use dustroute_library::assembly::Assembly;
    use dustroute_library::blueprint::{BlueprintInclusion, BlueprintRevisionId, InstanceId};
    use dustroute_library::builtin_blueprints::{NOT_TOP_REVISION, builtin_blueprints};

    #[test]
    fn nested_overlapping_design_references_survive_rotation_and_never_rebind_sources() {
        let mut catalog = builtin_blueprints().clone();
        let source_id = BlueprintRevisionId::new(NOT_TOP_REVISION).unwrap();
        let mut parent = catalog.revision(&source_id).unwrap().clone();
        parent.id = BlueprintRevisionId::new("diagnostic-parent.v1").unwrap();
        parent.blocks.clear();
        parent.inclusions = ["left", "right"]
            .into_iter()
            .map(|name| BlueprintInclusion {
                instance: InstanceId::new(name).unwrap(),
                revision: source_id.clone(),
                origin: Pos::default(),
                rotation: RotationY::R0,
            })
            .collect();
        let parent_id = parent.id.clone();
        catalog.insert_revision(parent).unwrap();
        let before = catalog.clone();
        let origin = Pos::new(-90, 180, 1000);
        let assembly = Assembly {
            name: "Diagnostic source links".into(),
            instances: vec![BlueprintInclusion {
                instance: InstanceId::new("root").unwrap(),
                revision: parent_id,
                origin,
                rotation: RotationY::R90,
            }],
            blocks: vec![],
            known_regions: vec![],
            connections: vec![],
            boundaries: vec![],
        };
        let view = assembly.inspect(&catalog).unwrap();
        let position = Pos::new(-90, 181, 1001); // rotated source (1,1,0)
        assert_eq!(view.membership[&position].len(), 3);
        let target = MinecraftSnapshot {
            min: position,
            max: position,
            blocks: vec![MinecraftSnapshotBlock {
                pos: position,
                name: "minecraft:stone".into(),
                properties: BTreeMap::new(),
            }],
        };
        let observed = MinecraftSnapshot {
            blocks: vec![],
            ..target.clone()
        };
        let mut report = Diagnosis::design_comparison(differences(&observed, &target).unwrap());
        report.attach_design(
            AssemblyRevisionId::new("diagnostic-state.v1").unwrap(),
            Some(&DesignIndex::from_view(&view)),
            false,
        );
        report.assess_repair(
            RepairStatus::Blocked,
            Some("source links are not validation".into()),
        );
        let finding = &report.findings[0];
        assert_eq!(finding.position(), Some(position));
        assert_eq!(finding.design_occurrences.len(), 3);
        assert_eq!(
            finding
                .design_occurrences
                .iter()
                .filter(|o| o.path.len() == 2 && o.revision == source_id)
                .count(),
            2
        );
        assert!(
            finding
                .design_occurrences
                .iter()
                .all(|o| o.origin == origin && o.rotation == RotationY::R90)
        );
        let FindingEvidence::DesignComparison(diff) = &finding.evidence else {
            panic!("design evidence")
        };
        // The supplied Assembly reference remains authoritative, even though
        // the Blueprint's source block at this position has a different kind.
        assert_eq!(diff.target.as_ref().unwrap().name, "minecraft:stone");
        assert!(!report.design.as_ref().unwrap().fresh_review_passed);
        assert_eq!(report.repair.finding_ids, vec![finding.finding_id.clone()]);
        let restored: Diagnosis =
            serde_json::from_str(&serde_json::to_string(&report).unwrap()).unwrap();
        assert_eq!(restored, report);
        assert_eq!(catalog, before);
        report.attach_design(
            AssemblyRevisionId::new("diagnostic-state.v1").unwrap(),
            None,
            false,
        );
        assert!(report.findings[0].design_occurrences.is_empty());
        assert!(!report.repair.permission_granted);
    }
}
