use dustroute_translate::{SignalDiagnostics, TruthTableComparison};

use dustroute_translate::physical::PhysicalCircuit;

use crate::{PlacementScore, PlacementWeights, placement_score};

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SemanticWeights {
    pub truth_table_penalty: f64,
    pub isolated_redstone: f64,
    pub extra_signal_island: f64,
    pub unreachable_component: f64,
    pub component_without_output: f64,
    pub invalid_support: f64,
    pub non_controllable_torch: f64,
    pub unavailable_truth_table: f64,
}

impl Default for SemanticWeights {
    fn default() -> Self {
        Self {
            truth_table_penalty: 1_000.0,
            isolated_redstone: 500.0,
            extra_signal_island: 2_000.0,
            unreachable_component: 250.0,
            component_without_output: 250.0,
            invalid_support: 5_000.0,
            non_controllable_torch: 10_000.0,
            unavailable_truth_table: 1_000_000.0,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SemanticScore {
    pub total: f64,
    pub truth_table_penalty: usize,
    pub isolated_redstone: usize,
    pub extra_signal_islands: usize,
    pub unreachable_components: usize,
    pub components_without_output: usize,
    pub invalid_supports: usize,
    pub non_controllable_torches: usize,
    pub truth_table_available: bool,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct CombinedPlacementScore {
    pub total: f64,
    pub geometric: PlacementScore,
    pub semantic: SemanticScore,
}

#[must_use]
pub fn semantic_score(
    comparison: Option<&TruthTableComparison>,
    diagnostics: &SignalDiagnostics,
    weights: SemanticWeights,
) -> SemanticScore {
    let truth_table_penalty = comparison.map_or(0, |value| value.fitness_penalty);
    let extra_signal_islands = diagnostics.signal_islands.len().saturating_sub(1);
    let truth_table_available = comparison.is_some();
    let score = SemanticScore {
        total: 0.0,
        truth_table_penalty,
        isolated_redstone: diagnostics.isolated_redstone.len(),
        extra_signal_islands,
        unreachable_components: diagnostics.unreachable_from_inputs.len(),
        components_without_output: diagnostics.cannot_reach_outputs.len(),
        invalid_supports: diagnostics.invalid_supports.len(),
        non_controllable_torches: diagnostics.non_controllable_torches.len(),
        truth_table_available,
    };
    SemanticScore {
        total: weights.truth_table_penalty * score.truth_table_penalty as f64
            + weights.isolated_redstone * score.isolated_redstone as f64
            + weights.extra_signal_island * score.extra_signal_islands as f64
            + weights.unreachable_component * score.unreachable_components as f64
            + weights.component_without_output * score.components_without_output as f64
            + weights.invalid_support * score.invalid_supports as f64
            + weights.non_controllable_torch * score.non_controllable_torches as f64
            + if truth_table_available {
                0.0
            } else {
                weights.unavailable_truth_table
            },
        ..score
    }
}

#[must_use]
pub fn combined_placement_score(
    geometric: PlacementScore,
    semantic: SemanticScore,
) -> CombinedPlacementScore {
    CombinedPlacementScore {
        total: geometric.total + semantic.total,
        geometric,
        semantic,
    }
}

#[must_use]
pub fn evaluate_placement_with_semantics(
    circuit: &PhysicalCircuit,
    placement_weights: PlacementWeights,
    comparison: Option<&TruthTableComparison>,
    diagnostics: &SignalDiagnostics,
    semantic_weights: SemanticWeights,
) -> CombinedPlacementScore {
    combined_placement_score(
        placement_score(circuit, placement_weights),
        semantic_score(comparison, diagnostics, semantic_weights),
    )
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeSet;

    use dustroute_translate::{Pos, SignalDiagnostics, TruthTableComparison};

    use super::*;

    #[test]
    fn broken_circuit_diagnostics_increase_optimizer_score() {
        let comparison = TruthTableComparison {
            comparable: false,
            expected_inputs: 2,
            actual_inputs: 2,
            expected_outputs: 2,
            actual_outputs: 3,
            differing_rows: 2,
            differing_bits: 2,
            terminal_count_delta: 1,
            fitness_penalty: 13,
        };
        let mut diagnostics = SignalDiagnostics::default();
        diagnostics
            .non_controllable_torches
            .insert(Pos::new(1, 2, 3));
        diagnostics.unreachable_from_inputs = BTreeSet::from([4, 5]);
        let score = semantic_score(Some(&comparison), &diagnostics, SemanticWeights::default());
        assert_eq!(score.truth_table_penalty, 13);
        assert_eq!(score.non_controllable_torches, 1);
        assert_eq!(score.unreachable_components, 2);
        assert!(score.total > 0.0);
    }

    #[test]
    fn missing_truth_table_is_not_treated_as_valid() {
        let score = semantic_score(
            None,
            &SignalDiagnostics::default(),
            SemanticWeights::default(),
        );
        assert!(!score.truth_table_available);
        assert_eq!(
            score.total,
            SemanticWeights::default().unavailable_truth_table
        );
    }

    #[test]
    fn combined_evaluator_adds_reverse_diagnostics_to_geometry() {
        let circuit = PhysicalCircuit::new();
        let mut diagnostics = SignalDiagnostics::default();
        diagnostics.invalid_supports.push((
            Pos::new(0, 1, 0),
            dustroute_translate::BlockKind::RedstoneWire,
            Some(Pos::new(0, 0, 0)),
        ));
        let combined = evaluate_placement_with_semantics(
            &circuit,
            PlacementWeights::default(),
            None,
            &diagnostics,
            SemanticWeights::default(),
        );
        assert_eq!(combined.geometric.total, 0.0);
        assert!(combined.semantic.total > 0.0);
        assert_eq!(combined.total, combined.semantic.total);
    }
}
