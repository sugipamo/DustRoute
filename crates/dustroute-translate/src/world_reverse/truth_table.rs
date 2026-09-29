//! Budgeted compatibility-model execution and truth-table comparison.
use super::drivers::{inferred_driver_position, inferred_input_driver};
use super::{InferredTruthTable, RegionAnalysis, TruthTableRow};
use crate::sim::{RedstoneTickSimulator, TickState};
use crate::wire::update_wire_shapes;
use crate::world::{BlockKind, Pos, World};
use serde::{Deserialize, Serialize};
use std::time::Instant;

/// Bounds the amount of exhaustive simulation used for reverse translation.
///
/// `max_rows` limits the number of input assignments.  `max_work_units` is a
/// conservative estimate of the full-world work performed by each assignment:
/// one initial pass plus one pass per requested settle tick over every observed
/// block.  The estimate is intentionally checked before the first simulator is
/// created so an oversized request fails closed without allocating a row-sized
/// set of worlds.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct TruthTableBudget {
    pub max_rows: usize,
    pub max_work_units: u128,
    /// Maximum cumulative instantaneous solver iterations across all rows.
    ///
    /// The static work estimate cannot account for feedback loops or other
    /// circuits that need many fixed-point iterations.  This dynamic guard is
    /// charged after every simulator step and makes that cost bounded too.
    pub max_solver_iterations: usize,
    /// Optional wall-clock budget for exhaustive inference.  The timer starts
    /// immediately before the first input row is simulated.  A limit is
    /// intentionally optional at the library layer so callers that already
    /// enforce their own deadline can opt out.
    pub max_elapsed_millis: Option<u64>,
}

/// Runtime counters collected while exhaustive truth-table inference runs.
///
/// These counters are intentionally separate from [`InferredTruthTable`]: the
/// table remains the stable result type, while callers that need to attribute
/// cost (benchmarks, telemetry, or a UI) can opt into the extended API without
/// changing existing inference call sites.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct TruthTableExecutionStats {
    /// Number of input assignments planned by the inference.
    pub rows_requested: usize,
    /// Number of rows that were fully simulated and included in the result.
    pub rows_completed: usize,
    /// Maximum number of settle ticks requested for each row.
    pub settle_ticks_requested: usize,
    /// Total `advance_tick` calls across all completed rows.  Early settling
    /// can make this lower than `rows_completed * settle_ticks_requested`.
    pub settle_ticks_executed: usize,
    /// Sum of instantaneous fixed-point iterations reported by every
    /// simulator snapshot and tick.
    pub solver_iterations: usize,
    /// Time spent cloning the observed world for each input assignment.
    pub world_clone_nanos: u64,
    /// Time spent applying inferred input values to each cloned world.
    pub input_drive_nanos: u64,
    /// Time spent recomputing redstone wire connection shapes.
    pub wire_shape_update_nanos: u64,
    /// Time spent constructing a simulator and taking its initial snapshot.
    pub simulator_init_nanos: u64,
    /// Time spent advancing and checking settle ticks.
    pub settle_nanos: u64,
    /// Time spent reading output terminals and appending completed rows.
    pub output_read_nanos: u64,
    /// Wall-clock duration of inference, rounded down to milliseconds.
    pub elapsed_millis: u64,
}

impl Default for TruthTableBudget {
    fn default() -> Self {
        Self::DEFAULT
    }
}

impl TruthTableBudget {
    pub const DEFAULT: Self = Self {
        max_rows: 256,
        max_work_units: 2_000_000,
        max_solver_iterations: 1_000_000,
        max_elapsed_millis: Some(120_000),
    };

    #[must_use]
    pub const fn new(max_rows: usize, max_work_units: u128) -> Self {
        Self {
            max_rows,
            max_work_units,
            max_solver_iterations: Self::DEFAULT.max_solver_iterations,
            max_elapsed_millis: Self::DEFAULT.max_elapsed_millis,
        }
    }

    #[must_use]
    pub const fn with_max_solver_iterations(mut self, max_solver_iterations: usize) -> Self {
        self.max_solver_iterations = max_solver_iterations;
        self
    }

    #[must_use]
    pub const fn with_max_elapsed_millis(mut self, max_elapsed_millis: Option<u64>) -> Self {
        self.max_elapsed_millis = max_elapsed_millis;
        self
    }

    #[must_use]
    pub fn estimate_work_units(
        self,
        world_blocks: usize,
        input_count: usize,
        settle_ticks: usize,
    ) -> Option<(usize, u128)> {
        let input_count = u32::try_from(input_count).ok()?;
        let rows = 1_usize.checked_shl(input_count)?;
        let work_units = (rows as u128)
            .saturating_mul(world_blocks as u128)
            .saturating_mul((settle_ticks as u128).saturating_add(1));
        Some((rows, work_units))
    }
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct TruthTableComparison {
    pub comparable: bool,
    pub expected_inputs: usize,
    pub actual_inputs: usize,
    pub expected_outputs: usize,
    pub actual_outputs: usize,
    pub differing_rows: usize,
    pub differing_bits: usize,
    pub terminal_count_delta: usize,
    pub fitness_penalty: usize,
}

#[must_use]
pub fn compare_truth_tables(
    expected: &InferredTruthTable,
    actual: &InferredTruthTable,
) -> TruthTableComparison {
    let comparable = expected.inputs.len() == actual.inputs.len()
        && expected.outputs.len() == actual.outputs.len()
        && expected.rows.len() == actual.rows.len();
    let common_rows = expected.rows.iter().zip(&actual.rows);
    let differing_rows = common_rows
        .clone()
        .filter(|(expected, actual)| {
            expected
                .outputs
                .iter()
                .zip(&actual.outputs)
                .any(|(expected, actual)| expected != actual)
        })
        .count();
    let differing_bits = common_rows
        .map(|(expected, actual)| {
            expected
                .outputs
                .iter()
                .zip(&actual.outputs)
                .filter(|(expected, actual)| expected != actual)
                .count()
        })
        .sum();
    let terminal_count_delta = expected.inputs.len().abs_diff(actual.inputs.len())
        + expected.outputs.len().abs_diff(actual.outputs.len());
    let structural_penalty = if comparable {
        0
    } else {
        expected.rows.len() * expected.outputs.len().max(actual.outputs.len()).max(1)
            + terminal_count_delta
    };
    let fitness_penalty = differing_bits + structural_penalty;
    TruthTableComparison {
        comparable,
        expected_inputs: expected.inputs.len(),
        actual_inputs: actual.inputs.len(),
        expected_outputs: expected.outputs.len(),
        actual_outputs: actual.outputs.len(),
        differing_rows,
        differing_bits,
        terminal_count_delta,
        fitness_penalty,
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum TruthTableError {
    TooManyInputs(usize),
    BudgetExceeded {
        rows: usize,
        max_rows: usize,
        estimated_work_units: u128,
        max_work_units: u128,
    },
    RuntimeBudgetExceeded {
        rows: usize,
        completed_rows: usize,
        solver_iterations: usize,
        max_solver_iterations: usize,
    },
    ElapsedBudgetExceeded {
        rows: usize,
        completed_rows: usize,
        elapsed_millis: u128,
        max_elapsed_millis: u64,
    },
    NonSettling {
        row: usize,
        settle_ticks: usize,
        pending_events: bool,
    },
    IncompleteObservation,
    NoInputs,
    NoOutputs,
    UnmappedExternalInputs(Vec<Pos>),
    UnmappedObservableOutputs(Vec<Pos>),
    AmbiguousInputMapping {
        external_inputs: usize,
        inferred_inputs: usize,
    },
    AmbiguousOutputMapping {
        observable_outputs: usize,
        inferred_outputs: usize,
    },
    NoDriverPosition(Pos),
    InvalidDriver {
        position: Pos,
        expected: &'static str,
        actual: BlockKind,
    },
    Simulation(String),
}

impl std::fmt::Display for TruthTableError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::TooManyInputs(count) => write!(f, "cannot enumerate {count} inferred inputs"),
            Self::BudgetExceeded {
                rows,
                max_rows,
                estimated_work_units,
                max_work_units,
            } => write!(
                f,
                "truth-table budget exceeded: {rows} rows (max {max_rows}), estimated {estimated_work_units} work units (max {max_work_units})"
            ),
            Self::RuntimeBudgetExceeded {
                rows,
                completed_rows,
                solver_iterations,
                max_solver_iterations,
            } => write!(
                f,
                "truth-table runtime budget exceeded after {completed_rows}/{rows} rows: {solver_iterations} solver iterations (max {max_solver_iterations})"
            ),
            Self::ElapsedBudgetExceeded {
                rows,
                completed_rows,
                elapsed_millis,
                max_elapsed_millis,
            } => write!(
                f,
                "truth-table elapsed-time budget exceeded after {completed_rows}/{rows} rows: {elapsed_millis} ms (max {max_elapsed_millis} ms)"
            ),
            Self::NonSettling {
                row,
                settle_ticks,
                pending_events,
            } => write!(
                f,
                "truth-table row {row} did not settle within {settle_ticks} ticks (pending_events={pending_events})"
            ),
            Self::IncompleteObservation => {
                f.write_str("cannot infer a truth table from an incomplete physical observation")
            }
            Self::NoInputs => f.write_str("cannot verify a circuit without an inferred input"),
            Self::NoOutputs => f.write_str("cannot verify a circuit without an observable output"),
            Self::UnmappedExternalInputs(positions) => write!(
                f,
                "external input sources are not mapped to inferred terminals: {positions:?}"
            ),
            Self::UnmappedObservableOutputs(positions) => write!(
                f,
                "observable outputs are not mapped to inferred terminals: {positions:?}"
            ),
            Self::AmbiguousInputMapping {
                external_inputs,
                inferred_inputs,
            } => write!(
                f,
                "external input mapping is ambiguous: {external_inputs} physical sources map to {inferred_inputs} inferred terminals"
            ),
            Self::AmbiguousOutputMapping {
                observable_outputs,
                inferred_outputs,
            } => write!(
                f,
                "observable output mapping is ambiguous: {observable_outputs} physical sinks map to {inferred_outputs} inferred terminals"
            ),
            Self::NoDriverPosition(pos) => {
                write!(f, "no safe driver position for input at {pos:?}")
            }
            Self::InvalidDriver {
                position,
                expected,
                actual,
            } => write!(
                f,
                "input driver at {position:?} expected {expected}, found {actual:?}"
            ),
            Self::Simulation(message) => f.write_str(message),
        }
    }
}

impl std::error::Error for TruthTableError {}

pub fn infer_truth_table(
    world: &World,
    analysis: &RegionAnalysis,
    max_inputs: usize,
    settle_ticks: usize,
) -> Result<InferredTruthTable, TruthTableError> {
    infer_truth_table_with_budget(
        world,
        analysis,
        max_inputs,
        settle_ticks,
        TruthTableBudget::default(),
    )
}

pub fn infer_truth_table_with_budget(
    world: &World,
    analysis: &RegionAnalysis,
    max_inputs: usize,
    settle_ticks: usize,
    budget: TruthTableBudget,
) -> Result<InferredTruthTable, TruthTableError> {
    infer_truth_table_with_budget_and_stats(world, analysis, max_inputs, settle_ticks, budget)
        .map(|(table, _stats)| table)
}

/// Infer a complete truth table and return execution counters alongside it.
///
/// This is the instrumented counterpart to [`infer_truth_table_with_budget`].
/// It shares exactly the same budget checks and fail-closed behavior; the only
/// difference is that successful callers also receive measured simulation
/// cost.  Incomplete rows are never returned as a successful table.
pub fn infer_truth_table_with_budget_and_stats(
    world: &World,
    analysis: &RegionAnalysis,
    max_inputs: usize,
    settle_ticks: usize,
    budget: TruthTableBudget,
) -> Result<(InferredTruthTable, TruthTableExecutionStats), TruthTableError> {
    if !analysis.interface.unmapped_inputs.is_empty() {
        return Err(TruthTableError::UnmappedExternalInputs(
            analysis.interface.unmapped_inputs.iter().copied().collect(),
        ));
    }
    if !analysis.interface.unmapped_outputs.is_empty() {
        return Err(TruthTableError::UnmappedObservableOutputs(
            analysis
                .interface
                .unmapped_outputs
                .iter()
                .copied()
                .collect(),
        ));
    }
    if analysis.inputs.is_empty() {
        return Err(TruthTableError::NoInputs);
    }
    if analysis.outputs.is_empty() {
        return Err(TruthTableError::NoOutputs);
    }
    if !analysis.interface.external_inputs.is_empty()
        && analysis.interface.mapped_inputs.len() != analysis.inputs.len()
    {
        return Err(TruthTableError::AmbiguousInputMapping {
            external_inputs: analysis.interface.external_inputs.len(),
            inferred_inputs: analysis.inputs.len(),
        });
    }
    if !analysis.interface.observable_outputs.is_empty()
        && analysis.interface.mapped_outputs.len() != analysis.interface.observable_outputs.len()
    {
        return Err(TruthTableError::AmbiguousOutputMapping {
            observable_outputs: analysis.interface.observable_outputs.len(),
            inferred_outputs: analysis.outputs.len(),
        });
    }
    if analysis.inputs.len() > max_inputs || analysis.inputs.len() >= usize::BITS as usize {
        return Err(TruthTableError::TooManyInputs(analysis.inputs.len()));
    }
    let (rows, estimated_work_units) = budget
        .estimate_work_units(world.iter().count(), analysis.inputs.len(), settle_ticks)
        .ok_or(TruthTableError::TooManyInputs(analysis.inputs.len()))?;
    if rows > budget.max_rows || estimated_work_units > budget.max_work_units {
        return Err(TruthTableError::BudgetExceeded {
            rows,
            max_rows: budget.max_rows,
            estimated_work_units,
            max_work_units: budget.max_work_units,
        });
    }
    let drivers = analysis
        .inputs
        .iter()
        .map(|terminal| inferred_input_driver(world, analysis, terminal))
        .collect::<Result<Vec<_>, _>>()?;
    let started = Instant::now();
    let mut solver_iterations = 0_usize;
    let mut settle_ticks_executed = 0_usize;
    let mut world_clone_nanos = 0_u64;
    let mut input_drive_nanos = 0_u64;
    let mut wire_shape_update_nanos = 0_u64;
    let mut simulator_init_nanos = 0_u64;
    let mut settle_nanos = 0_u64;
    let mut output_read_nanos = 0_u64;
    let mut truth_table_rows = Vec::new();
    const STABLE_TICKS_REQUIRED: usize = 2;
    for (completed_rows, bits) in (0..(1_usize << analysis.inputs.len())).enumerate() {
        enforce_runtime_budget(budget, rows, completed_rows, solver_iterations, started)?;
        let inputs: Vec<_> = (0..analysis.inputs.len())
            .map(|index| bits & (1 << index) != 0)
            .collect();
        let phase_started = Instant::now();
        let mut baseline = world.clone();
        add_elapsed_nanos(&mut world_clone_nanos, phase_started);

        let phase_started = Instant::now();
        update_wire_shapes(&mut baseline);
        add_elapsed_nanos(&mut wire_shape_update_nanos, phase_started);

        let phase_started = Instant::now();
        let mut simulator = RedstoneTickSimulator::new(baseline)
            .map_err(|error| TruthTableError::Simulation(error.to_string()))?;
        add_elapsed_nanos(&mut simulator_init_nanos, phase_started);

        let phase_started = Instant::now();
        let input_states: Vec<_> = drivers
            .iter()
            .zip(&inputs)
            .map(|(driver, value)| (inferred_driver_position(*driver), *value))
            .collect();
        simulator
            .set_input_states(&input_states)
            .map_err(|error| TruthTableError::Simulation(error.to_string()))?;
        for (terminal, _driver) in analysis.inputs.iter().zip(&drivers) {
            debug_assert!(
                analysis.components[terminal.component]
                    .positions
                    .contains(&terminal.anchor)
            );
        }
        add_elapsed_nanos(&mut input_drive_nanos, phase_started);

        let mut state = simulator.snapshot();
        solver_iterations = solver_iterations.saturating_add(state.instantaneous_iterations);
        enforce_runtime_budget(budget, rows, completed_rows, solver_iterations, started)?;
        let mut previous_state = state.clone();
        let mut stable_ticks = 0_usize;
        let phase_started = Instant::now();
        for _ in 0..settle_ticks {
            state = simulator
                .advance_tick()
                .map_err(|error| TruthTableError::Simulation(error.to_string()))?;
            settle_ticks_executed = settle_ticks_executed.saturating_add(1);
            solver_iterations = solver_iterations.saturating_add(state.instantaneous_iterations);
            enforce_runtime_budget(budget, rows, completed_rows, solver_iterations, started)?;
            if !simulator.has_pending_events() && same_electrical_state(&state, &previous_state) {
                stable_ticks += 1;
            } else {
                stable_ticks = 0;
            }
            previous_state = state.clone();
            if stable_ticks >= STABLE_TICKS_REQUIRED.min(settle_ticks) {
                break;
            }
        }
        add_elapsed_nanos(&mut settle_nanos, phase_started);
        let pending_events = simulator.has_pending_events();
        if settle_ticks > 0
            && (pending_events || stable_ticks < STABLE_TICKS_REQUIRED.min(settle_ticks))
        {
            return Err(TruthTableError::NonSettling {
                row: completed_rows,
                settle_ticks,
                pending_events,
            });
        }
        let phase_started = Instant::now();
        let outputs = analysis
            .outputs
            .iter()
            .map(|terminal| state.strength(terminal.anchor) > 0)
            .collect();
        truth_table_rows.push(TruthTableRow { inputs, outputs });
        add_elapsed_nanos(&mut output_read_nanos, phase_started);
    }
    let stats = TruthTableExecutionStats {
        rows_requested: rows,
        rows_completed: truth_table_rows.len(),
        settle_ticks_requested: settle_ticks,
        settle_ticks_executed,
        solver_iterations,
        world_clone_nanos,
        input_drive_nanos,
        wire_shape_update_nanos,
        simulator_init_nanos,
        settle_nanos,
        output_read_nanos,
        elapsed_millis: u64::try_from(started.elapsed().as_millis()).unwrap_or(u64::MAX),
    };
    Ok((
        InferredTruthTable {
            inputs: analysis.inputs.clone(),
            outputs: analysis.outputs.clone(),
            rows: truth_table_rows,
        },
        stats,
    ))
}

fn add_elapsed_nanos(total: &mut u64, started: Instant) {
    *total = total.saturating_add(u64::try_from(started.elapsed().as_nanos()).unwrap_or(u64::MAX));
}

fn same_electrical_state(left: &TickState, right: &TickState) -> bool {
    left.strengths == right.strengths
        && left.block_power == right.block_power
        && left.repeater_powered == right.repeater_powered
        && left.torch_lit == right.torch_lit
        && left.comparator_output == right.comparator_output
        && left.observer_powered == right.observer_powered
        && left.lamp_lit == right.lamp_lit
        && left.torch_burnout_candidates == right.torch_burnout_candidates
}

fn enforce_runtime_budget(
    budget: TruthTableBudget,
    rows: usize,
    completed_rows: usize,
    solver_iterations: usize,
    started: Instant,
) -> Result<(), TruthTableError> {
    if solver_iterations > budget.max_solver_iterations {
        return Err(TruthTableError::RuntimeBudgetExceeded {
            rows,
            completed_rows,
            solver_iterations,
            max_solver_iterations: budget.max_solver_iterations,
        });
    }
    if let Some(max_elapsed_millis) = budget.max_elapsed_millis {
        let elapsed_millis = started.elapsed().as_millis();
        if elapsed_millis >= u128::from(max_elapsed_millis) {
            return Err(TruthTableError::ElapsedBudgetExceeded {
                rows,
                completed_rows,
                elapsed_millis,
                max_elapsed_millis,
            });
        }
    }
    Ok(())
}
