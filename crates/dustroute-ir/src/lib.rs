//! Circuit intermediate representations and explicit abstraction conversions.

pub mod expr;
pub mod logic;
mod physical_projection;

pub use expr::{Expr, best_by_size, rewrites_once, search_equivalents};
pub use logic::{DagBuilder, GateKind, LogicDag, LogicError, LogicNode, NodeId};
pub use physical_projection::{
    AbstractionLevel, IrProjection, PhysicalProjection, ProjectionError,
};
