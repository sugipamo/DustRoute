use dustroute_ir::{HierarchicalIr, RecognizedGate};
use dustroute_physical::{
    BlockCapabilities, BlockCapabilityGroup, BlockKind, CapabilityIssue, CapabilityLevel,
    CapabilityStage, ComponentId, PhysicalScene, Pos,
};
use dustroute_translate::analysis::LocalSignalRole;
use dustroute_translate::liveness::{
    RankedLivenessFinding, RequiredInputAssessment, RequiredInputStatus, SignalSource,
    SignalSourceKind, UndrivenInput,
};
use serde::{Serialize, Serializer, ser::SerializeMap};
use std::collections::{BTreeMap, BTreeSet};

#[derive(Clone, Debug, PartialEq, Serialize)]
pub(crate) struct FocusedComponent {
    position: Pos,
    block: Option<BlockKind>,
    observed_name: Option<String>,
    observed_properties: Option<BTreeMap<String, String>>,
    capabilities: Option<BlockCapabilities>,
    physical_component: Option<ComponentId>,
    recognized_gates: Vec<RecognizedGate>,
    #[serde(skip_serializing_if = "Option::is_none")]
    signal_component: Option<usize>,
    #[serde(skip_serializing_if = "Option::is_none")]
    incoming_components: Option<BTreeSet<usize>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    outgoing_components: Option<BTreeSet<usize>>,
    role: LocalSignalRole,
}
pub(crate) fn focused_component(
    translated: &dustroute_translate::api::ReverseResult,
    target: Pos,
) -> FocusedComponent {
    let focused = dustroute_translate::analysis::classify_focused_role(translated, target);
    let component = translated.analysis.scene.component_at(target);
    let signal = focused.signal_component.is_some();
    FocusedComponent {
        position: target,
        block: component.map(|c| c.block.kind),
        observed_name: component.and_then(|c| c.block.observed_name.clone()),
        observed_properties: component.map(|c| c.block.observed_properties.clone()),
        capabilities: component.map(|c| c.block.capabilities()),
        physical_component: component.map(|c| c.id),
        recognized_gates: component
            .map(|c| {
                translated
                    .gate_view
                    .gates
                    .iter()
                    .filter(|gate| gate.physical_components.contains(&c.id))
                    .cloned()
                    .collect()
            })
            .unwrap_or_default(),
        signal_component: focused.signal_component,
        incoming_components: signal.then_some(focused.incoming_components),
        outgoing_components: signal.then_some(focused.outgoing_components),
        role: if signal {
            focused.role
        } else {
            LocalSignalRole::SupportOrUnresolved
        },
    }
}
#[derive(Clone, Debug, PartialEq, Serialize)]
pub(crate) struct FocusedHierarchy {
    position: Pos,
    role: HierarchyRole,
    recognized_cells: Vec<RecognizedGate>,
    #[serde(flatten)]
    component: Option<HierarchyComponent>,
}
#[derive(Clone, Debug, PartialEq, Serialize)]
struct HierarchyComponent {
    block: BlockKind,
    observed_name: Option<String>,
    observed_properties: BTreeMap<String, String>,
    capabilities: BlockCapabilities,
    physical_component: ComponentId,
    incoming_components: BTreeSet<ComponentId>,
    outgoing_components: BTreeSet<ComponentId>,
    physical_origin: Option<Pos>,
}
pub(crate) fn focused_hierarchy(
    scene: &PhysicalScene,
    hierarchy: &HierarchicalIr,
    target: Pos,
) -> FocusedHierarchy {
    let Some(component) = scene.component_at(target) else {
        return FocusedHierarchy {
            position: target,
            role: HierarchyRole::SupportOrUnresolved,
            recognized_cells: vec![],
            component: None,
        };
    };
    let incoming = scene
        .connections
        .iter()
        .filter(|c| c.sink.component == component.id)
        .map(|c| c.source.component)
        .collect::<BTreeSet<_>>();
    let outgoing = scene
        .connections
        .iter()
        .filter(|c| c.source.component == component.id)
        .map(|c| c.sink.component)
        .collect::<BTreeSet<_>>();
    let role = if incoming.len() > 1 {
        HierarchyRole::SignalMerge
    } else if outgoing.len() > 1 {
        HierarchyRole::SignalBranch
    } else if !incoming.is_empty() || !outgoing.is_empty() {
        HierarchyRole::IntermediatePath
    } else {
        HierarchyRole::IsolatedOrUnresolved
    };
    FocusedHierarchy {
        position: target,
        role,
        recognized_cells: hierarchy
            .cell_graph
            .value
            .cells
            .gates
            .iter()
            .filter(|cell| cell.physical_components.contains(&component.id))
            .cloned()
            .collect(),
        component: Some(HierarchyComponent {
            block: component.block.kind,
            observed_name: component.block.observed_name.clone(),
            observed_properties: component.block.observed_properties.clone(),
            capabilities: component.block.capabilities(),
            physical_component: component.id,
            incoming_components: incoming,
            outgoing_components: outgoing,
            physical_origin: hierarchy
                .cell_graph
                .provenance
                .physical_positions
                .get(&component.id)
                .copied(),
        }),
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
enum HierarchyRole {
    SupportOrUnresolved,
    SignalMerge,
    SignalBranch,
    IntermediatePath,
    IsolatedOrUnresolved,
}

#[derive(Clone, Debug, PartialEq, Serialize)]
pub(super) struct CapabilityReport {
    groups: Vec<BlockCapabilityGroup>,
    issue_count: usize,
    issue_counts_by_stage_and_level: CapabilityCounts,
    issue_samples: Vec<CapabilityIssue>,
    issues_truncated: bool,
}
#[derive(Clone, Debug, PartialEq)]
struct CapabilityCounts(Vec<(CapabilityStage, CapabilityLevel, usize)>);
impl Serialize for CapabilityCounts {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        let mut map = serializer.serialize_map(Some(self.0.len()))?;
        for (stage, level, count) in &self.0 {
            let stage = match stage {
                CapabilityStage::PhysicalClassification => "physicalclassification",
                CapabilityStage::Connectivity => "connectivity",
                CapabilityStage::SteadyState => "steadystate",
                CapabilityStage::Temporal => "temporal",
                CapabilityStage::Repair => "repair",
                CapabilityStage::Placement => "placement",
            };
            let level = match level {
                CapabilityLevel::Full => "full",
                CapabilityLevel::Partial => "partial",
                CapabilityLevel::Unsupported => "unsupported",
                CapabilityLevel::NotApplicable => "notapplicable",
            };
            map.serialize_entry(&format!("{stage}:{level}"), count)?;
        }
        map.end()
    }
}
pub(super) fn capability_report(scene: &PhysicalScene) -> CapabilityReport {
    let report = scene.capability_report();
    let mut counts = Vec::<(CapabilityStage, CapabilityLevel, usize)>::new();
    for issue in &report.issues {
        if let Some((_, _, count)) = counts
            .iter_mut()
            .find(|(stage, level, _)| *stage == issue.stage && *level == issue.level)
        {
            *count += 1;
        } else {
            counts.push((issue.stage, issue.level, 1));
        }
    }
    CapabilityReport {
        groups: report.groups,
        issue_count: report.issues.len(),
        issue_counts_by_stage_and_level: CapabilityCounts(counts),
        issue_samples: report.issues.iter().take(32).cloned().collect(),
        issues_truncated: report.issues.len() > 32,
    }
}
#[derive(Clone, Debug, PartialEq, Serialize)]
pub(super) struct SignalLiveness {
    physical_traversal_group_count: usize,
    directed_signal_region_count: usize,
    cyclic_directed_signal_region_count: usize,
    drive_source_count: usize,
    source_counts_by_kind: SourceCounts,
    source_evidence: Vec<SignalSource>,
    drive_reachable_component_count: usize,
    potentially_drive_reachable_component_count: usize,
    external_input_waiting_count: usize,
    external_input_waiting: Vec<RequiredInputAssessment>,
    undriven_required_input_count: usize,
    undriven_required_inputs: Vec<UndrivenInput>,
    ranked_findings_near_focus: Option<Vec<RankedLivenessFinding>>,
    findings_truncated: bool,
    interpretation: &'static str,
}
#[derive(Clone, Debug, PartialEq)]
struct SourceCounts(Vec<(SignalSourceKind, usize)>);
impl Serialize for SourceCounts {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        let mut map = serializer.serialize_map(Some(self.0.len()))?;
        for (kind, count) in &self.0 {
            map.serialize_entry(kind, count)?;
        }
        map.end()
    }
}
pub(super) fn signal_liveness(scene: &PhysicalScene, focus: Option<Pos>) -> SignalLiveness {
    let report = dustroute_translate::liveness::analyze_signal_liveness(scene);
    let ranked = focus.map(|focus| {
        dustroute_translate::liveness::rank_liveness_findings(scene, &report, focus)
            .into_iter()
            .take(16)
            .collect()
    });
    let mut counts = Vec::<(SignalSourceKind, usize)>::new();
    for source in &report.sources {
        if let Some((_, count)) = counts.iter_mut().find(|(kind, _)| *kind == source.kind) {
            *count += 1;
        } else {
            counts.push((source.kind, 1));
        }
    }
    let waiting = report
        .required_input_assessments
        .iter()
        .filter(|a| a.status == RequiredInputStatus::AwaitingExternalInput);
    SignalLiveness {
        physical_traversal_group_count: scene.physical_traversal_groups().len(),
        directed_signal_region_count: report.directed_regions.len(),
        cyclic_directed_signal_region_count: report
            .directed_regions
            .iter()
            .filter(|r| r.cyclic)
            .count(),
        drive_source_count: report.drive_sources.len(),
        source_counts_by_kind: SourceCounts(counts),
        source_evidence: report.sources.iter().take(64).cloned().collect(),
        drive_reachable_component_count: report.drive_reachable.len(),
        potentially_drive_reachable_component_count: report.potential_drive_reachable.len(),
        external_input_waiting_count: waiting.clone().count(),
        external_input_waiting: waiting.take(64).cloned().collect(),
        undriven_required_input_count: report.undriven_inputs.len(),
        undriven_required_inputs: report.undriven_inputs.iter().take(64).cloned().collect(),
        ranked_findings_near_focus: ranked,
        findings_truncated: report.undriven_inputs.len() > 64,
        interpretation: "confirmed sources, inferred primary inputs, and genuine no-source failures are separate; inferred external inputs are not automatic repair evidence",
    }
}
