//! Source-derived, read-only electrical queries. This is not a runtime or a
//! placement certificate: callers still own callbacks, support and motion.
use std::collections::BTreeMap;

use crate::piston_electrical_law::{
    Emission, EmissionFacts, PistonPowerQuery, QueryDirection, SignalSource, builtin_laws,
};
use crate::time::runtime::RuntimeError;
use crate::{
    Block, BlockKind, Facing, ObservationClassification, Pos, Region, WireConnection, World,
};

pub const SIDES: [Facing; 6] = [
    Facing::Down,
    Facing::Up,
    Facing::North,
    Facing::South,
    Facing::West,
    Facing::East,
];
pub const HORIZONTAL: [Facing; 4] = [Facing::North, Facing::East, Facing::South, Facing::West];

fn invalid(message: impl Into<String>) -> RuntimeError {
    RuntimeError::Handler(message.into())
}

pub fn along(position: Pos, direction: Facing) -> Result<Pos, RuntimeError> {
    let d = direction.offset();
    Ok(Pos::new(
        position
            .x
            .checked_add(d.x)
            .ok_or(RuntimeError::ClockOverflow)?,
        position
            .y
            .checked_add(d.y)
            .ok_or(RuntimeError::ClockOverflow)?,
        position
            .z
            .checked_add(d.z)
            .ok_or(RuntimeError::ClockOverflow)?,
    ))
}

fn name(direction: Facing) -> &'static str {
    match direction {
        Facing::Down => "down",
        Facing::Up => "up",
        Facing::North => "north",
        Facing::South => "south",
        Facing::East => "east",
        Facing::West => "west",
    }
}

fn wire_name(connection: WireConnection) -> &'static str {
    match connection {
        WireConnection::None => "none",
        WireConnection::Side => "side",
        WireConnection::Up => "up",
    }
}

fn power(block: &Block) -> Result<u8, RuntimeError> {
    block
        .powered
        .map(|b| if b { 15 } else { 0 })
        .ok_or_else(|| invalid("explicit device power required"))
}
fn level(block: &Block) -> Result<u8, RuntimeError> {
    block
        .power_level
        .filter(|n| *n <= 15)
        .ok_or_else(|| invalid("explicit wire power 0..15 required"))
}
fn output(block: &Block) -> Result<Facing, RuntimeError> {
    block
        .facing
        .filter(|d| d.horizontal_offset().is_some())
        .ok_or_else(|| invalid("explicit horizontal repeater output required"))
}
fn support(block: &Block) -> Result<Facing, RuntimeError> {
    SIDES
        .into_iter()
        .find(|d| Some(d.offset()) == block.support_offset)
        .ok_or_else(|| invalid("explicit adjacent lever support required"))
}
fn arms(block: &Block) -> Result<&BTreeMap<Facing, WireConnection>, RuntimeError> {
    block
        .wire_connections
        .as_ref()
        .filter(|a| a.len() == 4 && HORIZONTAL.iter().all(|d| a.contains_key(d)))
        .ok_or_else(|| invalid("complete four-arm wire state required"))
}

/// Evidence gate for electrical identities only. Stable piston/head pairing and
/// physical support must additionally pass the runtime/placement gates.
pub fn validate_evidence(block: &Block) -> Result<(), RuntimeError> {
    if !crate::execution_context::WorldExecutionProfile::UnifiedPistonElectricalCallbacksJava12111V12
        .admits_kind(block.kind)
    {
        return Err(invalid(format!("unsupported electrical kind {:?}", block.kind)));
    }
    if block.observation_classification == ObservationClassification::Coarse
        || block.requires_live_observation()
    {
        return Err(invalid("coarse or unsupported electrical observation"));
    }
    let observed = block
        .observed_name
        .as_deref()
        .map(|n| n.strip_prefix("minecraft:").unwrap_or(n));
    let property = |key: &str| block.observed_properties.get(key).map(String::as_str);
    if let Some(program) = crate::device_program::program(block) {
        use crate::device_program::Orientation;
        let definition = &program.definition;
        definition.validate_state(block).map_err(invalid)?;
        let direction_valid = match definition.physical().orientation() {
            Orientation::None => true,
            Orientation::Output => block.facing.is_some(),
            Orientation::FloorOutput => {
                block
                    .facing
                    .is_some_and(|d| d.horizontal_offset().is_some())
                    && block.support_offset == Some(Facing::Down.offset())
            }
            Orientation::Attached => {
                support(block)?;
                true
            }
        };
        let properties_valid = observed.is_none_or(|name| {
            let orientation = match definition.physical().orientation() {
                Orientation::None => true,
                Orientation::Output | Orientation::FloorOutput => {
                    block.facing.is_some_and(|direction| {
                        property("facing") == Some(self::name(direction.opposite()))
                    })
                }
                Orientation::Attached => {
                    let facing = block.facing.filter(|d| d.horizontal_offset().is_some());
                    let expected = match property("face") {
                        Some("floor") => Some(Facing::Down.offset()),
                        Some("ceiling") => Some(Facing::Up.offset()),
                        Some("wall") => facing.map(|d| d.opposite().offset()),
                        _ => None,
                    };
                    expected == block.support_offset
                        && facing.is_some_and(|d| property("facing") == Some(self::name(d)))
                }
            };
            definition.observed_names.iter().any(|n| n == name) && orientation
        });
        return if direction_valid && properties_valid {
            Ok(())
        } else {
            Err(invalid("unsupported device observation"))
        };
    }
    let matches = match block.kind {
        BlockKind::Air => observed.is_none_or(|n| n == "air"),
        BlockKind::Solid => observed.is_none_or(|n| {
            matches!(
                n,
                "stone"
                    | "cobblestone"
                    | "smooth_stone"
                    | "obsidian"
                    | "smooth_quartz"
                    | "cyan_wool"
            )
        }),
        BlockKind::Transparent => observed.is_none_or(|n| n == "glass"),
        BlockKind::RedstoneBlock => {
            block.powered != Some(false) && observed.is_none_or(|n| n == "redstone_block")
        }
        BlockKind::Lever => {
            let power = power(block)? != 0;
            let attached = support(block)?;
            observed.is_none_or(|n| {
                let facing = block.facing.filter(|d| d.horizontal_offset().is_some());
                let expected = match property("face") {
                    Some("floor") => Some(Facing::Down),
                    Some("ceiling") => Some(Facing::Up),
                    Some("wall") => facing.map(Facing::opposite),
                    _ => None,
                };
                n == "lever"
                    && expected == Some(attached)
                    && facing.is_some_and(|d| property("facing") == Some(name(d)))
                    && property("powered").and_then(|s| s.parse::<bool>().ok()) == Some(power)
            })
        }
        BlockKind::RedstoneWire => {
            let power = level(block)?;
            let connections = arms(block)?;
            block.support_offset == Some(Facing::Down.offset())
                && observed.is_none_or(|n| {
                    n == "redstone_wire"
                        && property("power").and_then(|s| s.parse::<u8>().ok()) == Some(power)
                        && HORIZONTAL
                            .into_iter()
                            .all(|d| property(name(d)) == Some(wire_name(connections[&d])))
                })
        }
        BlockKind::Piston => {
            block.facing.is_some()
                && block.piston_state.is_some_and(|s| s.is_stable())
                && observed.is_none_or(|n| matches!(n, "piston" | "sticky_piston"))
        }
        BlockKind::PistonHead => {
            block.piston_head.is_some() && observed.is_none_or(|n| n == "piston_head")
        }
        BlockKind::MovingPiston => {
            let state = block
                .piston_entity
                .as_deref()
                .ok_or_else(|| invalid("carrier state required"))?;
            validate_evidence(&state.pushed_block)?;
            observed.is_none_or(|n| n == "moving_piston")
        }
        _ => false,
    };
    if !matches {
        return Err(invalid(format!(
            "unsupported electrical evidence for {:?}",
            block.kind
        )));
    }
    Ok(())
}

/// Current callback geometry is independent of historical spatial Law tags.
pub fn conducts(block: &Block) -> bool {
    crate::physical::of_kind(block.kind).conducts(block)
}

pub fn full_face(block: &Block, side: Facing) -> bool {
    crate::physical::of_kind(block.kind).full_face(block, side)
}

#[derive(Clone, Copy)]
pub struct ElectricalWorld<'a> {
    world: &'a World,
    region: Region,
    outputs: Option<&'a BTreeMap<Pos, u8>>,
}

impl<'a> ElectricalWorld<'a> {
    pub(crate) fn for_runtime(
        view: crate::time::runtime::RuntimeView<'a>,
    ) -> Result<Self, RuntimeError> {
        for (pos, block) in view.world().iter() {
            if !view.known_region().contains(*pos) {
                return Err(RuntimeError::UnknownSpace(*pos));
            }
            if let Some(planned) = view.staged_carriers.get(pos) {
                let mut bare = planned.clone();
                bare.piston_entity = None;
                if block != &bare {
                    return Err(RuntimeError::CarrierConflict(*pos));
                }
                validate_evidence(planned)?;
            } else {
                validate_evidence(block)?;
            }
        }
        Ok(Self {
            world: view.world(),
            region: view.known_region(),
            outputs: Some(view.outputs),
        })
    }

    pub fn new(world: &'a World, region: Region) -> Result<Self, RuntimeError> {
        for (pos, block) in world.iter() {
            if !region.contains(*pos) {
                return Err(RuntimeError::UnknownSpace(*pos));
            }
            validate_evidence(block)?;
        }
        Ok(Self {
            world,
            region,
            outputs: None,
        })
    }

    pub fn block(&self, pos: Pos) -> Result<Block, RuntimeError> {
        if !self.region.contains(pos) {
            return Err(RuntimeError::UnknownSpace(pos));
        }
        Ok(self
            .world
            .get(pos)
            .cloned()
            .unwrap_or_else(|| Block::new(BlockKind::Air)))
    }

    pub fn emission(
        &self,
        pos: Pos,
        query: Facing,
        wires_enabled: bool,
    ) -> Result<Emission, RuntimeError> {
        let block = self.block(pos)?;
        let mut facts = EmissionFacts {
            source: SignalSource::Passive,
            level: 0,
            query: match query {
                Facing::Down => QueryDirection::Down,
                Facing::Up => QueryDirection::Up,
                _ => QueryDirection::Horizontal,
            },
            direction_match: false,
            wire_connected: false,
            wires_enabled,
        };
        if let Some(program) = crate::device_program::program(&block) {
            let definition = program.definition();
            if definition.signal_level == crate::device_program::SignalLevel::StoredOutput {
                let stored = self.stored_output(pos)?;
                let level = if block.powered == Some(true) {
                    stored
                } else {
                    0
                };
                return definition
                    .emission_with_level(&block, query, level)
                    .map_err(invalid);
            }
            return definition.emission(&block, query).map_err(invalid);
        }
        match block.kind {
            BlockKind::Lever => {
                facts.source = SignalSource::Lever;
                facts.level = power(&block)?;
                facts.direction_match = query == support(&block)?.opposite();
            }
            BlockKind::RedstoneBlock => facts.source = SignalSource::RedstoneBlock,
            BlockKind::RedstoneWire => {
                facts.source = SignalSource::Wire;
                facts.level = level(&block)?;
                if wires_enabled && query.horizontal_offset().is_some() {
                    facts.wire_connected =
                        self.wire_shape(pos)?[&query.opposite()] != WireConnection::None;
                }
            }
            _ => {}
        }
        builtin_laws()
            .emission(facts)
            .ok_or_else(|| invalid("invalid emission facts"))
    }

    pub fn emitted(
        &self,
        pos: Pos,
        query: Facing,
        wires_enabled: bool,
    ) -> Result<u8, RuntimeError> {
        let weak = self.emission(pos, query, wires_enabled)?.weak;
        let conductor = conducts(&self.block(pos)?);
        let mut strong = 0;
        if conductor {
            for direction in SIDES {
                strong = strong.max(
                    self.emission(along(pos, direction)?, direction, wires_enabled)?
                        .strong,
                );
            }
        }
        Ok(builtin_laws()
            .emitted(weak, strong, conductor)
            .expect("bounded signal facts"))
    }

    pub fn receiving_power(&self, pos: Pos) -> Result<bool, RuntimeError> {
        for side in SIDES {
            if self.emitted(along(pos, side)?, side, true)? > 0 {
                return Ok(true);
            }
        }
        Ok(false)
    }

    pub fn comparator_readout(&self, pos: Pos) -> Result<Option<u8>, RuntimeError> {
        let block = self.block(pos)?;
        match crate::device_program::program(&block) {
            Some(program) => program
                .definition()
                .comparator_readout(&block)
                .map_err(invalid),
            None => Ok(None),
        }
    }

    /// Maximum incoming analog signal, including conductor-mediated strong power.
    /// Unlike a Boolean query this must inspect all six faces, even after finding
    /// a nonzero value. Unknown space is an error, never an implicit zero.
    pub fn receiving_level(&self, pos: Pos) -> Result<u8, RuntimeError> {
        let mut level = 0;
        for side in SIDES {
            level = level.max(self.emitted(along(pos, side)?, side, true)?);
        }
        Ok(level)
    }

    pub fn piston_powered(&self, pos: Pos) -> Result<bool, RuntimeError> {
        let block = self.block(pos)?;
        if block.kind != BlockKind::Piston {
            return Err(invalid("piston query needs a body"));
        }
        let facing = block
            .facing
            .ok_or_else(|| invalid("piston direction required"))?;
        let mut powered = false;
        for side in SIDES {
            let source = along(pos, side)?;
            self.block(source)?;
            if side != facing {
                powered |= builtin_laws().accepts(
                    PistonPowerQuery::Adjacent,
                    false,
                    self.emitted(source, side, true)? > 0,
                );
            }
        }
        powered |= builtin_laws().accepts(
            PistonPowerQuery::OwnPosition,
            false,
            self.emitted(pos, Facing::Down, true)? > 0,
        );
        let above = along(pos, Facing::Up)?;
        for side in SIDES.into_iter().filter(|d| *d != Facing::Down) {
            powered |= builtin_laws().accepts(
                PistonPowerQuery::Quasi,
                false,
                self.emitted(along(above, side)?, side, true)? > 0,
            );
        }
        Ok(powered)
    }

    fn wire_connects(block: &Block, side: Facing) -> bool {
        crate::physical::of_kind(block.kind).wire_connects(block, side)
    }

    pub fn wire_shape(&self, pos: Pos) -> Result<BTreeMap<Facing, WireConnection>, RuntimeError> {
        let block = self.block(pos)?;
        let seed = arms(&block)?;
        let dot = seed.values().all(|c| *c == WireConnection::None);
        self.wire_placement_shape(pos, dot)
    }

    fn wire_render_connection(
        &self,
        pos: Pos,
        side: Facing,
    ) -> Result<WireConnection, RuntimeError> {
        let clear = !conducts(&self.block(along(pos, Facing::Up)?)?);
        let beside = along(pos, side)?;
        let neighbor = self.block(beside)?;
        let connected = if clear
            && full_face(&neighbor, Facing::Up)
            && self.block(along(beside, Facing::Up)?)?.kind == BlockKind::RedstoneWire
        {
            if full_face(&neighbor, side.opposite()) {
                WireConnection::Up
            } else {
                WireConnection::Side
            }
        } else if Self::wire_connects(&neighbor, side)
            || (!conducts(&neighbor)
                && self.block(along(beside, Facing::Down)?)?.kind == BlockKind::RedstoneWire)
        {
            WireConnection::Side
        } else {
            WireConnection::None
        };
        Ok(connected)
    }

    /// Java getStateForNeighborUpdate retains dot/cross history and sometimes
    /// changes just the notified arm. This differs from a placement recompute.
    pub fn wire_neighbor_shape(
        &self,
        pos: Pos,
        side: Facing,
    ) -> Result<BTreeMap<Facing, WireConnection>, RuntimeError> {
        let block = self.block(pos)?;
        let mut shape = arms(&block)?.clone();
        if side == Facing::Down {
            if !full_face(&self.block(along(pos, side)?)?, Facing::Up) {
                return Err(invalid("wire support destruction is outside this profile"));
            }
            return Ok(shape);
        }
        if side == Facing::Up {
            return self.wire_shape(pos);
        }
        let connection = self.wire_render_connection(pos, side)?;
        if (connection != WireConnection::None) == (shape[&side] != WireConnection::None)
            && !shape.values().all(|c| *c != WireConnection::None)
        {
            shape.insert(side, connection);
            return Ok(shape);
        }
        self.wire_placement_shape(pos, false)
    }

    fn wire_placement_shape(
        &self,
        pos: Pos,
        dot: bool,
    ) -> Result<BTreeMap<Facing, WireConnection>, RuntimeError> {
        let mut shape = BTreeMap::new();
        for side in HORIZONTAL {
            shape.insert(side, self.wire_render_connection(pos, side)?);
        }
        if dot && shape.values().all(|c| *c == WireConnection::None) {
            return Ok(shape);
        }
        let ns = [Facing::North, Facing::South]
            .into_iter()
            .any(|d| shape[&d] != WireConnection::None);
        let ew = [Facing::East, Facing::West]
            .into_iter()
            .any(|d| shape[&d] != WireConnection::None);
        if !ns {
            for d in [Facing::East, Facing::West] {
                if shape[&d] == WireConnection::None {
                    shape.insert(d, WireConnection::Side);
                }
            }
        }
        if !ew {
            for d in [Facing::North, Facing::South] {
                if shape[&d] == WireConnection::None {
                    shape.insert(d, WireConnection::Side);
                }
            }
        }
        Ok(shape)
    }

    pub fn wire_power(&self, pos: Pos) -> Result<u8, RuntimeError> {
        if self.block(pos)?.kind != BlockKind::RedstoneWire {
            return Err(invalid("wire query needs a wire"));
        }
        self.wire_power_at(pos)
    }

    /// Controller sampling also occurs after a wire has been removed. The
    /// old wire state supplies only its previous level; the world is current.
    pub fn wire_power_at(&self, pos: Pos) -> Result<u8, RuntimeError> {
        self.block(pos)?;
        let mut external = 0;
        for side in SIDES {
            external = external.max(self.emitted(along(pos, side)?, side, false)?);
        }
        let mut neighboring = 0;
        let clear = !conducts(&self.block(along(pos, Facing::Up)?)?);
        for side in HORIZONTAL {
            let beside = along(pos, side)?;
            let block = self.block(beside)?;
            if block.kind == BlockKind::RedstoneWire {
                neighboring = neighboring.max(level(&block)?);
            }
            let diagonal = if conducts(&block) && clear {
                Some(along(beside, Facing::Up)?)
            } else if !conducts(&block) {
                Some(along(beside, Facing::Down)?)
            } else {
                None
            };
            if let Some(p) = diagonal {
                let block = self.block(p)?;
                if block.kind == BlockKind::RedstoneWire {
                    neighboring = neighboring.max(level(&block)?);
                }
            }
        }
        Ok(crate::dust_law::builtin_dust_law()
            .strength(external, neighboring)
            .expect("bounded dust facts"))
    }

    pub fn stored_output(&self, pos: Pos) -> Result<u8, RuntimeError> {
        self.block(pos)?;
        Ok(self
            .outputs
            .ok_or_else(|| {
                invalid("device output requires runtime state; block snapshot is insufficient")
            })?
            .get(&pos)
            .copied()
            .unwrap_or(0))
    }

    /// Block-only gate inputs, including comparator readout through one solid block.
    pub fn gate_output_level(&self, pos: Pos) -> Result<u8, RuntimeError> {
        let block = self.block(pos)?;
        let direction = output(&block)?;
        let rear_direction = direction.opposite();
        let rear = along(pos, rear_direction)?;
        let rear_block = self.block(rear)?;
        let mut input = self.emitted(rear, rear_direction, true)?;
        if rear_block.kind == BlockKind::RedstoneWire {
            input = input.max(level(&rear_block)?);
        }
        if let Some(readout) = self.comparator_readout(rear)? {
            input = readout;
        } else if input < 15 && conducts(&rear_block) {
            if let Some(readout) = self.comparator_readout(along(rear, rear_direction)?)? {
                input = readout;
            }
        }
        let mut side_input = 0;
        for side in HORIZONTAL
            .into_iter()
            .filter(|d| *d != direction && *d != rear_direction)
        {
            let pos = along(pos, side)?;
            let block = self.block(pos)?;
            let level = match block.kind {
                BlockKind::RedstoneWire => level(&block)?,
                BlockKind::RedstoneBlock => 15,
                _ => self.emission(pos, side, true)?.strong,
            };
            side_input = side_input.max(level);
        }
        let definition = crate::device_program::program(&block)
            .ok_or_else(|| invalid("missing gate definition"))?
            .definition();
        let subtract = definition
            .state(&block, crate::device_program::Property::ComparatorMode)
            .map_err(invalid)?
            != 0;
        crate::device_callback_law::comparator_signal(input, side_input, subtract)
            .ok_or_else(|| invalid("invalid gate input level"))
    }

    pub fn gate_input_powered(&self, pos: Pos) -> Result<bool, RuntimeError> {
        let query = output(&self.block(pos)?)?.opposite();
        let rear = along(pos, query)?;
        let emitted = self.emitted(rear, query, true)?;
        let block = self.block(rear)?;
        Ok(emitted > 0 || (block.kind == BlockKind::RedstoneWire && level(&block)? > 0))
    }

    pub fn side_gate_powered(&self, pos: Pos) -> Result<bool, RuntimeError> {
        let direction = output(&self.block(pos)?)?;
        let mut locked = false;
        for side in HORIZONTAL
            .into_iter()
            .filter(|d| *d != direction && *d != direction.opposite())
        {
            let target = along(pos, side)?;
            if matches!(
                self.block(target)?.kind,
                BlockKind::Repeater | BlockKind::Comparator
            ) {
                locked |= self.emission(target, side, true)?.strong > 0;
            }
        }
        Ok(locked)
    }

    pub fn output_gate_misaligned(&self, pos: Pos) -> Result<bool, RuntimeError> {
        let direction = output(&self.block(pos)?)?;
        let target = self.block(along(pos, direction)?)?;
        Ok(
            matches!(target.kind, BlockKind::Repeater | BlockKind::Comparator)
                && target.facing != Some(direction.opposite()),
        )
    }
}

/// Java 21 HashSet<BlockPos>, created empty and populated with seven distinct
/// positions: capacity 16, no resize, stable insertion within each bucket.
pub fn wire_notification_centers(pos: Pos) -> Result<Vec<Pos>, RuntimeError> {
    let mut centers = vec![pos];
    for side in SIDES {
        centers.push(along(pos, side)?);
    }
    centers.sort_by_key(|p| {
        let hash =
            p.y.wrapping_add(p.z.wrapping_mul(31))
                .wrapping_mul(31)
                .wrapping_add(p.x) as u32;
        (hash ^ (hash >> 16)) & 15
    });
    Ok(centers)
}
