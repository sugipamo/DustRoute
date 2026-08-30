//! Circuit intermediate representations and explicit abstraction conversions.

mod derived;
pub mod expr;
pub mod logic;
mod physical_projection;

pub use derived::{
    DerivedExpr, DerivedExpression, ExpressionId, ExpressionView, FunctionalCandidate,
    FunctionalKind, FunctionalView, GateEvidence, GateId, GateView, RecognitionStatus,
    RecognizedGate, RecognizedGateKind, classify_function, derive_expressions, recognize_gates,
};

pub use expr::{
    Expr, ExprToLogicError, best_by_size, logic_from_expressions, rewrites_once, search_equivalents,
};
pub use logic::{DagBuilder, GateKind, LogicDag, LogicError, LogicNode, NodeId};
pub use physical_projection::{
    AbstractionLevel, BehaviorEvent, BehaviorIr, BehaviorPattern, BehaviorTrace, IrProjection,
    PhysicalProjection, ProjectionError, ProjectionEvidence, SignalEdge, SignalIr, SignalNode,
    SignalNodeKind, TemporalDevice, TemporalSemantics,
};
