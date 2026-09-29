//! Steady-state boundary comparison.
use super::boundary::{boundary_terminal_mapping, set_driver_in_world};
use super::{ContextualVerificationState, MacroSteadyStateReport};
use dustroute_physical::World;
use dustroute_translate::{InferredTruthTable, RegionBounds, TruthTableRow};

/// Re-infers the materialized circuit, identifies its terminals by the fixed
/// boundary components, and compares rows in the original boundary order.
#[must_use]
pub fn verify_macro_steady_state(
    expected: &InferredTruthTable,
    original: &World,
    materialized: &World,
    max_inputs: usize,
    settle_ticks: usize,
) -> MacroSteadyStateReport {
    let Some((low, high)) = materialized.bounds() else {
        return unavailable_steady("materialized world is empty");
    };
    let analysis =
        dustroute_translate::analyze_world_region(materialized, RegionBounds::new(low, high));
    if expected.inputs.len() > max_inputs {
        return unavailable_steady("too many inputs for steady-state verification");
    }
    let Some((original_low, original_high)) = original.bounds() else {
        return unavailable_steady("original world is empty");
    };
    let original_analysis = dustroute_translate::analyze_world_region(
        original,
        RegionBounds::new(original_low, original_high),
    );
    let drivers = match expected
        .inputs
        .iter()
        .map(|terminal| {
            dustroute_translate::inferred_input_driver(original, &original_analysis, terminal)
        })
        .collect::<Result<Vec<_>, _>>()
    {
        Ok(drivers) => drivers,
        Err(error) => return unavailable_steady(&error.to_string()),
    };
    let actual = InferredTruthTable {
        inputs: analysis.inputs.clone(),
        outputs: analysis.outputs.clone(),
        rows: Vec::new(),
    };
    let input_mapping = boundary_terminal_mapping(
        expected.inputs.iter().map(|t| t.anchor),
        &actual,
        &analysis,
        true,
    )
    .unwrap_or_default();
    let output_mapping = boundary_terminal_mapping(
        expected.outputs.iter().map(|t| t.anchor),
        &actual,
        &analysis,
        false,
    )
    .unwrap_or_default();
    let mut rows = Vec::with_capacity(expected.rows.len());
    for expected_row in &expected.rows {
        let mut driven = materialized.clone();
        for (driver, powered) in drivers.iter().zip(&expected_row.inputs) {
            if let Err(error) = set_driver_in_world(&mut driven, *driver, *powered) {
                return unavailable_steady(&error);
            }
        }
        dustroute_translate::update_wire_shapes(&mut driven);
        let state = match dustroute_translate::RedstoneTickSimulator::new(driven)
            .and_then(|mut simulator| simulator.settle_ticks(settle_ticks))
        {
            Ok(state) => state,
            Err(error) => return unavailable_steady(&error.to_string()),
        };
        rows.push(TruthTableRow {
            inputs: expected_row.inputs.clone(),
            outputs: expected
                .outputs
                .iter()
                .map(|terminal| state.powered(terminal.anchor))
                .collect(),
        });
    }
    let normalized = InferredTruthTable {
        inputs: expected.inputs.clone(),
        outputs: expected.outputs.clone(),
        rows,
    };
    let comparison = dustroute_translate::compare_truth_tables(expected, &normalized);
    let differing_assignments = expected
        .rows
        .iter()
        .zip(&normalized.rows)
        .filter(|(expected, actual)| expected.outputs != actual.outputs)
        .map(|(expected, _)| expected.inputs.clone())
        .collect();
    MacroSteadyStateReport {
        state: if comparison.comparable && comparison.differing_bits == 0 {
            ContextualVerificationState::Passed
        } else {
            ContextualVerificationState::Failed
        },
        comparison: Some(comparison),
        input_mapping,
        output_mapping,
        differing_assignments,
        reason: None,
    }
}

fn unavailable_steady(reason: &str) -> MacroSteadyStateReport {
    MacroSteadyStateReport {
        state: ContextualVerificationState::Pending,
        comparison: None,
        input_mapping: Vec::new(),
        output_mapping: Vec::new(),
        differing_assignments: Vec::new(),
        reason: Some(reason.into()),
    }
}
