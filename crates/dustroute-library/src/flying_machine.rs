//! Authoring parameters for a bounded family of single-launch machines.
//! These are requests, never validation or permission to modify a world.
use dustroute_minecraft::{Pos, RotationY};
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Deserialize, Serialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct FlyingMachineRequest {
    /// Immutable ID prefix; use a new namespace for a different saved design.
    pub namespace: String,
    /// One-way travel before the generated stopper, between 1 and 16 blocks.
    pub distance: u16,
    /// Selects a declarative engine layout; all engines use the same runtime.
    #[serde(default)]
    pub engine: FlyingMachineEngine,
    #[serde(default)]
    pub body: FlyingMachineBody,
    /// Rotate the complete source design; placement can subsequently relocate it.
    #[serde(default)]
    pub rotation: RotationY,
    /// Reflect the source layout across z=0 before rotation.
    #[serde(default)]
    pub mirrored: bool,
    /// Additional moving blocks in the unmirrored, eastbound source frame.
    /// Detached, obstructing and overloaded additions must fail verification.
    #[serde(default)]
    pub attachments: Vec<FlyingMachineAttachment>,
}

#[derive(
    Clone, Copy, Debug, Default, Eq, PartialEq, Deserialize, Serialize, schemars::JsonSchema,
)]
#[serde(rename_all = "snake_case")]
pub enum FlyingMachineEngine {
    #[default]
    SlimeRelay,
    HoneyDirect,
}

#[derive(
    Clone, Copy, Debug, Default, Eq, PartialEq, Deserialize, Serialize, schemars::JsonSchema,
)]
#[serde(rename_all = "snake_case")]
pub enum FlyingMachineBody {
    #[default]
    Compact,
    SideBlocks,
    SlimeWings,
    HoneyNose,
}

#[derive(Clone, Debug, Deserialize, Serialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct FlyingMachineAttachment {
    pub position: Pos,
    pub material: FlyingMachineMaterial,
}

#[derive(Clone, Copy, Debug, Deserialize, Serialize, schemars::JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum FlyingMachineMaterial {
    Stone,
    Glass,
    Slime,
    Honey,
}
