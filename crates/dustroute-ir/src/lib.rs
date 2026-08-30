//! Circuit intermediate representations and explicit abstraction conversions.

mod derived;
pub mod expr;
mod hierarchy;
pub mod logic;
#[path = "physical_projection.rs"]
mod temporal_analysis;

pub use derived::{
    DerivedExpr, DerivedExpression, ExpressionId, ExpressionView, FunctionalCandidate,
    FunctionalKind, FunctionalView, GateEvidence, GateId, GateView, RecognitionStatus,
    RecognizedGate, RecognizedGateKind, classify_function, derive_expressions, recognize_gates,
};

pub use expr::{
    Expr, ExprToLogicError, best_by_size, logic_from_expressions, rewrites_once, search_equivalents,
};
pub use hierarchy::{
    CellGraph, DiagnosticSeverity, FunctionalGraph, HierarchicalIr, IrCompleteness, IrDiagnostic,
    IrStage, LogicGraph, PhysicalGraph, PhysicalSnapshot, ProvenanceMap, TransformResult,
    UnresolvedItem, build_cell_graph, build_functional_graph, build_logic_graph,
    build_physical_graph, build_physical_snapshot, derive_hierarchy, hierarchy_from_views,
};
pub use logic::{DagBuilder, GateKind, LogicDag, LogicError, LogicNode, NodeId};
pub use temporal_analysis::{
    BehaviorEvent, BehaviorIr, BehaviorPattern, BehaviorTrace, TemporalAnalysis, TemporalDevice,
    TemporalEvidence, TemporalSemantics,
};
