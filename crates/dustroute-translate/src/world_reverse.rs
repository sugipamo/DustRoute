//! Region analysis: topology and interface evidence feed bounded execution;
//! complete truth tables feed expression and dependency inference.
use crate::connectivity::PhysicalConnectivityGraph;
use crate::ir::expr::Expr;
use crate::world::{BlockKind, Pos};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};
mod analysis;
mod drivers;
mod network;
#[cfg(test)]
mod tests;
mod truth_table;

pub use analysis::analyze_world_region;
pub use analysis::analyze_world_region_in_dimension;
pub use drivers::InferredInputDriver;
pub use drivers::apply_inferred_input_driver;
pub use drivers::inferred_input_driver;
pub use network::derive_functional_network;
pub use network::derive_functional_network_with_budget;
pub use network::infer_output_expressions;
pub use truth_table::TruthTableBudget;
pub use truth_table::TruthTableComparison;
pub use truth_table::TruthTableError;
pub use truth_table::TruthTableExecutionStats;
pub use truth_table::compare_truth_tables;
pub use truth_table::infer_truth_table;
pub use truth_table::infer_truth_table_with_budget;
pub use truth_table::infer_truth_table_with_budget_and_stats;

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
