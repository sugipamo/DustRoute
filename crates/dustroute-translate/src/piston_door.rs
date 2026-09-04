//! Reusable execution of the bounded, non-compact 3x3 piston-door contract.
//!
//! The first fanout implementation lived entirely in an integration test.
//! This module keeps the same deliberately small physical subset, but makes
//! the layout and its executor reusable: callers provide a serialized
//! scenario, the module materializes its wire/repeater/piston world, and the
//! common runner executes `closed -> open -> closed` through the normal
//! redstone propagation path.

use std::collections::{BTreeMap, BTreeSet};
use std::error::Error;
use std::fmt::{Display, Formatter};

use dustroute_minecraft::time::{PhysicsEngine, PhysicsEngineError, TraceStatus};
use dustroute_minecraft::{
    Block, BlockKind, Facing, PISTON_PUSH_LIMIT, PistonState, PistonVariant, Pos, Region,
    WireConnection, World, piston_state,
};
use serde::{Deserialize, Serialize};

/// Schema accepted by [`PistonDoorScenario::from_json`].
pub const PISTON_DOOR_FANOUT_SCHEMA: &str = "dustroute.3x3-piston-shuttle-fanout.v1";
/// Event budget used by the bounded reference executor.
pub const DEFAULT_PISTON_DOOR_EVENT_BUDGET: usize = 8192;

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct PistonDoorScenario {
    pub schema_version: String,
    pub minecraft_version: String,
    pub id: String,
    pub evidence: String,
    pub coordinate_convention: PistonDoorCoordinateConvention,
    pub control: PistonDoorControl,
    pub cells: Vec<PistonDoorCell>,
    pub expected: PistonDoorExpected,
    pub scope: PistonDoorScope,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct PistonDoorCoordinateConvention {
    pub door_plane_z: i32,
    pub open_direction: String,
    pub close_direction: String,
    pub width_axis: String,
    pub height_axis: String,
    pub travel_axis: String,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct PistonDoorControl {
    pub lever: Pos,
    pub open_source: Pos,
    pub close_source: Pos,
    pub pulse_width_game_ticks: u64,
    pub row_order: Vec<i32>,
    pub open_branch_z: i32,
    pub close_branch_z: i32,
    pub branch_lane_z_open: Vec<i32>,
    pub branch_lane_z_close: Vec<i32>,
    pub branch_first_repeater_x: i32,
    pub trunk_repeater_counts: Vec<usize>,
    pub trunk_repeater_delay_redstone_ticks: u8,
    pub leaf_repeater_z_open: Vec<i32>,
    pub leaf_repeater_z_close: Vec<i32>,
    pub leaf_delay_redstone_ticks: Vec<Vec<Vec<u8>>>,
    pub fanout_levels: Vec<usize>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct PistonDoorCell {
    pub x: i32,
    pub y: i32,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct PistonDoorExpected {
    pub closed_door_z: i32,
    pub open_door_z: i32,
    pub stable_pistons_retracted: bool,
    pub pending_events_after_settle: usize,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct PistonDoorScope {
    pub supported: Vec<String>,
    pub out_of_scope: Vec<String>,
}

/// A materialized world and conservative planning boundary.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PistonDoorWorld {
    world: World,
    known_region: Region,
}

impl PistonDoorWorld {
    #[must_use]
    pub const fn world(&self) -> &World {
        &self.world
    }

    #[must_use]
    pub const fn known_region(&self) -> Region {
        self.known_region
    }

    #[must_use]
    pub fn into_parts(self) -> (World, Region) {
        (self.world, self.known_region)
    }
}

/// Errors are intentionally explicit so a malformed or unsupported layout
/// cannot be silently interpreted as a different circuit.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum PistonDoorScenarioError {
    Json(String),
    Invalid {
        reason: String,
    },
    Collision {
        position: Pos,
        existing: BlockKind,
        requested: BlockKind,
    },
    EmptyWorld,
    Physics(PhysicsEngineError),
}

impl Display for PistonDoorScenarioError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Json(message) => {
                write!(formatter, "invalid piston-door scenario JSON: {message}")
            }
            Self::Invalid { reason } => write!(formatter, "invalid piston-door scenario: {reason}"),
            Self::Collision {
                position,
                existing,
                requested,
            } => write!(
                formatter,
                "piston-door layout collision at {position:?}: {existing:?} vs {requested:?}"
            ),
            Self::EmptyWorld => write!(formatter, "piston-door layout produced an empty world"),
            Self::Physics(error) => write!(formatter, "piston-door execution failed: {error}"),
        }
    }
}

impl Error for PistonDoorScenarioError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Physics(error) => Some(error),
            _ => None,
        }
    }
}

impl From<PhysicsEngineError> for PistonDoorScenarioError {
    fn from(value: PhysicsEngineError) -> Self {
        Self::Physics(value)
    }
}

impl PistonDoorScenario {
    /// Parses and validates the versioned scenario contract.
    pub fn from_json(json: &str) -> Result<Self, PistonDoorScenarioError> {
        let scenario: Self = serde_json::from_str(json)
            .map_err(|error| PistonDoorScenarioError::Json(error.to_string()))?;
        scenario.validate()?;
        Ok(scenario)
    }

    /// Validates the geometry and all dimensions needed by the bounded
    /// one-to-three-to-nine executor.
    pub fn validate(&self) -> Result<(), PistonDoorScenarioError> {
        if self.schema_version != PISTON_DOOR_FANOUT_SCHEMA {
            return Err(invalid(format!(
                "schema_version must be {PISTON_DOOR_FANOUT_SCHEMA:?}"
            )));
        }
        if self.minecraft_version != "1.21.11" {
            return Err(invalid(
                "only the declared Minecraft 1.21.11 contract is supported",
            ));
        }
        if self.evidence != "designed_control_contract" {
            return Err(invalid(
                "only the designed_control_contract evidence level is executable",
            ));
        }
        if self.coordinate_convention.width_axis != "x"
            || self.coordinate_convention.height_axis != "y"
            || self.coordinate_convention.travel_axis != "z"
        {
            return Err(invalid(
                "coordinate axes must be width=x, height=y, travel=z",
            ));
        }
        let open_facing = parse_horizontal_facing(&self.coordinate_convention.open_direction)
            .ok_or_else(|| invalid("open_direction must be a horizontal facing"))?;
        let close_facing = parse_horizontal_facing(&self.coordinate_convention.close_direction)
            .ok_or_else(|| invalid("close_direction must be a horizontal facing"))?;
        if open_facing.opposite() != close_facing {
            return Err(invalid("open and close directions must be opposite"));
        }
        let open_offset = open_facing.offset();
        if open_offset.x != 0 || open_offset.y != 0 || open_offset.z == 0 {
            return Err(invalid("the bounded door travel must be along the z axis"));
        }
        if self.expected.open_door_z != self.expected.closed_door_z + open_offset.z {
            return Err(invalid("open_door_z must be one block in open_direction"));
        }
        if self.cells.len() != 9 {
            return Err(invalid("exactly nine door cells are required"));
        }
        let xs = self
            .cells
            .iter()
            .map(|cell| cell.x)
            .collect::<BTreeSet<_>>();
        let ys = self
            .cells
            .iter()
            .map(|cell| cell.y)
            .collect::<BTreeSet<_>>();
        if xs.len() != 3 || ys.len() != 3 {
            return Err(invalid(
                "door cells must contain three x and three y coordinates",
            ));
        }
        if !contiguous(&xs) || !contiguous(&ys) {
            return Err(invalid(
                "door cell coordinates must form contiguous 3x3 rows",
            ));
        }
        let expected_cells = xs
            .iter()
            .flat_map(|x| ys.iter().map(move |y| (*x, *y)))
            .collect::<BTreeSet<_>>();
        let actual_cells = self
            .cells
            .iter()
            .map(|cell| (cell.x, cell.y))
            .collect::<BTreeSet<_>>();
        if expected_cells != actual_cells {
            return Err(invalid(
                "door cells must be the complete 3x3 Cartesian product",
            ));
        }
        let rows = ys.iter().copied().collect::<Vec<_>>();
        if self.control.row_order.len() != 3
            || self.control.row_order.iter().collect::<BTreeSet<_>>()
                != rows.iter().collect::<BTreeSet<_>>()
        {
            return Err(invalid(
                "row_order must contain each of the three door rows once",
            ));
        }
        let fanout_y = rows[1];
        if self.control.row_order[1] != fanout_y {
            return Err(invalid(
                "row_order index 1 must identify the middle fanout row",
            ));
        }
        if self.control.open_source.y != fanout_y || self.control.close_source.y != fanout_y {
            return Err(invalid(
                "open and close sources must be on the middle fanout row",
            ));
        }
        if self.control.open_source == self.control.close_source
            || self.control.lever == self.control.open_source
            || self.control.lever == self.control.close_source
        {
            return Err(invalid("lever and open/close sources must be distinct"));
        }
        if self.control.open_branch_z == self.control.close_branch_z
            || self.control.pulse_width_game_ticks == 0
        {
            return Err(invalid(
                "open/close branches must differ and pulse width must be nonzero",
            ));
        }
        validate_len(&self.control.branch_lane_z_open, 3, "branch_lane_z_open")?;
        validate_len(&self.control.branch_lane_z_close, 3, "branch_lane_z_close")?;
        validate_len(
            &self.control.trunk_repeater_counts,
            3,
            "trunk_repeater_counts",
        )?;
        validate_len(
            &self.control.leaf_repeater_z_open,
            4,
            "leaf_repeater_z_open",
        )?;
        validate_len(
            &self.control.leaf_repeater_z_close,
            4,
            "leaf_repeater_z_close",
        )?;
        if self
            .control
            .branch_lane_z_open
            .iter()
            .collect::<BTreeSet<_>>()
            .len()
            != 3
            || self
                .control
                .branch_lane_z_close
                .iter()
                .collect::<BTreeSet<_>>()
                .len()
                != 3
        {
            return Err(invalid(
                "each branch must have three distinct lane coordinates",
            ));
        }
        if self.control.branch_lane_z_open[1] != self.control.open_branch_z
            || self.control.branch_lane_z_close[1] != self.control.close_branch_z
        {
            return Err(invalid(
                "the middle fanout lane must remain on its branch plane",
            ));
        }
        if self.control.trunk_repeater_counts.contains(&0) {
            return Err(invalid("each trunk must contain at least one repeater"));
        }
        if self.control.fanout_levels != [1, 3, 9] {
            return Err(invalid("fanout_levels must declare [1, 3, 9]"));
        }
        if self.control.leaf_delay_redstone_ticks.len() != 3
            || self
                .control
                .leaf_delay_redstone_ticks
                .iter()
                .any(|row| row.len() != 3 || row.iter().any(|cell| cell.len() != 4))
        {
            return Err(invalid("leaf_delay_redstone_ticks must be a 3x3x4 matrix"));
        }
        if self
            .control
            .leaf_delay_redstone_ticks
            .iter()
            .flatten()
            .flatten()
            .any(|delay| !(1..=4).contains(delay))
        {
            return Err(invalid(
                "leaf repeater delays must be between 1 and 4 redstone ticks",
            ));
        }
        if !(1..=4).contains(&self.control.trunk_repeater_delay_redstone_ticks) {
            return Err(invalid(
                "trunk repeater delay must be between 1 and 4 redstone ticks",
            ));
        }
        if self.control.open_branch_z == self.control.close_branch_z
            || self
                .control
                .branch_lane_z_open
                .contains(&self.control.close_branch_z)
            || self
                .control
                .branch_lane_z_close
                .contains(&self.control.open_branch_z)
        {
            return Err(invalid(
                "open and close fanout regions must not share a branch plane",
            ));
        }
        Ok(())
    }

    /// Translates every scenario coordinate by a fixed offset. This is useful
    /// for proving that the executor is layout-driven rather than tied to the
    /// original fixture's absolute coordinates.
    #[must_use]
    pub fn translated(&self, offset: Pos) -> Self {
        let mut translated = self.clone();
        translated.coordinate_convention.door_plane_z += offset.z;
        translated.control.lever = translated
            .control
            .lever
            .offset(offset.x, offset.y, offset.z);
        translated.control.open_source = translated
            .control
            .open_source
            .offset(offset.x, offset.y, offset.z);
        translated.control.close_source = translated
            .control
            .close_source
            .offset(offset.x, offset.y, offset.z);
        translated.control.open_branch_z += offset.z;
        translated.control.close_branch_z += offset.z;
        translated.control.branch_lane_z_open = translated
            .control
            .branch_lane_z_open
            .into_iter()
            .map(|z| z + offset.z)
            .collect();
        translated.control.branch_lane_z_close = translated
            .control
            .branch_lane_z_close
            .into_iter()
            .map(|z| z + offset.z)
            .collect();
        translated.control.branch_first_repeater_x += offset.x;
        translated.control.leaf_repeater_z_open = translated
            .control
            .leaf_repeater_z_open
            .into_iter()
            .map(|z| z + offset.z)
            .collect();
        translated.control.leaf_repeater_z_close = translated
            .control
            .leaf_repeater_z_close
            .into_iter()
            .map(|z| z + offset.z)
            .collect();
        translated.control.row_order = translated
            .control
            .row_order
            .into_iter()
            .map(|y| y + offset.y)
            .collect();
        translated.cells = translated
            .cells
            .into_iter()
            .map(|cell| PistonDoorCell {
                x: cell.x + offset.x,
                y: cell.y + offset.y,
            })
            .collect();
        translated.expected.closed_door_z += offset.z;
        translated.expected.open_door_z += offset.z;
        translated
    }

    /// Materializes the declared fanout, repeaters, and 18 piston cells.
    pub fn build_world(&self) -> Result<PistonDoorWorld, PistonDoorScenarioError> {
        self.validate()?;
        let open_facing = parse_horizontal_facing(&self.coordinate_convention.open_direction)
            .expect("validated open direction");
        let close_facing = open_facing.opposite();
        let rows = self
            .cells
            .iter()
            .map(|cell| cell.y)
            .collect::<BTreeSet<_>>()
            .into_iter()
            .collect::<Vec<_>>();
        let xs = self
            .cells
            .iter()
            .map(|cell| cell.x)
            .collect::<BTreeSet<_>>()
            .into_iter()
            .collect::<Vec<_>>();
        let fanout_y = rows[1];
        let mut world = World::new();
        let mut lever = Block::new(BlockKind::Lever);
        lever.powered = Some(false);
        set_block(&mut world, self.control.lever, lever)?;
        for cell in &self.cells {
            let door = Pos::new(cell.x, cell.y, self.coordinate_convention.door_plane_z);
            set_block(&mut world, door, Block::new(BlockKind::Solid))?;
            let open_piston = piston(open_facing, PistonVariant::Normal);
            set_block(
                &mut world,
                door.offset(
                    -open_facing.offset().x,
                    -open_facing.offset().y,
                    -open_facing.offset().z,
                ),
                open_piston,
            )?;
            let close_position = door.offset(
                open_facing.offset().x * 2,
                open_facing.offset().y * 2,
                open_facing.offset().z * 2,
            );
            set_block(
                &mut world,
                close_position,
                piston(close_facing, PistonVariant::Normal),
            )?;
        }
        self.build_branch(&mut world, true, &xs, fanout_y)?;
        self.build_branch(&mut world, false, &xs, fanout_y)?;
        for (row_index, row) in self.control.row_order.iter().copied().enumerate() {
            for (column, x) in xs.iter().copied().enumerate() {
                let delays = &self.control.leaf_delay_redstone_ticks[row_index][column];
                for (index, z) in self
                    .control
                    .leaf_repeater_z_open
                    .iter()
                    .copied()
                    .enumerate()
                {
                    set_block(
                        &mut world,
                        Pos::new(x, row, z),
                        repeater(open_facing, delays[index]),
                    )?;
                }
                for (index, z) in self
                    .control
                    .leaf_repeater_z_close
                    .iter()
                    .copied()
                    .enumerate()
                {
                    set_block(
                        &mut world,
                        Pos::new(x, row, z),
                        repeater(close_facing, delays[index]),
                    )?;
                }
            }
        }
        let (low, high) = world.bounds().ok_or(PistonDoorScenarioError::EmptyWorld)?;
        let margin = PISTON_PUSH_LIMIT as i32 + 1;
        let known_region = Region::new(
            low.offset(-margin, -margin, -margin),
            high.offset(margin, margin, margin),
        );
        Ok(PistonDoorWorld {
            world,
            known_region,
        })
    }

    /// Runs only the Lever ON/open half and returns the settled engine.
    pub fn run_open(&self) -> Result<PhysicsEngine, PistonDoorScenarioError> {
        let materialized = self.build_world()?;
        let mut engine = PhysicsEngine::new(materialized.world, DEFAULT_PISTON_DOOR_EVENT_BUDGET)
            .with_piston_planning_region(materialized.known_region);
        engine.schedule_lever_pulse_sequence(
            0,
            self.control.lever,
            true,
            [self.control.open_source],
            [self.control.close_source],
            self.control.pulse_width_game_ticks,
        );
        engine.run_redstone_propagation()?;
        self.assert_stable(&engine, self.expected.open_door_z, "open")?;
        Ok(engine)
    }

    /// Runs the complete common execution path: closed -> open -> closed.
    pub fn run_cycle(&self) -> Result<PhysicsEngine, PistonDoorScenarioError> {
        let mut engine = self.run_open()?;
        let off_tick = engine.time().game_tick + 1;
        engine.schedule_lever_pulse_sequence(
            off_tick,
            self.control.lever,
            false,
            [self.control.open_source],
            [self.control.close_source],
            self.control.pulse_width_game_ticks,
        );
        engine.run_redstone_propagation()?;
        self.assert_stable(&engine, self.expected.closed_door_z, "closed")?;
        Ok(engine)
    }

    fn build_branch(
        &self,
        world: &mut World,
        open: bool,
        xs: &[i32],
        fanout_y: i32,
    ) -> Result<(), PistonDoorScenarioError> {
        let control = &self.control;
        let branch_z = if open {
            control.open_branch_z
        } else {
            control.close_branch_z
        };
        let lanes = if open {
            &control.branch_lane_z_open
        } else {
            &control.branch_lane_z_close
        };
        let source = if open {
            control.open_source
        } else {
            control.close_source
        };
        let mut source_block = Block::new(BlockKind::RedstoneBlock);
        source_block.powered = Some(false);
        set_block(world, source, source_block)?;
        let branch_end_x = control.branch_first_repeater_x - 1;
        horizontal_wire_line(world, source.y, branch_z, source.x + 1, branch_end_x)?;
        connect_wire_component(world, Pos::new(source.x + 1, source.y, branch_z), source)?;
        for lane_z in lanes {
            if *lane_z != branch_z {
                z_wire_line(world, branch_end_x, source.y, branch_z, *lane_z)?;
            }
        }
        for (row_index, row) in control.row_order.iter().copied().enumerate() {
            let lane_z = lanes[row_index];
            let first_repeater = Pos::new(control.branch_first_repeater_x, source.y, lane_z);
            connect_wire_component(
                world,
                Pos::new(branch_end_x, source.y, lane_z),
                first_repeater,
            )?;
            for offset in 0..control.trunk_repeater_counts[row_index] {
                set_block(
                    world,
                    Pos::new(
                        control.branch_first_repeater_x + offset as i32,
                        source.y,
                        lane_z,
                    ),
                    repeater(Facing::East, control.trunk_repeater_delay_redstone_ticks),
                )?;
            }
            let output_x =
                control.branch_first_repeater_x + control.trunk_repeater_counts[row_index] as i32;
            let output_wire = Pos::new(output_x, source.y, lane_z);
            connect_wire_component(world, output_wire, Pos::new(output_x - 1, source.y, lane_z))?;
            if row == fanout_y {
                horizontal_wire_line(world, row, branch_z, output_x, *xs.last().expect("x"))?;
            } else {
                let sink_x = output_x + 1;
                let sink = Pos::new(sink_x, row, lane_z);
                if row < fanout_y {
                    set_block(
                        world,
                        Pos::new(output_x, row, lane_z),
                        Block::new(BlockKind::Solid),
                    )?;
                    add_wire_connection_shape(world, sink, Facing::West, WireConnection::Up)?;
                } else {
                    set_block(
                        world,
                        Pos::new(sink_x, row - 1, lane_z),
                        Block::new(BlockKind::Solid),
                    )?;
                    add_wire_connection_shape(
                        world,
                        output_wire,
                        Facing::East,
                        WireConnection::Up,
                    )?;
                }
                horizontal_wire_line(world, row, lane_z, sink_x, *xs.first().expect("x"))?;
                z_wire_line(world, *xs.first().expect("x"), row, lane_z, branch_z)?;
            }
        }
        let first_leaf_z = if open {
            control.leaf_repeater_z_open[0]
        } else {
            control.leaf_repeater_z_close[0]
        };
        for row in control.row_order.iter().copied() {
            for (column, x) in xs.iter().copied().enumerate() {
                let branch_wire = Pos::new(x, row, branch_z);
                place_wire(world, branch_wire)?;
                if column > 0 {
                    connect_wire_pair(world, Pos::new(xs[column - 1], row, branch_z), branch_wire)?;
                }
                connect_wire_component(world, branch_wire, Pos::new(x, row, first_leaf_z))?;
            }
        }
        Ok(())
    }

    fn assert_stable(
        &self,
        engine: &PhysicsEngine,
        door_z: i32,
        phase: &str,
    ) -> Result<(), PistonDoorScenarioError> {
        let open_facing = parse_horizontal_facing(&self.coordinate_convention.open_direction)
            .expect("validated open direction");
        for cell in &self.cells {
            let door = Pos::new(cell.x, cell.y, door_z);
            if engine.world().kind_at(door) != BlockKind::Solid {
                return Err(invalid(format!(
                    "{phase}: door block is missing at {door:?}"
                )));
            }
            let other_z = if door_z == self.expected.closed_door_z {
                self.expected.open_door_z
            } else {
                self.expected.closed_door_z
            };
            let other = Pos::new(cell.x, cell.y, other_z);
            if engine.world().kind_at(other) != BlockKind::Air {
                return Err(invalid(format!(
                    "{phase}: passage is occupied at {other:?}"
                )));
            }
            if self.expected.stable_pistons_retracted {
                let open_piston = Pos::new(
                    cell.x - open_facing.offset().x,
                    cell.y - open_facing.offset().y,
                    self.expected.closed_door_z - open_facing.offset().z,
                );
                let close_piston = Pos::new(
                    cell.x + open_facing.offset().x * 2,
                    cell.y + open_facing.offset().y * 2,
                    self.expected.closed_door_z + open_facing.offset().z * 2,
                );
                for piston in [open_piston, close_piston] {
                    if piston_state(engine.world().get(piston).ok_or_else(|| {
                        invalid(format!("{phase}: piston is missing at {piston:?}"))
                    })?) != PistonState::Retracted
                    {
                        return Err(invalid(format!(
                            "{phase}: piston is not retracted at {piston:?}"
                        )));
                    }
                }
            }
        }
        if engine.pending_event_count() != self.expected.pending_events_after_settle {
            return Err(invalid(format!(
                "{phase}: pending event count is {}, expected {}",
                engine.pending_event_count(),
                self.expected.pending_events_after_settle
            )));
        }
        if !matches!(engine.trace_status(), TraceStatus::Complete) {
            return Err(invalid(format!("{phase}: trace did not settle completely")));
        }
        Ok(())
    }
}

fn invalid(reason: impl Into<String>) -> PistonDoorScenarioError {
    PistonDoorScenarioError::Invalid {
        reason: reason.into(),
    }
}

fn validate_len<T>(
    values: &[T],
    expected: usize,
    name: &str,
) -> Result<(), PistonDoorScenarioError> {
    if values.len() == expected {
        Ok(())
    } else {
        Err(invalid(format!("{name} must contain {expected} entries")))
    }
}

fn contiguous(values: &BTreeSet<i32>) -> bool {
    let mut iter = values.iter().copied();
    let Some(mut previous) = iter.next() else {
        return false;
    };
    for value in iter {
        if value != previous + 1 {
            return false;
        }
        previous = value;
    }
    true
}

fn parse_horizontal_facing(value: &str) -> Option<Facing> {
    match value.to_ascii_lowercase().as_str() {
        "north" => Some(Facing::North),
        "east" => Some(Facing::East),
        "south" => Some(Facing::South),
        "west" => Some(Facing::West),
        _ => None,
    }
}

fn piston(facing: Facing, variant: PistonVariant) -> Block {
    let mut block = Block::new(BlockKind::Piston);
    block.facing = Some(facing);
    block.piston_variant = Some(variant);
    block.piston_state = Some(PistonState::Retracted);
    block
}

fn repeater(facing: Facing, delay: u8) -> Block {
    let mut block = Block::new(BlockKind::Repeater);
    block.facing = Some(facing);
    block.delay = Some(delay);
    block.powered = Some(false);
    block
}

fn wire() -> Block {
    let mut block = Block::new(BlockKind::RedstoneWire);
    block.wire_connections = Some(BTreeMap::new());
    block
}

fn set_block(
    world: &mut World,
    position: Pos,
    block: Block,
) -> Result<(), PistonDoorScenarioError> {
    if let Some(existing) = world.get(position) {
        return Err(PistonDoorScenarioError::Collision {
            position,
            existing: existing.kind,
            requested: block.kind,
        });
    }
    world.set(position, block);
    Ok(())
}

fn place_wire(world: &mut World, position: Pos) -> Result<(), PistonDoorScenarioError> {
    if let Some(existing) = world.get(position) {
        if existing.kind == BlockKind::RedstoneWire {
            return Ok(());
        }
        return Err(PistonDoorScenarioError::Collision {
            position,
            existing: existing.kind,
            requested: BlockKind::RedstoneWire,
        });
    }
    world.set(position, wire());
    Ok(())
}

fn add_wire_connection_shape(
    world: &mut World,
    position: Pos,
    facing: Facing,
    connection: WireConnection,
) -> Result<(), PistonDoorScenarioError> {
    place_wire(world, position)?;
    world
        .get_mut(position)
        .expect("place_wire must create the wire")
        .wire_connections
        .get_or_insert_with(BTreeMap::new)
        .insert(facing, connection);
    Ok(())
}

fn connect_wire_pair(
    world: &mut World,
    first: Pos,
    second: Pos,
) -> Result<(), PistonDoorScenarioError> {
    let direction = direction_between(first, second)?;
    add_wire_connection_shape(world, first, direction, WireConnection::Side)?;
    add_wire_connection_shape(world, second, direction.opposite(), WireConnection::Side)
}

fn connect_wire_component(
    world: &mut World,
    wire_position: Pos,
    component: Pos,
) -> Result<(), PistonDoorScenarioError> {
    add_wire_connection_shape(
        world,
        wire_position,
        direction_between(wire_position, component)?,
        WireConnection::Side,
    )
}

fn direction_between(source: Pos, target: Pos) -> Result<Facing, PistonDoorScenarioError> {
    match (
        target.x - source.x,
        target.y - source.y,
        target.z - source.z,
    ) {
        (1, 0, 0) => Ok(Facing::East),
        (-1, 0, 0) => Ok(Facing::West),
        (0, 1, 0) => Ok(Facing::Up),
        (0, -1, 0) => Ok(Facing::Down),
        (0, 0, 1) => Ok(Facing::South),
        (0, 0, -1) => Ok(Facing::North),
        delta => Err(invalid(format!("fanout path is not adjacent: {delta:?}"))),
    }
}

fn horizontal_wire_line(
    world: &mut World,
    y: i32,
    z: i32,
    start_x: i32,
    end_x: i32,
) -> Result<(), PistonDoorScenarioError> {
    let step = if end_x >= start_x { 1 } else { -1 };
    let mut x = start_x;
    place_wire(world, Pos::new(x, y, z))?;
    while x != end_x {
        let next = x + step;
        connect_wire_pair(world, Pos::new(x, y, z), Pos::new(next, y, z))?;
        x = next;
    }
    Ok(())
}

fn z_wire_line(
    world: &mut World,
    x: i32,
    y: i32,
    start_z: i32,
    end_z: i32,
) -> Result<(), PistonDoorScenarioError> {
    let step = if end_z >= start_z { 1 } else { -1 };
    let mut z = start_z;
    place_wire(world, Pos::new(x, y, z))?;
    while z != end_z {
        let next = z + step;
        connect_wire_pair(world, Pos::new(x, y, z), Pos::new(x, y, next))?;
        z = next;
    }
    Ok(())
}
