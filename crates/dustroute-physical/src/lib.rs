//! Canonical physical circuit and Minecraft world representation.

mod circuit;
mod patch;
mod world;

pub use circuit::{
    ComponentId, ConnectionKind, FragmentId, GapCandidate, GapEvidence, NetId, PhysicalCircuit,
    PhysicalComponent, PhysicalConnection, PhysicalFragment, PhysicalNet,
};
pub use patch::{PhysicalBlockChange, PhysicalPatch, RepairImpact, RepairProposal, RepairReason};
pub use world::{Block, BlockKind, BlockProperties, Facing, Pos, WireConnection, World};
