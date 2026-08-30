//! Canonical physical circuit and Minecraft world representation.

mod circuit;
mod patch;
mod scene;
mod temporal;

pub use circuit::{
    ComponentId, ConnectionKind, FragmentId, GapCandidate, GapEvidence, NetId, PhysicalComponent,
    PhysicalConnection, PhysicalFragment, PhysicalNet, VerifiedTopology,
};
pub use dustroute_minecraft::{
    Block, BlockBehaviorProfile, BlockKind, BlockProperties, Facing, Pos, UpdateModel,
    WireConnection, World, behavior_profile,
};
pub use patch::{
    PatchApplyError, PhysicalBlockChange, PhysicalPatch, PhysicalPatchReason, RepairImpact,
    RepairProposal, RepairReason,
};
pub use scene::{
    Confidence, FrontierReason, Observation, ObservationFrontier, ObservedRegion,
    PhysicalDiagnostic, PhysicalEvidence, PhysicalPort, PhysicalScene, PortChannel, PortConnection,
    PortId, PortRef, PortRole, RegionCompleteness, SceneBounds, SceneComponent, SupportRelation,
    TransferKind,
};
pub use temporal::{TemporalAssessment, TemporalReason, TemporalRequirement};
