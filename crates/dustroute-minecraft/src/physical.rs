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
//! Adhesion does not turn a glass descriptor into a slime material:
//! ```compile_fail,E0080
//! use dustroute_minecraft::{BlockKind, physical::*};
//! const BAD: CheckedPhysical = PhysicalSpec {
//!     adhesion: Adhesion::Slime,
//!     ..of_kind(BlockKind::Transparent).spec()
//! }.checked();
//! ```
//! Support loss requires an attachment with a compatible notification trigger:
//! ```compile_fail,E0080
//! use dustroute_minecraft::physical::*;
//! const BAD: CheckedPhysical = PhysicalSpec {
//!     support: Support::None,
//!     ..of_kind(dustroute_minecraft::BlockKind::RedstoneWire).spec()
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

pub mod environment;
pub mod lifetime;
pub mod native_state;
pub mod passive;
pub mod plants;
pub mod stairs;

pub const REVISION: &str = "dustroute.physical-admission.java-1-21-11.v10";

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum Orientation {
    None,
    Output,
    FloorOutput,
    Attached,
    StandingOrWall,
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
pub enum SupportTrigger {
    Shape,
    Neighbor,
    ShapeAndNeighbor,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum SupportFace {
    Full,
    Rigid,
    /// Standing torches use CENTER; wall torches use FULL.
    StandingCenter,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct SupportLoss {
    pub trigger: SupportTrigger,
    pub face: SupportFace,
    /// AbstractRedstoneGateBlock notifies around every neighbor after removal.
    pub notify_around_neighbors: bool,
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
    TopHalf,
    BottomHalf,
    Stairs,
    Honey,
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
    Up,
    Down,
    Stairs,
    Honey,
}

/// Material relation, independent of support, conduction and movability.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum Adhesion {
    None,
    Slime,
    Honey,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum PistonReaction {
    Normal,
    Destroy,
}
impl Adhesion {
    pub const fn sticks_to(self, other: Self) -> bool {
        !matches!(
            (self, other),
            (Self::Slime, Self::Honey) | (Self::Honey, Self::Slime)
        ) && (!matches!(self, Self::None) || !matches!(other, Self::None))
    }
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct PhysicalSpec {
    pub shape: Shape,
    pub conducting: Faces,
    pub supporting: Faces,
    pub support: Support,
    pub support_loss: Option<SupportLoss>,
    pub orientation: Orientation,
    pub wire_connection: WireConnectionRule,
    pub adhesion: Adhesion,
    pub piston_reaction: PistonReaction,
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
            match self.adhesion {
                Adhesion::None => true,
                Adhesion::Slime =>
                    matches!(self.shape, Shape::FullCube)
                        && matches!(self.supporting, Faces::All)
                        && matches!(self.conducting, Faces::All),
                Adhesion::Honey =>
                    matches!(self.shape, Shape::Honey)
                        && matches!(self.supporting, Faces::Honey)
                        && matches!(self.conducting, Faces::None),
            } && (matches!(self.adhesion, Adhesion::None) || matches!(self.support, Support::None)),
            "adhesive materials require their native geometry and no attachment"
        );
        check!(
            matches!(self.shape, Shape::Honey) == matches!(self.supporting, Faces::Honey),
            "honey support requires honey geometry"
        );
        check!(
            self.support_loss.is_none() || !matches!(self.support, Support::None),
            "support-loss behavior requires an attachment"
        );
        if let Some(rule) = self.support_loss {
            check!(
                !rule.notify_around_neighbors || !matches!(rule.trigger, SupportTrigger::Shape),
                "post-neighbor removal notifications require a neighbor trigger"
            );
            check!(
                !matches!(rule.face, SupportFace::StandingCenter)
                    || matches!(self.orientation, Orientation::StandingOrWall),
                "standing/wall support requires a standing/wall orientation"
            );
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
            !matches!(self.supporting, Faces::Up) || matches!(self.shape, Shape::TopHalf),
            "upper support face requires top-half geometry"
        );
        check!(
            !matches!(self.supporting, Faces::Down) || matches!(self.shape, Shape::BottomHalf),
            "lower support face requires bottom-half geometry"
        );
        check!(
            !matches!(self.supporting, Faces::Stairs) || matches!(self.shape, Shape::Stairs),
            "stair faces require stair geometry"
        );
        check!(
            !matches!(self.orientation, Orientation::FloorOutput)
                || matches!(self.support, Support::Below),
            "floor output requires support below"
        );
        check!(
            !matches!(
                self.orientation,
                Orientation::Attached | Orientation::StandingOrWall
            ) || matches!(self.support, Support::Attached),
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
        // The legacy aggregate has no direction field; stairs require the
        // directional wire_rise_connection query instead.
        let rise = (top && self.0.shape != Shape::Stairs).then_some(
            if matches!(self.0.supporting, Faces::Up) {
                crate::WireConnection::Side
            } else {
                crate::WireConnection::Up
            },
        );
        crate::BlockRedstoneTraits {
            occupied_shape: match self.0.shape {
                Shape::Empty => crate::OccupiedShape::Empty,
                Shape::FullCube => crate::OccupiedShape::FullCube,
                Shape::TopHalf => crate::OccupiedShape::TopHalf,
                Shape::BottomHalf => crate::OccupiedShape::BottomHalf,
                _ => crate::OccupiedShape::Partial,
            },
            supports_dust_on_top: top,
            conducts_weak_power: conductor,
            conducts_strong_power: conductor,
            strong_power_drives_dust: conductor,
            permits_wire_rise_beside: top,
            wire_rise_connection: rise,
            blocks_wire_rise_when_above: conductor,
        }
    }
    pub const fn same(self, other: Self) -> bool {
        self.0.shape as u8 == other.0.shape as u8
            && self.0.conducting as u8 == other.0.conducting as u8
            && self.0.supporting as u8 == other.0.supporting as u8
            && self.0.support as u8 == other.0.support as u8
            && match (self.0.support_loss, other.0.support_loss) {
                (None, None) => true,
                (Some(a), Some(b)) => {
                    a.trigger as u8 == b.trigger as u8
                        && a.face as u8 == b.face as u8
                        && a.notify_around_neighbors == b.notify_around_neighbors
                }
                _ => false,
            }
            && self.0.orientation as u8 == other.0.orientation as u8
            && self.0.wire_connection as u8 == other.0.wire_connection as u8
            && self.0.adhesion as u8 == other.0.adhesion as u8
            && self.0.piston_reaction as u8 == other.0.piston_reaction as u8
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
    pub const fn support_loss(self) -> Option<SupportLoss> {
        self.0.support_loss
    }
    pub fn supports_attachment(self, support: &Block, toward_support: Facing) -> bool {
        let face = toward_support.opposite();
        let Some(physical) = of_block(support) else {
            return false;
        };
        if self
            .0
            .support_loss
            .is_some_and(|rule| rule.face == SupportFace::StandingCenter)
            && toward_support == Facing::Down
        {
            return physical.center_face(support, face);
        }
        // FULL and RIGID coincide for the admitted cube/slab/stair/body/head shapes.
        physical.full_face(support, face)
    }
    pub fn center_face(self, block: &Block, side: Facing) -> bool {
        if of_block(block) != Some(self) {
            return false;
        }
        match self.0.supporting {
            Faces::All => true,
            Faces::None => false,
            Faces::Honey => side == Facing::Down,
            Faces::Up => side == Facing::Up,
            Faces::Down => side == Facing::Down,
            Faces::Stairs => stairs::StairState::parse(block).is_some_and(|s| s.full_face(side)),
            Faces::PistonBody => {
                block.piston_state == Some(PistonState::Retracted)
                    || block.facing.is_some_and(|f| f != side)
            }
            Faces::PistonHead => block
                .facing
                .is_some_and(|f| side == f || side == f.opposite()),
        }
    }
    pub fn conducts(self, block: &Block) -> bool {
        matches!(self.0.conducting, Faces::All) && of_block(block) == Some(self)
    }
    pub fn full_face(self, block: &Block, side: Facing) -> bool {
        of_block(block) == Some(self)
            && match self.0.supporting {
                Faces::None => false,
                Faces::Honey => false,
                Faces::All => true,
                Faces::Up => side == Facing::Up,
                Faces::Down => side == Facing::Down,
                Faces::Stairs => {
                    stairs::StairState::parse(block).is_some_and(|s| s.full_face(side))
                }
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

pub(crate) fn known_geometry(block: &Block) -> bool {
    of_block(block).is_some()
}

/// Direction from the lower wire to its adjacent support. The top and the
/// vertical face are separate predicates in native dust rendering.
pub fn wire_rise_connection(
    block: &Block,
    toward_support: Facing,
) -> Option<crate::WireConnection> {
    let p = of_block(block)?;
    if toward_support.horizontal_offset().is_none() || !p.full_face(block, Facing::Up) {
        return None;
    }
    Some(if p.full_face(block, toward_support.opposite()) {
        crate::WireConnection::Up
    } else {
        crate::WireConnection::Side
    })
}

/// Resolve current physical facts from identity and state. Kind-only defaults
/// remain available for synthetic devices and historical finite Law adapters.
pub fn of_block(block: &Block) -> Option<CheckedPhysical> {
    if block.observation_classification == ObservationClassification::Coarse
        || (block.requires_live_observation() && !environment::is_water(block))
    {
        return None;
    }
    if let Some(spec) = environment::of_block(block) {
        return Some(spec.physical);
    }
    if let Some(spec) = plants::of_block(block) {
        return Some(spec.physical);
    }
    if matches!(block.kind, BlockKind::Solid | BlockKind::Transparent) {
        return passive::resolve(block);
    }
    Some(of_kind(block.kind))
}

const EMPTY: PhysicalSpec = PhysicalSpec {
    shape: Shape::Empty,
    conducting: Faces::None,
    supporting: Faces::None,
    support: Support::None,
    support_loss: None,
    orientation: Orientation::None,
    wire_connection: WireConnectionRule::None,
    adhesion: Adhesion::None,
    piston_reaction: PistonReaction::Normal,
};
const CUBE: PhysicalSpec = PhysicalSpec {
    shape: Shape::FullCube,
    supporting: Faces::All,
    ..EMPTY
};
const COMPONENT: PhysicalSpec = PhysicalSpec {
    shape: Shape::Partial,
    piston_reaction: PistonReaction::Destroy,
    ..EMPTY
};
const GATE: PhysicalSpec = PhysicalSpec {
    support: Support::Below,
    support_loss: Some(SupportLoss {
        trigger: SupportTrigger::ShapeAndNeighbor,
        face: SupportFace::Rigid,
        notify_around_neighbors: true,
    }),
    orientation: Orientation::FloorOutput,
    wire_connection: WireConnectionRule::Axis,
    ..COMPONENT
};
const ATTACHED: PhysicalSpec = PhysicalSpec {
    support: Support::Attached,
    support_loss: Some(SupportLoss {
        trigger: SupportTrigger::Shape,
        face: SupportFace::Full,
        notify_around_neighbors: false,
    }),
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
    piston_reaction: PistonReaction::Destroy,
    support: Support::Below,
    support_loss: Some(SupportLoss {
        trigger: SupportTrigger::ShapeAndNeighbor,
        face: SupportFace::Full,
        notify_around_neighbors: false,
    }),
    wire_connection: WireConnectionRule::Any,
    ..COMPONENT
}
.checked();
const TORCH: CheckedPhysical = PhysicalSpec {
    orientation: Orientation::StandingOrWall,
    support_loss: Some(SupportLoss {
        trigger: SupportTrigger::Shape,
        face: SupportFace::StandingCenter,
        notify_around_neighbors: false,
    }),
    ..ATTACHED
}
.checked();
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
    if let Some(spec) = environment::named(name) {
        return (spec.kind, ObservationClassification::Exact);
    }
    if let Some(spec) = plants::named(name) {
        return (spec.kind, ObservationClassification::Exact);
    }
    if let Some(spec) = passive::named(name) {
        return (spec.kind, ObservationClassification::Exact);
    }
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
        n if n.ends_with("_slab") || n.ends_with("_stairs") => Transparent,
        "dirt" | "grass_block" | "bedrock" | "deepslate" => Solid,
        _ => return (Solid, ObservationClassification::Coarse),
    };
    (kind, ObservationClassification::Exact)
}

/// Routing hint only: these identities need the world callback adapter rather
/// than the ordinary fixed circuit placement validator. Not admission proof.
pub fn requires_callback_runtime_name(name: &str) -> bool {
    matches!(
        classify(name).0,
        BlockKind::Piston | BlockKind::PistonHead | BlockKind::MovingPiston
    ) || plants::named(name).is_some()
        || environment::named(name)
            .is_some_and(|s| s.role == environment::EnvironmentRole::EnclosedWaterSource)
}
pub fn requires_callback_runtime(block: &Block) -> bool {
    matches!(
        block.kind,
        BlockKind::Piston | BlockKind::PistonHead | BlockKind::MovingPiston
    ) || block
        .observed_name
        .as_deref()
        .is_some_and(requires_callback_runtime_name)
}
