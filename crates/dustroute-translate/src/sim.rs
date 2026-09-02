use std::collections::{BTreeMap, BTreeSet, VecDeque};

use crate::connectivity::{
    comparator_input_pos, comparator_output_pos, device_side_positions, repeater_input_pos,
    repeater_output_pos,
};
use crate::electrical::{
    DeviceOutputState, InstantaneousElectricalState, InstantaneousSolveDidNotConverge,
    PoweredBlockState, repeater_input_level, solve_instantaneous, torch_support_is_powered,
};
use crate::world::{BlockKind, Pos, World};

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum InputMutationError {
    Missing {
        position: Pos,
    },
    WrongKind {
        position: Pos,
        expected: &'static str,
        actual: BlockKind,
    },
    InvalidPressureLevel {
        position: Pos,
        level: u8,
    },
    Solver(InstantaneousSolveDidNotConverge),
}

impl std::fmt::Display for InputMutationError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Missing { position } => write!(f, "no input block at {position:?}"),
            Self::WrongKind {
                position,
                expected,
                actual,
            } => write!(
                f,
                "input at {position:?} must be {expected}, found {actual:?}"
            ),
            Self::InvalidPressureLevel { position, level } => write!(
                f,
                "pressure plate level {level} at {position:?} is outside 0..=15"
            ),
            Self::Solver(error) => error.fmt(f),
        }
    }
}

impl std::error::Error for InputMutationError {}

impl From<InstantaneousSolveDidNotConverge> for InputMutationError {
    fn from(value: InstantaneousSolveDidNotConverge) -> Self {
        Self::Solver(value)
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct TickState {
    pub tick: u64,
    pub strengths: BTreeMap<Pos, u8>,
    pub block_power: BTreeMap<Pos, PoweredBlockState>,
    pub repeater_powered: BTreeMap<Pos, bool>,
    pub torch_lit: BTreeMap<Pos, bool>,
    pub comparator_output: BTreeMap<Pos, u8>,
    pub lamp_lit: BTreeMap<Pos, bool>,
    pub torch_burnout_candidates: BTreeSet<Pos>,
    pub instantaneous_iterations: usize,
}

impl TickState {
    #[must_use]
    pub fn strength(&self, pos: Pos) -> u8 {
        self.strengths.get(&pos).copied().unwrap_or(0)
    }

    #[must_use]
    pub fn power(&self, pos: Pos) -> PoweredBlockState {
        self.block_power.get(&pos).copied().unwrap_or_default()
    }

    #[must_use]
    pub fn powered(&self, pos: Pos) -> bool {
        self.lamp_lit.get(&pos).copied().unwrap_or(false)
            || self.strength(pos) > 0
            || self.power(pos).powered()
    }
}

#[derive(Clone)]
pub struct RedstoneTickSimulator {
    world: World,
    tick: u64,
    repeater_powered: BTreeMap<Pos, bool>,
    repeater_queues: BTreeMap<Pos, VecDeque<bool>>,
    torch_lit: BTreeMap<Pos, bool>,
    torch_toggle_ticks: BTreeMap<Pos, VecDeque<u64>>,
    torch_burnout_candidates: BTreeSet<Pos>,
    comparator_output: BTreeMap<Pos, u8>,
    comparator_queues: BTreeMap<Pos, VecDeque<u8>>,
    lamp_lit: BTreeMap<Pos, bool>,
    lamp_off_deadline: BTreeMap<Pos, u64>,
    instantaneous: InstantaneousElectricalState,
}

impl RedstoneTickSimulator {
    pub fn new(world: World) -> Result<Self, InstantaneousSolveDidNotConverge> {
        let devices = DeviceOutputState::initially_lit(&world);
        let repeater_queues = world
            .iter()
            .filter(|(_, block)| block.kind == BlockKind::Repeater)
            .map(|(pos, block)| {
                let delay = usize::from(block.delay.unwrap_or(1).clamp(1, 4));
                let powered = devices.repeater_powered.get(pos).copied().unwrap_or(false);
                (*pos, VecDeque::from(vec![powered; delay]))
            })
            .collect();
        let instantaneous = solve_instantaneous(&world, &devices, 128)?;
        let comparator_queues = devices
            .comparator_output
            .keys()
            .map(|pos| {
                (
                    *pos,
                    VecDeque::from([devices.comparator_output.get(pos).copied().unwrap_or(0)]),
                )
            })
            .collect();
        let lamp_lit = world
            .iter()
            .filter(|(_, block)| block.kind == BlockKind::RedstoneLamp)
            .map(|(pos, _)| (*pos, instantaneous.power(*pos).powered()))
            .collect();
        Ok(Self {
            world,
            tick: 0,
            repeater_powered: devices.repeater_powered,
            repeater_queues,
            torch_lit: devices.torch_lit,
            torch_toggle_ticks: BTreeMap::new(),
            torch_burnout_candidates: BTreeSet::new(),
            comparator_output: devices.comparator_output,
            comparator_queues,
            lamp_lit,
            lamp_off_deadline: BTreeMap::new(),
            instantaneous,
        })
    }

    fn devices(&self) -> DeviceOutputState {
        DeviceOutputState {
            repeater_powered: self.repeater_powered.clone(),
            torch_lit: self.torch_lit.clone(),
            comparator_output: self.comparator_output.clone(),
        }
    }

    pub fn settle_instantaneous(&mut self) -> Result<TickState, InstantaneousSolveDidNotConverge> {
        self.instantaneous = solve_instantaneous(&self.world, &self.devices(), 128)?;
        Ok(self.snapshot())
    }

    #[must_use]
    pub fn snapshot(&self) -> TickState {
        TickState {
            tick: self.tick,
            strengths: self.instantaneous.signal_levels.clone(),
            block_power: self.instantaneous.block_power.clone(),
            repeater_powered: self.repeater_powered.clone(),
            torch_lit: self.torch_lit.clone(),
            comparator_output: self.comparator_output.clone(),
            lamp_lit: self.lamp_lit.clone(),
            torch_burnout_candidates: self.torch_burnout_candidates.clone(),
            instantaneous_iterations: self.instantaneous.iterations,
        }
    }

    /// Returns whether a scheduled device or delayed output still has work
    /// queued for a future tick.  The queue is initialized with the current
    /// state, so merely having a repeater/comparator queue is not considered
    /// pending work; only a queued value that differs from the current output
    /// keeps the simulator non-quiescent.
    #[must_use]
    pub fn has_pending_events(&self) -> bool {
        self.repeater_queues.iter().any(|(pos, queue)| {
            queue
                .iter()
                .any(|value| self.repeater_powered.get(pos).copied() != Some(*value))
        }) || self.comparator_queues.iter().any(|(pos, queue)| {
            queue
                .iter()
                .any(|value| self.comparator_output.get(pos).copied() != Some(*value))
        }) || self
            .lamp_off_deadline
            .values()
            .any(|deadline| *deadline > self.tick)
    }

    pub fn advance_tick(&mut self) -> Result<TickState, InstantaneousSolveDidNotConverge> {
        let mut next_repeaters = BTreeMap::new();
        for (pos, block) in self.world.iter() {
            if block.kind != BlockKind::Repeater {
                continue;
            }
            let requested = repeater_input_pos(&self.world, *pos).is_some_and(|input| {
                repeater_input_level(&self.world, input, &self.instantaneous) > 0
            });
            let locked = device_side_positions(&self.world, *pos).is_some_and(|sides| {
                sides.into_iter().any(|side| {
                    self.repeater_powered.get(&side).copied().unwrap_or(false)
                        && repeater_output_pos(&self.world, side) == Some(*pos)
                        || self.comparator_output.get(&side).copied().unwrap_or(0) > 0
                            && comparator_output_pos(&self.world, side) == Some(*pos)
                })
            });
            if locked {
                next_repeaters.insert(
                    *pos,
                    self.repeater_powered.get(pos).copied().unwrap_or(false),
                );
                continue;
            }
            let queue = self
                .repeater_queues
                .get_mut(pos)
                .expect("repeater queue initialized");
            queue.pop_front();
            queue.push_back(requested);
            next_repeaters.insert(*pos, queue.front().copied().unwrap_or(requested));
        }
        let next_torches: BTreeMap<_, _> = self
            .world
            .iter()
            .filter(|(_, block)| block.kind == BlockKind::RedstoneTorch)
            .map(|(pos, _)| {
                (
                    *pos,
                    !torch_support_is_powered(&self.world, *pos, &self.instantaneous),
                )
            })
            .collect();
        for (pos, next) in &next_torches {
            if self.torch_lit.get(pos).copied() != Some(*next) {
                let toggles = self.torch_toggle_ticks.entry(*pos).or_default();
                toggles.push_back(self.tick + 1);
                while toggles
                    .front()
                    .is_some_and(|tick| self.tick + 1 - tick > 30)
                {
                    toggles.pop_front();
                }
                if toggles.len() >= 8 {
                    self.torch_burnout_candidates.insert(*pos);
                }
            }
        }
        let mut next_comparators = BTreeMap::new();
        for (pos, block) in self.world.iter() {
            if block.kind != BlockKind::Comparator {
                continue;
            }
            let rear = comparator_input_pos(&self.world, *pos)
                .map(|input| electrical_level_at(input, &self.instantaneous))
                .unwrap_or(0);
            let side = device_side_positions(&self.world, *pos)
                .map(|sides| {
                    sides
                        .into_iter()
                        .map(|side| electrical_level_at(side, &self.instantaneous))
                        .max()
                        .unwrap_or(0)
                })
                .unwrap_or(0);
            let requested =
                if block.observed_properties.get("mode").map(String::as_str) == Some("subtract") {
                    rear.saturating_sub(side)
                } else if rear >= side {
                    rear
                } else {
                    0
                };
            let queue = self
                .comparator_queues
                .get_mut(pos)
                .expect("comparator queue initialized");
            queue.pop_front();
            queue.push_back(requested);
            next_comparators.insert(*pos, queue.front().copied().unwrap_or(requested));
        }
        self.repeater_powered = next_repeaters;
        self.torch_lit = next_torches;
        self.comparator_output = next_comparators;
        self.tick += 1;
        self.instantaneous = solve_instantaneous(&self.world, &self.devices(), 128)?;
        for (pos, block) in self.world.iter() {
            if block.kind != BlockKind::RedstoneLamp {
                continue;
            }
            if self.instantaneous.power(*pos).powered() {
                self.lamp_lit.insert(*pos, true);
                self.lamp_off_deadline.remove(pos);
            } else if self.lamp_lit.get(pos).copied().unwrap_or(false) {
                let deadline = *self.lamp_off_deadline.entry(*pos).or_insert(self.tick + 2);
                if self.tick >= deadline {
                    self.lamp_lit.insert(*pos, false);
                    self.lamp_off_deadline.remove(pos);
                }
            }
        }
        Ok(self.snapshot())
    }

    pub fn set_powered(
        &mut self,
        pos: Pos,
        powered: bool,
    ) -> Result<TickState, InputMutationError> {
        let Some(block) = self.world.get(pos).cloned() else {
            return Err(InputMutationError::Missing { position: pos });
        };
        if !matches!(
            block.kind,
            BlockKind::Lever | BlockKind::Button | BlockKind::PressurePlate
        ) {
            return Err(InputMutationError::WrongKind {
                position: pos,
                expected: "lever, button, or pressure_plate",
                actual: block.kind,
            });
        }
        let mut changed = block;
        changed.powered = Some(powered);
        if changed.kind == BlockKind::PressurePlate {
            changed.power_level = Some(if powered { 15 } else { 0 });
        }
        self.world.set(pos, changed);
        self.settle_instantaneous()
            .map_err(InputMutationError::from)
    }

    pub fn set_lever_state(
        &mut self,
        pos: Pos,
        powered: bool,
    ) -> Result<TickState, InputMutationError> {
        self.set_stateful_powered(pos, BlockKind::Lever, powered)
    }

    pub fn set_button_state(
        &mut self,
        pos: Pos,
        powered: bool,
    ) -> Result<TickState, InputMutationError> {
        self.set_stateful_powered(pos, BlockKind::Button, powered)
    }

    /// Sets a pressure plate's analog redstone level.  Entity occupancy is an
    /// external concern; the simulator accepts the observed level explicitly.
    pub fn set_pressure_plate_level(
        &mut self,
        pos: Pos,
        level: u8,
    ) -> Result<TickState, InputMutationError> {
        if level > 15 {
            return Err(InputMutationError::InvalidPressureLevel {
                position: pos,
                level,
            });
        }
        let Some(block) = self.world.get(pos).cloned() else {
            return Err(InputMutationError::Missing { position: pos });
        };
        if block.kind != BlockKind::PressurePlate {
            return Err(InputMutationError::WrongKind {
                position: pos,
                expected: "pressure_plate",
                actual: block.kind,
            });
        }
        let mut changed = block;
        changed.power_level = Some(level);
        changed.powered = Some(level > 0);
        self.world.set(pos, changed);
        self.settle_instantaneous()
            .map_err(InputMutationError::from)
    }

    fn set_stateful_powered(
        &mut self,
        pos: Pos,
        expected: BlockKind,
        powered: bool,
    ) -> Result<TickState, InputMutationError> {
        let Some(block) = self.world.get(pos).cloned() else {
            return Err(InputMutationError::Missing { position: pos });
        };
        if block.kind != expected {
            return Err(InputMutationError::WrongKind {
                position: pos,
                expected: match expected {
                    BlockKind::Lever => "lever",
                    BlockKind::Button => "button",
                    _ => "stateful input",
                },
                actual: block.kind,
            });
        }
        let mut changed = block;
        changed.powered = Some(powered);
        self.world.set(pos, changed);
        self.settle_instantaneous()
            .map_err(InputMutationError::from)
    }

    /// Drives an inferred open-boundary input without making the temporary
    /// source part of the circuit's persistent physical representation.
    pub fn set_external_powered(
        &mut self,
        pos: Pos,
        powered: bool,
    ) -> Result<TickState, InputMutationError> {
        let current = self.world.kind_at(pos);
        if !matches!(current, BlockKind::Air | BlockKind::RedstoneBlock) {
            return Err(InputMutationError::WrongKind {
                position: pos,
                expected: "air or redstone_block for an external driver",
                actual: current,
            });
        }
        if powered {
            self.world
                .set(pos, crate::world::Block::new(BlockKind::RedstoneBlock));
        } else if self.world.kind_at(pos) == BlockKind::RedstoneBlock {
            self.world.remove(pos);
        }
        self.settle_instantaneous()
            .map_err(InputMutationError::from)
    }

    pub fn settle_ticks(
        &mut self,
        count: usize,
    ) -> Result<TickState, InstantaneousSolveDidNotConverge> {
        let mut state = self.snapshot();
        for _ in 0..count {
            state = self.advance_tick()?;
        }
        Ok(state)
    }
}

fn electrical_level_at(pos: Pos, state: &InstantaneousElectricalState) -> u8 {
    state.signal(pos).max(state.power(pos).level())
}

#[cfg(test)]
mod tests {
    use crate::wire::update_wire_shapes;
    use crate::world::{Block, Facing};

    use super::*;

    #[test]
    fn torch_changes_only_on_tick() {
        let mut world = World::new();
        world.set(Pos::new(0, 0, 0), Block::new(BlockKind::Solid));
        let lever = world.place(BlockKind::Lever, Pos::new(-1, 0, 0));
        lever.powered = Some(true);
        lever.support_offset = Some(Pos::new(1, 0, 0));
        let torch = world.place(BlockKind::RedstoneTorch, Pos::new(1, 0, 0));
        torch.facing = Some(Facing::East);
        torch.support_offset = Some(Pos::new(-1, 0, 0));
        let mut simulator = RedstoneTickSimulator::new(world).unwrap();
        assert_eq!(simulator.snapshot().strength(Pos::new(1, 0, 0)), 15);
        simulator.settle_instantaneous().unwrap();
        assert_eq!(simulator.snapshot().strength(Pos::new(1, 0, 0)), 15);
        assert_eq!(
            simulator
                .advance_tick()
                .unwrap()
                .strength(Pos::new(1, 0, 0)),
            0
        );
    }

    #[test]
    fn repeater_refreshes_signal_after_delay() {
        let mut world = World::new();
        world.fill(
            Pos::new(0, 0, 0),
            Pos::new(3, 0, 0),
            Block::new(BlockKind::Solid),
        );
        world.set(Pos::new(0, 1, 0), Block::new(BlockKind::RedstoneBlock));
        world.place(BlockKind::RedstoneWire, Pos::new(1, 1, 0));
        let repeater = world.place(BlockKind::Repeater, Pos::new(2, 1, 0));
        repeater.facing = Some(Facing::East);
        repeater.delay = Some(1);
        world.place(BlockKind::RedstoneWire, Pos::new(3, 1, 0));
        update_wire_shapes(&mut world);
        let mut simulator = RedstoneTickSimulator::new(world).unwrap();
        assert_eq!(simulator.snapshot().strength(Pos::new(3, 1, 0)), 0);
        assert_eq!(
            simulator
                .advance_tick()
                .unwrap()
                .strength(Pos::new(3, 1, 0)),
            15
        );
    }

    #[test]
    fn powered_side_repeater_locks_main_repeater_low() {
        let mut world = World::new();
        world.fill(
            Pos::new(0, 0, -2),
            Pos::new(2, 0, 0),
            Block::new(BlockKind::Solid),
        );
        let input = world.place(BlockKind::Lever, Pos::new(0, 1, 0));
        input.powered = Some(false);
        input.support_offset = Some(Pos::new(0, -1, 0));
        let main = world.place(BlockKind::Repeater, Pos::new(1, 1, 0));
        main.facing = Some(Facing::East);
        main.delay = Some(1);
        let lock = world.place(BlockKind::Repeater, Pos::new(1, 1, -1));
        lock.facing = Some(Facing::South);
        lock.delay = Some(1);
        world.set(Pos::new(1, 1, -2), Block::new(BlockKind::RedstoneBlock));

        let mut simulator = RedstoneTickSimulator::new(world).unwrap();
        simulator.advance_tick().unwrap();
        assert!(simulator.snapshot().repeater_powered[&Pos::new(1, 1, -1)]);
        simulator.set_powered(Pos::new(0, 1, 0), true).unwrap();
        simulator.advance_tick().unwrap();
        assert!(!simulator.snapshot().repeater_powered[&Pos::new(1, 1, 0)]);
    }

    #[test]
    fn imported_powered_repeater_keeps_its_initial_delay_state() {
        let mut world = World::new();
        world.fill(
            Pos::new(0, 0, 0),
            Pos::new(2, 0, 0),
            Block::new(BlockKind::Solid),
        );
        let source = world.place(BlockKind::Lever, Pos::new(0, 1, 0));
        source.powered = Some(true);
        source.support_offset = Some(Pos::new(0, -1, 0));
        let repeater = world.place(BlockKind::Repeater, Pos::new(1, 1, 0));
        repeater.facing = Some(Facing::East);
        repeater.delay = Some(2);
        repeater.powered = Some(true);

        let mut simulator = RedstoneTickSimulator::new(world).unwrap();
        assert!(simulator.snapshot().repeater_powered[&Pos::new(1, 1, 0)]);
        assert!(simulator.advance_tick().unwrap().repeater_powered[&Pos::new(1, 1, 0)]);
    }

    #[test]
    fn one_tick_input_pulse_survives_a_two_tick_repeater_delay() {
        let mut world = World::new();
        world.fill(
            Pos::new(0, 0, 0),
            Pos::new(2, 0, 0),
            Block::new(BlockKind::Solid),
        );
        let input = world.place(BlockKind::Lever, Pos::new(0, 1, 0));
        input.powered = Some(false);
        input.support_offset = Some(Pos::new(0, -1, 0));
        let repeater = world.place(BlockKind::Repeater, Pos::new(1, 1, 0));
        repeater.facing = Some(Facing::East);
        repeater.delay = Some(2);
        let mut simulator = RedstoneTickSimulator::new(world).unwrap();
        simulator.set_powered(Pos::new(0, 1, 0), true).unwrap();
        simulator.advance_tick().unwrap();
        simulator.set_powered(Pos::new(0, 1, 0), false).unwrap();
        assert!(simulator.advance_tick().unwrap().repeater_powered[&Pos::new(1, 1, 0)]);
        assert!(!simulator.advance_tick().unwrap().repeater_powered[&Pos::new(1, 1, 0)]);
    }

    fn comparator_world(mode: &str) -> World {
        let mut world = World::new();
        world.fill(
            Pos::new(0, 0, -3),
            Pos::new(3, 0, 0),
            Block::new(BlockKind::Solid),
        );
        world.set(Pos::new(1, 1, 0), Block::new(BlockKind::RedstoneBlock));
        world.set(Pos::new(2, 1, -3), Block::new(BlockKind::RedstoneBlock));
        world.place(BlockKind::RedstoneWire, Pos::new(2, 1, -2));
        world.place(BlockKind::RedstoneWire, Pos::new(2, 1, -1));
        let comparator = world.place(BlockKind::Comparator, Pos::new(2, 1, 0));
        comparator.facing = Some(Facing::East);
        comparator
            .observed_properties
            .insert("mode".to_owned(), mode.to_owned());
        world.place(BlockKind::RedstoneWire, Pos::new(3, 1, 0));
        update_wire_shapes(&mut world);
        world
    }

    #[test]
    fn comparator_compare_and_subtract_preserve_analog_strength() {
        let mut compare = RedstoneTickSimulator::new(comparator_world("compare")).unwrap();
        assert_eq!(
            compare.advance_tick().unwrap().strength(Pos::new(3, 1, 0)),
            15
        );

        let mut subtract = RedstoneTickSimulator::new(comparator_world("subtract")).unwrap();
        assert_eq!(
            subtract.advance_tick().unwrap().strength(Pos::new(3, 1, 0)),
            1
        );
    }

    #[test]
    fn lamp_turns_on_from_wire_and_uses_delayed_off() {
        let mut world = World::new();
        world.fill(
            Pos::new(0, 0, 0),
            Pos::new(2, 0, 0),
            Block::new(BlockKind::Solid),
        );
        let input = world.place(BlockKind::Button, Pos::new(0, 1, 0));
        input.powered = Some(true);
        input.support_offset = Some(Pos::new(0, -1, 0));
        world.place(BlockKind::RedstoneWire, Pos::new(1, 1, 0));
        world.set(Pos::new(2, 1, 0), Block::new(BlockKind::RedstoneLamp));
        update_wire_shapes(&mut world);
        let mut simulator = RedstoneTickSimulator::new(world).unwrap();
        assert!(simulator.snapshot().lamp_lit[&Pos::new(2, 1, 0)]);
        simulator.set_powered(Pos::new(0, 1, 0), false).unwrap();
        assert!(simulator.advance_tick().unwrap().lamp_lit[&Pos::new(2, 1, 0)]);
        assert!(simulator.advance_tick().unwrap().lamp_lit[&Pos::new(2, 1, 0)]);
        assert!(!simulator.advance_tick().unwrap().lamp_lit[&Pos::new(2, 1, 0)]);
    }

    #[test]
    fn input_mutations_reject_missing_or_non_input_blocks() {
        let mut world = World::new();
        world.set(Pos::new(0, 0, 0), Block::new(BlockKind::Solid));
        world.place(BlockKind::RedstoneWire, Pos::new(0, 1, 0));
        let mut simulator = RedstoneTickSimulator::new(world).unwrap();
        assert!(matches!(
            simulator.set_powered(Pos::new(0, 1, 0), true),
            Err(InputMutationError::WrongKind { .. })
        ));
        assert!(matches!(
            simulator.set_lever_state(Pos::new(9, 9, 9), true),
            Err(InputMutationError::Missing { .. })
        ));
    }

    #[test]
    fn weighted_pressure_plate_level_is_changed_with_its_boolean_state() {
        let mut world = World::new();
        world.set(Pos::new(0, 0, 0), Block::new(BlockKind::Solid));
        world.place(BlockKind::PressurePlate, Pos::new(0, 1, 0));
        let mut simulator = RedstoneTickSimulator::new(world).unwrap();
        simulator
            .set_pressure_plate_level(Pos::new(0, 1, 0), 7)
            .unwrap();
        assert_eq!(simulator.snapshot().power(Pos::new(0, 0, 0)).level(), 7);
        assert!(simulator.snapshot().powered(Pos::new(0, 0, 0)));
    }

    #[test]
    fn rapid_torch_toggling_is_reported_as_a_burnout_candidate() {
        let mut world = World::new();
        world.set(Pos::new(0, 0, 0), Block::new(BlockKind::Solid));
        let lever = world.place(BlockKind::Lever, Pos::new(-1, 0, 0));
        lever.powered = Some(false);
        lever.support_offset = Some(Pos::new(1, 0, 0));
        let torch = world.place(BlockKind::RedstoneTorch, Pos::new(1, 0, 0));
        torch.support_offset = Some(Pos::new(-1, 0, 0));
        let mut simulator = RedstoneTickSimulator::new(world).unwrap();
        for tick in 0..8 {
            simulator
                .set_powered(Pos::new(-1, 0, 0), tick % 2 == 0)
                .unwrap();
            simulator.advance_tick().unwrap();
        }
        assert!(
            simulator
                .snapshot()
                .torch_burnout_candidates
                .contains(&Pos::new(1, 0, 0))
        );
    }
}
