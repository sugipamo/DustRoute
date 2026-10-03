//! Versioned, finite spatial laws shared by placement, routing and execution.
//! Numeric tags below are an adapter ABI, not inferred classification meaning.
//! The pinned programs are also published as immutable Blueprint Revisions by
//! the library layer; this layer does not depend on the catalog or own a world.
use std::sync::OnceLock;

use crate::law::{LawProgram, finite::FiniteLaw};
use crate::{
    Block, BlockKind, BlockProperties, BlockRedstoneTraits, Facing, OccupiedShape, Pos,
    WireConnection, World,
};

pub const BLOCK_TRAITS_LAW: &str = "dustroute.law.spatial-block-traits.v1";
pub const WIRE_SHAPE_LAW: &str = "dustroute.law.wire-shape.v1";
pub const WIRE_TRANSFER_LAW: &str = "dustroute.law.wire-transfer.v1";
pub const WIRE_WEAK_POWER_LAW: &str = "dustroute.law.wire-weak-power.v1";
pub const SPATIAL_LAW_IDS: [&str; 4] = [
    BLOCK_TRAITS_LAW,
    WIRE_SHAPE_LAW,
    WIRE_TRANSFER_LAW,
    WIRE_WEAK_POWER_LAW,
];

pub fn builtin_programs() -> &'static [LawProgram; 4] {
    static PROGRAMS: OnceLock<[LawProgram; 4]> = OnceLock::new();
    PROGRAMS.get_or_init(|| {
        [
            crate::law::builtins::spatial::BLOCK_TRAITS_V1,
            crate::law::builtins::spatial::WIRE_SHAPE_V1,
            crate::law::builtins::spatial::WIRE_TRANSFER_V1,
            crate::law::builtins::spatial::WIRE_WEAK_POWER_V1,
        ]
        .map(|definition| definition.program())
    })
}

pub fn builtin_spatial_laws() -> &'static SpatialLaws {
    static LAWS: OnceLock<SpatialLaws> = OnceLock::new();
    LAWS.get_or_init(|| {
        SpatialLaws::compile(builtin_programs()).expect("embedded finite spatial laws")
    })
}

#[derive(Clone, Debug)]
pub struct SpatialLaws {
    traits: FiniteLaw,
    shape: FiniteLaw,
    transfer: FiniteLaw,
    weak: FiniteLaw,
}

/// Geometric target pattern. The program chooses the pattern; the world
/// adapter supplies actual facing/support coordinates and device output state.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum StrongTargets {
    None,
    Support,
    ForwardHorizontal,
    ForwardAny,
    Above,
    Adjacent,
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum DirectPower {
    None,
    Adjacent,
    ForwardHorizontal,
    ForwardAny,
}

const TRAIT_OUTPUTS: [(&str, u16); 18] = [
    ("shape", 4),
    ("dust_support", 1),
    ("weak", 1),
    ("strong", 1),
    ("dust_drive", 1),
    ("rise_shape", 2),
    ("blocks_rise", 1),
    ("kind_support", 1),
    ("kind_weak", 1),
    ("kind_strong", 1),
    ("repeater_reads", 1),
    ("kind_dust_drive", 1),
    ("needs_support", 1),
    ("support_below", 1),
    ("wire_connect_mode", 3),
    ("strong_targets", 5),
    ("direct_mode", 3),
    ("relay_receiver", 1),
];

impl SpatialLaws {
    pub fn compile(programs: &[LawProgram; 4]) -> Result<Self, String> {
        Ok(Self {
            traits: FiniteLaw::compile(&programs[0], &[("kind", 15), ("form", 7)], &TRAIT_OUTPUTS)?,
            shape: FiniteLaw::compile(
                &programs[1],
                &[
                    ("side_connects", 1),
                    ("support_shape", 2),
                    ("upper_wire", 1),
                    ("blocked", 1),
                    ("side_air", 1),
                    ("lower_wire", 1),
                ],
                &[("shape", 2)],
            )?,
            transfer: FiniteLaw::compile(
                &programs[2],
                &[
                    ("elevation", 2),
                    ("source_arm", 2),
                    ("sink_arm", 2),
                    ("support_shape", 2),
                    ("conducts", 1),
                    ("clear", 1),
                    ("enforce_clearance", 1),
                ],
                &[("connected", 1)],
            )?,
            weak: FiniteLaw::compile(
                &programs[3],
                &[("relation", 2), ("arm", 2)],
                &[("connected", 1)],
            )?,
        })
    }

    fn row(&self, kind: BlockKind, form: u16) -> &[u16] {
        // Diagnostic geometry remains total. Unsupported kinds have no paths,
        // support or emission here; admission rejects them before execution.
        let Some(kind) = spatial_kind_v1(kind) else {
            return &[0; 18];
        };
        self.traits
            .evaluate(&[kind, form])
            .expect("bounded trait facts")
    }

    pub fn properties(&self, kind: BlockKind) -> BlockProperties {
        let r = self.row(kind, 0);
        BlockProperties {
            supports_components: r[7] != 0,
            receives_weak_power: r[8] != 0,
            receives_strong_power: r[9] != 0,
            repeater_reads_block_power: r[10] != 0,
            strong_power_drives_dust: r[11] != 0,
        }
    }

    pub fn block_traits(&self, block: &Block) -> BlockRedstoneTraits {
        let r = self.row(block.kind, observed_form(block));
        let shape = [
            OccupiedShape::Empty,
            OccupiedShape::FullCube,
            OccupiedShape::TopHalf,
            OccupiedShape::BottomHalf,
            OccupiedShape::Partial,
        ][usize::from(r[0])];
        BlockRedstoneTraits {
            occupied_shape: shape,
            supports_dust_on_top: r[1] != 0,
            conducts_weak_power: r[2] != 0,
            conducts_strong_power: r[3] != 0,
            strong_power_drives_dust: r[4] != 0,
            permits_wire_rise_beside: r[5] != 0,
            wire_rise_connection: (r[5] != 0).then(|| connection(r[5])),
            blocks_wire_rise_when_above: r[6] != 0,
        }
    }

    pub fn requires_support(&self, kind: BlockKind) -> bool {
        self.row(kind, 0)[12] != 0
    }
    pub fn requires_support_below(&self, kind: BlockKind) -> bool {
        self.row(kind, 0)[13] != 0
    }
    pub fn accepts_relayed_weak_power(&self, kind: BlockKind) -> bool {
        self.row(kind, 0)[17] != 0
    }
    pub fn strong_targets(&self, kind: BlockKind) -> StrongTargets {
        [
            StrongTargets::None,
            StrongTargets::Support,
            StrongTargets::ForwardHorizontal,
            StrongTargets::ForwardAny,
            StrongTargets::Above,
            StrongTargets::Adjacent,
        ][usize::from(self.row(kind, 0)[15])]
    }
    pub fn direct_power(&self, kind: BlockKind) -> DirectPower {
        [
            DirectPower::None,
            DirectPower::Adjacent,
            DirectPower::ForwardHorizontal,
            DirectPower::ForwardAny,
        ][usize::from(self.row(kind, 0)[16])]
    }
    /// The supported directional devices expose one forward output and an
    /// opposite input. The law determines whether the pattern is horizontal
    /// or admits all six faces; this does not implement the device's timing.
    pub fn directional_offset(&self, block: &Block, input: bool) -> Option<Pos> {
        let facing = if input {
            block.facing?.opposite()
        } else {
            block.facing?
        };
        match self.strong_targets(block.kind) {
            StrongTargets::ForwardHorizontal => facing.horizontal_offset(),
            StrongTargets::ForwardAny => Some(facing.offset()),
            _ => None,
        }
    }
    /// Resolve the law-selected geometric pattern against actual world state.
    /// A target here is a potential emission; receiver conduction and current
    /// signal level are checked separately by the world executor.
    pub fn strong_output_targets(&self, world: &World, pos: Pos) -> Option<Vec<Pos>> {
        let block = world.get(pos)?;
        Some(match self.strong_targets(block.kind) {
            StrongTargets::None => return None,
            StrongTargets::Support => block.support_pos(pos).into_iter().collect(),
            StrongTargets::ForwardHorizontal => block
                .facing
                .and_then(|f| f.horizontal_offset())
                .map(|d| pos.offset(d.x, d.y, d.z))
                .into_iter()
                .collect(),
            StrongTargets::ForwardAny => block
                .facing
                .map(|f| f.offset())
                .map(|d| pos.offset(d.x, d.y, d.z))
                .into_iter()
                .collect(),
            StrongTargets::Above => vec![pos.offset(0, 1, 0)],
            StrongTargets::Adjacent => [
                Facing::East,
                Facing::West,
                Facing::Up,
                Facing::Down,
                Facing::South,
                Facing::North,
            ]
            .into_iter()
            .map(|f| f.offset())
            .map(|d| pos.offset(d.x, d.y, d.z))
            .collect(),
        })
    }
    pub fn component_connects(&self, block: &Block, toward: Facing) -> bool {
        match self.row(block.kind, 0)[14] {
            1 => true,
            2 => block
                .facing
                .is_some_and(|f| f == toward || f == toward.opposite()),
            3 => block.facing == Some(toward.opposite()),
            _ => false,
        }
    }
    pub fn infer_shape(&self, facts: WireShapeFacts) -> WireConnection {
        connection(
            self.shape
                .evaluate(&[
                    u16::from(facts.side_connects),
                    tag(facts.support_shape.unwrap_or(WireConnection::None)),
                    u16::from(facts.upper_wire),
                    u16::from(facts.blocked),
                    u16::from(facts.side_air),
                    u16::from(facts.lower_wire),
                ])
                .expect("bounded shape facts")[0],
        )
    }
    pub fn dust_transfers(&self, facts: DustTransferFacts) -> bool {
        self.transfer
            .evaluate(&[
                facts.elevation as u16,
                tag(facts.source_arm),
                tag(facts.sink_arm),
                tag(facts.support_shape.unwrap_or(WireConnection::None)),
                u16::from(facts.conducts),
                u16::from(facts.clear),
                u16::from(facts.enforce_clearance),
            ])
            .expect("bounded transfer facts")[0]
            != 0
    }
    /// `relation`: below=0, horizontal=1, other=2. This describes a potential
    /// path, independently of the current signal level.
    pub fn weakly_powers(&self, relation: WeakTarget, arm: WireConnection) -> bool {
        self.weak
            .evaluate(&[relation as u16, tag(arm)])
            .expect("bounded weak-power facts")[0]
            != 0
    }
}

#[derive(Clone, Copy, Debug)]
pub struct WireShapeFacts {
    pub side_connects: bool,
    pub support_shape: Option<WireConnection>,
    pub upper_wire: bool,
    pub blocked: bool,
    pub side_air: bool,
    pub lower_wire: bool,
}
#[derive(Clone, Copy, Debug)]
pub enum DustElevation {
    Horizontal = 0,
    Rise = 1,
    Fall = 2,
}
#[derive(Clone, Copy, Debug)]
pub enum WeakTarget {
    Below = 0,
    Horizontal = 1,
    Other = 2,
}
#[derive(Clone, Copy, Debug)]
pub struct DustTransferFacts {
    pub elevation: DustElevation,
    pub source_arm: WireConnection,
    pub sink_arm: WireConnection,
    pub support_shape: Option<WireConnection>,
    pub conducts: bool,
    pub clear: bool,
    /// The historical translate model trusts explicit arms; the bounded event
    /// runner additionally checks rise clearance. This pin is not state data.
    pub enforce_clearance: bool,
}

fn tag(value: WireConnection) -> u16 {
    match value {
        WireConnection::None => 0,
        WireConnection::Side => 1,
        WireConnection::Up => 2,
    }
}
fn connection(value: u16) -> WireConnection {
    [
        WireConnection::None,
        WireConnection::Side,
        WireConnection::Up,
    ][usize::from(value)]
}
pub(crate) const fn spatial_kind_v1(kind: BlockKind) -> Option<u16> {
    Some(match kind {
        BlockKind::Air => 0,
        BlockKind::Solid => 1,
        BlockKind::Transparent => 2,
        BlockKind::RedstoneWire => 3,
        BlockKind::RedstoneTorch => 4,
        BlockKind::Repeater => 5,
        BlockKind::Comparator => 6,
        BlockKind::Lever => 7,
        BlockKind::Button => 8,
        BlockKind::PressurePlate => 9,
        BlockKind::RedstoneLamp => 10,
        BlockKind::RedstoneBlock => 11,
        BlockKind::Observer => 12,
        BlockKind::Piston => 13,
        BlockKind::PistonHead => 14,
        BlockKind::MovingPiston => 15,
        BlockKind::CopperBulb => return None,
    })
}
fn observed_form(block: &Block) -> u16 {
    if block.requires_live_observation() {
        return 1;
    }
    let name = block.observed_name.as_deref().unwrap_or_default();
    if name.ends_with("_slab") {
        match block.observed_properties.get("type").map(String::as_str) {
            Some("top") => 3,
            Some("double") => 4,
            _ => 2,
        }
    } else if name.ends_with("_stairs") {
        if block.observed_properties.get("half").map(String::as_str) == Some("top") {
            6
        } else {
            5
        }
    } else if matches!(name, "minecraft:glass" | "minecraft:tinted_glass") {
        7
    } else {
        0
    }
}
