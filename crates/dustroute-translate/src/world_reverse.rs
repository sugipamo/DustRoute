use std::collections::{BTreeMap, BTreeSet};
use std::time::Instant;

use serde::{Deserialize, Serialize};

use crate::connectivity::{
    ConnectivityEdge, PhysicalConnectivityGraph, build_physical_circuit, extract_connectivity,
};
use crate::expr::Expr;
use crate::sim::{RedstoneTickSimulator, TickState};
use crate::wire::update_wire_shapes;
use crate::world::Block;
use crate::world::{BlockKind, Pos, World};

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct RegionBounds {
    pub min: Pos,
    pub max: Pos,
}

impl RegionBounds {
    #[must_use]
    pub const fn new(a: Pos, b: Pos) -> Self {
        Self {
            min: Pos::new(min_i32(a.x, b.x), min_i32(a.y, b.y), min_i32(a.z, b.z)),
            max: Pos::new(max_i32(a.x, b.x), max_i32(a.y, b.y), max_i32(a.z, b.z)),
        }
    }

    #[must_use]
    pub const fn contains(self, pos: Pos) -> bool {
        pos.x >= self.min.x
            && pos.x <= self.max.x
            && pos.y >= self.min.y
            && pos.y <= self.max.y
            && pos.z >= self.min.z
            && pos.z <= self.max.z
    }
}

const fn min_i32(a: i32, b: i32) -> i32 {
    if a < b { a } else { b }
}

const fn max_i32(a: i32, b: i32) -> i32 {
    if a > b { a } else { b }
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub enum TerminalConfidence {
    Certain,
    Likely,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct InferredTerminal {
    pub anchor: Pos,
    pub component: usize,
    pub confidence: TerminalConfidence,
}

/// Evidence that every observed external source and observable sink is
/// represented by the inferred interface.  This is deliberately separate
/// from the logical truth table: a table is not a contract unless its
/// physical boundary is accounted for first.
#[derive(Clone, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
pub struct InterfaceEvidence {
    pub external_inputs: BTreeSet<Pos>,
    pub mapped_inputs: BTreeSet<Pos>,
    pub unmapped_inputs: BTreeSet<Pos>,
    pub observable_outputs: BTreeSet<Pos>,
    pub mapped_outputs: BTreeSet<Pos>,
    pub unmapped_outputs: BTreeSet<Pos>,
}

impl InterfaceEvidence {
    #[must_use]
    pub fn complete(&self) -> bool {
        self.unmapped_inputs.is_empty()
            && self.unmapped_outputs.is_empty()
            && self.external_inputs.len() == self.mapped_inputs.len()
            && self.observable_outputs.len() == self.mapped_outputs.len()
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SignalComponent {
    pub id: usize,
    pub positions: BTreeSet<Pos>,
    pub incoming: BTreeSet<usize>,
    pub outgoing: BTreeSet<usize>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RegionAnalysis {
    pub bounds: RegionBounds,
    pub redstone_blocks: BTreeMap<Pos, BlockKind>,
    pub graph: PhysicalConnectivityGraph,
    /// Canonical physical observation.
    pub scene: dustroute_physical::PhysicalScene,
    pub components: Vec<SignalComponent>,
    pub inputs: Vec<InferredTerminal>,
    pub outputs: Vec<InferredTerminal>,
    pub interface: InterfaceEvidence,
    pub unsupported: BTreeMap<Pos, BlockKind>,
    pub diagnostics: SignalDiagnostics,
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct SignalDiagnostics {
    pub isolated_redstone: BTreeSet<Pos>,
    pub signal_islands: Vec<BTreeSet<usize>>,
    pub unreachable_from_inputs: BTreeSet<usize>,
    pub cannot_reach_outputs: BTreeSet<usize>,
    pub invalid_supports: Vec<(Pos, BlockKind, Option<Pos>)>,
    pub non_controllable_torches: BTreeSet<Pos>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct TruthTableRow {
    pub inputs: Vec<bool>,
    pub outputs: Vec<bool>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct InferredTruthTable {
    pub inputs: Vec<InferredTerminal>,
    pub outputs: Vec<InferredTerminal>,
    pub rows: Vec<TruthTableRow>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct InferredOutputFunction {
    pub output_index: usize,
    pub terminal: InferredTerminal,
    pub expression: Expr,
    pub truth_column: Vec<bool>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct PhysicalInfluence {
    pub component: usize,
    pub positions: BTreeSet<Pos>,
    pub input_dependencies: BTreeSet<usize>,
    pub output_dependencies: BTreeSet<usize>,
    pub shared_role: bool,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct FunctionalNetworkModel {
    pub truth_table: InferredTruthTable,
    pub output_functions: Vec<InferredOutputFunction>,
    pub physical_influences: Vec<PhysicalInfluence>,
}

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
        let mut driven = world.clone();
        add_elapsed_nanos(&mut world_clone_nanos, phase_started);

        let phase_started = Instant::now();
        for ((terminal, driver), value) in analysis.inputs.iter().zip(&drivers).zip(&inputs) {
            apply_inferred_input_driver(&mut driven, *driver, *value)?;
            debug_assert!(
                analysis.components[terminal.component]
                    .positions
                    .contains(&terminal.anchor)
            );
        }
        add_elapsed_nanos(&mut input_drive_nanos, phase_started);

        let phase_started = Instant::now();
        update_wire_shapes(&mut driven);
        add_elapsed_nanos(&mut wire_shape_update_nanos, phase_started);

        let phase_started = Instant::now();
        let mut simulator = RedstoneTickSimulator::new(driven)
            .map_err(|error| TruthTableError::Simulation(error.to_string()))?;
        let mut state = simulator.snapshot();
        add_elapsed_nanos(&mut simulator_init_nanos, phase_started);
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

#[must_use]
pub fn infer_output_expressions(table: &InferredTruthTable) -> Vec<Expr> {
    (0..table.outputs.len())
        .map(|output| infer_output_expression(table, output))
        .collect()
}

pub fn derive_functional_network(
    world: &World,
    analysis: &RegionAnalysis,
    max_inputs: usize,
    settle_ticks: usize,
) -> Result<FunctionalNetworkModel, TruthTableError> {
    derive_functional_network_with_budget(
        world,
        analysis,
        max_inputs,
        settle_ticks,
        TruthTableBudget::default(),
    )
}

pub fn derive_functional_network_with_budget(
    world: &World,
    analysis: &RegionAnalysis,
    max_inputs: usize,
    settle_ticks: usize,
    budget: TruthTableBudget,
) -> Result<FunctionalNetworkModel, TruthTableError> {
    let truth_table =
        infer_truth_table_with_budget(world, analysis, max_inputs, settle_ticks, budget)?;
    let expressions = infer_output_expressions(&truth_table);
    let output_functions = truth_table
        .outputs
        .iter()
        .cloned()
        .zip(expressions)
        .enumerate()
        .map(
            |(output_index, (terminal, expression))| InferredOutputFunction {
                output_index,
                terminal,
                expression,
                truth_column: truth_table
                    .rows
                    .iter()
                    .map(|row| row.outputs[output_index])
                    .collect(),
            },
        )
        .collect();
    let physical_influences = analysis
        .components
        .iter()
        .map(|component| {
            let input_dependencies = analysis
                .inputs
                .iter()
                .enumerate()
                .filter(|(_, terminal)| {
                    component_reaches(analysis, terminal.component, component.id)
                })
                .map(|(index, _)| index)
                .collect::<BTreeSet<_>>();
            let output_dependencies = analysis
                .outputs
                .iter()
                .enumerate()
                .filter(|(_, terminal)| {
                    component_reaches(analysis, component.id, terminal.component)
                })
                .map(|(index, _)| index)
                .collect::<BTreeSet<_>>();
            PhysicalInfluence {
                component: component.id,
                positions: component.positions.clone(),
                shared_role: input_dependencies.len() > 1 || output_dependencies.len() > 1,
                input_dependencies,
                output_dependencies,
            }
        })
        .collect();
    Ok(FunctionalNetworkModel {
        truth_table,
        output_functions,
        physical_influences,
    })
}

fn component_reaches(analysis: &RegionAnalysis, start: usize, target: usize) -> bool {
    let mut pending = vec![start];
    let mut visited = BTreeSet::new();
    while let Some(component) = pending.pop() {
        if component == target {
            return true;
        }
        if !visited.insert(component) {
            continue;
        }
        pending.extend(analysis.components[component].outgoing.iter().copied());
    }
    false
}

fn infer_output_expression(table: &InferredTruthTable, output: usize) -> Expr {
    let vars: Vec<_> = (0..table.inputs.len())
        .map(|index| Expr::Var(format!("in{index}")))
        .collect();
    let mut candidates = vec![Expr::Const(false), Expr::Const(true)];
    for var in &vars {
        candidates.push(var.clone());
        candidates.push(Expr::Not(Box::new(var.clone())));
    }
    for left in 0..vars.len() {
        for right in left + 1..vars.len() {
            let pair = vec![vars[left].clone(), vars[right].clone()];
            candidates.push(Expr::And(pair.clone()));
            candidates.push(Expr::Or(pair.clone()));
            candidates.push(Expr::Xor(pair.clone()));
            candidates.push(Expr::Nand(pair));
        }
    }
    if vars.len() > 2 {
        candidates.push(Expr::And(vars.clone()));
        candidates.push(Expr::Or(vars.clone()));
        candidates.push(Expr::Xor(vars.clone()));
        candidates.push(Expr::Nand(vars.clone()));
    }
    if vars.len() == 3 {
        candidates.push(Expr::Or(vec![
            Expr::And(vec![vars[0].clone(), vars[1].clone()]),
            Expr::And(vec![vars[0].clone(), vars[2].clone()]),
            Expr::And(vec![vars[1].clone(), vars[2].clone()]),
        ]));
    }
    candidates
        .into_iter()
        .filter(|candidate| expression_matches(table, output, candidate))
        .min_by_key(|candidate| (candidate.size(), candidate.clone()))
        .unwrap_or_else(|| canonical_sum_of_products(table, output, &vars))
}

fn expression_matches(table: &InferredTruthTable, output: usize, expression: &Expr) -> bool {
    table.rows.iter().all(|row| {
        let env = row
            .inputs
            .iter()
            .enumerate()
            .map(|(index, value)| (format!("in{index}"), *value))
            .collect();
        expression.evaluate(&env) == row.outputs[output]
    })
}

fn canonical_sum_of_products(table: &InferredTruthTable, output: usize, vars: &[Expr]) -> Expr {
    let terms: Vec<_> = table
        .rows
        .iter()
        .filter(|row| row.outputs[output])
        .map(|row| {
            Expr::And(
                vars.iter()
                    .cloned()
                    .zip(&row.inputs)
                    .map(|(var, value)| {
                        if *value {
                            var
                        } else {
                            Expr::Not(Box::new(var))
                        }
                    })
                    .collect(),
            )
        })
        .collect();
    match terms.as_slice() {
        [] => Expr::Const(false),
        [single] => single.clone(),
        _ => Expr::Or(terms),
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum InferredInputDriver {
    Lever(Pos),
    Button(Pos),
    PressurePlate(Pos),
    External(Pos),
}

/// Applies the same physical input driver used by truth-table inference and
/// transition verification.  Keeping this operation typed prevents a wire or
/// an arbitrary block from being silently mutated as an input.
pub fn apply_inferred_input_driver(
    world: &mut World,
    driver: InferredInputDriver,
    powered: bool,
) -> Result<(), TruthTableError> {
    match driver {
        InferredInputDriver::Lever(pos) => {
            set_stateful_input(world, pos, BlockKind::Lever, powered)
        }
        InferredInputDriver::Button(pos) => {
            set_stateful_input(world, pos, BlockKind::Button, powered)
        }
        InferredInputDriver::PressurePlate(pos) => {
            let Some(block) = world.get(pos).cloned() else {
                return Err(TruthTableError::InvalidDriver {
                    position: pos,
                    expected: "pressure_plate",
                    actual: BlockKind::Air,
                });
            };
            if block.kind != BlockKind::PressurePlate {
                return Err(TruthTableError::InvalidDriver {
                    position: pos,
                    expected: "pressure_plate",
                    actual: block.kind,
                });
            }
            let mut changed = block;
            changed.powered = Some(powered);
            changed.power_level = Some(if powered { 15 } else { 0 });
            world.set(pos, changed);
            Ok(())
        }
        InferredInputDriver::External(pos) => {
            let current = world.kind_at(pos);
            if !matches!(current, BlockKind::Air | BlockKind::RedstoneBlock) {
                return Err(TruthTableError::InvalidDriver {
                    position: pos,
                    expected: "air or redstone_block for an external driver",
                    actual: current,
                });
            }
            if powered {
                world.set(pos, Block::new(BlockKind::RedstoneBlock));
            } else if world.kind_at(pos) == BlockKind::RedstoneBlock {
                world.remove(pos);
            }
            Ok(())
        }
    }
}

fn set_stateful_input(
    world: &mut World,
    pos: Pos,
    expected: BlockKind,
    powered: bool,
) -> Result<(), TruthTableError> {
    let Some(block) = world.get(pos).cloned() else {
        return Err(TruthTableError::InvalidDriver {
            position: pos,
            expected: match expected {
                BlockKind::Lever => "lever",
                BlockKind::Button => "button",
                _ => "stateful input",
            },
            actual: BlockKind::Air,
        });
    };
    if block.kind != expected {
        return Err(TruthTableError::InvalidDriver {
            position: pos,
            expected: match expected {
                BlockKind::Lever => "lever",
                BlockKind::Button => "button",
                _ => "stateful input",
            },
            actual: block.kind,
        });
    }
    let mut changed = block;
    changed.powered = Some(powered);
    world.set(pos, changed);
    Ok(())
}

pub fn inferred_input_driver(
    world: &World,
    analysis: &RegionAnalysis,
    terminal: &InferredTerminal,
) -> Result<InferredInputDriver, TruthTableError> {
    match world.kind_at(terminal.anchor) {
        BlockKind::Lever => return Ok(InferredInputDriver::Lever(terminal.anchor)),
        BlockKind::Button => return Ok(InferredInputDriver::Button(terminal.anchor)),
        BlockKind::PressurePlate => {
            return Ok(InferredInputDriver::PressurePlate(terminal.anchor));
        }
        _ => {}
    }
    let horizontal = [
        Pos::new(-1, 0, 0),
        Pos::new(1, 0, 0),
        Pos::new(0, 0, -1),
        Pos::new(0, 0, 1),
    ];
    if let Some(pos) = horizontal
        .iter()
        .map(|delta| terminal.anchor.offset(delta.x, delta.y, delta.z))
        .find(|pos| !analysis.bounds.contains(*pos) && world.kind_at(*pos) == BlockKind::Air)
    {
        return Ok(InferredInputDriver::External(pos));
    }
    let component = &analysis.components[terminal.component];
    let downstream: BTreeSet<_> = component
        .outgoing
        .iter()
        .flat_map(|id| analysis.components[*id].positions.iter().copied())
        .collect();
    for next in downstream {
        let dx = next.x - terminal.anchor.x;
        let dz = next.z - terminal.anchor.z;
        if next.y == terminal.anchor.y && dx.abs() + dz.abs() == 1 {
            let pos = terminal.anchor.offset(-dx, 0, -dz);
            if world.kind_at(pos) == BlockKind::Air {
                return Ok(InferredInputDriver::External(pos));
            }
        }
    }
    Err(TruthTableError::NoDriverPosition(terminal.anchor))
}

#[must_use]
pub fn analyze_world_region(world: &World, bounds: RegionBounds) -> RegionAnalysis {
    analyze_world_region_in_dimension(world, bounds, "unknown")
}

#[must_use]
pub fn analyze_world_region_in_dimension(
    world: &World,
    bounds: RegionBounds,
    dimension: impl Into<String>,
) -> RegionAnalysis {
    let bounded = bounded_world(world, bounds);
    let graph = extract_connectivity(&bounded);
    let physical = build_physical_circuit(&bounded, &graph);
    let propagating_supports: BTreeSet<_> = graph.edges.iter().map(|edge| edge.source).collect();
    let functional_nodes: BTreeSet<_> = graph
        .nodes
        .iter()
        .copied()
        .filter(|pos| is_redstone_kind(bounded.kind_at(*pos)) || propagating_supports.contains(pos))
        .collect();
    let functional_edges: BTreeSet<_> = graph
        .edges
        .iter()
        .filter(|edge| {
            functional_nodes.contains(&edge.source) && functional_nodes.contains(&edge.sink)
        })
        .copied()
        .collect();
    let active_nodes: BTreeSet<_> = functional_edges
        .iter()
        .flat_map(|edge| [edge.source, edge.sink])
        .collect();
    let redstone_blocks = bounded
        .iter()
        .filter(|(_, block)| is_redstone_kind(block.kind) || block.requires_live_observation())
        .map(|(pos, block)| (*pos, block.kind))
        .collect();
    let unsupported = bounded
        .iter()
        .filter(|(_, block)| {
            matches!(block.kind, BlockKind::Comparator | BlockKind::Piston)
                || block.requires_live_observation()
        })
        .map(|(pos, block)| (*pos, block.kind))
        .collect();
    let raw_components = strongly_connected_components(&active_nodes, &functional_edges);
    let owner: BTreeMap<_, _> = raw_components
        .iter()
        .enumerate()
        .flat_map(|(id, positions)| positions.iter().map(move |pos| (*pos, id)))
        .collect();
    let mut components: Vec<_> = raw_components
        .into_iter()
        .enumerate()
        .map(|(id, positions)| SignalComponent {
            id,
            positions,
            incoming: BTreeSet::new(),
            outgoing: BTreeSet::new(),
        })
        .collect();
    for edge in &functional_edges {
        let (Some(source), Some(sink)) = (owner.get(&edge.source), owner.get(&edge.sink)) else {
            continue;
        };
        if source != sink {
            components[*source].outgoing.insert(*sink);
            components[*sink].incoming.insert(*source);
        }
    }
    let inferred_inputs: Vec<_> = components
        .iter()
        .filter(|component| component.incoming.is_empty() && !component.outgoing.is_empty())
        .filter_map(|component| infer_input(&bounded, component))
        .collect();
    let inferred_outputs: Vec<_> = components
        .iter()
        .filter(|component| component.outgoing.is_empty() && !component.incoming.is_empty())
        .filter_map(|component| infer_output(&bounded, component))
        .collect();
    let (buffered_inputs, buffered_outputs) =
        infer_buffered_boundaries(&bounded, &graph, &components, &owner);
    let (inputs, outputs) = if buffered_inputs.is_empty() || buffered_outputs.is_empty() {
        (inferred_inputs, inferred_outputs)
    } else {
        (buffered_inputs, buffered_outputs)
    };
    let interface = interface_evidence(&bounded, &graph, &components, &inputs, &outputs);
    let diagnostics = signal_diagnostics(
        &bounded,
        &graph,
        &components,
        &inputs,
        &outputs,
        &redstone_blocks,
    );
    let mut observation = dustroute_physical::Observation::complete(
        dimension,
        dustroute_physical::SceneBounds::new(bounds.min, bounds.max),
    );
    observation.frontier = observation_frontier(&physical, bounds);
    if !observation.frontier.is_empty() {
        observation.regions[0].completeness = dustroute_physical::RegionCompleteness::OpenBoundary;
    }
    let scene = dustroute_physical::PhysicalScene::from_topology_and_world(
        observation,
        &physical,
        &bounded,
    );
    RegionAnalysis {
        bounds,
        redstone_blocks,
        graph,
        scene,
        components,
        inputs,
        outputs,
        interface,
        unsupported,
        diagnostics,
    }
}

fn interface_evidence(
    world: &World,
    graph: &PhysicalConnectivityGraph,
    components: &[SignalComponent],
    inputs: &[InferredTerminal],
    outputs: &[InferredTerminal],
) -> InterfaceEvidence {
    let external_inputs: BTreeSet<_> = world
        .iter()
        .filter(|(_, block)| block.is_external_input_source())
        .map(|(pos, _)| *pos)
        .collect();
    let observable_outputs: BTreeSet<_> = world
        .iter()
        .filter(|(_, block)| block.is_observable_output())
        .map(|(pos, _)| *pos)
        .collect();
    let mapped_inputs: BTreeSet<_> = external_inputs
        .iter()
        .copied()
        .filter(|position| {
            inputs.iter().any(|terminal| {
                terminal.component < components.len()
                    && components[terminal.component].positions.contains(position)
            })
        })
        .collect();
    let mapped_outputs: BTreeSet<_> = observable_outputs
        .iter()
        .copied()
        .filter(|position| {
            outputs.iter().any(|terminal| {
                terminal.component < components.len()
                    && components[terminal.component].positions.contains(position)
            }) || graph.edges.iter().any(|edge| {
                edge.sink == *position
                    && outputs.iter().any(|terminal| {
                        terminal.component < components.len()
                            && components[terminal.component]
                                .positions
                                .contains(&edge.source)
                    })
            })
        })
        .collect();
    let unmapped_inputs = external_inputs
        .difference(&mapped_inputs)
        .copied()
        .collect();
    let unmapped_outputs = observable_outputs
        .difference(&mapped_outputs)
        .copied()
        .collect();
    InterfaceEvidence {
        external_inputs,
        mapped_inputs,
        unmapped_inputs,
        observable_outputs,
        mapped_outputs,
        unmapped_outputs,
    }
}

fn infer_buffered_boundaries(
    world: &World,
    graph: &PhysicalConnectivityGraph,
    components: &[SignalComponent],
    owner: &BTreeMap<Pos, usize>,
) -> (Vec<InferredTerminal>, Vec<InferredTerminal>) {
    let mut inputs = BTreeMap::<usize, InferredTerminal>::new();
    let mut outputs = BTreeMap::<usize, InferredTerminal>::new();
    for (repeater_pos, repeater) in world
        .iter()
        .filter(|(_, block)| block.kind == BlockKind::Repeater)
    {
        let Some(facing) = repeater.facing else {
            continue;
        };
        let Some(delta) = facing.horizontal_offset() else {
            continue;
        };
        let input_pos = repeater_pos.offset(-delta.x, 0, -delta.z);
        let output_pos = repeater_pos.offset(delta.x, 0, delta.z);
        if world.kind_at(input_pos) != BlockKind::RedstoneWire
            || world.kind_at(output_pos) != BlockKind::RedstoneWire
            || [input_pos, *repeater_pos, output_pos]
                .iter()
                .any(|pos| world.kind_at(pos.offset(0, -1, 0)) != BlockKind::Solid)
        {
            continue;
        }
        let cell_positions = BTreeSet::from([
            input_pos,
            *repeater_pos,
            output_pos,
            input_pos.offset(0, -1, 0),
            repeater_pos.offset(0, -1, 0),
            output_pos.offset(0, -1, 0),
        ]);
        let externally_connected = |position: Pos| {
            graph.edges.iter().any(|edge| {
                (edge.source == position && !cell_positions.contains(&edge.sink))
                    || (edge.sink == position && !cell_positions.contains(&edge.source))
            })
        };
        let input_connected = externally_connected(input_pos);
        let output_connected = externally_connected(output_pos);
        if !input_connected && output_connected {
            if let Some(component) = owner.get(&input_pos).copied() {
                inputs.insert(
                    component,
                    InferredTerminal {
                        anchor: input_pos,
                        component,
                        confidence: TerminalConfidence::Likely,
                    },
                );
            }
        } else if input_connected
            && !output_connected
            && let Some(component) = owner.get(&output_pos).copied()
        {
            outputs.insert(
                component,
                InferredTerminal {
                    anchor: output_pos,
                    component,
                    confidence: TerminalConfidence::Likely,
                },
            );
        }
    }
    let valid_component = |terminal: &InferredTerminal| terminal.component < components.len();
    (
        inputs.into_values().filter(valid_component).collect(),
        outputs.into_values().filter(valid_component).collect(),
    )
}

fn observation_frontier(
    physical: &dustroute_physical::VerifiedTopology,
    bounds: RegionBounds,
) -> Vec<dustroute_physical::ObservationFrontier> {
    let mut frontier = Vec::new();
    for component in physical
        .components
        .iter()
        .filter(|component| component.block.kind.is_redstone_related())
    {
        for (at_boundary, direction) in [
            (component.pos.x == bounds.min.x, crate::Facing::West),
            (component.pos.x == bounds.max.x, crate::Facing::East),
            (component.pos.y == bounds.min.y, crate::Facing::Down),
            (component.pos.y == bounds.max.y, crate::Facing::Up),
            (component.pos.z == bounds.min.z, crate::Facing::North),
            (component.pos.z == bounds.max.z, crate::Facing::South),
        ] {
            if at_boundary {
                frontier.push(dustroute_physical::ObservationFrontier {
                    position: component.pos,
                    direction,
                    reason: dustroute_physical::FrontierReason::ScanLimitReached,
                });
            }
        }
    }
    frontier
}

fn signal_diagnostics(
    world: &World,
    graph: &PhysicalConnectivityGraph,
    components: &[SignalComponent],
    inputs: &[InferredTerminal],
    outputs: &[InferredTerminal],
    redstone_blocks: &BTreeMap<Pos, BlockKind>,
) -> SignalDiagnostics {
    let incident: BTreeSet<_> = graph
        .edges
        .iter()
        .flat_map(|edge| [edge.source, edge.sink])
        .collect();
    let isolated_redstone = redstone_blocks
        .keys()
        .filter(|pos| !incident.contains(pos))
        .copied()
        .collect();
    let signal_islands = component_islands(components);
    let input_components: BTreeSet<_> = inputs.iter().map(|terminal| terminal.component).collect();
    let output_components: BTreeSet<_> =
        outputs.iter().map(|terminal| terminal.component).collect();
    let reachable = component_reachable(components, &input_components, false);
    let reaches_output = component_reachable(components, &output_components, true);
    SignalDiagnostics {
        isolated_redstone,
        signal_islands,
        unreachable_from_inputs: (0..components.len())
            .filter(|id| !reachable.contains(id))
            .collect(),
        cannot_reach_outputs: (0..components.len())
            .filter(|id| !reaches_output.contains(id))
            .collect(),
        invalid_supports: world.support_issues(),
        non_controllable_torches: world
            .iter()
            .filter(|(_, block)| block.kind == BlockKind::RedstoneTorch)
            .filter(|(pos, block)| {
                block
                    .support_pos(**pos)
                    .is_none_or(|support| !world.kind_at(support).properties().can_be_powered())
            })
            .map(|(pos, _)| *pos)
            .collect(),
    }
}

fn component_islands(components: &[SignalComponent]) -> Vec<BTreeSet<usize>> {
    let mut unseen: BTreeSet<_> = (0..components.len()).collect();
    let mut islands = Vec::new();
    while let Some(start) = unseen.pop_first() {
        let mut island = BTreeSet::new();
        let mut stack = vec![start];
        while let Some(id) = stack.pop() {
            if !island.insert(id) {
                continue;
            }
            unseen.remove(&id);
            stack.extend(components[id].incoming.iter().copied());
            stack.extend(components[id].outgoing.iter().copied());
        }
        islands.push(island);
    }
    islands
}

fn component_reachable(
    components: &[SignalComponent],
    starts: &BTreeSet<usize>,
    reverse: bool,
) -> BTreeSet<usize> {
    let mut seen = BTreeSet::new();
    let mut stack: Vec<_> = starts.iter().copied().collect();
    while let Some(id) = stack.pop() {
        if !seen.insert(id) {
            continue;
        }
        let next = if reverse {
            &components[id].incoming
        } else {
            &components[id].outgoing
        };
        stack.extend(next.iter().copied());
    }
    seen
}

fn bounded_world(world: &World, bounds: RegionBounds) -> World {
    let mut bounded = World::new();
    for (pos, block) in world.iter().filter(|(pos, _)| bounds.contains(**pos)) {
        bounded.set(*pos, block.clone());
    }
    bounded
}

const fn is_redstone_kind(kind: BlockKind) -> bool {
    matches!(
        kind,
        BlockKind::RedstoneWire
            | BlockKind::RedstoneTorch
            | BlockKind::Repeater
            | BlockKind::Comparator
            | BlockKind::Lever
            | BlockKind::Button
            | BlockKind::PressurePlate
            | BlockKind::RedstoneBlock
            | BlockKind::Piston
    )
}

fn infer_input(world: &World, component: &SignalComponent) -> Option<InferredTerminal> {
    let certain = component.positions.iter().copied().find(|pos| {
        matches!(
            world.kind_at(*pos),
            BlockKind::Lever
                | BlockKind::Button
                | BlockKind::PressurePlate
                | BlockKind::RedstoneBlock
        )
    });
    let likely = component
        .positions
        .iter()
        .copied()
        .find(|pos| world.kind_at(*pos) == BlockKind::RedstoneWire);
    certain
        .map(|anchor| InferredTerminal {
            anchor,
            component: component.id,
            confidence: TerminalConfidence::Certain,
        })
        .or_else(|| {
            likely.map(|anchor| InferredTerminal {
                anchor,
                component: component.id,
                confidence: TerminalConfidence::Likely,
            })
        })
}

fn infer_output(world: &World, component: &SignalComponent) -> Option<InferredTerminal> {
    let preferred = component
        .positions
        .iter()
        .copied()
        .filter(|pos| {
            matches!(
                world.kind_at(*pos),
                BlockKind::RedstoneWire | BlockKind::Repeater | BlockKind::Piston
            )
        })
        .max();
    preferred
        .or_else(|| {
            component
                .positions
                .iter()
                .copied()
                .find(|pos| world.kind_at(*pos) == BlockKind::RedstoneLamp)
        })
        .map(|anchor| InferredTerminal {
            anchor,
            component: component.id,
            confidence: TerminalConfidence::Likely,
        })
}

fn strongly_connected_components(
    nodes: &BTreeSet<Pos>,
    edges: &BTreeSet<ConnectivityEdge>,
) -> Vec<BTreeSet<Pos>> {
    let mut outgoing = BTreeMap::<Pos, Vec<Pos>>::new();
    let mut incoming = BTreeMap::<Pos, Vec<Pos>>::new();
    for edge in edges {
        outgoing.entry(edge.source).or_default().push(edge.sink);
        incoming.entry(edge.sink).or_default().push(edge.source);
    }
    let mut visited = BTreeSet::new();
    let mut order = Vec::new();
    for node in nodes {
        visit_order(*node, &outgoing, &mut visited, &mut order);
    }
    visited.clear();
    let mut result = Vec::new();
    while let Some(node) = order.pop() {
        if visited.contains(&node) {
            continue;
        }
        let mut component = BTreeSet::new();
        collect_component(node, &incoming, &mut visited, &mut component);
        result.push(component);
    }
    result
}

fn visit_order(
    node: Pos,
    adjacency: &BTreeMap<Pos, Vec<Pos>>,
    visited: &mut BTreeSet<Pos>,
    order: &mut Vec<Pos>,
) {
    if !visited.insert(node) {
        return;
    }
    for next in adjacency.get(&node).into_iter().flatten() {
        visit_order(*next, adjacency, visited, order);
    }
    order.push(node);
}

fn collect_component(
    node: Pos,
    adjacency: &BTreeMap<Pos, Vec<Pos>>,
    visited: &mut BTreeSet<Pos>,
    component: &mut BTreeSet<Pos>,
) {
    if !visited.insert(node) {
        return;
    }
    component.insert(node);
    for next in adjacency.get(&node).into_iter().flatten() {
        collect_component(*next, adjacency, visited, component);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        BaselineCompileConfig, BaselineCompiler, decoder_1_to_2, full_adder, half_adder,
        half_subtractor, mux_2_to_1,
    };

    #[test]
    fn infers_half_adder_boundaries_from_directionality() {
        let compiled = BaselineCompiler::new(BaselineCompileConfig::default())
            .compile(&half_adder())
            .unwrap();
        let (min, max) = compiled.world.bounds().unwrap();
        let analysis = analyze_world_region(&compiled.world, RegionBounds::new(min, max));
        assert_eq!(analysis.inputs.len(), 2, "{analysis:#?}");
        assert_eq!(analysis.outputs.len(), 2, "{analysis:#?}");
        assert!(analysis.unsupported.is_empty());
        let table = infer_truth_table(&compiled.world, &analysis, 16, 16).unwrap();
        let columns: Vec<Vec<_>> = (0..table.outputs.len())
            .map(|output| table.rows.iter().map(|row| row.outputs[output]).collect())
            .collect();
        assert!(columns.contains(&vec![false, false, false, true]));
        assert!(columns.contains(&vec![false, true, true, false]));
        let expressions = infer_output_expressions(&table);
        assert!(expressions.iter().any(|expr| matches!(expr, Expr::And(_))));
        assert!(expressions.iter().any(|expr| matches!(expr, Expr::Xor(_))));
        assert_eq!(analysis.diagnostics.signal_islands.len(), 1);
        assert!(analysis.diagnostics.unreachable_from_inputs.is_empty());
        assert!(analysis.diagnostics.cannot_reach_outputs.is_empty());
        assert!(analysis.diagnostics.non_controllable_torches.is_empty());
    }

    #[test]
    fn infers_boundaries_for_all_regression_circuits() {
        for (dag, expected_inputs, expected_outputs) in [
            (half_subtractor(), 2, 2),
            (mux_2_to_1(), 3, 1),
            (decoder_1_to_2(), 2, 2),
            (full_adder(), 3, 2),
        ] {
            let compiled = BaselineCompiler::new(BaselineCompileConfig::default())
                .compile(&dag)
                .unwrap();
            let (min, max) = compiled.world.bounds().unwrap();
            let analysis = analyze_world_region(&compiled.world, RegionBounds::new(min, max));
            assert_eq!(analysis.inputs.len(), expected_inputs, "{analysis:#?}");
            assert_eq!(analysis.outputs.len(), expected_outputs, "{analysis:#?}");
            assert_eq!(
                analysis.diagnostics.signal_islands.len(),
                1,
                "{analysis:#?}"
            );
            if expected_inputs == 3 && expected_outputs == 2 {
                let table = infer_truth_table(&compiled.world, &analysis, 16, 60).unwrap();
                let columns: Vec<Vec<_>> = (0..table.outputs.len())
                    .map(|output| table.rows.iter().map(|row| row.outputs[output]).collect())
                    .collect();
                assert!(
                    columns.contains(&vec![false, false, false, true, false, true, true, true])
                );
                assert!(
                    columns.contains(&vec![false, true, true, false, true, false, false, true])
                );
                let expressions = infer_output_expressions(&table);
                assert!(expressions.iter().any(|expr| matches!(expr, Expr::Or(_))));
                assert!(expressions.iter().any(|expr| matches!(expr, Expr::Xor(_))));
            }
        }
    }

    #[test]
    fn broken_torch_support_is_detected_and_changes_truth_table() {
        let compiled = BaselineCompiler::new(BaselineCompileConfig::default())
            .compile(&half_adder())
            .unwrap();
        let (min, max) = compiled.world.bounds().unwrap();
        let bounds = RegionBounds::new(min, max);
        let healthy_analysis = analyze_world_region(&compiled.world, bounds);
        let healthy = infer_truth_table(&compiled.world, &healthy_analysis, 16, 16).unwrap();
        let mut broken_world = compiled.world.clone();
        let (torch, support) = broken_world
            .iter()
            .find(|(_, block)| block.kind == BlockKind::RedstoneTorch)
            .and_then(|(pos, block)| block.support_pos(*pos).map(|support| (*pos, support)))
            .unwrap();
        broken_world.set(support, Block::new(BlockKind::Transparent));
        let broken_analysis = analyze_world_region(&broken_world, bounds);
        assert!(
            broken_analysis
                .diagnostics
                .non_controllable_torches
                .contains(&torch)
        );
        let broken = infer_truth_table(&broken_world, &broken_analysis, 16, 16).unwrap();
        let comparison = compare_truth_tables(&healthy, &broken);
        assert!(comparison.comparable, "{comparison:?}");
        assert_eq!(comparison.actual_outputs, 2, "{comparison:?}");
        assert_eq!(comparison.terminal_count_delta, 0, "{comparison:?}");
        assert!(comparison.differing_bits > 0, "{comparison:?}");
        assert!(comparison.fitness_penalty > 0, "{comparison:?}");
    }

    #[test]
    fn compact_xor_is_derived_from_shared_physics_without_gate_partitioning() {
        let cell = crate::compact_compiled_xor_cell().unwrap();
        let (min, max) = cell.world.bounds().unwrap();
        let analysis = analyze_world_region(&cell.world, RegionBounds::new(min, max));
        let model = derive_functional_network(&cell.world, &analysis, 16, 64).unwrap();

        assert_eq!(model.truth_table.inputs.len(), 2);
        assert_eq!(model.truth_table.outputs.len(), 1);
        assert!(matches!(model.output_functions[0].expression, Expr::Xor(_)));
        assert_eq!(
            model.output_functions[0].truth_column,
            vec![false, true, true, false]
        );
        assert!(model.physical_influences.iter().any(|influence| {
            influence.shared_role && influence.input_dependencies == BTreeSet::from([0, 1])
        }));
    }

    #[test]
    fn truth_table_requires_non_empty_interface_evidence() {
        let world = World::new();
        let analysis = analyze_world_region(
            &world,
            RegionBounds::new(Pos::new(-1, -1, -1), Pos::new(1, 1, 1)),
        );
        assert_eq!(
            infer_truth_table(&world, &analysis, 4, 1),
            Err(TruthTableError::NoInputs)
        );

        let mut source_only = World::new();
        source_only.set(Pos::new(0, 0, 0), Block::new(BlockKind::Solid));
        source_only.set(Pos::new(1, 0, 0), Block::new(BlockKind::Solid));
        let source = source_only.place(BlockKind::Lever, Pos::new(0, 1, 0));
        source.powered = Some(false);
        source_only.place(BlockKind::RedstoneWire, Pos::new(1, 1, 0));
        update_wire_shapes(&mut source_only);
        let analysis = analyze_world_region(
            &source_only,
            RegionBounds::new(Pos::new(-1, -1, -1), Pos::new(2, 2, 1)),
        );
        assert!(!analysis.inputs.is_empty(), "{analysis:#?}");
        let mut no_outputs = analysis.clone();
        no_outputs.outputs.clear();
        assert_eq!(
            infer_truth_table(&source_only, &no_outputs, 4, 1),
            Err(TruthTableError::NoOutputs)
        );

        let mut ambiguous = analysis.clone();
        ambiguous
            .interface
            .external_inputs
            .insert(Pos::new(9, 9, 9));
        ambiguous.interface.mapped_inputs.insert(Pos::new(9, 9, 9));
        assert!(matches!(
            infer_truth_table(&source_only, &ambiguous, 4, 1),
            Err(TruthTableError::AmbiguousInputMapping { .. })
        ));
    }

    #[test]
    fn truth_table_budget_reports_rows_before_simulation() {
        let budget = TruthTableBudget::new(2, u128::MAX);
        assert_eq!(budget.estimate_work_units(100, 3, 4), Some((8, 4_000)));

        let compiled = BaselineCompiler::new(BaselineCompileConfig::default())
            .compile(&half_adder())
            .unwrap();
        let (min, max) = compiled.world.bounds().unwrap();
        let analysis = analyze_world_region(&compiled.world, RegionBounds::new(min, max));
        let error = infer_truth_table_with_budget(&compiled.world, &analysis, 16, 16, budget)
            .expect_err("row budget should reject before creating simulators");
        assert!(matches!(
            error,
            TruthTableError::BudgetExceeded {
                rows: 4,
                max_rows: 2,
                ..
            }
        ));
    }

    #[test]
    fn runtime_budget_fails_closed_without_returning_partial_rows() {
        let compiled = BaselineCompiler::new(BaselineCompileConfig::default())
            .compile(&half_adder())
            .unwrap();
        let (min, max) = compiled.world.bounds().unwrap();
        let bounds = RegionBounds::new(min, max);
        let request = crate::ReverseRequest::new(bounds)
            .with_truth_table(16)
            .with_settle_ticks(16)
            .with_truth_table_budget(
                TruthTableBudget::new(usize::MAX, u128::MAX)
                    .with_max_solver_iterations(0)
                    .with_max_elapsed_millis(None),
            );
        let result = crate::Translator.reverse(&compiled.world, request);
        assert!(result.truth_table.is_none());
        assert!(result.functional_network.is_none());
        assert!(matches!(
            result.truth_table_error,
            Some(TruthTableError::RuntimeBudgetExceeded {
                completed_rows: 0,
                max_solver_iterations: 0,
                ..
            })
        ));
    }

    #[test]
    fn elapsed_budget_fails_closed_before_simulation() {
        let compiled = BaselineCompiler::new(BaselineCompileConfig::default())
            .compile(&half_adder())
            .unwrap();
        let (min, max) = compiled.world.bounds().unwrap();
        let analysis = analyze_world_region(&compiled.world, RegionBounds::new(min, max));
        let error = infer_truth_table_with_budget(
            &compiled.world,
            &analysis,
            16,
            16,
            TruthTableBudget::new(usize::MAX, u128::MAX)
                .with_max_solver_iterations(usize::MAX)
                .with_max_elapsed_millis(Some(0)),
        )
        .expect_err("zero elapsed budget must reject before the first row");
        assert!(matches!(
            error,
            TruthTableError::ElapsedBudgetExceeded {
                completed_rows: 0,
                max_elapsed_millis: 0,
                ..
            }
        ));
    }

    #[test]
    fn incomplete_settle_window_is_not_claimed_as_a_truth_table() {
        let mut world = World::new();
        world.fill(
            Pos::new(0, 0, 0),
            Pos::new(3, 0, 0),
            Block::new(BlockKind::Solid),
        );
        let lever = world.place(BlockKind::Lever, Pos::new(0, 1, 0));
        lever.support_offset = Some(Pos::new(0, -1, 0));
        world.place(BlockKind::RedstoneWire, Pos::new(1, 1, 0));
        let repeater = world.place(BlockKind::Repeater, Pos::new(2, 1, 0));
        repeater.facing = Some(crate::Facing::East);
        repeater.delay = Some(1);
        world.place(BlockKind::RedstoneWire, Pos::new(3, 1, 0));
        update_wire_shapes(&mut world);
        let bounds = RegionBounds::new(Pos::new(0, 0, 0), Pos::new(3, 1, 0));
        let analysis = analyze_world_region(&world, bounds);
        assert_eq!(analysis.inputs.len(), 1, "{analysis:#?}");
        assert_eq!(analysis.outputs.len(), 1, "{analysis:#?}");
        let error = infer_truth_table_with_budget(
            &world,
            &analysis,
            4,
            1,
            TruthTableBudget::new(4, u128::MAX).with_max_elapsed_millis(None),
        )
        .expect_err("a changing final tick is not settled evidence");
        assert!(matches!(
            error,
            TruthTableError::NonSettling {
                row: 1,
                settle_ticks: 1,
                pending_events: false,
            }
        ));
    }

    #[test]
    fn execution_stats_report_actual_settle_work_without_changing_table() {
        let compiled = BaselineCompiler::new(BaselineCompileConfig::default())
            .compile(&half_adder())
            .unwrap();
        let (min, max) = compiled.world.bounds().unwrap();
        let analysis = analyze_world_region(&compiled.world, RegionBounds::new(min, max));
        let (instrumented, stats) = infer_truth_table_with_budget_and_stats(
            &compiled.world,
            &analysis,
            16,
            16,
            TruthTableBudget::default(),
        )
        .unwrap();
        let ordinary = infer_truth_table(&compiled.world, &analysis, 16, 16).unwrap();

        assert_eq!(instrumented, ordinary);
        assert_eq!(stats.rows_requested, 4);
        assert_eq!(stats.rows_completed, 4);
        assert_eq!(stats.settle_ticks_requested, 16);
        assert!(stats.settle_ticks_executed > 0);
        assert!(stats.settle_ticks_executed <= 4 * stats.settle_ticks_requested);
        assert!(stats.solver_iterations > 0);
    }

    #[test]
    fn isolated_observable_sink_is_reported_as_unmapped() {
        let mut world = World::new();
        world.set(Pos::new(0, 0, 0), Block::new(BlockKind::Solid));
        world.set(Pos::new(1, 0, 0), Block::new(BlockKind::Solid));
        let source = world.place(BlockKind::Lever, Pos::new(0, 1, 0));
        source.powered = Some(false);
        world.place(BlockKind::RedstoneWire, Pos::new(1, 1, 0));
        update_wire_shapes(&mut world);
        world.set(Pos::new(3, 1, 0), Block::new(BlockKind::RedstoneLamp));
        let analysis = analyze_world_region(
            &world,
            RegionBounds::new(Pos::new(-1, -1, -1), Pos::new(4, 2, 1)),
        );
        assert!(
            analysis
                .interface
                .unmapped_outputs
                .contains(&Pos::new(3, 1, 0))
        );
        assert!(matches!(
            infer_truth_table(&world, &analysis, 4, 1),
            Err(TruthTableError::NoOutputs) | Err(TruthTableError::UnmappedObservableOutputs(_))
        ));
    }
}
