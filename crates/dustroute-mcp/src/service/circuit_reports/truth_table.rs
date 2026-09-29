//! Truth-table diagnostics and full reverse-analysis rendering.
use super::analysis::{capability_report_json, logical_role_json, signal_liveness_json};
use super::{bounds_json, truth_table_status};
use serde_json::{Value, json};

fn json_u128(value: u128) -> Value {
    u64::try_from(value)
        .map(|value| json!(value))
        .unwrap_or_else(|_| Value::String(value.to_string()))
}

pub(in super::super) fn truth_table_error_details(
    error: Option<&dustroute_translate::TruthTableError>,
) -> Value {
    let Some(error) = error else {
        return Value::Null;
    };
    match error {
        dustroute_translate::TruthTableError::BudgetExceeded {
            rows,
            max_rows,
            estimated_work_units,
            max_work_units,
        } => json!({
            "code": "budget_exceeded",
            "message": error.to_string(),
            "rows": rows,
            "max_rows": max_rows,
            "estimated_work_units": json_u128(*estimated_work_units),
            "max_work_units": json_u128(*max_work_units),
        }),
        dustroute_translate::TruthTableError::RuntimeBudgetExceeded {
            rows,
            completed_rows,
            solver_iterations,
            max_solver_iterations,
        } => json!({
            "code": "runtime_budget_exceeded",
            "message": error.to_string(),
            "rows": rows,
            "completed_rows": completed_rows,
            "solver_iterations": solver_iterations,
            "max_solver_iterations": max_solver_iterations,
        }),
        dustroute_translate::TruthTableError::ElapsedBudgetExceeded {
            rows,
            completed_rows,
            elapsed_millis,
            max_elapsed_millis,
        } => json!({
            "code": "elapsed_budget_exceeded",
            "message": error.to_string(),
            "rows": rows,
            "completed_rows": completed_rows,
            "elapsed_millis": json_u128(*elapsed_millis),
            "max_elapsed_millis": max_elapsed_millis,
        }),
        dustroute_translate::TruthTableError::NonSettling {
            row,
            settle_ticks,
            pending_events,
        } => json!({
            "code": "non_settling",
            "message": error.to_string(),
            "row": row,
            "settle_ticks": settle_ticks,
            "pending_events": pending_events,
        }),
        dustroute_translate::TruthTableError::TooManyInputs(count) => json!({
            "code": "too_many_inputs",
            "message": error.to_string(),
            "input_count": count,
        }),
        dustroute_translate::TruthTableError::IncompleteObservation => {
            json!({ "code": "incomplete_observation", "message": error.to_string() })
        }
        dustroute_translate::TruthTableError::NoInputs => {
            json!({ "code": "no_inputs", "message": error.to_string() })
        }
        dustroute_translate::TruthTableError::NoOutputs => {
            json!({ "code": "no_outputs", "message": error.to_string() })
        }
        dustroute_translate::TruthTableError::UnmappedExternalInputs(positions) => json!({
            "code": "unmapped_external_inputs",
            "message": error.to_string(),
            "positions": positions,
        }),
        dustroute_translate::TruthTableError::UnmappedObservableOutputs(positions) => json!({
            "code": "unmapped_observable_outputs",
            "message": error.to_string(),
            "positions": positions,
        }),
        dustroute_translate::TruthTableError::AmbiguousInputMapping {
            external_inputs,
            inferred_inputs,
        } => json!({
            "code": "ambiguous_input_mapping",
            "message": error.to_string(),
            "external_input_count": external_inputs,
            "inferred_input_count": inferred_inputs,
        }),
        dustroute_translate::TruthTableError::AmbiguousOutputMapping {
            observable_outputs,
            inferred_outputs,
        } => json!({
            "code": "ambiguous_output_mapping",
            "message": error.to_string(),
            "observable_output_count": observable_outputs,
            "inferred_output_count": inferred_outputs,
        }),
        dustroute_translate::TruthTableError::NoDriverPosition(position) => json!({
            "code": "no_driver_position",
            "message": error.to_string(),
            "position": position,
        }),
        dustroute_translate::TruthTableError::InvalidDriver {
            position,
            expected,
            actual,
        } => json!({
            "code": "invalid_driver",
            "message": error.to_string(),
            "position": position,
            "expected": expected,
            "actual": actual,
        }),
        dustroute_translate::TruthTableError::Simulation(message) => json!({
            "code": "simulation_error",
            "message": message,
        }),
    }
}

pub(in super::super) fn reverse_result_json(
    bounds: dustroute_translate::RegionBounds,
    translated: &dustroute_translate::ReverseResult,
) -> Value {
    let mut hierarchy = dustroute_ir::hierarchy_from_views(
        &translated.analysis.scene,
        translated.gate_view.clone(),
        translated.expression_view.clone(),
        translated.functional_view.clone(),
    );
    hierarchy.temporal = translated.temporal.clone();
    json!({
        "ok": true,
        "bounds": bounds_json(bounds),
        "redstone_blocks": translated.analysis.redstone_blocks.len(),
        "physical": {
            "components": translated.analysis.scene.components.len(),
            "verified_connections": translated.analysis.scene.connections.len(),
            "physical_traversal_groups": translated.analysis.scene.physical_traversal_groups().len(),
            "connected_fragments": translated.analysis.scene.fragments.len(),
            "nearby_gap_candidates": translated.analysis.scene.gap_candidates(2),
            "observation": translated.analysis.scene.observation,
            "analysis_complete": translated.analysis.scene.observation.is_complete(),
            "block_capabilities": capability_report_json(&translated.analysis.scene),
        },
        "signal_liveness": signal_liveness_json(&translated.analysis.scene, None),
        "stages": {
            "observed_world": {
                "bounds": bounds_json(bounds),
                "redstone_blocks": translated.analysis.redstone_blocks.len()
            },
            "physical_scene": {
                "completeness": hierarchy.physical_snapshot.completeness,
                "components": hierarchy.physical_snapshot.value.scene.components.len(),
                "diagnostics": hierarchy.physical_snapshot.diagnostics
            },
            "electrical_network": {
                "completeness": hierarchy.physical_graph.completeness,
                "directed_connections": hierarchy.physical_graph.value.scene.connections.len(),
                "unresolved": hierarchy.physical_graph.unresolved
            },
            "timed_behavior": {
                "timing": hierarchy.temporal.timing,
                "devices": hierarchy.temporal.behavior.devices.len(),
                "traces": hierarchy.temporal.behavior.traces.len()
            },
            "local_logic": {
                "completeness": hierarchy.logic_graph.completeness,
                "cells": hierarchy.cell_graph.value.cells,
                "expressions": hierarchy.logic_graph.value.expressions
            },
            "functional_candidates": {
                "completeness": hierarchy.functional_graph.completeness,
                "functions": hierarchy.functional_graph.value.functions,
                "unresolved": hierarchy.functional_graph.unresolved
            }
        },
        "gate_view": translated.gate_view,
        "expression_view": translated.expression_view,
        "functional_view": translated.functional_view,
        "physical_function_model": translated.functional_network.as_ref().map(|model| json!({
            "output_functions": model.output_functions.iter().map(|output| json!({
                "output_index": output.output_index,
                "position": output.terminal.anchor,
                "expression": output.expression.to_string(),
                "truth_column": output.truth_column,
            })).collect::<Vec<_>>(),
            "shared_physical_components": model.physical_influences.iter()
                .filter(|influence| influence.shared_role)
                .map(|influence| json!({
                    "component": influence.component,
                    "positions": influence.positions,
                    "input_dependencies": influence.input_dependencies,
                    "output_dependencies": influence.output_dependencies,
                })).collect::<Vec<_>>(),
            "interpretation": "Output functions are derived from the shared physical network. Physical components are not assigned exclusive gate identities."
        })),
        "functional_validity": translated.temporal.timing,
        "behavior_ir": {
            "temporal_devices": translated.temporal.behavior.devices,
            "trace_count": translated.temporal.behavior.traces.len(),
            "timing": translated.temporal.timing,
            "timed_nodes": translated.temporal.timed_circuit.nodes.len(),
            "timed_edges": translated.temporal.timed_circuit.edges.len(),
            "steady_state_projection": translated.temporal.steady_state,
            "transient_assessment": {
                "status": if translated.temporal.behavior.traces.is_empty() { "not_simulated" } else { "observed_initial_state_only" },
                "assessments": translated.temporal.transients,
                "guidance": "hazard_candidate means a measured transient has no registered intent; hazard_confirmed requires an explicit signal contract. Initial-state settling does not cover every input transition."
            },
        },
        "inputs": translated.analysis.inputs.iter().map(|terminal| json!({
            "position": terminal.anchor,
            "component": terminal.component,
            "confidence": format!("{:?}", terminal.confidence).to_lowercase(),
        })).collect::<Vec<_>>(),
        "interface_evidence": translated.analysis.interface,
        "unsupported_observed_blocks": translated.analysis.unsupported.iter().map(|(position, block)| json!({"position": position, "block": block})).collect::<Vec<_>>(),
        "outputs": translated.analysis.outputs.iter().map(|terminal| json!({
            "position": terminal.anchor,
            "component": terminal.component,
            "confidence": format!("{:?}", terminal.confidence).to_lowercase(),
        })).collect::<Vec<_>>(),
        "expressions": translated.expressions.iter().map(ToString::to_string).collect::<Vec<_>>(),
        "logical_role": logical_role_json(translated),
        "truth_table_semantics": translated.truth_table_semantics,
        "truth_table": translated.truth_table.as_ref().map(|table| table.rows.iter().map(|row| json!({
            "inputs": row.inputs,
            "outputs": row.outputs,
        })).collect::<Vec<_>>()),
        "truth_table_status": truth_table_status(translated),
        "truth_table_error": translated.truth_table_error.as_ref().map(ToString::to_string),
        "truth_table_error_details": truth_table_error_details(translated.truth_table_error.as_ref()),
        "diagnostics": {
            "signal_islands": translated.analysis.diagnostics.signal_islands.len(),
            "isolated_redstone": translated.analysis.diagnostics.isolated_redstone.len(),
            "unreachable_components": translated.analysis.diagnostics.unreachable_from_inputs.len(),
            "components_without_output_path": translated.analysis.diagnostics.cannot_reach_outputs.len(),
            "invalid_supports": translated.analysis.diagnostics.invalid_supports.len(),
            "non_controllable_torches": translated.analysis.diagnostics.non_controllable_torches.len(),
        }
    })
}
