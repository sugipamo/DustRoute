//! Canonical physical circuit and Minecraft world representation.

mod circuit;
mod world;

pub use circuit::{
    ComponentId, ConnectionKind, FragmentId, GapCandidate, GapEvidence, NetId, PhysicalCircuit,
    PhysicalComponent, PhysicalConnection, PhysicalFragment, PhysicalNet,
};
pub use world::{Block, BlockKind, BlockProperties, Facing, Pos, WireConnection, World};
