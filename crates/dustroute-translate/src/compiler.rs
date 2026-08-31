use std::collections::BTreeMap;
use std::error::Error;
use std::fmt::{Display, Formatter};

use crate::cells::{PlacedCell, RotationY, baseline_cell_for};
use crate::logic::{GateKind, LogicDag, LogicError, NodeId};
use crate::multinet::{
    LegalityReport, MultiNetError, MultiNetRouting, NetId, RipupRoutingError, RoutingJob,
    materialize_multinet, route_jobs_ripup, validate_routing_legality,
};
use crate::physical::{CellId, PhysicalError, PlacementCircuit, TerminalDirection};
use crate::port_realization::PortRealizationError;
use crate::routing::RouterConfig;
use crate::world::{Pos, World};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct BaselineCompileConfig {
    pub spacing_x: i32,
    pub lane_gap: i32,
    pub router: RouterConfig,
    pub ripup_attempts: usize,
    pub ripup_width: usize,
}

impl Default for BaselineCompileConfig {
    fn default() -> Self {
        Self {
            spacing_x: 12,
            lane_gap: 8,
            router: RouterConfig::default(),
            ripup_attempts: 128,
            ripup_width: 3,
        }
    }
}

#[derive(Clone, Debug)]
pub struct BaselineCompileResult {
    pub abstract_dag: LogicDag,
    pub primitive_dag: LogicDag,
    pub physical: PlacementCircuit,
    pub routing: MultiNetRouting,
    pub world: World,
    pub node_to_cell: BTreeMap<NodeId, CellId>,
    pub input_positions: BTreeMap<String, Pos>,
    pub output_positions: BTreeMap<String, Pos>,
    pub legality: LegalityReport,
}

#[derive(Debug)]
pub enum CompileError {
    Logic(LogicError),
    Physical(PhysicalError),
    Port(PortRealizationError),
    Routing(MultiNetError),
    Ripup(RipupRoutingError),
    MissingCell(GateKind),
    Illegal(LegalityReport),
}

impl Display for CompileError {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Logic(error) => Display::fmt(error, f),
            Self::Physical(error) => Display::fmt(error, f),
            Self::Port(error) => Display::fmt(error, f),
            Self::Routing(error) => Display::fmt(error, f),
            Self::Ripup(error) => Display::fmt(error, f),
            Self::MissingCell(kind) => write!(f, "no baseline cell for {kind:?}"),
            Self::Illegal(report) => write!(f, "compiled routing is illegal: {report:?}"),
        }
    }
}

impl Error for CompileError {}
impl From<LogicError> for CompileError {
    fn from(value: LogicError) -> Self {
        Self::Logic(value)
    }
}
impl From<PhysicalError> for CompileError {
    fn from(value: PhysicalError) -> Self {
        Self::Physical(value)
    }
}
impl From<PortRealizationError> for CompileError {
    fn from(value: PortRealizationError) -> Self {
        Self::Port(value)
    }
}
impl From<MultiNetError> for CompileError {
    fn from(value: MultiNetError) -> Self {
        Self::Routing(value)
    }
}
impl From<RipupRoutingError> for CompileError {
    fn from(value: RipupRoutingError) -> Self {
        Self::Ripup(value)
    }
}

pub struct BaselineCompiler {
    config: BaselineCompileConfig,
}

impl BaselineCompiler {
    #[must_use]
    pub const fn new(config: BaselineCompileConfig) -> Self {
        Self { config }
    }

    pub fn compile(&self, abstract_dag: &LogicDag) -> Result<BaselineCompileResult, CompileError> {
        let primitive = abstract_dag.lower_xor()?;
        let origins =
            fanout_aware_origins(&primitive, self.config.spacing_x, self.config.lane_gap)?;
        let mut physical = PlacementCircuit::new();
        let mut node_to_cell = BTreeMap::new();
        for node in primitive.nodes() {
            let cell = baseline_cell_for(node.kind).ok_or(CompileError::MissingCell(node.kind))?;
            let id = physical.add_cell(
                node.kind,
                PlacedCell {
                    cell,
                    origin: origins[&node.id],
                    rotation: RotationY::R0,
                },
            );
            if node.kind == GateKind::Input {
                let name = node
                    .name
                    .clone()
                    .unwrap_or_else(|| format!("in{}", node.id.0));
                physical.add_terminal(
                    name,
                    TerminalDirection::Input,
                    physical.input_endpoint(id, "in")?,
                );
            }
            node_to_cell.insert(node.id, id);
        }
        let depths = primitive.logic_depths()?;
        let mut output_cells = BTreeMap::new();
        for (index, (name, source)) in primitive.outputs().iter().enumerate() {
            let origin = Pos::new(
                i32::try_from(depths[source] + 1).expect("depth fits i32") * self.config.spacing_x,
                2,
                i32::try_from(index).expect("output index fits i32") * self.config.lane_gap * 3,
            );
            let cell = baseline_cell_for(GateKind::Output).expect("output baseline cell");
            let output_cell = physical.add_cell(
                GateKind::Output,
                PlacedCell {
                    cell,
                    origin,
                    rotation: RotationY::R0,
                },
            );
            physical.add_terminal(
                name.clone(),
                TerminalDirection::Output,
                physical.output_endpoint(output_cell, "out")?,
            );
            output_cells.insert(name.clone(), output_cell);
        }

        let mut sinks_by_source: BTreeMap<NodeId, Vec<(CellId, String)>> = BTreeMap::new();
        for node in primitive.nodes() {
            for (index, source) in node.inputs.iter().enumerate() {
                let port = if index == 0 { "a" } else { "b" };
                sinks_by_source
                    .entry(*source)
                    .or_default()
                    .push((node_to_cell[&node.id], port.into()));
            }
        }
        for (name, source) in primitive.outputs() {
            sinks_by_source
                .entry(*source)
                .or_default()
                .push((output_cells[name], "in".into()));
        }
        let mut jobs = Vec::new();
        for (sequence, node_id) in primitive.topological_order()?.into_iter().enumerate() {
            let Some(sink_specs) = sinks_by_source.get(&node_id) else {
                continue;
            };
            let source = physical.output_endpoint(node_to_cell[&node_id], "out")?;
            let sinks = sink_specs
                .iter()
                .map(|(cell, port)| physical.input_endpoint(*cell, port))
                .collect::<Result<Vec<_>, _>>()?;
            jobs.push((
                NetId(u32::try_from(sequence).expect("net count fits u32")),
                source,
                sinks,
            ));
        }
        jobs.sort_by_key(|(_, _, sinks)| std::cmp::Reverse(sinks.len()));

        let routing = route_jobs_ripup(
            &physical,
            jobs.into_iter()
                .map(|(id, source, sinks)| RoutingJob { id, source, sinks })
                .collect(),
            self.config.router,
            self.config.ripup_attempts,
            self.config.ripup_width,
        )?
        .routing;
        let world = materialize_multinet(&physical, &routing)?;
        let legality = validate_routing_legality(&physical, &routing, &world, 12);
        if !legality.valid() {
            return Err(CompileError::Illegal(legality));
        }
        let input_positions = primitive
            .nodes()
            .iter()
            .filter(|node| node.kind == GateKind::Input)
            .map(|node| {
                let name = node
                    .name
                    .clone()
                    .unwrap_or_else(|| format!("in{}", node.id.0));
                physical
                    .input_endpoint(node_to_cell[&node.id], "in")
                    .map(|endpoint| (name, endpoint.pos))
            })
            .collect::<Result<_, _>>()?;
        let output_positions = output_cells
            .iter()
            .map(|(name, cell)| {
                physical
                    .output_endpoint(*cell, "out")
                    .map(|endpoint| (name.clone(), endpoint.pos))
            })
            .collect::<Result<_, _>>()?;
        Ok(BaselineCompileResult {
            abstract_dag: abstract_dag.clone(),
            primitive_dag: primitive,
            physical,
            routing,
            world,
            node_to_cell,
            input_positions,
            output_positions,
            legality,
        })
    }
}

fn fanout_aware_origins(
    dag: &LogicDag,
    spacing_x: i32,
    lane_gap: i32,
) -> Result<BTreeMap<NodeId, Pos>, LogicError> {
    let users = dag.users();
    let depths = dag.logic_depths()?;
    let output_targets: BTreeMap<_, _> = dag
        .outputs()
        .iter()
        .enumerate()
        .map(|(index, (name, _))| {
            (
                name.clone(),
                f64::from(i32::try_from(index).expect("output index fits i32") * lane_gap * 3),
            )
        })
        .collect();
    let mut desired: BTreeMap<_, _> = dag
        .outputs()
        .iter()
        .map(|(name, node)| (*node, output_targets[name]))
        .collect();
    for node in dag.topological_order()?.into_iter().rev() {
        if desired.contains_key(&node) {
            continue;
        }
        let values: Vec<_> = users
            .get(&node)
            .into_iter()
            .flatten()
            .filter_map(|user| desired.get(user))
            .copied()
            .collect();
        desired.insert(
            node,
            if values.is_empty() {
                0.0
            } else {
                values.iter().sum::<f64>() / values.len() as f64
            },
        );
    }
    let mut layers: BTreeMap<usize, Vec<NodeId>> = BTreeMap::new();
    for node in dag.topological_order()? {
        layers.entry(depths[&node]).or_default().push(node);
    }
    let mut origins = BTreeMap::new();
    for (depth, mut nodes) in layers {
        nodes.sort_by(|a, b| desired[a].total_cmp(&desired[b]).then(a.cmp(b)));
        let mut used: Vec<i32> = Vec::new();
        for node in nodes {
            let mut z = desired[&node].round() as i32;
            while used.iter().any(|other| (z - *other).abs() < lane_gap) {
                z += lane_gap;
            }
            used.push(z);
            origins.insert(
                node,
                Pos::new(
                    i32::try_from(depth).expect("depth fits i32") * spacing_x,
                    2,
                    z,
                ),
            );
        }
    }
    Ok(origins)
}

#[cfg(test)]
mod tests {
    use crate::circuits::{decoder_1_to_2, half_adder, half_subtractor, mux_2_to_1};
    use crate::sim::RedstoneTickSimulator;
    use crate::wire::update_wire_shapes;
    use crate::world::{Block, BlockKind};

    use super::*;

    #[test]
    fn compiles_half_adder_to_legal_world() {
        let result = BaselineCompiler::new(BaselineCompileConfig::default())
            .compile(&half_adder())
            .unwrap();
        assert!(
            result
                .primitive_dag
                .nodes()
                .iter()
                .all(|node| node.kind != GateKind::Xor)
        );
        assert_eq!(result.input_positions.len(), 2);
        assert_eq!(result.output_positions.len(), 2);
        assert!(result.legality.valid());
        for (a, b, carry, sum) in [
            (false, true, false, true),
            (false, false, false, false),
            (true, false, false, true),
            (true, true, true, false),
        ] {
            let mut world = result.world.clone();
            for (name, value) in [("a", a), ("b", b)] {
                if value {
                    world.set(
                        result.input_positions[name].offset(-1, 0, 0),
                        Block::new(BlockKind::RedstoneBlock),
                    );
                }
            }
            update_wire_shapes(&mut world);
            let state = RedstoneTickSimulator::new(world)
                .unwrap()
                .settle_ticks(12)
                .unwrap();
            assert_eq!(
                state.strength(result.output_positions["carry"]) > 0,
                carry,
                "carry for a={a} b={b}"
            );
            assert_eq!(
                state.strength(result.output_positions["sum"]) > 0,
                sum,
                "sum for a={a} b={b}"
            );
        }
    }

    #[test]
    fn compiles_confirmed_regression_circuits() {
        for dag in [mux_2_to_1(), decoder_1_to_2(), half_subtractor()] {
            let result = BaselineCompiler::new(BaselineCompileConfig::default())
                .compile(&dag)
                .unwrap();
            assert!(result.legality.valid());
        }
    }

    #[test]
    fn mux_select_high_with_low_inputs_stays_low_physically() {
        let result = BaselineCompiler::new(BaselineCompileConfig::default())
            .compile(&mux_2_to_1())
            .unwrap();
        let mut world = result.world.clone();
        world.set(
            result.input_positions["s"].offset(-1, 0, 0),
            Block::new(BlockKind::RedstoneBlock),
        );
        update_wire_shapes(&mut world);
        let state = RedstoneTickSimulator::new(world)
            .unwrap()
            .settle_ticks(12)
            .unwrap();
        assert_eq!(state.strength(result.output_positions["out"]), 0);
    }
}
