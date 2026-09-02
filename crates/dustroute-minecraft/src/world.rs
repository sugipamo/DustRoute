//! Minecraft block and world primitives, independent from DustRoute IRs.

use std::collections::BTreeMap;
use std::error::Error;
use std::fmt::{Display, Formatter};

use serde::{Deserialize, Serialize};

#[derive(
    Clone, Copy, Debug, Default, Deserialize, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize,
)]
pub struct Pos {
    pub x: i32,
    pub y: i32,
    pub z: i32,
}

impl Pos {
    #[must_use]
    pub const fn new(x: i32, y: i32, z: i32) -> Self {
        Self { x, y, z }
    }

    #[must_use]
    pub const fn offset(self, dx: i32, dy: i32, dz: i32) -> Self {
        Self::new(self.x + dx, self.y + dy, self.z + dz)
    }
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, Hash, PartialEq, Serialize)]
pub enum BlockKind {
    Air,
    Solid,
    Transparent,
    RedstoneWire,
    RedstoneTorch,
    Repeater,
    Comparator,
    Lever,
    Button,
    PressurePlate,
    RedstoneLamp,
    RedstoneBlock,
    Piston,
}

/// How completely DustRoute can use an observed block at a specific analysis
/// boundary. Observation is kept even when later semantic stages are not
/// supported.
#[derive(Clone, Copy, Debug, Deserialize, Eq, Hash, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum CapabilityLevel {
    Full,
    Partial,
    Unsupported,
    NotApplicable,
}

#[derive(Clone, Copy, Debug, Default, Deserialize, Eq, Hash, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ObservationClassification {
    /// Constructed internally rather than read from a live Minecraft world.
    #[default]
    Synthetic,
    /// The observed identifier maps exactly to the modeled physical class.
    Exact,
    /// The original observation is retained, but its physical class is a
    /// conservative fallback.
    Coarse,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct BlockCapabilities {
    pub observation: CapabilityLevel,
    pub physical_classification: CapabilityLevel,
    pub connectivity: CapabilityLevel,
    pub steady_state: CapabilityLevel,
    pub temporal: CapabilityLevel,
    pub repair: CapabilityLevel,
    pub placement: CapabilityLevel,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize)]
pub enum Facing {
    North,
    East,
    South,
    West,
    Up,
    Down,
}

impl Facing {
    #[must_use]
    pub const fn horizontal_offset(self) -> Option<Pos> {
        match self {
            Self::North => Some(Pos::new(0, 0, -1)),
            Self::East => Some(Pos::new(1, 0, 0)),
            Self::South => Some(Pos::new(0, 0, 1)),
            Self::West => Some(Pos::new(-1, 0, 0)),
            Self::Up | Self::Down => None,
        }
    }

    #[must_use]
    pub const fn opposite(self) -> Self {
        match self {
            Self::North => Self::South,
            Self::East => Self::West,
            Self::South => Self::North,
            Self::West => Self::East,
            Self::Up => Self::Down,
            Self::Down => Self::Up,
        }
    }
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, Hash, PartialEq, Serialize)]
pub enum WireConnection {
    None,
    Side,
    Up,
}

/// Geometry and electrical properties that affect redstone independently of a
/// block's display name.  This deliberately avoids Minecraft's overloaded
/// "transparent" classification.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum OccupiedShape {
    Empty,
    FullCube,
    TopHalf,
    BottomHalf,
    Partial,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct BlockRedstoneTraits {
    pub occupied_shape: OccupiedShape,
    pub supports_dust_on_top: bool,
    pub conducts_weak_power: bool,
    pub conducts_strong_power: bool,
    pub strong_power_drives_dust: bool,
    /// A wire beside this block may form an `up` arm to wire above it.
    pub permits_wire_rise_beside: bool,
    /// The block-state shape used by the lower wire. Top-half supports use a
    /// visually horizontal `side` arm even though power rises one block.
    pub wire_rise_connection: Option<WireConnection>,
    /// A full block above the lower wire prevents the rising arm.
    pub blocks_wire_rise_when_above: bool,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct BlockProperties {
    pub supports_components: bool,
    pub receives_weak_power: bool,
    pub receives_strong_power: bool,
    pub repeater_reads_block_power: bool,
    pub strong_power_drives_dust: bool,
}

impl BlockProperties {
    pub(crate) const fn support_only(supports_components: bool) -> Self {
        Self {
            supports_components,
            receives_weak_power: false,
            receives_strong_power: false,
            repeater_reads_block_power: false,
            strong_power_drives_dust: false,
        }
    }

    #[must_use]
    pub const fn can_be_powered(self) -> bool {
        self.receives_weak_power || self.receives_strong_power
    }
}

impl BlockKind {
    /// Whether this block is an explicit redstone circuit element rather than
    /// structural support that may incidentally carry power.
    #[must_use]
    pub const fn is_redstone_related(self) -> bool {
        matches!(
            self,
            Self::RedstoneWire
                | Self::RedstoneTorch
                | Self::Repeater
                | Self::Comparator
                | Self::Lever
                | Self::Button
                | Self::PressurePlate
                | Self::RedstoneLamp
                | Self::RedstoneBlock
                | Self::Piston
        )
    }

    #[must_use]
    pub const fn properties(self) -> BlockProperties {
        crate::blocks::behavior_profile(self).properties
    }
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct Block {
    pub kind: BlockKind,
    /// Namespaced identifier reported by Minecraft. Synthetic blocks leave
    /// this unset; imported observations retain it even when `kind` is only a
    /// coarse DustRoute classification.
    #[serde(default)]
    pub observed_name: Option<String>,
    /// Complete Java block-state properties as observed at the world boundary.
    #[serde(default)]
    pub observed_properties: BTreeMap<String, String>,
    #[serde(default)]
    pub observation_classification: ObservationClassification,
    pub facing: Option<Facing>,
    pub powered: Option<bool>,
    /// Observed analog redstone strength, when the block state exposes one.
    pub power_level: Option<u8>,
    pub delay: Option<u8>,
    pub support_offset: Option<Pos>,
    pub wire_connections: Option<BTreeMap<Facing, WireConnection>>,
}

impl Block {
    #[must_use]
    pub const fn new(kind: BlockKind) -> Self {
        Self {
            kind,
            observed_name: None,
            observed_properties: BTreeMap::new(),
            observation_classification: ObservationClassification::Synthetic,
            facing: None,
            powered: None,
            power_level: None,
            delay: None,
            support_offset: None,
            wire_connections: None,
        }
    }

    #[must_use]
    pub fn capabilities(&self) -> BlockCapabilities {
        use CapabilityLevel::{Full, NotApplicable, Partial, Unsupported};
        if self.requires_live_observation() {
            return BlockCapabilities {
                observation: Full,
                physical_classification: Unsupported,
                connectivity: Unsupported,
                steady_state: Unsupported,
                temporal: Unsupported,
                repair: Unsupported,
                placement: Unsupported,
            };
        }
        match self.kind {
            BlockKind::Air => BlockCapabilities {
                observation: Full,
                physical_classification: Full,
                connectivity: NotApplicable,
                steady_state: NotApplicable,
                temporal: NotApplicable,
                repair: NotApplicable,
                placement: Full,
            },
            BlockKind::Solid | BlockKind::Transparent => {
                let exact = self.observation_classification != ObservationClassification::Coarse;
                BlockCapabilities {
                    observation: Full,
                    physical_classification: if exact { Full } else { Partial },
                    connectivity: if exact { Full } else { Partial },
                    steady_state: NotApplicable,
                    temporal: NotApplicable,
                    repair: NotApplicable,
                    placement: if exact { Full } else { Unsupported },
                }
            }
            BlockKind::RedstoneWire => BlockCapabilities {
                observation: Full,
                physical_classification: Full,
                connectivity: Full,
                steady_state: Full,
                temporal: Partial,
                repair: Full,
                placement: Full,
            },
            BlockKind::RedstoneTorch | BlockKind::Repeater => BlockCapabilities {
                observation: Full,
                physical_classification: Full,
                connectivity: Full,
                steady_state: Full,
                temporal: Partial,
                repair: Partial,
                placement: Full,
            },
            BlockKind::Lever | BlockKind::RedstoneBlock => BlockCapabilities {
                observation: Full,
                physical_classification: Full,
                connectivity: Full,
                steady_state: Full,
                temporal: Full,
                repair: Partial,
                placement: Full,
            },
            BlockKind::PressurePlate => BlockCapabilities {
                observation: Full,
                physical_classification: Full,
                connectivity: Full,
                steady_state: Full,
                // Occupancy, entity filtering, and weighted level changes are
                // observed but not reproduced by the block-only simulator.
                temporal: Partial,
                repair: Partial,
                placement: Full,
            },
            BlockKind::Button | BlockKind::RedstoneLamp => BlockCapabilities {
                observation: Full,
                physical_classification: Full,
                connectivity: Full,
                steady_state: Full,
                temporal: Partial,
                repair: Partial,
                placement: Full,
            },
            BlockKind::Comparator => BlockCapabilities {
                observation: Full,
                physical_classification: Full,
                connectivity: Full,
                steady_state: Full,
                temporal: Partial,
                repair: Partial,
                placement: Full,
            },
            BlockKind::Piston => BlockCapabilities {
                observation: Full,
                physical_classification: Partial,
                connectivity: Partial,
                steady_state: Unsupported,
                temporal: Unsupported,
                repair: Unsupported,
                placement: Unsupported,
            },
        }
    }

    #[must_use]
    pub fn support_pos(&self, pos: Pos) -> Option<Pos> {
        self.support_offset
            .map(|offset| pos.offset(offset.x, offset.y, offset.z))
    }

    /// Returns true when the live block identifier is known to have
    /// redstone-adjacent semantics that this block-only model does not
    /// reproduce.  The observed name remains available for diagnostics, but
    /// callers must not silently treat the coarse physical fallback as a
    /// simulated solid block.
    #[must_use]
    pub fn requires_live_observation(&self) -> bool {
        self.observed_name
            .as_deref()
            .is_some_and(observed_name_requires_live_observation)
    }

    #[must_use]
    pub const fn is_external_input_source(&self) -> bool {
        matches!(
            self.kind,
            BlockKind::Lever
                | BlockKind::Button
                | BlockKind::PressurePlate
                | BlockKind::RedstoneBlock
        )
    }

    #[must_use]
    pub const fn is_observable_output(&self) -> bool {
        matches!(self.kind, BlockKind::RedstoneLamp)
    }

    /// Resolves physical redstone behavior from the observed block and its
    /// block state. Synthetic blocks retain the historical kind defaults.
    #[must_use]
    pub fn redstone_traits(&self) -> BlockRedstoneTraits {
        let properties = self.kind.properties();
        let mut traits = BlockRedstoneTraits {
            occupied_shape: match self.kind {
                BlockKind::Air => OccupiedShape::Empty,
                BlockKind::Solid
                | BlockKind::Transparent
                | BlockKind::RedstoneBlock
                | BlockKind::RedstoneLamp => OccupiedShape::FullCube,
                _ => OccupiedShape::Partial,
            },
            supports_dust_on_top: properties.supports_components,
            conducts_weak_power: properties.receives_weak_power,
            conducts_strong_power: properties.receives_strong_power,
            strong_power_drives_dust: properties.strong_power_drives_dust,
            permits_wire_rise_beside: properties.supports_components,
            wire_rise_connection: properties.supports_components.then_some(WireConnection::Up),
            blocks_wire_rise_when_above: self.kind == BlockKind::Solid,
        };
        if self.requires_live_observation() {
            traits.occupied_shape = OccupiedShape::FullCube;
            traits.supports_dust_on_top = false;
            traits.conducts_weak_power = false;
            traits.conducts_strong_power = false;
            traits.strong_power_drives_dust = false;
            traits.permits_wire_rise_beside = false;
            traits.wire_rise_connection = None;
            traits.blocks_wire_rise_when_above = true;
            return traits;
        }
        let name = self.observed_name.as_deref().unwrap_or_default();
        if name.ends_with("_slab") {
            let top = self.observed_properties.get("type").map(String::as_str) == Some("top");
            let double = self.observed_properties.get("type").map(String::as_str) == Some("double");
            traits.occupied_shape = if double {
                OccupiedShape::FullCube
            } else if top {
                OccupiedShape::TopHalf
            } else {
                OccupiedShape::BottomHalf
            };
            traits.supports_dust_on_top = top || double;
            traits.permits_wire_rise_beside = top || double;
            traits.wire_rise_connection = if top {
                Some(WireConnection::Side)
            } else if double {
                Some(WireConnection::Up)
            } else {
                None
            };
            traits.blocks_wire_rise_when_above = double;
            traits.conducts_weak_power = double;
            traits.conducts_strong_power = double;
            traits.strong_power_drives_dust = double;
        } else if name.ends_with("_stairs") {
            let top = self.observed_properties.get("half").map(String::as_str) == Some("top");
            traits.occupied_shape = OccupiedShape::Partial;
            traits.supports_dust_on_top = top;
            traits.permits_wire_rise_beside = top;
            traits.wire_rise_connection = top.then_some(WireConnection::Side);
            traits.blocks_wire_rise_when_above = false;
            traits.conducts_weak_power = false;
            traits.conducts_strong_power = false;
            traits.strong_power_drives_dust = false;
        } else if matches!(name, "minecraft:glass" | "minecraft:tinted_glass") {
            traits.occupied_shape = OccupiedShape::FullCube;
            traits.supports_dust_on_top = true;
            traits.permits_wire_rise_beside = true;
            traits.wire_rise_connection = Some(WireConnection::Up);
            traits.blocks_wire_rise_when_above = false;
            traits.conducts_weak_power = false;
            traits.conducts_strong_power = false;
            traits.strong_power_drives_dust = false;
        }
        traits
    }
}

/// Names whose state or event semantics are intentionally outside the
/// block-only simulator.  Keeping this list in the Minecraft layer prevents
/// translation, scenario, and optimization code from drifting apart.
#[must_use]
pub fn observed_name_requires_live_observation(name: &str) -> bool {
    let short_name = name.strip_prefix("minecraft:").unwrap_or(name);
    matches!(
        short_name,
        "observer"
            | "target"
            | "daylight_detector"
            | "dispenser"
            | "dropper"
            | "hopper"
            | "slime_block"
            | "honey_block"
            | "water"
            | "lava"
            | "tripwire_hook"
            | "sculk_sensor"
            | "calibrated_sculk_sensor"
    ) || short_name.ends_with("_rail")
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct World {
    blocks: BTreeMap<Pos, Block>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SupportError(pub Vec<(Pos, BlockKind, Option<Pos>)>);

impl Display for SupportError {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        write!(f, "{} component(s) have invalid support", self.0.len())
    }
}

impl Error for SupportError {}

impl World {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    pub fn set(&mut self, pos: Pos, block: Block) {
        if block.kind == BlockKind::Air {
            self.blocks.remove(&pos);
        } else {
            self.blocks.insert(pos, block);
        }
    }

    pub fn place(&mut self, kind: BlockKind, pos: Pos) -> &mut Block {
        let mut block = Block::new(kind);
        if matches!(
            kind,
            BlockKind::RedstoneWire
                | BlockKind::Repeater
                | BlockKind::Comparator
                | BlockKind::Lever
                | BlockKind::Button
                | BlockKind::PressurePlate
        ) {
            block.support_offset = Some(Pos::new(0, -1, 0));
        }
        self.blocks.insert(pos, block);
        self.blocks.get_mut(&pos).expect("block was just inserted")
    }

    #[must_use]
    pub fn get(&self, pos: Pos) -> Option<&Block> {
        self.blocks.get(&pos)
    }

    #[must_use]
    pub fn kind_at(&self, pos: Pos) -> BlockKind {
        self.get(pos).map_or(BlockKind::Air, |block| block.kind)
    }

    pub fn remove(&mut self, pos: Pos) -> Option<Block> {
        self.blocks.remove(&pos)
    }

    pub fn iter(&self) -> impl Iterator<Item = (&Pos, &Block)> {
        self.blocks.iter()
    }

    pub fn positions(&self) -> impl Iterator<Item = Pos> + '_ {
        self.blocks.keys().copied()
    }

    pub fn fill(&mut self, a: Pos, b: Pos, block: Block) {
        for x in a.x.min(b.x)..=a.x.max(b.x) {
            for y in a.y.min(b.y)..=a.y.max(b.y) {
                for z in a.z.min(b.z)..=a.z.max(b.z) {
                    self.set(Pos::new(x, y, z), block.clone());
                }
            }
        }
    }

    #[must_use]
    pub fn bounds(&self) -> Option<(Pos, Pos)> {
        let mut positions = self.blocks.keys();
        let first = *positions.next()?;
        let (mut low, mut high) = (first, first);
        for pos in positions {
            low = Pos::new(low.x.min(pos.x), low.y.min(pos.y), low.z.min(pos.z));
            high = Pos::new(high.x.max(pos.x), high.y.max(pos.y), high.z.max(pos.z));
        }
        Some((low, high))
    }

    #[must_use]
    pub fn support_issues(&self) -> Vec<(Pos, BlockKind, Option<Pos>)> {
        self.blocks
            .iter()
            .filter_map(|(pos, block)| {
                let requires = matches!(
                    block.kind,
                    BlockKind::RedstoneWire
                        | BlockKind::RedstoneTorch
                        | BlockKind::Repeater
                        | BlockKind::Comparator
                        | BlockKind::Lever
                        | BlockKind::Button
                        | BlockKind::PressurePlate
                );
                if !requires {
                    return None;
                }
                let support = block.support_pos(*pos);
                let valid = support
                    .and_then(|at| self.get(at))
                    .is_some_and(|support| support.redstone_traits().supports_dust_on_top);
                (!valid).then_some((*pos, block.kind, support))
            })
            .collect()
    }

    pub fn validate_supports(&self) -> Result<(), SupportError> {
        let issues = self.support_issues();
        if issues.is_empty() {
            Ok(())
        } else {
            Err(SupportError(issues))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn air_is_not_stored_and_bounds_are_deterministic() {
        let mut world = World::new();
        world.set(Pos::new(3, 2, -1), Block::new(BlockKind::Solid));
        world.set(Pos::new(-2, 5, 4), Block::new(BlockKind::Solid));
        world.set(Pos::new(3, 2, -1), Block::new(BlockKind::Air));
        assert_eq!(
            world.bounds(),
            Some((Pos::new(-2, 5, 4), Pos::new(-2, 5, 4)))
        );
    }

    #[test]
    fn validates_component_support() {
        let mut world = World::new();
        world.set(Pos::new(0, 0, 0), Block::new(BlockKind::Solid));
        world.place(BlockKind::RedstoneWire, Pos::new(0, 1, 0));
        assert!(world.validate_supports().is_ok());
        world.remove(Pos::new(0, 0, 0));
        assert!(world.validate_supports().is_err());
    }

    #[test]
    fn reports_capabilities_without_discarding_an_unknown_observation() {
        let mut block = Block::new(BlockKind::Solid);
        block.observed_name = Some("minecraft:target".to_owned());
        block.observation_classification = ObservationClassification::Coarse;
        block
            .observed_properties
            .insert("power".to_owned(), "7".to_owned());
        assert_eq!(
            block.capabilities().physical_classification,
            CapabilityLevel::Unsupported
        );
        assert_eq!(block.observed_name.as_deref(), Some("minecraft:target"));
        assert_eq!(block.observed_properties["power"], "7");
    }

    #[test]
    fn known_redstone_observations_are_fail_closed_and_do_not_conduct() {
        let mut block = Block::new(BlockKind::Solid);
        block.observed_name = Some("minecraft:target".to_owned());
        block.observation_classification = ObservationClassification::Coarse;
        assert!(block.requires_live_observation());
        let traits = block.redstone_traits();
        assert!(!traits.conducts_weak_power);
        assert!(!traits.conducts_strong_power);
        assert_eq!(
            block.capabilities().steady_state,
            CapabilityLevel::Unsupported
        );
    }

    #[test]
    fn button_and_pressure_plate_require_physical_support() {
        let mut world = World::new();
        world.place(BlockKind::Button, Pos::new(0, 1, 0));
        world.place(BlockKind::PressurePlate, Pos::new(1, 1, 0));
        assert_eq!(world.support_issues().len(), 2);
        world.set(Pos::new(0, 0, 0), Block::new(BlockKind::Solid));
        world.set(Pos::new(1, 0, 0), Block::new(BlockKind::Solid));
        assert!(world.support_issues().is_empty());
    }

    #[test]
    fn top_and_bottom_slabs_resolve_to_different_support_traits() {
        let mut top = Block::new(BlockKind::Transparent);
        top.observed_name = Some("minecraft:stone_slab".to_owned());
        top.observed_properties
            .insert("type".to_owned(), "top".to_owned());
        let mut bottom = top.clone();
        bottom
            .observed_properties
            .insert("type".to_owned(), "bottom".to_owned());

        assert_eq!(top.redstone_traits().occupied_shape, OccupiedShape::TopHalf);
        assert!(top.redstone_traits().supports_dust_on_top);
        assert!(!top.redstone_traits().conducts_weak_power);
        assert_eq!(
            bottom.redstone_traits().occupied_shape,
            OccupiedShape::BottomHalf
        );
        assert!(!bottom.redstone_traits().supports_dust_on_top);
    }
}
