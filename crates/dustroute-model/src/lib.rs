//! Compatibility facade for the split physical and IR crates.

pub mod expr {
    pub use dustroute_ir::expr::*;
}

pub mod logic {
    pub use dustroute_ir::logic::*;
}

pub mod world {
    pub use dustroute_physical::*;
}

pub use dustroute_ir::{
    DagBuilder, Expr, GateKind, LogicDag, LogicError, LogicNode, NodeId, best_by_size,
    rewrites_once, search_equivalents,
};
pub use dustroute_physical::{
    Block, BlockKind, BlockProperties, Facing, Pos, WireConnection, World,
};
