//! Physical facts for the current Java callback adapter. These declarations do
//! not authorize an executor, placement, a piston payload, or an observation.
//! Historical finite spatial laws retain their own immutable kind ABI.
//!
//! Inconsistent geometry is rejected when a Rust constant is compiled:
//! ```compile_fail,E0080
//! use dustroute_minecraft::physical::*;
//! const BAD: CheckedPhysical = PhysicalSpec {
//!     shape: Shape::Empty, conducting: Faces::All,
//!     ..of_kind(dustroute_minecraft::BlockKind::Solid).spec()
//! }.checked();
//! ```
//! Directional dust attachment requires an output axis:
//! ```compile_fail,E0080
//! use dustroute_minecraft::physical::*;
//! const BAD: CheckedPhysical = PhysicalSpec {
//!     wire_connection: WireConnectionRule::Axis,
//!     ..of_kind(dustroute_minecraft::BlockKind::RedstoneLamp).spec()
//! }.checked();
//! ```
use serde::Deserialize;

use crate::{Block, BlockKind, Facing, ObservationClassification, PistonState};

pub const REVISION: &str = "dustroute.physical-admission.java-1-21-11.v3";

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum Orientation {
    None,
    Output,
    FloorOutput,
    Attached,
}
impl Orientation {
    pub const fn has_output(self) -> bool {
        matches!(self, Self::Output | Self::FloorOutput)
    }
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum WireConnectionRule {
    None,
    Output,
    Axis,
    Any,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum Support {
    None,
    Below,
    Attached,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum Shape {
    Empty,
    FullCube,
    Partial,
    PistonBody,
    PistonHead,
    Moving,
}

/// Face rules describe support and conduction independently. For example an
/// observer supports blocks on every face but is not a redstone conductor.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum Faces {
    None,
    All,
    PistonBody,
    PistonHead,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct PhysicalSpec {
    pub shape: Shape,
    pub conducting: Faces,
    pub supporting: Faces,
    pub support: Support,
    pub orientation: Orientation,
    pub wire_connection: WireConnectionRule,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq)]
#[serde(try_from = "PhysicalSpec")]
pub struct CheckedPhysical(PhysicalSpec);

impl PhysicalSpec {
    const fn validation_error(self) -> Option<&'static str> {
        macro_rules! check {
            ($condition:expr, $message:literal) => {
                if !$condition {
                    return Some($message);
                }
            };
        }
        check!(
            matches!(self.conducting, Faces::None | Faces::All),
            "state-dependent conduction is not implemented"
        );
        check!(
            !matches!(self.conducting, Faces::All) || matches!(self.shape, Shape::FullCube),
            "conduction requires a full cube"
        );
        check!(
            !matches!(self.supporting, Faces::All) || matches!(self.shape, Shape::FullCube),
            "full support requires a full cube"
        );
        check!(
            !matches!(self.supporting, Faces::PistonBody)
                || matches!(self.shape, Shape::PistonBody),
            "piston body faces require piston geometry"
        );
        check!(
            !matches!(self.supporting, Faces::PistonHead)
                || matches!(self.shape, Shape::PistonHead),
            "piston head faces require head geometry"
        );
        check!(
            !matches!(self.orientation, Orientation::FloorOutput)
                || matches!(self.support, Support::Below),
            "floor output requires support below"
        );
        check!(
            !matches!(self.orientation, Orientation::Attached)
                || matches!(self.support, Support::Attached),
            "attached orientation requires adjacent support"
        );
        check!(
            !matches!(
                self.wire_connection,
                WireConnectionRule::Axis | WireConnectionRule::Output
            ) || self.orientation.has_output(),
            "directional dust connection requires an output axis"
        );
        None
    }
    pub const fn checked(self) -> CheckedPhysical {
        if let Some(error) = self.validation_error() {
            panic!("{}", error);
        }
        CheckedPhysical(self)
    }
}

impl TryFrom<PhysicalSpec> for CheckedPhysical {
    type Error = &'static str;
    fn try_from(spec: PhysicalSpec) -> Result<Self, Self::Error> {
        match spec.validation_error() {
            Some(error) => Err(error),
            None => Ok(Self(spec)),
        }
    }
}

impl CheckedPhysical {
    /// Current descriptors project new kinds into ordinary placement/routing
    /// traits. Old execution laws continue to use their own finite projection.
    pub fn properties(self) -> crate::BlockProperties {
        let conductor = self.0.conducting == Faces::All;
        crate::BlockProperties {
            supports_components: self.0.supporting == Faces::All,
            receives_weak_power: conductor,
            receives_strong_power: conductor,
            repeater_reads_block_power: conductor,
            strong_power_drives_dust: conductor,
        }
    }
    pub fn block_traits(self, block: &Block) -> crate::BlockRedstoneTraits {
        let top = self.full_face(block, Facing::Up);
        let conductor = self.conducts(block);
        let rise = top.then_some(crate::WireConnection::Up);
        crate::BlockRedstoneTraits {
            occupied_shape: match self.0.shape {
                Shape::Empty => crate::OccupiedShape::Empty,
                Shape::FullCube => crate::OccupiedShape::FullCube,
                _ => crate::OccupiedShape::Partial,
            },
            supports_dust_on_top: top,
            conducts_weak_power: conductor,
            conducts_strong_power: conductor,
            strong_power_drives_dust: conductor,
            permits_wire_rise_beside: rise.is_some(),
            wire_rise_connection: rise,
            blocks_wire_rise_when_above: conductor,
        }
    }
    pub const fn same(self, other: Self) -> bool {
        self.0.shape as u8 == other.0.shape as u8
            && self.0.conducting as u8 == other.0.conducting as u8
            && self.0.supporting as u8 == other.0.supporting as u8
            && self.0.support as u8 == other.0.support as u8
            && self.0.orientation as u8 == other.0.orientation as u8
            && self.0.wire_connection as u8 == other.0.wire_connection as u8
    }
    pub const fn spec(self) -> PhysicalSpec {
        self.0
    }
    pub const fn orientation(self) -> Orientation {
        self.0.orientation
    }
    pub const fn support(self) -> Support {
        self.0.support
    }
    pub fn conducts(self, block: &Block) -> bool {
        matches!(self.0.conducting, Faces::All) && known_geometry(block)
    }
    pub fn full_face(self, block: &Block, side: Facing) -> bool {
        known_geometry(block)
            && match self.0.supporting {
                Faces::None => false,
                Faces::All => true,
                Faces::PistonBody => {
                    block.piston_state == Some(PistonState::Retracted)
                        || block.facing == Some(side.opposite())
                }
                Faces::PistonHead => block.facing == Some(side),
            }
    }
    pub fn wire_connects(self, block: &Block, side: Facing) -> bool {
        match self.0.wire_connection {
            WireConnectionRule::None => false,
            WireConnectionRule::Output => block.facing == Some(side.opposite()),
            WireConnectionRule::Axis => block
                .facing
                .is_some_and(|d| d == side || d == side.opposite()),
            WireConnectionRule::Any => true,
        }
    }
}

fn known_geometry(block: &Block) -> bool {
    block.observation_classification != ObservationClassification::Coarse
        && !block.requires_live_observation()
        && !block
            .observed_name
            .as_deref()
            .is_some_and(|n| n.ends_with("_slab") || n.ends_with("_stairs"))
}

const EMPTY: PhysicalSpec = PhysicalSpec {
    shape: Shape::Empty,
    conducting: Faces::None,
    supporting: Faces::None,
    support: Support::None,
    orientation: Orientation::None,
    wire_connection: WireConnectionRule::None,
};
const CUBE: PhysicalSpec = PhysicalSpec {
    shape: Shape::FullCube,
    supporting: Faces::All,
    ..EMPTY
};
const COMPONENT: PhysicalSpec = PhysicalSpec {
    shape: Shape::Partial,
    ..EMPTY
};
const GATE: PhysicalSpec = PhysicalSpec {
    support: Support::Below,
    orientation: Orientation::FloorOutput,
    wire_connection: WireConnectionRule::Axis,
    ..COMPONENT
};
const ATTACHED: PhysicalSpec = PhysicalSpec {
    support: Support::Attached,
    orientation: Orientation::Attached,
    wire_connection: WireConnectionRule::Any,
    ..COMPONENT
};

// Each initializer is checked at compile time; of_kind is exhaustive so a new
// kind requires a physical declaration even when no executor supports it yet.
const AIR: CheckedPhysical = EMPTY.checked();
const SOLID: CheckedPhysical = PhysicalSpec {
    conducting: Faces::All,
    ..CUBE
}
.checked();
const GLASS: CheckedPhysical = CUBE.checked();
const WIRE: CheckedPhysical = PhysicalSpec {
    support: Support::Below,
    wire_connection: WireConnectionRule::Any,
    ..COMPONENT
}
.checked();
const TORCH: CheckedPhysical = ATTACHED.checked();
const DIODE: CheckedPhysical = GATE.checked();
const COMPARATOR: CheckedPhysical = PhysicalSpec {
    wire_connection: WireConnectionRule::Any,
    ..GATE
}
.checked();
const INPUT: CheckedPhysical = ATTACHED.checked();
const PLATE: CheckedPhysical = PhysicalSpec {
    support: Support::Below,
    wire_connection: WireConnectionRule::Any,
    ..COMPONENT
}
.checked();
const POWER: CheckedPhysical = PhysicalSpec {
    wire_connection: WireConnectionRule::Any,
    ..CUBE
}
.checked();
const OBSERVER: CheckedPhysical = PhysicalSpec {
    orientation: Orientation::Output,
    wire_connection: WireConnectionRule::Output,
    ..CUBE
}
.checked();
const PISTON: CheckedPhysical = PhysicalSpec {
    shape: Shape::PistonBody,
    supporting: Faces::PistonBody,
    ..EMPTY
}
.checked();
const HEAD: CheckedPhysical = PhysicalSpec {
    shape: Shape::PistonHead,
    supporting: Faces::PistonHead,
    ..EMPTY
}
.checked();
const MOVING: CheckedPhysical = PhysicalSpec {
    shape: Shape::Moving,
    ..EMPTY
}
.checked();

pub const fn of_kind(kind: BlockKind) -> CheckedPhysical {
    match kind {
        BlockKind::Air => AIR,
        BlockKind::Solid | BlockKind::RedstoneLamp | BlockKind::CopperBulb => SOLID,
        BlockKind::Transparent => GLASS,
        BlockKind::RedstoneWire => WIRE,
        BlockKind::RedstoneTorch => TORCH,
        BlockKind::Repeater => DIODE,
        BlockKind::Comparator => COMPARATOR,
        BlockKind::Lever | BlockKind::Button => INPUT,
        BlockKind::PressurePlate => PLATE,
        BlockKind::RedstoneBlock => POWER,
        BlockKind::Observer => OBSERVER,
        BlockKind::Piston => PISTON,
        BlockKind::PistonHead => HEAD,
        BlockKind::MovingPiston => MOVING,
    }
}

/// Identity classification is intentionally weaker than executor admission.
/// Families (such as wood buttons) can be observed without being simulated.
pub fn classify(name: &str) -> (BlockKind, ObservationClassification) {
    use BlockKind::*;
    let name = name.strip_prefix("minecraft:").unwrap_or(name);
    for device in crate::device_program::BUILTIN_DEVICES {
        let spec = device.spec();
        if spec.observed_names.contains(&name) {
            return (spec.kind, ObservationClassification::Exact);
        }
    }
    let kind = match name {
        "air" | "cave_air" | "void_air" => Air,
        "redstone_wire" => RedstoneWire,
        "redstone_torch" | "redstone_wall_torch" => RedstoneTorch,
        "comparator" => Comparator,
        "lever" => Lever,
        n if n.ends_with("_button") => Button,
        n if n.ends_with("_pressure_plate") => PressurePlate,
        "redstone_block" => RedstoneBlock,
        "piston" | "sticky_piston" => Piston,
        "piston_head" => PistonHead,
        "moving_piston" => MovingPiston,
        "glass" | "tinted_glass" => Transparent,
        n if n.ends_with("_slab") || n.ends_with("_stairs") => Transparent,
        "stone" | "dirt" | "grass_block" | "bedrock" | "cobblestone" | "deepslate"
        | "smooth_quartz" | "cyan_wool" => Solid,
        _ => return (Solid, ObservationClassification::Coarse),
    };
    (kind, ObservationClassification::Exact)
}
