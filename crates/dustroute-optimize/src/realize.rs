use std::collections::BTreeSet;
use std::error::Error;
use std::fmt::{Display, Formatter};

use dustroute_physical::{
    Block, BlockKind, PhysicalBlockChange, PhysicalPatch, PhysicalPatchReason, World,
};
use dustroute_translate::multinet::{
    LegalityReport, MultiNetRouting, NetId, RipupRoutingError, RoutingJob, materialize_multinet,
    route_jobs_ripup, validate_routing_legality,
};
use dustroute_translate::physical::{Endpoint, PhysicalError, PlacementCircuit};
use dustroute_translate::routing::RouterConfig;
use dustroute_translate::world_reverse::{
    InferredTerminal, InferredTruthTable, RegionAnalysis, RegionBounds, TerminalConfidence,
    TruthTableComparison, analyze_world_region, compare_truth_tables, infer_truth_table,
};
use dustroute_translate::{RedstoneTickSimulator, TruthTableRow, update_wire_shapes};

use crate::phased::optimize_staged_with_validator;
use crate::{OptimizationPlan, StagedOptimizationResult};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct OptimizationRoutingConfig {
    pub router: RouterConfig,
    pub max_attempts: usize,
    pub ripup_width: usize,
    pub max_wire_run: usize,
}

impl Default for OptimizationRoutingConfig {
    fn default() -> Self {
        Self {
            router: RouterConfig::default(),
            max_attempts: 64,
            ripup_width: 2,
            max_wire_run: 12,
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum OptimizationRealizationError {
    Physical(PhysicalError),
    Routing(String),
    IllegalRouting(LegalityReport),
    SourceMismatch {
        position: dustroute_physical::Pos,
        expected: Box<Block>,
        actual: Box<Block>,
    },
    TargetOccupied {
        position: dustroute_physical::Pos,
        actual: Box<Block>,
    },
}

impl Display for OptimizationRealizationError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Physical(error) => Display::fmt(error, formatter),
            Self::Routing(message) => write!(formatter, "optimization routing failed: {message}"),
            Self::IllegalRouting(_) => {
                formatter.write_str("optimized routing is electrically illegal")
            }
            Self::SourceMismatch { position, .. } => {
                write!(formatter, "observed source differs at {position:?}")
            }
            Self::TargetOccupied { position, .. } => {
                write!(formatter, "optimized target is occupied at {position:?}")
            }
        }
    }
}

impl Error for OptimizationRealizationError {}

impl From<PhysicalError> for OptimizationRealizationError {
    fn from(value: PhysicalError) -> Self {
        Self::Physical(value)
    }
}

impl From<RipupRoutingError> for OptimizationRealizationError {
    fn from(value: RipupRoutingError) -> Self {
        Self::Routing(value.to_string())
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct RealizedOptimization {
    pub optimization: StagedOptimizationResult,
    pub routing: MultiNetRouting,
    pub legality: LegalityReport,
    pub world: World,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct BehavioralVerificationConfig {
    pub max_inputs: usize,
    pub settle_ticks: usize,
    pub bounds_margin: i32,
}

impl Default for BehavioralVerificationConfig {
    fn default() -> Self {
        Self {
            max_inputs: 8,
            settle_ticks: 8,
            bounds_margin: 2,
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum BehavioralEquivalence {
    Verified(TruthTableComparison),
    Mismatch(TruthTableComparison),
    Unavailable(String),
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct OptimizationVerification {
    pub topology_preserved: bool,
    pub original_analysis: RegionAnalysis,
    pub optimized_analysis: RegionAnalysis,
    pub behavior: BehavioralEquivalence,
}

impl OptimizationVerification {
    #[must_use]
    pub fn verified(&self) -> bool {
        self.topology_preserved
            && self
                .optimized_analysis
                .diagnostics
                .invalid_supports
                .is_empty()
            && matches!(self.behavior, BehavioralEquivalence::Verified(_))
    }
}

fn routing_jobs(circuit: &PlacementCircuit) -> Vec<RoutingJob> {
    let mut grouped: Vec<(Endpoint, Vec<Endpoint>)> = Vec::new();
    for route in circuit.routes.values() {
        if let Some((_, sinks)) = grouped
            .iter_mut()
            .find(|(source, _)| source == &route.source)
        {
            if !sinks.contains(&route.sink) {
                sinks.push(route.sink.clone());
            }
        } else {
            grouped.push((route.source.clone(), vec![route.sink.clone()]));
        }
    }
    grouped
        .into_iter()
        .enumerate()
        .map(|(index, (source, sinks))| RoutingJob {
            id: NetId(u32::try_from(index).expect("route group count fits u32")),
            source,
            sinks,
        })
        .collect()
}

#[cfg(test)]
fn realize_staged_optimization(
    circuit: &PlacementCircuit,
    plan: &OptimizationPlan,
    config: OptimizationRoutingConfig,
) -> Result<RealizedOptimization, OptimizationRealizationError> {
    let optimization = crate::optimize_staged(circuit, plan);
    let (routing, legality, world) = route_and_materialize(&optimization.circuit, config)?;
    Ok(RealizedOptimization {
        optimization,
        routing,
        legality,
        world,
    })
}

pub fn realize_staged_optimization_against(
    circuit: &PlacementCircuit,
    original_world: &World,
    plan: &OptimizationPlan,
    config: OptimizationRoutingConfig,
) -> Result<RealizedOptimization, OptimizationRealizationError> {
    let verification_config = BehavioralVerificationConfig::default();
    let optimization = optimize_staged_with_validator(circuit, plan, |candidate| {
        let Ok(candidate_world) =
            route_and_materialize(candidate, config).map(|(_, _, world)| world)
        else {
            return false;
        };
        behavior_matches(
            original_world,
            circuit,
            &candidate_world,
            candidate,
            verification_config,
        )
    });
    if optimization
        .phases
        .iter()
        .all(|phase| phase.accepted.is_empty())
    {
        return Ok(RealizedOptimization {
            optimization,
            routing: MultiNetRouting::default(),
            legality: LegalityReport::default(),
            world: original_world.clone(),
        });
    }
    let (routing, legality, world) = route_and_materialize(&optimization.circuit, config)?;
    Ok(RealizedOptimization {
        optimization,
        routing,
        legality,
        world,
    })
}

fn route_and_materialize(
    circuit: &PlacementCircuit,
    config: OptimizationRoutingConfig,
) -> Result<(MultiNetRouting, LegalityReport, World), OptimizationRealizationError> {
    let routed = route_jobs_ripup(
        circuit,
        routing_jobs(circuit),
        config.router,
        config.max_attempts,
        config.ripup_width,
    )?;
    let world = materialize_multinet(circuit, &routed.routing)
        .map_err(|error| OptimizationRealizationError::Routing(error.to_string()))?;
    let legality = validate_routing_legality(circuit, &routed.routing, &world, config.max_wire_run);
    if !legality.valid() {
        return Err(OptimizationRealizationError::IllegalRouting(legality));
    }
    Ok((routed.routing, legality, world))
}

fn behavior_matches(
    original_world: &World,
    original: &PlacementCircuit,
    candidate_world: &World,
    candidate: &PlacementCircuit,
    config: BehavioralVerificationConfig,
) -> bool {
    let bounds = analysis_bounds(original_world, candidate_world, config.bounds_margin);
    let original_analysis = analyze_world_region(original_world, bounds);
    let candidate_analysis = analyze_world_region(candidate_world, bounds);
    match (
        named_truth_table(original_world, &original_analysis, original, config),
        named_truth_table(candidate_world, &candidate_analysis, candidate, config),
    ) {
        (Ok(before), Ok(after)) => {
            let comparison = compare_truth_tables(&before, &after);
            comparison.comparable && comparison.fitness_penalty == 0
        }
        _ => false,
    }
}

fn logical_topology_preserved(before: &PlacementCircuit, after: &PlacementCircuit) -> bool {
    before.cells.len() == after.cells.len()
        && before.cells.iter().all(|(id, node)| {
            after
                .cells
                .get(id)
                .is_some_and(|candidate| candidate.logical_kind == node.logical_kind)
        })
        && before.routes.len() == after.routes.len()
        && before.routes.iter().all(|(id, route)| {
            after.routes.get(id).is_some_and(|candidate| {
                route.source.cell == candidate.source.cell
                    && route.source.port == candidate.source.port
                    && route.source.kind == candidate.source.kind
                    && route.sink.cell == candidate.sink.cell
                    && route.sink.port == candidate.sink.port
                    && route.sink.kind == candidate.sink.kind
            })
        })
}

fn analysis_bounds(first: &World, second: &World, margin: i32) -> RegionBounds {
    let bounds = first
        .positions()
        .chain(second.positions())
        .fold(None, |bounds, position| match bounds {
            None => Some((position, position)),
            Some((minimum, maximum)) => Some((
                dustroute_physical::Pos::new(
                    minimum.x.min(position.x),
                    minimum.y.min(position.y),
                    minimum.z.min(position.z),
                ),
                dustroute_physical::Pos::new(
                    maximum.x.max(position.x),
                    maximum.y.max(position.y),
                    maximum.z.max(position.z),
                ),
            )),
        })
        .unwrap_or_default();
    RegionBounds::new(
        bounds.0.offset(-margin, -margin, -margin),
        bounds.1.offset(margin, margin, margin),
    )
}

fn boundary_endpoints(circuit: &PlacementCircuit, inputs: bool) -> Vec<Endpoint> {
    let direction = if inputs {
        dustroute_translate::TerminalDirection::Input
    } else {
        dustroute_translate::TerminalDirection::Output
    };
    if !circuit.terminals.is_empty() {
        return circuit
            .terminals
            .values()
            .filter(|terminal| terminal.direction == direction)
            .map(|terminal| terminal.endpoint.clone())
            .collect();
    }
    let mut endpoints = circuit
        .routes
        .values()
        .filter_map(|route| {
            let endpoint = if inputs { &route.source } else { &route.sink };
            endpoint.cell.is_none().then_some(endpoint.clone())
        })
        .collect::<Vec<_>>();
    endpoints.sort_by(|left, right| (&left.port, left.pos).cmp(&(&right.port, right.pos)));
    endpoints.dedup_by(|left, right| left.port == right.port && left.pos == right.pos);
    endpoints
}

fn terminal_order(
    analysis: &RegionAnalysis,
    terminals: &[InferredTerminal],
    boundaries: &[Endpoint],
) -> Option<Vec<usize>> {
    if terminals.len() != boundaries.len() {
        return None;
    }
    let mut used = BTreeSet::new();
    let mut order = Vec::with_capacity(boundaries.len());
    for boundary in boundaries {
        let exact = terminals.iter().enumerate().find_map(|(index, terminal)| {
            (!used.contains(&index)
                && analysis.components[terminal.component]
                    .positions
                    .contains(&boundary.pos))
            .then_some(index)
        });
        let index = exact.or_else(|| {
            terminals
                .iter()
                .enumerate()
                .filter(|(index, _)| !used.contains(index))
                .min_by_key(|(_, terminal)| {
                    terminal.anchor.x.abs_diff(boundary.pos.x)
                        + terminal.anchor.y.abs_diff(boundary.pos.y)
                        + terminal.anchor.z.abs_diff(boundary.pos.z)
                })
                .map(|(index, _)| index)
        })?;
        used.insert(index);
        order.push(index);
    }
    Some(order)
}

fn canonical_table(
    table: &InferredTruthTable,
    input_order: &[usize],
    output_order: &[usize],
) -> InferredTruthTable {
    let mut rows = table
        .rows
        .iter()
        .map(|row| dustroute_translate::TruthTableRow {
            inputs: input_order.iter().map(|index| row.inputs[*index]).collect(),
            outputs: output_order
                .iter()
                .map(|index| row.outputs[*index])
                .collect(),
        })
        .collect::<Vec<_>>();
    rows.sort_by_key(|row| {
        row.inputs
            .iter()
            .enumerate()
            .fold(0_usize, |bits, (index, value)| {
                bits | (usize::from(*value) << index)
            })
    });
    InferredTruthTable {
        inputs: input_order
            .iter()
            .map(|index| table.inputs[*index].clone())
            .collect(),
        outputs: output_order
            .iter()
            .map(|index| table.outputs[*index].clone())
            .collect(),
        rows,
    }
}

fn named_truth_table(
    world: &World,
    analysis: &RegionAnalysis,
    circuit: &PlacementCircuit,
    config: BehavioralVerificationConfig,
) -> Result<InferredTruthTable, String> {
    let inputs = boundary_endpoints(circuit, true);
    let outputs = boundary_endpoints(circuit, false);
    if !inputs.is_empty() && !outputs.is_empty() {
        return direct_boundary_table(world, &inputs, &outputs, config);
    }
    let table = infer_truth_table(world, analysis, config.max_inputs, config.settle_ticks)
        .map_err(|error| error.to_string())?;
    let input_order = terminal_order(analysis, &table.inputs, &inputs)
        .ok_or_else(|| "inferred inputs do not match named circuit boundaries".to_owned())?;
    let output_order = terminal_order(analysis, &table.outputs, &outputs)
        .ok_or_else(|| "inferred outputs do not match named circuit boundaries".to_owned())?;
    Ok(canonical_table(&table, &input_order, &output_order))
}

fn direct_boundary_table(
    world: &World,
    inputs: &[Endpoint],
    outputs: &[Endpoint],
    config: BehavioralVerificationConfig,
) -> Result<InferredTruthTable, String> {
    if inputs.len() > config.max_inputs || inputs.len() >= usize::BITS as usize {
        return Err(format!("too many boundary inputs: {}", inputs.len()));
    }
    let mut rows = Vec::new();
    for bits in 0..(1_usize << inputs.len()) {
        let values = (0..inputs.len())
            .map(|index| bits & (1 << index) != 0)
            .collect::<Vec<_>>();
        let mut driven = world.clone();
        for (endpoint, powered) in inputs.iter().zip(&values) {
            if *powered {
                driven.set(endpoint.pos, Block::new(BlockKind::RedstoneBlock));
            }
        }
        update_wire_shapes(&mut driven);
        let state = RedstoneTickSimulator::new(driven)
            .and_then(|mut simulator| simulator.settle_ticks(config.settle_ticks))
            .map_err(|error| error.to_string())?;
        rows.push(TruthTableRow {
            inputs: values,
            outputs: outputs
                .iter()
                .map(|endpoint| state.powered(endpoint.pos))
                .collect(),
        });
    }
    let terminal = |endpoint: &Endpoint| InferredTerminal {
        anchor: endpoint.pos,
        component: 0,
        confidence: TerminalConfidence::Certain,
    };
    Ok(InferredTruthTable {
        inputs: inputs.iter().map(terminal).collect(),
        outputs: outputs.iter().map(terminal).collect(),
        rows,
    })
}

#[must_use]
pub fn verify_realized_optimization(
    original_world: &World,
    original: &PlacementCircuit,
    realized: &RealizedOptimization,
    config: BehavioralVerificationConfig,
) -> OptimizationVerification {
    let bounds = analysis_bounds(original_world, &realized.world, config.bounds_margin);
    let original_analysis = analyze_world_region(original_world, bounds);
    let optimized_analysis = analyze_world_region(&realized.world, bounds);
    let behavior = match (
        named_truth_table(original_world, &original_analysis, original, config),
        named_truth_table(
            &realized.world,
            &optimized_analysis,
            &realized.optimization.circuit,
            config,
        ),
    ) {
        (Ok(before), Ok(after)) => {
            let comparison = compare_truth_tables(&before, &after);
            if comparison.comparable && comparison.fitness_penalty == 0 {
                BehavioralEquivalence::Verified(comparison)
            } else {
                BehavioralEquivalence::Mismatch(comparison)
            }
        }
        (Err(before), Err(after)) => {
            BehavioralEquivalence::Unavailable(format!("original: {before}; optimized: {after}"))
        }
        (Err(error), _) => BehavioralEquivalence::Unavailable(format!("original: {error}")),
        (_, Err(error)) => BehavioralEquivalence::Unavailable(format!("optimized: {error}")),
    };
    OptimizationVerification {
        topology_preserved: logical_topology_preserved(original, &realized.optimization.circuit),
        original_analysis,
        optimized_analysis,
        behavior,
    }
}

/// Builds a stale-safe, reversible patch over the footprint owned by the
/// original and optimized circuits. Unrelated observed blocks are untouched.
pub fn optimization_patch(
    observed: &World,
    original: &PlacementCircuit,
    realized: &RealizedOptimization,
) -> Result<PhysicalPatch, OptimizationRealizationError> {
    let original_world = original.build_world()?;
    let positions = original_world
        .iter()
        .map(|(position, _)| *position)
        .chain(realized.world.iter().map(|(position, _)| *position))
        .collect::<BTreeSet<_>>();
    let mut changes = Vec::new();
    for position in positions {
        let expected = original_world
            .get(position)
            .cloned()
            .unwrap_or_else(|| Block::new(BlockKind::Air));
        let actual = observed
            .get(position)
            .cloned()
            .unwrap_or_else(|| Block::new(BlockKind::Air));
        let after = realized
            .world
            .get(position)
            .cloned()
            .unwrap_or_else(|| Block::new(BlockKind::Air));
        if expected.kind != BlockKind::Air && actual != expected {
            return Err(OptimizationRealizationError::SourceMismatch {
                position,
                expected: Box::new(expected),
                actual: Box::new(actual),
            });
        }
        if expected.kind == BlockKind::Air
            && after.kind != BlockKind::Air
            && actual.kind != BlockKind::Air
        {
            return Err(OptimizationRealizationError::TargetOccupied {
                position,
                actual: Box::new(actual),
            });
        }
        if actual != after {
            changes.push(PhysicalBlockChange {
                pos: position,
                before: actual,
                after,
            });
        }
    }
    Ok(PhysicalPatch {
        reason: PhysicalPatchReason::OptimizePlacement,
        affected_fragments: Vec::new(),
        confidence_percent: 100,
        explanation: format!(
            "apply {} staged placement optimization phase(s) with verified routing",
            realized.optimization.phases.len()
        ),
        changes,
    })
}

#[cfg(test)]
mod tests {
    use dustroute_translate::cells::{PlacedCell, PortKind, RotationY, not_cell};
    use dustroute_translate::logic::GateKind;
    use dustroute_translate::physical::PlacementCircuit;
    use dustroute_translate::world::Pos;
    use dustroute_translate::{BaselineCompileConfig, BaselineCompiler, half_adder};

    use super::*;

    fn circuit() -> PlacementCircuit {
        let mut circuit = PlacementCircuit::new();
        let gate = circuit.add_cell(
            GateKind::Not,
            PlacedCell {
                cell: not_cell(),
                origin: Pos::new(4, 2, 0),
                rotation: RotationY::R0,
            },
        );
        circuit.add_route(
            PlacementCircuit::boundary("in", Pos::new(-2, 3, 0), PortKind::Wire, None),
            circuit.input_endpoint(gate, "a").unwrap(),
            vec![],
            vec![],
        );
        circuit.add_route(
            circuit.output_endpoint(gate, "out").unwrap(),
            PlacementCircuit::boundary("out", Pos::new(12, 3, 0), PortKind::Wire, None),
            vec![],
            vec![],
        );
        circuit
    }

    #[test]
    fn routed_optimization_becomes_an_exact_reversible_patch() {
        let circuit = circuit();
        let observed = circuit.build_world().unwrap();
        let plan = OptimizationPlan { phases: vec![] };
        let realized =
            realize_staged_optimization(&circuit, &plan, OptimizationRoutingConfig::default())
                .unwrap();
        let patch = optimization_patch(&observed, &circuit, &realized).unwrap();
        assert!(!patch.changes.is_empty());
        let optimized = patch.apply_virtual(&observed).unwrap();
        assert_eq!(optimized, realized.world);
        assert_eq!(patch.inverse().apply_virtual(&optimized).unwrap(), observed);
    }

    #[test]
    fn patch_refuses_to_overwrite_an_unowned_target() {
        let circuit = circuit();
        let mut observed = circuit.build_world().unwrap();
        let plan = OptimizationPlan { phases: vec![] };
        let realized =
            realize_staged_optimization(&circuit, &plan, OptimizationRoutingConfig::default())
                .unwrap();
        let target = realized
            .world
            .iter()
            .map(|(position, _)| *position)
            .find(|position| observed.kind_at(*position) == BlockKind::Air)
            .unwrap();
        observed.set(target, Block::new(BlockKind::RedstoneBlock));
        assert!(matches!(
            optimization_patch(&observed, &circuit, &realized),
            Err(OptimizationRealizationError::TargetOccupied { .. })
        ));
    }

    #[test]
    fn verification_reanalyzes_physics_and_compares_named_boundaries() {
        let circuit = circuit();
        let plan = OptimizationPlan { phases: vec![] };
        let baseline =
            realize_staged_optimization(&circuit, &plan, OptimizationRoutingConfig::default())
                .unwrap();
        let candidate =
            realize_staged_optimization(&circuit, &plan, OptimizationRoutingConfig::default())
                .unwrap();
        let verification = verify_realized_optimization(
            &baseline.world,
            &circuit,
            &candidate,
            BehavioralVerificationConfig::default(),
        );
        assert!(verification.topology_preserved);
        assert!(
            matches!(
                verification.behavior,
                BehavioralEquivalence::Verified(TruthTableComparison {
                    fitness_penalty: 0,
                    ..
                })
            ),
            "{:?}",
            verification.behavior
        );
        assert!(verification.verified());
    }

    #[test]
    fn verification_rejects_a_physically_broken_candidate() {
        let circuit = circuit();
        let plan = OptimizationPlan { phases: vec![] };
        let baseline =
            realize_staged_optimization(&circuit, &plan, OptimizationRoutingConfig::default())
                .unwrap();
        let mut candidate = baseline.clone();
        candidate.world.remove(Pos::new(12, 3, 0));
        let verification = verify_realized_optimization(
            &baseline.world,
            &circuit,
            &candidate,
            BehavioralVerificationConfig::default(),
        );
        assert!(verification.topology_preserved);
        assert!(matches!(
            verification.behavior,
            BehavioralEquivalence::Mismatch(TruthTableComparison {
                fitness_penalty: 1..,
                ..
            })
        ));
        assert!(!verification.verified());
    }

    #[test]
    fn optimized_half_adder_preserves_boundaries_and_behavior() {
        let compiled = BaselineCompiler::new(BaselineCompileConfig::default())
            .compile(&half_adder())
            .unwrap();
        let plan = OptimizationPlan::directional_then_global(
            crate::CompressionAxis::X,
            crate::CompressionDirection::TowardMinimum,
            crate::AnchorPolicy::Inputs,
        );
        let realized = realize_staged_optimization_against(
            &compiled.physical,
            &compiled.world,
            &plan,
            OptimizationRoutingConfig::default(),
        )
        .unwrap();
        let verification = verify_realized_optimization(
            &compiled.world,
            &compiled.physical,
            &realized,
            BehavioralVerificationConfig::default(),
        );
        assert_eq!(
            realized
                .optimization
                .circuit
                .terminals
                .values()
                .filter(|terminal| {
                    terminal.direction == dustroute_translate::TerminalDirection::Input
                })
                .count(),
            2
        );
        assert_eq!(
            realized
                .optimization
                .circuit
                .terminals
                .values()
                .filter(|terminal| {
                    terminal.direction == dustroute_translate::TerminalDirection::Output
                })
                .count(),
            2
        );
        assert!(
            matches!(verification.behavior, BehavioralEquivalence::Verified(_)),
            "accepted={:?}, behavior={:?}",
            realized
                .optimization
                .phases
                .iter()
                .flat_map(|phase| &phase.accepted)
                .collect::<Vec<_>>(),
            verification.behavior
        );
        assert!(verification.verified());
    }
}
