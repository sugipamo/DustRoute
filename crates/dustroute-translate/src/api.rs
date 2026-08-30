use std::error::Error;
use std::fmt::{Display, Formatter};

use crate::{
    BaselineCompileConfig, BaselineCompileResult, BaselineCompiler, CompileError, Expr,
    InferredTruthTable, LogicDag, RegionAnalysis, RegionBounds, TruthTableComparison,
    TruthTableError, World, analyze_world_region, compare_truth_tables, infer_output_expressions,
    infer_truth_table,
};
use dustroute_ir::TemporalAnalysis;

/// Stable entry point for both directions of circuit translation.
#[derive(Clone, Copy, Debug, Default)]
pub struct Translator;

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct ForwardOptions {
    pub compile: BaselineCompileConfig,
}

#[derive(Clone, Debug)]
pub struct ForwardResult {
    pub compiled: BaselineCompileResult,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ReverseRequest {
    pub bounds: RegionBounds,
    pub infer_truth_table: bool,
    pub max_inputs: usize,
    pub settle_ticks: usize,
}

impl ReverseRequest {
    #[must_use]
    pub const fn new(bounds: RegionBounds) -> Self {
        Self {
            bounds,
            infer_truth_table: false,
            max_inputs: 16,
            settle_ticks: 60,
        }
    }

    #[must_use]
    pub const fn with_truth_table(mut self, max_inputs: usize) -> Self {
        self.infer_truth_table = true;
        self.max_inputs = max_inputs;
        self
    }
}

#[derive(Clone, Debug)]
pub struct ReverseResult {
    pub analysis: RegionAnalysis,
    pub temporal: TemporalAnalysis,
    pub gate_view: dustroute_ir::GateView,
    pub expression_view: dustroute_ir::ExpressionView,
    pub functional_view: dustroute_ir::FunctionalView,
    pub truth_table: Option<InferredTruthTable>,
    pub expressions: Vec<Expr>,
    pub logic: Option<LogicDag>,
    pub truth_table_error: Option<TruthTableError>,
}

#[derive(Debug)]
pub enum TranslateError {
    Compile(CompileError),
}

impl Display for TranslateError {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Compile(error) => Display::fmt(error, f),
        }
    }
}

impl Error for TranslateError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Compile(error) => Some(error),
        }
    }
}

impl From<CompileError> for TranslateError {
    fn from(value: CompileError) -> Self {
        Self::Compile(value)
    }
}

impl Translator {
    pub fn forward(
        &self,
        circuit: &LogicDag,
        options: ForwardOptions,
    ) -> Result<ForwardResult, TranslateError> {
        let compiled = BaselineCompiler::new(options.compile).compile(circuit)?;
        Ok(ForwardResult { compiled })
    }

    #[must_use]
    pub fn reverse(&self, world: &World, request: ReverseRequest) -> ReverseResult {
        let analysis = analyze_world_region(world, request.bounds);
        let mut temporal = TemporalAnalysis::from_scene(&analysis.scene);
        if !temporal.behavior.devices.is_empty()
            && let Ok(trace) = crate::simulate_behavior_trace(
                world,
                &analysis.scene,
                &temporal,
                request.settle_ticks,
                "observed initial state",
            )
        {
            temporal.behavior.traces.push(trace);
        }
        let mut gate_view = dustroute_ir::recognize_gates(&analysis.scene);
        let mut expression_view = dustroute_ir::derive_expressions(&analysis.scene, &gate_view);
        let (truth_table, expressions, logic, truth_table_error) = if request.infer_truth_table {
            match infer_truth_table(world, &analysis, request.max_inputs, request.settle_ticks) {
                Ok(truth_table) => {
                    let expressions = infer_output_expressions(&truth_table);
                    let logic = dustroute_ir::logic_from_expressions(
                        expressions
                            .iter()
                            .cloned()
                            .enumerate()
                            .map(|(index, expression)| (format!("o{index}"), expression)),
                    )
                    .ok();
                    (Some(truth_table), expressions, logic, None)
                }
                Err(error) => (None, Vec::new(), None, Some(error)),
            }
        } else {
            (None, Vec::new(), None, None)
        };
        if let Some(table) = &truth_table {
            append_truth_table_views(
                &analysis,
                table,
                &expressions,
                &mut gate_view,
                &mut expression_view,
            );
        }
        let functional_view = dustroute_ir::classify_function(&gate_view, &expression_view);
        ReverseResult {
            analysis,
            temporal,
            gate_view,
            expression_view,
            functional_view,
            truth_table,
            expressions,
            logic,
            truth_table_error,
        }
    }

    #[must_use]
    pub fn verify(
        &self,
        expected: &InferredTruthTable,
        actual: &ReverseResult,
    ) -> Option<TruthTableComparison> {
        actual
            .truth_table
            .as_ref()
            .map(|table| compare_truth_tables(expected, table))
    }
}

fn append_truth_table_views(
    analysis: &RegionAnalysis,
    table: &InferredTruthTable,
    expressions: &[Expr],
    gates: &mut dustroute_ir::GateView,
    expression_view: &mut dustroute_ir::ExpressionView,
) {
    use std::collections::BTreeSet;

    let input_ports = table
        .inputs
        .iter()
        .filter_map(|terminal| {
            analysis
                .scene
                .component_at(terminal.anchor)
                .and_then(|component| {
                    component
                        .ports
                        .first()
                        .map(|port| dustroute_physical::PortRef {
                            component: component.id,
                            port: port.id,
                        })
                })
        })
        .collect::<Vec<_>>();
    for (terminal, expression) in table.outputs.iter().zip(expressions) {
        let Some(kind) = expression_gate_kind(expression) else {
            continue;
        };
        let physical_components = analysis
            .components
            .get(terminal.component)
            .into_iter()
            .flat_map(|component| &component.positions)
            .filter_map(|pos| {
                analysis
                    .scene
                    .component_at(*pos)
                    .map(|component| component.id)
            })
            .collect::<BTreeSet<_>>();
        let outputs = analysis
            .scene
            .component_at(terminal.anchor)
            .and_then(|component| {
                component
                    .ports
                    .first()
                    .map(|port| dustroute_physical::PortRef {
                        component: component.id,
                        port: port.id,
                    })
            })
            .into_iter()
            .collect::<Vec<_>>();
        let boundary_limited =
            !physical_components.is_disjoint(&analysis.scene.open_frontier_components());
        let status = if boundary_limited {
            dustroute_ir::RecognitionStatus::BoundaryLimited
        } else {
            dustroute_ir::RecognitionStatus::Complete
        };
        let id = dustroute_ir::GateId(gates.gates.len());
        gates.gates.push(dustroute_ir::RecognizedGate {
            id,
            kind,
            status,
            inputs: input_ports.clone(),
            outputs,
            physical_components: physical_components.clone(),
            confidence: if boundary_limited {
                dustroute_physical::Confidence::Medium
            } else {
                dustroute_physical::Confidence::High
            },
            evidence: vec![dustroute_ir::GateEvidence::TruthTableInference {
                rows: table.rows.len(),
            }],
        });
        expression_view
            .expressions
            .push(dustroute_ir::DerivedExpression {
                id: dustroute_ir::ExpressionId(expression_view.expressions.len()),
                gate: id,
                expression: derived_truth_expression(expression, &input_ports),
                physical_components,
                status,
            });
    }
}

fn expression_gate_kind(expression: &Expr) -> Option<dustroute_ir::RecognizedGateKind> {
    match expression {
        Expr::Not(_) => Some(dustroute_ir::RecognizedGateKind::Not),
        Expr::And(_) => Some(dustroute_ir::RecognizedGateKind::And),
        Expr::Or(_) => Some(dustroute_ir::RecognizedGateKind::Or),
        Expr::Xor(_) => Some(dustroute_ir::RecognizedGateKind::Xor),
        Expr::Nand(_) => Some(dustroute_ir::RecognizedGateKind::Nand),
        Expr::Var(_) | Expr::Const(_) => None,
    }
}

fn derived_truth_expression(
    expression: &Expr,
    inputs: &[dustroute_physical::PortRef],
) -> dustroute_ir::DerivedExpr {
    match expression {
        Expr::Var(name) => name
            .strip_prefix('x')
            .and_then(|index| index.parse::<usize>().ok())
            .and_then(|index| inputs.get(index).copied())
            .map_or_else(
                || dustroute_ir::DerivedExpr::Unknown(Vec::new()),
                dustroute_ir::DerivedExpr::Signal,
            ),
        Expr::Const(_) => dustroute_ir::DerivedExpr::Unknown(Vec::new()),
        Expr::Not(inner) => {
            dustroute_ir::DerivedExpr::Not(Box::new(derived_truth_expression(inner, inputs)))
        }
        Expr::And(values) => dustroute_ir::DerivedExpr::And(
            values
                .iter()
                .map(|value| derived_truth_expression(value, inputs))
                .collect(),
        ),
        Expr::Or(values) => dustroute_ir::DerivedExpr::Or(
            values
                .iter()
                .map(|value| derived_truth_expression(value, inputs))
                .collect(),
        ),
        Expr::Xor(values) => dustroute_ir::DerivedExpr::Xor(
            values
                .iter()
                .map(|value| derived_truth_expression(value, inputs))
                .collect(),
        ),
        Expr::Nand(values) => {
            dustroute_ir::DerivedExpr::Not(Box::new(dustroute_ir::DerivedExpr::And(
                values
                    .iter()
                    .map(|value| derived_truth_expression(value, inputs))
                    .collect(),
            )))
        }
    }
}

#[cfg(test)]
mod tests {
    use dustroute_ir::{DerivedExpr, RecognizedGateKind};

    use super::*;
    use crate::half_adder;

    #[test]
    fn truth_table_is_explicitly_opted_in() {
        let bounds = RegionBounds::new(crate::Pos::new(0, 0, 0), crate::Pos::new(1, 1, 1));
        let default_request = ReverseRequest::new(bounds);
        assert!(!default_request.infer_truth_table);
        let requested = default_request.with_truth_table(4);
        assert!(requested.infer_truth_table);
        assert_eq!(requested.max_inputs, 4);
    }

    #[test]
    fn physical_first_reverse_exposes_local_gates_before_function_metadata() {
        let forward = Translator
            .forward(&half_adder(), ForwardOptions::default())
            .unwrap();
        let (min, max) = forward.compiled.world.bounds().unwrap();
        let reverse = Translator.reverse(
            &forward.compiled.world,
            ReverseRequest::new(RegionBounds::new(
                min.offset(-1, -1, -1),
                max.offset(1, 1, 1),
            ))
            .with_truth_table(16),
        );
        assert!(
            reverse
                .gate_view
                .gates
                .iter()
                .any(|gate| gate.kind == RecognizedGateKind::Not)
        );
        assert!(
            reverse
                .gate_view
                .gates
                .iter()
                .any(|gate| gate.kind == RecognizedGateKind::Or)
        );
        assert!(
            reverse
                .expression_view
                .expressions
                .iter()
                .any(|expression| { matches!(expression.expression, DerivedExpr::And(_)) }),
            "{:#?}",
            reverse.expression_view
        );
        assert!(
            reverse
                .functional_view
                .candidates
                .iter()
                .any(|candidate| candidate.kind == dustroute_ir::FunctionalKind::HalfAdder),
            "{:#?}",
            reverse.functional_view
        );
        assert!(reverse.analysis.scene.observation.is_complete());
    }

    #[test]
    fn broken_physical_scene_keeps_local_gates_and_proposes_a_patch() {
        let forward = Translator
            .forward(&half_adder(), ForwardOptions::default())
            .unwrap();
        let mut world = forward.compiled.world.clone();
        let missing = world
            .iter()
            .find_map(|(pos, block)| {
                if block.kind != crate::BlockKind::RedstoneWire {
                    return None;
                }
                let east_west = world.kind_at(pos.offset(-1, 0, 0))
                    == crate::BlockKind::RedstoneWire
                    && world.kind_at(pos.offset(1, 0, 0)) == crate::BlockKind::RedstoneWire;
                let north_south = world.kind_at(pos.offset(0, 0, -1))
                    == crate::BlockKind::RedstoneWire
                    && world.kind_at(pos.offset(0, 0, 1)) == crate::BlockKind::RedstoneWire;
                (east_west || north_south).then_some(*pos)
            })
            .expect("compiled half adder should contain an inline wire");
        world.remove(missing);
        crate::update_wire_shapes(&mut world);
        let (min, max) = forward.compiled.world.bounds().unwrap();
        let bounds = RegionBounds::new(min.offset(-1, -1, -1), max.offset(1, 1, 1));
        let analysis = analyze_world_region(&world, bounds);
        let gates = dustroute_ir::recognize_gates(&analysis.scene);
        let repairs = crate::propose_scene_repairs(&world, &analysis.scene, 2);
        assert!(
            gates
                .gates
                .iter()
                .any(|gate| gate.kind == RecognizedGateKind::Not)
        );
        assert!(analysis.scene.fragments.len() > 1);
        assert!(repairs.iter().any(|proposal| {
            proposal.patch.reason == dustroute_physical::RepairReason::ConnectMissingWire
                && proposal
                    .patch
                    .changes
                    .iter()
                    .any(|change| change.pos == missing)
        }));
    }

    #[test]
    fn scan_edge_is_preserved_as_boundary_limited_evidence() {
        let forward = Translator
            .forward(&half_adder(), ForwardOptions::default())
            .unwrap();
        let (min, max) = forward.compiled.world.bounds().unwrap();
        let analysis = analyze_world_region(&forward.compiled.world, RegionBounds::new(min, max));
        let gates = dustroute_ir::recognize_gates(&analysis.scene);
        assert!(!analysis.scene.observation.is_complete());
        assert!(
            gates
                .gates
                .iter()
                .any(|gate| { gate.status == dustroute_ir::RecognitionStatus::BoundaryLimited })
        );
    }
}
