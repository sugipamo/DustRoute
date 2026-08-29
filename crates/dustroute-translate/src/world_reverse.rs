use std::collections::{BTreeMap, BTreeSet};

use crate::connectivity::{ConnectivityEdge, PhysicalConnectivityGraph, extract_connectivity};
use crate::expr::Expr;
use crate::sim::RedstoneTickSimulator;
use crate::wire::update_wire_shapes;
use crate::world::Block;
use crate::world::{BlockKind, Pos, World};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
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

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum TerminalConfidence {
    Certain,
    Likely,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct InferredTerminal {
    pub anchor: Pos,
    pub component: usize,
    pub confidence: TerminalConfidence,
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
    pub components: Vec<SignalComponent>,
    pub inputs: Vec<InferredTerminal>,
    pub outputs: Vec<InferredTerminal>,
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

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct TruthTableRow {
    pub inputs: Vec<bool>,
    pub outputs: Vec<bool>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct InferredTruthTable {
    pub inputs: Vec<InferredTerminal>,
    pub outputs: Vec<InferredTerminal>,
    pub rows: Vec<TruthTableRow>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
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
    NoDriverPosition(Pos),
    Simulation(String),
}

impl std::fmt::Display for TruthTableError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::TooManyInputs(count) => write!(f, "cannot enumerate {count} inferred inputs"),
            Self::NoDriverPosition(pos) => {
                write!(f, "no safe driver position for input at {pos:?}")
            }
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
    if analysis.inputs.len() > max_inputs || analysis.inputs.len() >= usize::BITS as usize {
        return Err(TruthTableError::TooManyInputs(analysis.inputs.len()));
    }
    let drivers = analysis
        .inputs
        .iter()
        .map(|terminal| input_driver(world, analysis, terminal))
        .collect::<Result<Vec<_>, _>>()?;
    let mut rows = Vec::new();
    for bits in 0..(1_usize << analysis.inputs.len()) {
        let inputs: Vec<_> = (0..analysis.inputs.len())
            .map(|index| bits & (1 << index) != 0)
            .collect();
        let mut driven = world.clone();
        for ((terminal, driver), value) in analysis.inputs.iter().zip(&drivers).zip(&inputs) {
            match driver {
                InputDriver::Lever(pos) => {
                    if let Some(block) = driven.get(*pos).cloned() {
                        let mut block = block;
                        block.powered = Some(*value);
                        driven.set(*pos, block);
                    }
                }
                InputDriver::External(pos) if *value => {
                    driven.set(*pos, Block::new(BlockKind::RedstoneBlock));
                }
                InputDriver::External(pos) => {
                    driven.remove(*pos);
                }
            }
            debug_assert!(
                analysis.components[terminal.component]
                    .positions
                    .contains(&terminal.anchor)
            );
        }
        update_wire_shapes(&mut driven);
        let state = RedstoneTickSimulator::new(driven)
            .and_then(|mut simulator| simulator.settle_ticks(settle_ticks))
            .map_err(|error| TruthTableError::Simulation(error.to_string()))?;
        let outputs = analysis
            .outputs
            .iter()
            .map(|terminal| state.strength(terminal.anchor) > 0)
            .collect();
        rows.push(TruthTableRow { inputs, outputs });
    }
    Ok(InferredTruthTable {
        inputs: analysis.inputs.clone(),
        outputs: analysis.outputs.clone(),
        rows,
    })
}

#[must_use]
pub fn infer_output_expressions(table: &InferredTruthTable) -> Vec<Expr> {
    (0..table.outputs.len())
        .map(|output| infer_output_expression(table, output))
        .collect()
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
enum InputDriver {
    Lever(Pos),
    External(Pos),
}

fn input_driver(
    world: &World,
    analysis: &RegionAnalysis,
    terminal: &InferredTerminal,
) -> Result<InputDriver, TruthTableError> {
    if world.kind_at(terminal.anchor) == BlockKind::Lever {
        return Ok(InputDriver::Lever(terminal.anchor));
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
        return Ok(InputDriver::External(pos));
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
                return Ok(InputDriver::External(pos));
            }
        }
    }
    Err(TruthTableError::NoDriverPosition(terminal.anchor))
}

#[must_use]
pub fn analyze_world_region(world: &World, bounds: RegionBounds) -> RegionAnalysis {
    let bounded = bounded_world(world, bounds);
    let graph = extract_connectivity(&bounded);
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
        .filter(|(_, block)| is_redstone_kind(block.kind))
        .map(|(pos, block)| (*pos, block.kind))
        .collect();
    let unsupported = bounded
        .iter()
        .filter(|(_, block)| matches!(block.kind, BlockKind::Comparator | BlockKind::Piston))
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
    let inputs: Vec<_> = components
        .iter()
        .filter(|component| component.incoming.is_empty() && !component.outgoing.is_empty())
        .filter_map(|component| infer_input(&bounded, component))
        .collect();
    let outputs: Vec<_> = components
        .iter()
        .filter(|component| component.outgoing.is_empty() && !component.incoming.is_empty())
        .filter_map(|component| infer_output(&bounded, component))
        .collect();
    let diagnostics = signal_diagnostics(
        &bounded,
        &graph,
        &components,
        &inputs,
        &outputs,
        &redstone_blocks,
    );
    RegionAnalysis {
        bounds,
        redstone_blocks,
        graph,
        components,
        inputs,
        outputs,
        unsupported,
        diagnostics,
    }
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
            | BlockKind::RedstoneBlock
            | BlockKind::Piston
    )
}

fn infer_input(world: &World, component: &SignalComponent) -> Option<InferredTerminal> {
    let certain = component.positions.iter().copied().find(|pos| {
        matches!(
            world.kind_at(*pos),
            BlockKind::Lever | BlockKind::RedstoneBlock
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
    component
        .positions
        .iter()
        .copied()
        .filter(|pos| {
            matches!(
                world.kind_at(*pos),
                BlockKind::RedstoneWire | BlockKind::Repeater | BlockKind::Piston
            )
        })
        .max()
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
        assert!(!comparison.comparable, "{comparison:?}");
        assert_eq!(comparison.actual_outputs, 3, "{comparison:?}");
        assert!(comparison.terminal_count_delta > 0, "{comparison:?}");
        assert!(comparison.differing_bits > 0, "{comparison:?}");
        assert!(comparison.fitness_penalty > 0, "{comparison:?}");
    }
}
