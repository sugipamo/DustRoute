//! Stable shared data models used across DustRoute crates.

pub mod expr;
pub mod logic;
pub mod world;

pub use expr::{Expr, best_by_size, rewrites_once, search_equivalents};
pub use logic::{DagBuilder, GateKind, LogicDag, LogicError, LogicNode, NodeId};
pub use world::{Block, BlockKind, BlockProperties, Facing, Pos, WireConnection, World};
