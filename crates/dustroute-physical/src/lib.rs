//! Canonical physical circuit and Minecraft world representation.

mod circuit;
mod patch;
mod scene;
mod world;

pub use circuit::{
    ComponentId, ConnectionKind, FragmentId, GapCandidate, GapEvidence, NetId, PhysicalComponent,
    PhysicalConnection, PhysicalFragment, PhysicalNet, VerifiedTopology,
};
pub use patch::{
    PatchApplyError, PhysicalBlockChange, PhysicalPatch, RepairImpact, RepairProposal, RepairReason,
};
pub use scene::{
    Confidence, FrontierReason, Observation, ObservationFrontier, ObservedRegion,
    PhysicalDiagnostic, PhysicalEvidence, PhysicalPort, PhysicalScene, PortChannel, PortConnection,
    PortId, PortRef, PortRole, RegionCompleteness, SceneBounds, SceneComponent, SupportRelation,
    TransferKind,
};
pub use world::{Block, BlockKind, BlockProperties, Facing, Pos, WireConnection, World};
