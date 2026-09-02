//! Canonical physical circuit and Minecraft world representation.

mod circuit;
mod patch;
mod scene;
mod temporal;

pub use circuit::{
    ComponentId, ConnectionKind, FragmentId, GapCandidate, GapEvidence, NetId, PhysicalComponent,
    PhysicalConnection, PhysicalFragment, PhysicalNet, PhysicalTraversalGroup,
    PhysicalTraversalGroupId, VerifiedTopology,
};
pub use dustroute_minecraft::{
    Block, BlockBehaviorProfile, BlockCapabilities, BlockKind, BlockProperties, CapabilityLevel,
    Facing, ObservationClassification, Pos, UpdateModel, WireConnection, World, behavior_profile,
    observed_name_requires_live_observation,
};
pub use patch::{
    PatchApplyError, PhysicalBlockChange, PhysicalPatch, PhysicalPatchReason, RepairImpact,
    RepairProposal, RepairReason,
};
pub use scene::{
    BlockCapabilityGroup, CapabilityIssue, CapabilityStage, Confidence, FrontierReason,
    Observation, ObservationFrontier, ObservedRegion, PhysicalDiagnostic, PhysicalEvidence,
    PhysicalPort, PhysicalScene, PortChannel, PortConnection, PortId, PortRef, PortRole,
    RegionCompleteness, SceneBounds, SceneCapabilityReport, SceneComponent, SupportRelation,
    TransferKind,
};
pub use temporal::{TemporalAssessment, TemporalReason, TemporalRequirement};
