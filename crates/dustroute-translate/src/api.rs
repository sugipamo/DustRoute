use std::error::Error;
use std::fmt::{Display, Formatter};

use crate::{
    BaselineCompileConfig, BaselineCompileResult, BaselineCompiler, CompileError, Expr,
    InferredTruthTable, LogicDag, RegionAnalysis, RegionBounds, TruthTableComparison,
    TruthTableError, World, analyze_world_region, compare_truth_tables, infer_output_expressions,
    infer_truth_table,
};
use dustroute_ir::PhysicalProjection;

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
    pub projection: PhysicalProjection,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ReverseRequest {
    pub bounds: RegionBounds,
    pub max_inputs: usize,
    pub settle_ticks: usize,
}

impl ReverseRequest {
    #[must_use]
    pub const fn new(bounds: RegionBounds) -> Self {
        Self {
            bounds,
            max_inputs: 16,
            settle_ticks: 60,
        }
    }
}

#[derive(Clone, Debug)]
pub struct ReverseResult {
    pub analysis: RegionAnalysis,
    pub projection: PhysicalProjection,
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
        let projection = compiled
            .world
            .bounds()
            .map(|(min, max)| analyze_world_region(&compiled.world, RegionBounds::new(min, max)))
            .map(|analysis| PhysicalProjection::from_physical(&analysis.physical))
            .unwrap_or_else(|| PhysicalProjection::from_physical(&Default::default()));
        Ok(ForwardResult {
            compiled,
            projection,
        })
    }

    #[must_use]
    pub fn reverse(&self, world: &World, request: ReverseRequest) -> ReverseResult {
        let analysis = analyze_world_region(world, request.bounds);
        let mut projection = PhysicalProjection::from_physical(&analysis.physical);
        if !projection.behavior.devices.is_empty()
            && let Ok(trace) = crate::simulate_behavior_trace(
                world,
                &analysis.physical,
                &projection,
                request.settle_ticks,
                "observed initial state",
            )
        {
            projection.behavior.traces.push(trace);
        }
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
                ReverseResult {
                    analysis,
                    projection,
                    truth_table: Some(truth_table),
                    expressions,
                    logic,
                    truth_table_error: None,
                }
            }
            Err(error) => ReverseResult {
                analysis,
                projection,
                truth_table: None,
                expressions: Vec::new(),
                logic: None,
                truth_table_error: Some(error),
            },
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
