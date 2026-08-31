use std::collections::BTreeSet;

use dustroute_translate::cell_library::default_cell_library;
use dustroute_translate::physical::{CellId, PlacementCircuit};
use dustroute_translate::world::Pos;

use crate::placement::electrical_keepout_contacts;
use crate::{
    MutationKind, PlacementMutation, PlacementScore, PlacementWeights, apply_mutation,
    candidate_mutations, placement_score,
};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CompressionAxis {
    X,
    Y,
    Z,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CompressionDirection {
    TowardMinimum,
    TowardMaximum,
}

/// Selects cells which must not move during directional compression.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum AnchorPolicy {
    /// Cells directly driven by an external input boundary.
    Inputs,
    /// Cells which directly drive an external output boundary.
    Outputs,
    /// An explicitly selected stable group.
    Cells(BTreeSet<CellId>),
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct DirectionalWeights {
    pub axial_span: f64,
    pub wire_distance: f64,
    pub bounding_volume: f64,
    pub cell_block_count: f64,
    pub overlap_penalty: f64,
}

impl Default for DirectionalWeights {
    fn default() -> Self {
        Self {
            axial_span: 10.0,
            wire_distance: 1.0,
            bounding_volume: 0.002,
            cell_block_count: 0.05,
            overlap_penalty: 1_000_000.0,
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub enum OptimizationPhase {
    DirectionalCompress {
        axis: CompressionAxis,
        direction: CompressionDirection,
        anchor: AnchorPolicy,
        max_steps: usize,
        move_step: i32,
        weights: DirectionalWeights,
    },
    GlobalCompact {
        max_steps: usize,
        move_step: i32,
        weights: PlacementWeights,
    },
}

#[derive(Clone, Debug, PartialEq)]
pub struct OptimizationPlan {
    pub phases: Vec<OptimizationPhase>,
}

impl OptimizationPlan {
    #[must_use]
    pub fn directional_then_global(
        axis: CompressionAxis,
        direction: CompressionDirection,
        anchor: AnchorPolicy,
    ) -> Self {
        Self {
            phases: vec![
                OptimizationPhase::DirectionalCompress {
                    axis,
                    direction,
                    anchor,
                    max_steps: 128,
                    move_step: 1,
                    weights: DirectionalWeights::default(),
                },
                OptimizationPhase::GlobalCompact {
                    max_steps: 128,
                    move_step: 1,
                    weights: PlacementWeights::default(),
                },
            ],
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PhaseScore {
    pub total: f64,
    pub axial_span: Option<i32>,
    pub placement: PlacementScore,
}

#[derive(Clone, Debug, PartialEq)]
pub struct PhaseOptimizationResult {
    pub phase: OptimizationPhase,
    pub initial_score: PhaseScore,
    pub final_score: PhaseScore,
    pub accepted: Vec<PlacementMutation>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct StagedOptimizationResult {
    pub circuit: PlacementCircuit,
    pub phases: Vec<PhaseOptimizationResult>,
}

fn coordinate(pos: Pos, axis: CompressionAxis) -> i32 {
    match axis {
        CompressionAxis::X => pos.x,
        CompressionAxis::Y => pos.y,
        CompressionAxis::Z => pos.z,
    }
}

fn axial_span(circuit: &PlacementCircuit, axis: CompressionAxis) -> i32 {
    let mut coordinates = circuit
        .cells
        .values()
        .flat_map(|node| node.placed.blocks())
        .map(|(pos, _)| coordinate(pos, axis));
    let Some(first) = coordinates.next() else {
        return 0;
    };
    let (minimum, maximum) = coordinates.fold((first, first), |(minimum, maximum), value| {
        (minimum.min(value), maximum.max(value))
    });
    maximum - minimum + 1
}

fn anchored_cells(circuit: &PlacementCircuit, policy: &AnchorPolicy) -> BTreeSet<CellId> {
    match policy {
        AnchorPolicy::Inputs => circuit
            .routes
            .values()
            .filter(|route| route.source.cell.is_none())
            .filter_map(|route| route.sink.cell)
            .collect(),
        AnchorPolicy::Outputs => circuit
            .routes
            .values()
            .filter(|route| route.sink.cell.is_none())
            .filter_map(|route| route.source.cell)
            .collect(),
        AnchorPolicy::Cells(cells) => cells.clone(),
    }
}

fn directional_score(
    circuit: &PlacementCircuit,
    axis: CompressionAxis,
    weights: DirectionalWeights,
) -> PhaseScore {
    let placement_weights = PlacementWeights {
        wire_distance: weights.wire_distance,
        bounding_volume: weights.bounding_volume,
        cell_block_count: weights.cell_block_count,
        overlap_penalty: weights.overlap_penalty,
    };
    let placement = placement_score(circuit, placement_weights);
    let span = axial_span(circuit, axis);
    PhaseScore {
        total: placement.total + weights.axial_span * f64::from(span),
        axial_span: Some(span),
        placement,
    }
}

fn global_score(circuit: &PlacementCircuit, weights: PlacementWeights) -> PhaseScore {
    let placement = placement_score(circuit, weights);
    PhaseScore {
        total: placement.total,
        axial_span: None,
        placement,
    }
}

fn move_matches_direction(
    mutation: &PlacementMutation,
    axis: CompressionAxis,
    direction: CompressionDirection,
) -> bool {
    if mutation.kind != MutationKind::Move {
        return false;
    }
    let displacement = coordinate(mutation.delta, axis);
    match direction {
        CompressionDirection::TowardMinimum => displacement < 0,
        CompressionDirection::TowardMaximum => displacement > 0,
    }
}

fn optimize_phase(
    circuit: &PlacementCircuit,
    phase: &OptimizationPhase,
    focus: Option<&BTreeSet<CellId>>,
    accept_candidate: &mut impl FnMut(&PlacementCircuit) -> bool,
) -> (PlacementCircuit, PhaseOptimizationResult) {
    let library = default_cell_library();
    let mut current = circuit.clone();
    let (max_steps, move_step) = match phase {
        OptimizationPhase::DirectionalCompress {
            max_steps,
            move_step,
            ..
        }
        | OptimizationPhase::GlobalCompact {
            max_steps,
            move_step,
            ..
        } => (*max_steps, *move_step),
    };
    let score = |candidate: &PlacementCircuit| match phase {
        OptimizationPhase::DirectionalCompress { axis, weights, .. } => {
            directional_score(candidate, *axis, *weights)
        }
        OptimizationPhase::GlobalCompact { weights, .. } => global_score(candidate, *weights),
    };
    let initial_score = score(&current);
    let mut current_score = initial_score;
    let mut accepted = Vec::new();
    let mut keepout_contacts = electrical_keepout_contacts(&current);
    let anchors = match phase {
        OptimizationPhase::DirectionalCompress { anchor, .. } => anchored_cells(&current, anchor),
        OptimizationPhase::GlobalCompact { .. } => BTreeSet::new(),
    };

    for _ in 0..max_steps {
        let mut improving = Vec::new();
        for mutation in candidate_mutations(&current, &library, move_step) {
            if focus
                .is_some_and(|focus| mutation.affected_cells().any(|cell| !focus.contains(&cell)))
            {
                continue;
            }
            if mutation
                .affected_cells()
                .any(|cell| anchors.contains(&cell))
            {
                continue;
            }
            if let OptimizationPhase::DirectionalCompress {
                axis, direction, ..
            } = phase
                && !move_matches_direction(&mutation, *axis, *direction)
            {
                continue;
            }
            let candidate = apply_mutation(&current, &mutation, &library);
            if electrical_keepout_contacts(&candidate) > keepout_contacts {
                continue;
            }
            let candidate_score = score(&candidate);
            if candidate_score.total < current_score.total {
                improving.push((mutation, candidate, candidate_score));
            }
        }
        improving.sort_by(|left, right| left.2.total.total_cmp(&right.2.total));
        let Some((mutation, candidate, candidate_score)) = improving
            .into_iter()
            .find(|(_, candidate, _)| accept_candidate(candidate))
        else {
            break;
        };
        accepted.push(mutation);
        current = candidate;
        current_score = candidate_score;
        keepout_contacts = electrical_keepout_contacts(&current);
    }

    (
        current,
        PhaseOptimizationResult {
            phase: phase.clone(),
            initial_score,
            final_score: current_score,
            accepted,
        },
    )
}

/// Executes each phase with its own objective. A later phase may therefore
/// accept a different trade-off than an earlier phase.
#[must_use]
pub fn optimize_staged(
    circuit: &PlacementCircuit,
    plan: &OptimizationPlan,
) -> StagedOptimizationResult {
    optimize_staged_with_validator(circuit, plan, |_| true)
}

pub(crate) fn optimize_staged_with_validator(
    circuit: &PlacementCircuit,
    plan: &OptimizationPlan,
    mut accept_candidate: impl FnMut(&PlacementCircuit) -> bool,
) -> StagedOptimizationResult {
    let mut current = circuit.clone();
    let mut results = Vec::with_capacity(plan.phases.len());
    for phase in &plan.phases {
        let (next, result) = optimize_phase(&current, phase, None, &mut accept_candidate);
        current = next;
        results.push(result);
    }
    StagedOptimizationResult {
        circuit: current,
        phases: results,
    }
}

fn focus_windows(circuit: &PlacementCircuit, max_cells: usize) -> Vec<BTreeSet<CellId>> {
    let mut adjacency = std::collections::BTreeMap::<CellId, BTreeSet<CellId>>::new();
    for id in circuit.cells.keys() {
        adjacency.entry(*id).or_default();
    }
    for route in circuit.routes.values() {
        let (Some(first), Some(second)) = (route.source.cell, route.sink.cell) else {
            continue;
        };
        adjacency.entry(first).or_default().insert(second);
        adjacency.entry(second).or_default().insert(first);
    }
    adjacency
        .into_iter()
        .map(|(seed, neighbors)| {
            std::iter::once(seed)
                .chain(neighbors)
                .take(max_cells.max(1))
                .collect()
        })
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect()
}

pub(crate) fn optimize_staged_windowed_with_validator(
    circuit: &PlacementCircuit,
    plan: &OptimizationPlan,
    max_focus_cells: usize,
    max_sweeps: usize,
    mut accept_candidate: impl FnMut(&PlacementCircuit) -> bool,
) -> StagedOptimizationResult {
    let mut current = circuit.clone();
    let initial_scores = plan
        .phases
        .iter()
        .map(|phase| match phase {
            OptimizationPhase::DirectionalCompress { axis, weights, .. } => {
                directional_score(&current, *axis, *weights)
            }
            OptimizationPhase::GlobalCompact { weights, .. } => global_score(&current, *weights),
        })
        .collect::<Vec<_>>();
    let mut accepted = vec![Vec::new(); plan.phases.len()];
    for _ in 0..max_sweeps.max(1) {
        let mut improved = false;
        for focus in focus_windows(&current, max_focus_cells) {
            for (index, phase) in plan.phases.iter().enumerate() {
                let (next, result) =
                    optimize_phase(&current, phase, Some(&focus), &mut accept_candidate);
                improved |= !result.accepted.is_empty();
                accepted[index].extend(result.accepted);
                current = next;
            }
        }
        if !improved {
            break;
        }
    }
    let phases = plan
        .phases
        .iter()
        .enumerate()
        .map(|(index, phase)| PhaseOptimizationResult {
            phase: phase.clone(),
            initial_score: initial_scores[index],
            final_score: match phase {
                OptimizationPhase::DirectionalCompress { axis, weights, .. } => {
                    directional_score(&current, *axis, *weights)
                }
                OptimizationPhase::GlobalCompact { weights, .. } => {
                    global_score(&current, *weights)
                }
            },
            accepted: std::mem::take(&mut accepted[index]),
        })
        .collect();
    StagedOptimizationResult {
        circuit: current,
        phases,
    }
}

#[cfg(test)]
mod tests {
    use dustroute_translate::cells::{PlacedCell, PortKind, RotationY, not_cell};
    use dustroute_translate::logic::GateKind;

    use super::*;

    fn linear_circuit() -> (PlacementCircuit, CellId, CellId) {
        let mut circuit = PlacementCircuit::new();
        let first = circuit.add_cell(
            GateKind::Not,
            PlacedCell {
                cell: not_cell(),
                origin: Pos::new(0, 2, 0),
                rotation: RotationY::R0,
            },
        );
        let second = circuit.add_cell(
            GateKind::Not,
            PlacedCell {
                cell: not_cell(),
                origin: Pos::new(14, 2, 6),
                rotation: RotationY::R0,
            },
        );
        let input = PlacementCircuit::boundary("in", Pos::new(-2, 3, 0), PortKind::Wire, None);
        let output = PlacementCircuit::boundary("out", Pos::new(24, 3, 6), PortKind::Wire, None);
        circuit.add_route(
            input,
            circuit.input_endpoint(first, "a").unwrap(),
            vec![],
            vec![],
        );
        circuit.add_route(
            circuit.output_endpoint(first, "out").unwrap(),
            circuit.input_endpoint(second, "a").unwrap(),
            vec![],
            vec![],
        );
        circuit.add_route(
            circuit.output_endpoint(second, "out").unwrap(),
            output,
            vec![],
            vec![],
        );
        (circuit, first, second)
    }

    #[test]
    fn directional_phase_keeps_input_anchor_and_compresses_toward_it() {
        let (circuit, first, second) = linear_circuit();
        let plan = OptimizationPlan {
            phases: vec![OptimizationPhase::DirectionalCompress {
                axis: CompressionAxis::X,
                direction: CompressionDirection::TowardMinimum,
                anchor: AnchorPolicy::Inputs,
                max_steps: 32,
                move_step: 1,
                weights: DirectionalWeights::default(),
            }],
        };
        let result = optimize_staged(&circuit, &plan);
        assert_eq!(
            result.circuit.cells[&first].placed.origin,
            Pos::new(0, 2, 0)
        );
        assert!(result.circuit.cells[&second].placed.origin.x < 14);
        assert!(
            result.phases[0].final_score.axial_span < result.phases[0].initial_score.axial_span
        );
    }

    #[test]
    fn staged_plan_records_independent_phase_scores() {
        let (circuit, _, _) = linear_circuit();
        let plan = OptimizationPlan::directional_then_global(
            CompressionAxis::X,
            CompressionDirection::TowardMinimum,
            AnchorPolicy::Inputs,
        );
        let result = optimize_staged(&circuit, &plan);
        assert_eq!(result.phases.len(), 2);
        assert!(result.phases[0].final_score.axial_span.is_some());
        assert!(result.phases[1].final_score.axial_span.is_none());
        assert!(
            result
                .phases
                .iter()
                .all(|phase| phase.final_score.total <= phase.initial_score.total)
        );
    }

    #[test]
    fn directional_phase_can_trade_wire_length_for_a_smaller_span() {
        let (mut circuit, first, second) = linear_circuit();
        let extra_output =
            PlacementCircuit::boundary("out_2", Pos::new(24, 3, 6), PortKind::Wire, None);
        circuit.add_route(
            circuit.output_endpoint(second, "out").unwrap(),
            extra_output,
            vec![],
            vec![],
        );
        let plan = OptimizationPlan {
            phases: vec![OptimizationPhase::DirectionalCompress {
                axis: CompressionAxis::X,
                direction: CompressionDirection::TowardMinimum,
                anchor: AnchorPolicy::Cells(BTreeSet::from([first])),
                max_steps: 1,
                move_step: 1,
                weights: DirectionalWeights::default(),
            }],
        };
        let result = optimize_staged(&circuit, &plan);
        let phase = &result.phases[0];
        assert!(phase.final_score.axial_span < phase.initial_score.axial_span);
        assert!(
            phase.final_score.placement.wire_distance > phase.initial_score.placement.wire_distance
        );
        assert!(phase.final_score.total < phase.initial_score.total);
    }

    #[test]
    fn validator_falls_back_to_the_next_best_improving_candidate() {
        let (circuit, first, second) = linear_circuit();
        let plan = OptimizationPlan {
            phases: vec![OptimizationPhase::GlobalCompact {
                max_steps: 1,
                move_step: 1,
                weights: PlacementWeights::default(),
            }],
        };
        let initial_first = circuit.cells[&first].placed.origin;
        let initial_second = circuit.cells[&second].placed.origin;
        let result = optimize_staged_with_validator(&circuit, &plan, |candidate| {
            candidate.cells[&first].placed.origin == initial_first
        });
        assert_eq!(result.phases[0].accepted.len(), 1);
        assert_eq!(result.circuit.cells[&first].placed.origin, initial_first);
        assert_ne!(result.circuit.cells[&second].placed.origin, initial_second);
    }

    #[test]
    fn focus_windows_are_bounded_and_cover_every_cell() {
        let (circuit, _, _) = linear_circuit();
        let windows = focus_windows(&circuit, 2);
        assert!(windows.iter().all(|window| window.len() <= 2));
        let covered = windows.iter().flatten().copied().collect::<BTreeSet<_>>();
        assert_eq!(covered, circuit.cells.keys().copied().collect());
    }

    #[test]
    fn windowed_sweeps_accumulate_local_improvements_until_convergence() {
        let (circuit, _, _) = linear_circuit();
        let plan = OptimizationPlan {
            phases: vec![OptimizationPhase::GlobalCompact {
                max_steps: 1,
                move_step: 1,
                weights: PlacementWeights::default(),
            }],
        };
        let result = optimize_staged_windowed_with_validator(&circuit, &plan, 1, 4, |_| true);
        assert!(result.phases[0].accepted.len() >= 2);
        assert!(result.phases[0].final_score.total < result.phases[0].initial_score.total);
    }
}
