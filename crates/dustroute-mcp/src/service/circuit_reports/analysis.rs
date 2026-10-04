//! Physical, hierarchical and mixed-IR report projections.
use super::MAX_FLAT_ANALYSIS_COMPONENTS;
use super::bounds_json;
use dustroute_physical::Pos;
use serde_json::{Value, json};
use std::collections::{BTreeMap, BTreeSet};

pub(in super::super) fn logical_role_json(
    translated: &dustroute_translate::api::ReverseResult,
) -> Value {
    serde_json::to_value(dustroute_translate::analysis::derive_local_logic(
        translated,
    ))
    .unwrap_or_else(|error| json!({ "classification": "unknown", "reason": error.to_string() }))
}

pub(in super::super) fn focused_role_json(
    translated: &dustroute_translate::api::ReverseResult,
    target: Pos,
) -> Value {
    let focused = dustroute_translate::analysis::classify_focused_role(translated, target);
    let physical_component = translated
        .analysis
        .scene
        .component_at(target)
        .map(|component| component.id);
    let recognized_gates = physical_component
        .map(|component| {
            translated
                .gate_view
                .gates
                .iter()
                .filter(|gate| gate.physical_components.contains(&component))
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();
    let observed_block = translated
        .analysis
        .scene
        .component_at(target)
        .map(|component| &component.block);
    let Some(component_id) = focused.signal_component else {
        return json!({
            "position": target,
            "block": observed_block.map(|block| block.kind),
            "observed_name": observed_block.and_then(|block| block.observed_name.as_deref()),
            "observed_properties": observed_block.map(|block| &block.observed_properties),
            "capabilities": observed_block.map(dustroute_physical::Block::capabilities),
            "physical_component": physical_component,
            "recognized_gates": recognized_gates,
            "role": "support_or_unresolved"
        });
    };
    json!({
        "position": target,
        "block": observed_block.map(|block| block.kind),
        "observed_name": observed_block.and_then(|block| block.observed_name.as_deref()),
        "observed_properties": observed_block.map(|block| &block.observed_properties),
        "capabilities": observed_block.map(dustroute_physical::Block::capabilities),
        "physical_component": physical_component,
        "recognized_gates": recognized_gates,
        "signal_component": component_id,
        "incoming_components": focused.incoming_components,
        "outgoing_components": focused.outgoing_components,
        "role": focused.role
    })
}

pub(in super::super) fn focused_explanation_json(
    analysis: &dustroute_translate::analysis::PhysicalAnalysis,
    target: Pos,
    analysis_complete: bool,
) -> Value {
    serde_json::to_value(dustroute_translate::analysis::explain_focused_component(
        analysis,
        target,
        analysis_complete,
    ))
    .unwrap_or_else(|error| {
        json!({
            "position": target,
            "status": "unavailable",
            "reason": format!("focused explanation serialization failed: {error}"),
        })
    })
}

fn focused_scene_explanation_json(
    scene: &dustroute_physical::PhysicalScene,
    hierarchy: &dustroute_ir::HierarchicalIr,
    target: Pos,
    analysis_complete: bool,
) -> Value {
    serde_json::to_value(dustroute_translate::analysis::explain_focused_scene(
        scene,
        hierarchy,
        target,
        analysis_complete,
    ))
    .unwrap_or_else(|error| {
        json!({
            "position": target,
            "status": "unavailable",
            "reason": format!("focused explanation serialization failed: {error}"),
        })
    })
}

pub(in super::super) fn focused_hierarchy_role_json(
    scene: &dustroute_physical::PhysicalScene,
    hierarchy: &dustroute_ir::HierarchicalIr,
    target: Pos,
) -> Value {
    let Some(component) = scene.component_at(target) else {
        return json!({
            "position": target,
            "role": "support_or_unresolved",
            "recognized_cells": []
        });
    };
    let incoming = scene
        .connections
        .iter()
        .filter(|connection| connection.sink.component == component.id)
        .map(|connection| connection.source.component)
        .collect::<BTreeSet<_>>();
    let outgoing = scene
        .connections
        .iter()
        .filter(|connection| connection.source.component == component.id)
        .map(|connection| connection.sink.component)
        .collect::<BTreeSet<_>>();
    let cells = hierarchy
        .cell_graph
        .value
        .cells
        .gates
        .iter()
        .filter(|cell| cell.physical_components.contains(&component.id))
        .collect::<Vec<_>>();
    let role = if incoming.len() > 1 {
        "signal_merge"
    } else if outgoing.len() > 1 {
        "signal_branch"
    } else if !incoming.is_empty() || !outgoing.is_empty() {
        "intermediate_path"
    } else {
        "isolated_or_unresolved"
    };
    json!({
        "position": target,
        "block": component.block.kind,
        "observed_name": component.block.observed_name,
        "observed_properties": component.block.observed_properties,
        "capabilities": component.block.capabilities(),
        "physical_component": component.id,
        "incoming_components": incoming,
        "outgoing_components": outgoing,
        "recognized_cells": cells,
        "role": role,
        "physical_origin": hierarchy.cell_graph.provenance.physical_positions.get(&component.id)
    })
}

pub(in super::super) fn capability_report_json(scene: &dustroute_physical::PhysicalScene) -> Value {
    const MAX_CAPABILITY_ISSUE_SAMPLES: usize = 32;
    let report = scene.capability_report();
    let mut counts = BTreeMap::<String, usize>::new();
    for issue in &report.issues {
        *counts
            .entry(format!("{:?}:{:?}", issue.stage, issue.level).to_lowercase())
            .or_default() += 1;
    }
    json!({
        "groups": report.groups,
        "issue_count": report.issues.len(),
        "issue_counts_by_stage_and_level": counts,
        "issue_samples": report.issues.iter().take(MAX_CAPABILITY_ISSUE_SAMPLES).collect::<Vec<_>>(),
        "issues_truncated": report.issues.len() > MAX_CAPABILITY_ISSUE_SAMPLES
    })
}

pub(in super::super) fn signal_liveness_json(
    scene: &dustroute_physical::PhysicalScene,
    focus: Option<dustroute_physical::Pos>,
) -> Value {
    const MAX_FINDINGS: usize = 64;
    const MAX_RANKED_FINDINGS: usize = 16;
    let report = dustroute_translate::liveness::analyze_signal_liveness(scene);
    let ranked = focus.map(|focus| {
        dustroute_translate::liveness::rank_liveness_findings(scene, &report, focus)
            .into_iter()
            .take(MAX_RANKED_FINDINGS)
            .collect::<Vec<_>>()
    });
    let source_counts =
        report
            .sources
            .iter()
            .fold(BTreeMap::<String, usize>::new(), |mut counts, source| {
                let kind = match source.kind {
                    dustroute_translate::liveness::SignalSourceKind::ControllableInput => {
                        "controllable_input"
                    }
                    dustroute_translate::liveness::SignalSourceKind::IntrinsicSource => {
                        "intrinsic_source"
                    }
                    dustroute_translate::liveness::SignalSourceKind::ObservationBoundary => {
                        "observation_boundary"
                    }
                    dustroute_translate::liveness::SignalSourceKind::InferredPrimaryInput => {
                        "inferred_primary_input"
                    }
                };
                *counts.entry(kind.to_owned()).or_default() += 1;
                counts
            });
    let external_input_waiting = report
        .required_input_assessments
        .iter()
        .filter(|assessment| {
            assessment.status
                == dustroute_translate::liveness::RequiredInputStatus::AwaitingExternalInput
        })
        .take(MAX_FINDINGS)
        .collect::<Vec<_>>();
    json!({
        "physical_traversal_group_count": scene.physical_traversal_groups().len(),
        "directed_signal_region_count": report.directed_regions.len(),
        "cyclic_directed_signal_region_count": report.directed_regions.iter().filter(|region| region.cyclic).count(),
        "drive_source_count": report.drive_sources.len(),
        "source_counts_by_kind": source_counts,
        "source_evidence": report.sources.iter().take(MAX_FINDINGS).collect::<Vec<_>>(),
        "drive_reachable_component_count": report.drive_reachable.len(),
        "potentially_drive_reachable_component_count": report.potential_drive_reachable.len(),
        "external_input_waiting_count": report.required_input_assessments.iter().filter(|assessment| assessment.status == dustroute_translate::liveness::RequiredInputStatus::AwaitingExternalInput).count(),
        "external_input_waiting": external_input_waiting,
        "undriven_required_input_count": report.undriven_inputs.len(),
        "undriven_required_inputs": report.undriven_inputs.iter().take(MAX_FINDINGS).collect::<Vec<_>>(),
        "ranked_findings_near_focus": ranked,
        "findings_truncated": report.undriven_inputs.len() > MAX_FINDINGS,
        "interpretation": "confirmed sources, inferred primary inputs, and genuine no-source failures are separate; inferred external inputs are not automatic repair evidence"
    })
}

pub(in super::super) fn hierarchical_result_json(
    bounds: dustroute_translate::world_reverse::RegionBounds,
    hierarchy: &dustroute_ir::HierarchicalIr,
    focused: Value,
    expansion: &super::super::circuit_capture::ExpansionEvidence,
    focus: Option<dustroute_physical::Pos>,
) -> Value {
    let scene = &hierarchy.physical_graph.value.scene;
    let mixed = dustroute_ir::build_mixed_ir(hierarchy);
    let mixed_counts = mixed
        .nodes
        .iter()
        .fold(BTreeMap::new(), |mut counts, node| {
            let representation = match &node.kind {
                dustroute_ir::MixedNodeKind::LogicGate { .. } => "logic_gate",
                dustroute_ir::MixedNodeKind::TimedCell { .. } => "timed_cell",
                dustroute_ir::MixedNodeKind::PhysicalRegion => "physical_region",
                dustroute_ir::MixedNodeKind::Boundary { .. } => "boundary",
            };
            *counts.entry(representation).or_insert(0_usize) += 1;
            counts
        });
    let analysis_complete = !expansion.limit_reached() && scene.observation.is_complete();
    let focused_explanation = focus
        .map(|target| focused_scene_explanation_json(scene, hierarchy, target, analysis_complete));
    json!({
        "ok": true,
        "analysis_mode": "hierarchical_local_first",
        "bounds": bounds_json(bounds),
        "analysis_complete": !expansion.limit_reached(),
        "focused_component": focused,
        "focused_explanation": focused_explanation,
        "expansion": expansion,
        "block_capabilities": capability_report_json(scene),
        "signal_liveness": signal_liveness_json(scene, focus),
        "stages": {
            "physical_snapshot": {
                "completeness": hierarchy.physical_snapshot.completeness,
                "components": scene.components.len(),
                "diagnostic_count": hierarchy.physical_snapshot.diagnostics.len(),
                "diagnostics": hierarchy.physical_snapshot.diagnostics.iter().take(16).collect::<Vec<_>>(),
                "diagnostics_truncated": hierarchy.physical_snapshot.diagnostics.len() > 16
            },
            "physical_graph": {
                "completeness": hierarchy.physical_graph.completeness,
                "directed_connections": scene.connections.len(),
                "physical_traversal_groups": scene.physical_traversal_groups().len(),
                "fragments": scene.fragments.len(),
                "unresolved": hierarchy.physical_graph.unresolved
            },
            "cell_graph": {
                "completeness": hierarchy.cell_graph.completeness,
                "cell_count": hierarchy.cell_graph.value.cells.gates.len(),
                "unresolved_component_count": hierarchy.cell_graph.unresolved.len(),
                "detail": "represented by mixed_ir node references; recursive cell payload omitted"
            },
            "logic_graph": {
                "completeness": hierarchy.logic_graph.completeness,
                "expression_count": hierarchy.logic_graph.value.expressions.expressions.len(),
                "detail": "recursive expressions omitted; follow mixed_ir edges by node id"
            },
            "mixed_ir": {
                "physical_component_count": mixed.physical_component_count,
                "recognized_component_count": mixed.recognized_component_count,
                "unresolved_component_count": mixed.unresolved_component_count,
                "node_count": mixed.nodes.len(),
                "edge_count": mixed.edges.len(),
                "representation_counts": mixed_counts,
                "nodes": mixed.nodes,
                "edges": mixed.edges
            },
            "functional_graph": {
                "completeness": hierarchy.functional_graph.completeness,
                "functions": hierarchy.functional_graph.value.functions,
                "validity": hierarchy.temporal.timing,
            }
        },
        "temporal": {
            "timing": hierarchy.temporal.timing,
            "timed_nodes": hierarchy.temporal.timed_circuit.nodes.len(),
            "timed_edges": hierarchy.temporal.timed_circuit.edges.len(),
            "steady_state_retained_components": hierarchy.temporal.steady_state.retained_components.len(),
            "steady_state_compressed_components": hierarchy.temporal.steady_state.compressed_components.len(),
            "steady_state_edges": hierarchy.temporal.steady_state.edges.len(),
            "transient_assessment": {
                "status": "not_simulated",
                "findings": [],
                "guidance": "timing risk is structural only; run transition scenarios before claiming that a pulse was observed"
            }
        },
        "truth_table": null,
        "truth_table_status": "skipped_large_circuit",
        "truth_table_skip": {
            "code": "flat_analysis_component_threshold",
            "component_count": expansion.components_loaded(),
            "threshold": MAX_FLAT_ANALYSIS_COMPONENTS,
            "guidance": "set include_truth_table=true to request bounded exhaustive simulation"
        },
        "truth_table_skipped": "large circuits use local cells and hierarchical summaries instead of a flat whole-circuit truth table"
    })
}

pub(in super::super) fn circuit_identity_json(
    hierarchy: &dustroute_ir::HierarchicalIr,
    logical_role: Option<&dustroute_translate::analysis::LogicalRole>,
    analysis_complete: bool,
    repair_count: usize,
) -> Value {
    serde_json::to_value(crate::recorded_analysis::circuit_identity(
        hierarchy,
        logical_role,
        analysis_complete,
        repair_count,
    ))
    .expect("serializable circuit identity")
}

pub(in super::super) fn mixed_ir_json(
    hierarchy: &dustroute_ir::HierarchicalIr,
    expanded_node_id: Option<usize>,
) -> Result<Value, String> {
    let scene = &hierarchy.physical_graph.value.scene;
    let mixed = dustroute_ir::build_mixed_ir(hierarchy);
    let nodes = mixed
        .nodes
        .iter()
        .map(|node| {
            let positions = node
                .physical_components
                .iter()
                .filter_map(|component| scene.components.get(component.0).map(|item| item.pos))
                .collect::<Vec<_>>();
            let min = positions.iter().copied().reduce(|left, right| Pos {
                x: left.x.min(right.x),
                y: left.y.min(right.y),
                z: left.z.min(right.z),
            });
            let max = positions.iter().copied().reduce(|left, right| Pos {
                x: left.x.max(right.x),
                y: left.y.max(right.y),
                z: left.z.max(right.z),
            });
            json!({
                "id": node.id,
                "kind": node.kind,
                "recognition": node.recognition,
                "confidence": node.confidence,
                "component_count": node.physical_components.len(),
                "bounds": { "min": min, "max": max },
                "expandable": node.expandable,
            })
        })
        .collect::<Vec<_>>();
    let expanded_node = if let Some(id) = expanded_node_id {
        let node = mixed
            .nodes
            .get(id)
            .filter(|node| node.id.0 == id)
            .ok_or_else(|| format!("mixed IR node {id} does not exist"))?;
        let components = node
            .physical_components
            .iter()
            .filter_map(|component| scene.components.get(component.0))
            .map(|component| {
                json!({
                    "id": component.id,
                    "position": component.pos,
                    "block": component.block.kind,
                    "observed_name": component.block.observed_name,
                    "observed_properties": component.block.observed_properties,
                })
            })
            .collect::<Vec<_>>();
        let incoming = mixed
            .edges
            .iter()
            .filter(|edge| edge.sink == node.id)
            .collect::<Vec<_>>();
        let outgoing = mixed
            .edges
            .iter()
            .filter(|edge| edge.source == node.id)
            .collect::<Vec<_>>();
        Some(json!({
            "id": node.id,
            "kind": node.kind,
            "recognition": node.recognition,
            "confidence": node.confidence,
            "components": components,
            "incoming": incoming,
            "outgoing": outgoing,
        }))
    } else {
        None
    };
    Ok(json!({
        "physical_component_count": mixed.physical_component_count,
        "recognized_component_count": mixed.recognized_component_count,
        "unresolved_component_count": mixed.unresolved_component_count,
        "node_count": mixed.nodes.len(),
        "edge_count": mixed.edges.len(),
        "nodes": nodes,
        "edges": mixed.edges,
        "expanded_node": expanded_node,
    }))
}
