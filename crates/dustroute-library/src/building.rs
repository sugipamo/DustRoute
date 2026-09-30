//! Bounded building intent. These requests contain no execution permission.
use crate::blueprint::{AssemblyRevisionId, InstancePath, TypeRevisionId};
use dustroute_minecraft::{Region, RotationY};
use serde::{Deserialize, Serialize};

/// Compose an existing typed door with an enclosure. Generation grants no
/// adoption or execution permission. The selected source remains immutable.
#[derive(Clone, Debug, Deserialize, Serialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct BuildingWithDoorRequest {
    pub building: BuildingRequest,
    pub door: BuildingDoorAttachment,
}

#[derive(Clone, Debug, Deserialize, Serialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct BuildingDoorAttachment {
    pub assembly_revision_id: AssemblyRevisionId,
    /// Occurrence declaring the selected PistonDoor behavioral binding.
    pub instance: InstancePath,
    pub behavior_type: TypeRevisionId,
    /// Orient the source aperture parallel to the building's north wall.
    pub rotation: RotationY,
    /// Motion space in the original Assembly frame, not building coordinates.
    /// All mechanism cells and terminals must fit. The first composition path
    /// supports a one-block-deep mechanism inside the north-wall frame.
    pub reserved_space: Region,
}

#[derive(Clone, Debug, Deserialize, Serialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct BuildingRequest {
    /// Immutable definition prefix. Use a new namespace for a different design.
    pub namespace: String,
    /// Outside dimensions, including the floor and optional roof.
    pub width: u16,
    pub depth: u16,
    pub height: u16,
    #[serde(default)]
    pub floor_material: BuildingMaterial,
    #[serde(default)]
    pub wall_material: BuildingMaterial,
    #[serde(default)]
    pub roof_material: BuildingMaterial,
    #[serde(default)]
    pub roof: BuildingRoof,
    /// North wall opening. Omitted means a centered 1-wide, 2-high passage.
    pub entrance: Option<BuildingEntrance>,
}

#[derive(Clone, Copy, Debug, Default, Deserialize, Serialize, schemars::JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum BuildingRoof {
    #[default]
    Flat,
    None,
}

#[derive(Clone, Copy, Debug, Default, Deserialize, Serialize, schemars::JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum BuildingMaterial {
    #[default]
    Stone,
    Cobblestone,
    SmoothStone,
    SmoothQuartz,
    Glass,
    TintedGlass,
}
impl BuildingMaterial {
    pub const fn native_name(self) -> &'static str {
        match self {
            Self::Stone => "minecraft:stone",
            Self::Cobblestone => "minecraft:cobblestone",
            Self::SmoothStone => "minecraft:smooth_stone",
            Self::SmoothQuartz => "minecraft:smooth_quartz",
            Self::Glass => "minecraft:glass",
            Self::TintedGlass => "minecraft:tinted_glass",
        }
    }
}

#[derive(Clone, Copy, Debug, Deserialize, Serialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct BuildingEntrance {
    /// X offset on z=0, strictly between the two outside corners.
    pub offset: u16,
    pub width: u16,
    /// Opening begins above the floor at y=1.
    pub height: u16,
}
