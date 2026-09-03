//! Circuit intermediate representations and explicit abstraction conversions.

mod derived;
mod events;
pub mod expr;
mod hierarchy;
pub mod logic;
mod mixed;
#[path = "physical_projection.rs"]
mod temporal_analysis;
mod transient;
mod transitions;

pub use derived::{
    DerivedExpr, DerivedExpression, ExpressionId, ExpressionView, FunctionalCandidate,
    FunctionalKind, FunctionalView, GateEvidence, GateId, GateView, RecognitionStatus,
    RecognizedGate, RecognizedGateKind, classify_function, derive_expressions, recognize_gates,
};
pub use events::{EventCause, EventKind, EventSource, TraceStatus};

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
pub use mixed::{
    BoundaryDirection, MixedEdge, MixedIr, MixedNode, MixedNodeId, MixedNodeKind, build_mixed_ir,
};
pub use temporal_analysis::{
    BehaviorEvent, BehaviorIr, BehaviorPattern, BehaviorTrace, DelayRange, EdgeBehavior,
    SteadyStateEdge, SteadyStateProjection, TemporalAnalysis, TemporalDevice, TemporalEvidence,
    TemporalNode, TemporalNodeKind, TemporalScope, TemporalSemantics, TimedCircuit, TimedEdge,
    TimingAssessment, TimingReason, TraceTimeUnit, TransitionDelay,
};
pub use transient::{
    PulseObservation, PulsePolarity, SignalIntent, TransientAssessment, TransientFinding,
    TransientVerdict, assess_transients, observe_pulses,
};
pub use transitions::{
    LogicalElapsed, TransitionElapsed, TransitionId, TransitionPhase, TransitionRecord,
    TransitionTime, TransitionTrace,
};
