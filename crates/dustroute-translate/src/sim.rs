use std::collections::{BTreeMap, VecDeque};

use crate::connectivity::repeater_input_pos;
use crate::electrical::{
    DeviceOutputState, InstantaneousElectricalState, InstantaneousSolveDidNotConverge,
    PoweredBlockState, repeater_input_level, solve_instantaneous, torch_support_is_powered,
};
use crate::world::{BlockKind, Pos, World};

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct TickState {
    pub tick: u64,
    pub strengths: BTreeMap<Pos, u8>,
    pub block_power: BTreeMap<Pos, PoweredBlockState>,
    pub repeater_powered: BTreeMap<Pos, bool>,
    pub torch_lit: BTreeMap<Pos, bool>,
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
        self.strength(pos) > 0 || self.power(pos).powered()
    }
}

pub struct RedstoneTickSimulator {
    world: World,
    tick: u64,
    repeater_powered: BTreeMap<Pos, bool>,
    repeater_queues: BTreeMap<Pos, VecDeque<bool>>,
    torch_lit: BTreeMap<Pos, bool>,
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
                (*pos, VecDeque::from(vec![false; delay]))
            })
            .collect();
        let instantaneous = solve_instantaneous(&world, &devices, 128)?;
        Ok(Self {
            world,
            tick: 0,
            repeater_powered: devices.repeater_powered,
            repeater_queues,
            torch_lit: devices.torch_lit,
            instantaneous,
        })
    }

    fn devices(&self) -> DeviceOutputState {
        DeviceOutputState {
            repeater_powered: self.repeater_powered.clone(),
            torch_lit: self.torch_lit.clone(),
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
            instantaneous_iterations: self.instantaneous.iterations,
        }
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
            let queue = self
                .repeater_queues
                .get_mut(pos)
                .expect("repeater queue initialized");
            queue.pop_front();
            queue.push_back(requested);
            next_repeaters.insert(*pos, queue.front().copied().unwrap_or(requested));
        }
        let next_torches = self
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
        self.repeater_powered = next_repeaters;
        self.torch_lit = next_torches;
        self.tick += 1;
        self.settle_instantaneous()
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
}
