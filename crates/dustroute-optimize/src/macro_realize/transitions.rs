//! Transition traces and boundary-strength verification.
use super::boundary::{boundary_terminal_mapping, set_driver_in_world};
use super::{
    ContextualVerificationState, MacroTransitionCase, MacroTransitionEdge, MacroTransitionReport,
};
use dustroute_physical::{Pos, World};
use dustroute_translate::{InferredTruthTable, RegionBounds};

/// Exhaustively compares ordered input transitions for small Boolean cells.
/// Output samples include tick zero after the simultaneous input update.
#[must_use]
pub fn verify_macro_transitions(
    expected: &InferredTruthTable,
    original: &World,
    candidate: &World,
    settle_ticks: usize,
    observe_ticks: usize,
    max_inputs: usize,
) -> MacroTransitionReport {
    let original_context = match prepare_transition_context(original, expected, settle_ticks) {
        Ok(context) => context,
        Err(reason) => return unavailable_transitions(reason),
    };
    let candidate_context = MacroTransitionContext {
        world: candidate.clone(),
        drivers: original_context.drivers.clone(),
        outputs: original_context.outputs.clone(),
    };
    compare_transition_contexts(
        expected.inputs.len(),
        original_context,
        candidate_context,
        settle_ticks,
        observe_ticks,
        max_inputs,
    )
}

/// Compares independently inferred interfaces by terminal order. This is for
/// optimizations where an inferred terminal anchor may move even though the
/// Boolean interface remains comparable.
#[must_use]
pub fn verify_world_transitions(
    original: &World,
    original_truth: &InferredTruthTable,
    candidate: &World,
    candidate_truth: &InferredTruthTable,
    settle_ticks: usize,
    observe_ticks: usize,
    max_inputs: usize,
) -> MacroTransitionReport {
    if original_truth.inputs.len() != candidate_truth.inputs.len()
        || original_truth.outputs.len() != candidate_truth.outputs.len()
    {
        return unavailable_transitions(
            "original and candidate terminal counts are not comparable".to_owned(),
        );
    }
    let original_context = match prepare_transition_context(original, original_truth, settle_ticks)
    {
        Ok(context) => context,
        Err(reason) => return unavailable_transitions(reason),
    };
    let candidate_context =
        match prepare_transition_context(candidate, candidate_truth, settle_ticks) {
            Ok(context) => context,
            Err(reason) => return unavailable_transitions(reason),
        };
    compare_transition_contexts(
        original_truth.inputs.len(),
        original_context,
        candidate_context,
        settle_ticks,
        observe_ticks,
        max_inputs,
    )
}

/// Verifies exact dust signal strength at fixed physical boundary positions
/// for every inferred steady-state input assignment.
pub fn verify_boundary_strengths(
    original: &World,
    candidate: &World,
    truth: &InferredTruthTable,
    positions: &[Pos],
    settle_ticks: usize,
) -> Result<(), String> {
    let (low, high) = original
        .bounds()
        .ok_or_else(|| "strength-verification world is empty".to_owned())?;
    let analysis =
        dustroute_translate::analyze_world_region(original, RegionBounds::new(low, high));
    let drivers = truth
        .inputs
        .iter()
        .map(|terminal| dustroute_translate::inferred_input_driver(original, &analysis, terminal))
        .collect::<Result<Vec<_>, _>>()
        .map_err(|error| error.to_string())?;
    for row in &truth.rows {
        let original_state =
            settled_state_for_inputs(original, &drivers, &row.inputs, settle_ticks)?;
        let candidate_state =
            settled_state_for_inputs(candidate, &drivers, &row.inputs, settle_ticks)?;
        for position in positions {
            let before = original_state.strength(*position);
            let after = candidate_state.strength(*position);
            if before != after {
                return Err(format!(
                    "boundary signal strength at {position:?} changes from {before} to {after} for inputs {:?}",
                    row.inputs
                ));
            }
        }
    }
    Ok(())
}

fn settled_state_for_inputs(
    world: &World,
    drivers: &[dustroute_translate::InferredInputDriver],
    inputs: &[bool],
    settle_ticks: usize,
) -> Result<dustroute_translate::TickState, String> {
    let mut driven = world.clone();
    for (driver, powered) in drivers.iter().zip(inputs) {
        set_driver_in_world(&mut driven, *driver, *powered)?;
    }
    dustroute_translate::update_wire_shapes(&mut driven);
    dustroute_translate::RedstoneTickSimulator::new(driven)
        .and_then(|mut simulator| simulator.settle_ticks(settle_ticks))
        .map_err(|error| error.to_string())
}

fn compare_transition_contexts(
    input_count: usize,
    original_context: MacroTransitionContext,
    candidate_context: MacroTransitionContext,
    settle_ticks: usize,
    observe_ticks: usize,
    max_inputs: usize,
) -> MacroTransitionReport {
    if input_count > max_inputs || input_count >= usize::BITS as usize {
        return unavailable_transitions(format!(
            "cannot exhaustively enumerate {input_count} transition inputs"
        ));
    }
    let states = 1_usize << input_count;
    let original_states = match settled_transition_states(&original_context, states, settle_ticks) {
        Ok(states) => states,
        Err(reason) => return unavailable_transitions(reason),
    };
    let candidate_states = match settled_transition_states(&candidate_context, states, settle_ticks)
    {
        Ok(states) => states,
        Err(reason) => return unavailable_transitions(reason),
    };
    let mut cases = Vec::new();
    for from_bits in 0..states {
        for to_bits in 0..states {
            if from_bits == to_bits {
                continue;
            }
            let from = bits(from_bits, input_count);
            let to = bits(to_bits, input_count);
            let original_outputs = match simulate_boundary_transition(
                &original_context,
                &original_states[from_bits],
                &to,
                observe_ticks,
            ) {
                Ok(trace) => trace,
                Err(reason) => return unavailable_transitions(reason),
            };
            let candidate_outputs = match simulate_boundary_transition(
                &candidate_context,
                &candidate_states[from_bits],
                &to,
                observe_ticks,
            ) {
                Ok(trace) => trace,
                Err(reason) => return unavailable_transitions(reason),
            };
            let first_difference_tick = original_outputs
                .iter()
                .zip(&candidate_outputs)
                .position(|(original, candidate)| original != candidate);
            cases.push(MacroTransitionCase {
                from,
                to,
                equivalent: first_difference_tick.is_none()
                    && original_outputs.len() == candidate_outputs.len(),
                first_difference_tick,
                original_outputs,
                candidate_outputs,
            });
        }
    }
    let differing_cases = cases.iter().filter(|case| !case.equivalent).count();
    MacroTransitionReport {
        state: if differing_cases == 0 {
            ContextualVerificationState::Passed
        } else {
            ContextualVerificationState::Failed
        },
        cases,
        differing_cases,
        reason: None,
    }
}

pub(super) fn transition_edges(trace: &[Vec<bool>]) -> Vec<MacroTransitionEdge> {
    let mut previous_edge_tick = None;
    trace
        .windows(2)
        .enumerate()
        .filter(|(_, pair)| pair[0] != pair[1])
        .map(|(index, pair)| {
            let at_tick = index + 1;
            let edge = MacroTransitionEdge {
                at_tick,
                from: pair[0].clone(),
                to: pair[1].clone(),
                elapsed_from_previous: previous_edge_tick
                    .map(|previous| at_tick.saturating_sub(previous)),
            };
            previous_edge_tick = Some(at_tick);
            edge
        })
        .collect()
}

struct MacroTransitionContext {
    world: World,
    drivers: Vec<dustroute_translate::InferredInputDriver>,
    outputs: Vec<Pos>,
}

fn prepare_transition_context(
    world: &World,
    expected: &InferredTruthTable,
    settle_ticks: usize,
) -> Result<MacroTransitionContext, String> {
    let (low, high) = world
        .bounds()
        .ok_or_else(|| "transition world is empty".to_owned())?;
    let analysis = dustroute_translate::analyze_world_region(world, RegionBounds::new(low, high));
    let inferred = dustroute_translate::infer_truth_table(
        world,
        &analysis,
        expected.inputs.len(),
        settle_ticks,
    )
    .map_err(|error| error.to_string())?;
    let mapping = boundary_terminal_mapping(
        expected.inputs.iter().map(|terminal| terminal.anchor),
        &inferred,
        &analysis,
        true,
    )
    .ok_or_else(|| "transition input boundary mapping is ambiguous".to_owned())?;
    let drivers = mapping
        .iter()
        .map(|index| {
            dustroute_translate::inferred_input_driver(world, &analysis, &inferred.inputs[*index])
                .map_err(|error| error.to_string())
        })
        .collect::<Result<Vec<_>, _>>()?;
    Ok(MacroTransitionContext {
        world: world.clone(),
        drivers,
        outputs: expected
            .outputs
            .iter()
            .map(|terminal| terminal.anchor)
            .collect(),
    })
}

fn simulate_boundary_transition(
    context: &MacroTransitionContext,
    settled: &dustroute_translate::RedstoneTickSimulator,
    to: &[bool],
    observe_ticks: usize,
) -> Result<Vec<Vec<bool>>, String> {
    let mut simulator = settled.clone();
    for (driver, powered) in context.drivers.iter().zip(to) {
        match driver {
            dustroute_translate::InferredInputDriver::Lever(pos) => simulator
                .set_powered(*pos, *powered)
                .map_err(|error| error.to_string())?,
            dustroute_translate::InferredInputDriver::Button(pos) => simulator
                .set_button_state(*pos, *powered)
                .map_err(|error| error.to_string())?,
            dustroute_translate::InferredInputDriver::PressurePlate(pos) => simulator
                .set_pressure_plate_level(*pos, if *powered { 15 } else { 0 })
                .map_err(|error| error.to_string())?,
            dustroute_translate::InferredInputDriver::External(pos) => simulator
                .set_external_powered(*pos, *powered)
                .map_err(|error| error.to_string())?,
        };
    }
    let observe = |state: &dustroute_translate::TickState| {
        context
            .outputs
            .iter()
            .map(|position| state.powered(*position))
            .collect::<Vec<_>>()
    };
    let mut trace = vec![observe(&simulator.snapshot())];
    for _ in 0..observe_ticks {
        let state = simulator
            .advance_tick()
            .map_err(|error| error.to_string())?;
        trace.push(observe(&state));
    }
    Ok(trace)
}

fn settled_transition_states(
    context: &MacroTransitionContext,
    state_count: usize,
    settle_ticks: usize,
) -> Result<Vec<dustroute_translate::RedstoneTickSimulator>, String> {
    (0..state_count)
        .map(|value| {
            let mut driven = context.world.clone();
            for (driver, powered) in context
                .drivers
                .iter()
                .zip(bits(value, context.drivers.len()))
            {
                set_driver_in_world(&mut driven, *driver, powered)?;
            }
            dustroute_translate::update_wire_shapes(&mut driven);
            let mut simulator = dustroute_translate::RedstoneTickSimulator::new(driven)
                .map_err(|error| error.to_string())?;
            simulator
                .settle_ticks(settle_ticks)
                .map_err(|error| error.to_string())?;
            Ok(simulator)
        })
        .collect()
}

fn bits(value: usize, count: usize) -> Vec<bool> {
    (0..count).map(|index| value & (1 << index) != 0).collect()
}

fn unavailable_transitions(reason: String) -> MacroTransitionReport {
    MacroTransitionReport {
        state: ContextualVerificationState::Pending,
        cases: Vec::new(),
        differing_cases: 0,
        reason: Some(reason),
    }
}
