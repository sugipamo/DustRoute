use std::collections::BTreeMap;
use std::error::Error;
use std::fmt::{Display, Formatter};

use crate::connectivity::{comparator_output_pos, repeater_output_pos};
use crate::wire::{HORIZONTAL, dust_transmits, wire_has_arm};
use crate::world::{BlockKind, Pos, World};

pub const MAX_SIGNAL: u8 = 15;
const ADJACENT: [Pos; 6] = [
    Pos::new(1, 0, 0),
    Pos::new(-1, 0, 0),
    Pos::new(0, 1, 0),
    Pos::new(0, -1, 0),
    Pos::new(0, 0, 1),
    Pos::new(0, 0, -1),
];

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct PoweredBlockState {
    pub weak: u8,
    pub strong: u8,
}

impl PoweredBlockState {
    #[must_use]
    pub fn level(self) -> u8 {
        self.weak.max(self.strong)
    }

    #[must_use]
    pub fn powered(self) -> bool {
        self.level() > 0
    }
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct DeviceOutputState {
    pub repeater_powered: BTreeMap<Pos, bool>,
    pub torch_lit: BTreeMap<Pos, bool>,
    pub comparator_output: BTreeMap<Pos, u8>,
}

impl DeviceOutputState {
    #[must_use]
    pub fn initially_lit(world: &World) -> Self {
        Self {
            repeater_powered: world
                .iter()
                .filter(|(_, block)| block.kind == BlockKind::Repeater)
                .map(|(pos, _)| (*pos, false))
                .collect(),
            torch_lit: world
                .iter()
                .filter(|(_, block)| block.kind == BlockKind::RedstoneTorch)
                .map(|(pos, _)| (*pos, true))
                .collect(),
            comparator_output: world
                .iter()
                .filter(|(_, block)| block.kind == BlockKind::Comparator)
                .map(|(pos, _)| (*pos, 0))
                .collect(),
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct InstantaneousElectricalState {
    pub signal_levels: BTreeMap<Pos, u8>,
    pub block_power: BTreeMap<Pos, PoweredBlockState>,
    pub iterations: usize,
}

impl InstantaneousElectricalState {
    #[must_use]
    pub fn signal(&self, pos: Pos) -> u8 {
        self.signal_levels.get(&pos).copied().unwrap_or(0)
    }

    #[must_use]
    pub fn power(&self, pos: Pos) -> PoweredBlockState {
        self.block_power.get(&pos).copied().unwrap_or_default()
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct InstantaneousSolveDidNotConverge {
    pub max_iterations: usize,
}

impl Display for InstantaneousSolveDidNotConverge {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "instantaneous network did not converge in {} iterations",
            self.max_iterations
        )
    }
}

impl Error for InstantaneousSolveDidNotConverge {}

fn component_output_level(world: &World, pos: Pos, devices: &DeviceOutputState) -> u8 {
    let Some(block) = world.get(pos) else {
        return 0;
    };
    match block.kind {
        BlockKind::RedstoneBlock => MAX_SIGNAL,
        BlockKind::Lever | BlockKind::Button | BlockKind::PressurePlate
            if block.powered.unwrap_or(false) =>
        {
            block.power_level.unwrap_or(MAX_SIGNAL).min(MAX_SIGNAL)
        }
        BlockKind::RedstoneTorch if devices.torch_lit.get(&pos).copied().unwrap_or(false) => {
            MAX_SIGNAL
        }
        BlockKind::Repeater if devices.repeater_powered.get(&pos).copied().unwrap_or(false) => {
            MAX_SIGNAL
        }
        BlockKind::Comparator => devices.comparator_output.get(&pos).copied().unwrap_or(0),
        _ => 0,
    }
}

fn dust_weak_power_targets(world: &World, pos: Pos) -> Vec<Pos> {
    let mut targets = vec![pos.offset(0, -1, 0)];
    for facing in HORIZONTAL {
        if wire_has_arm(world, pos, facing) {
            let delta = facing.horizontal_offset().expect("horizontal facing");
            targets.push(pos.offset(delta.x, 0, delta.z));
        }
    }
    targets
}

fn compute_powered_blocks(
    world: &World,
    signals: &BTreeMap<Pos, u8>,
    devices: &DeviceOutputState,
) -> BTreeMap<Pos, PoweredBlockState> {
    let mut weak = BTreeMap::<Pos, u8>::new();
    let mut strong = BTreeMap::<Pos, u8>::new();
    for (pos, block) in world.iter() {
        let output = component_output_level(world, *pos, devices);
        if output == 0 {
            continue;
        }
        let targets: Vec<_> = match block.kind {
            BlockKind::Lever | BlockKind::Button | BlockKind::PressurePlate => {
                block.support_pos(*pos).into_iter().collect()
            }
            BlockKind::Repeater => repeater_output_pos(world, *pos).into_iter().collect(),
            BlockKind::Comparator => comparator_output_pos(world, *pos).into_iter().collect(),
            BlockKind::RedstoneBlock => ADJACENT
                .into_iter()
                .map(|delta| pos.offset(delta.x, delta.y, delta.z))
                .collect(),
            _ => Vec::new(),
        };
        for target in targets.into_iter().filter(|target| {
            world
                .get(*target)
                .is_some_and(|block| block.redstone_traits().conducts_strong_power)
        }) {
            strong
                .entry(target)
                .and_modify(|value| *value = (*value).max(output))
                .or_insert(output);
        }
    }
    for (source, level) in strong.clone() {
        for delta in ADJACENT {
            let target = source.offset(delta.x, delta.y, delta.z);
            if world
                .get(target)
                .is_some_and(|block| block.redstone_traits().conducts_weak_power)
            {
                weak.entry(target)
                    .and_modify(|value| *value = (*value).max(level))
                    .or_insert(level);
            }
        }
    }
    for (pos, block) in world.iter() {
        if block.kind != BlockKind::RedstoneWire {
            continue;
        }
        let level = signals.get(pos).copied().unwrap_or(0);
        if level == 0 {
            continue;
        }
        for target in dust_weak_power_targets(world, *pos) {
            if world
                .get(target)
                .is_some_and(|block| block.redstone_traits().conducts_weak_power)
            {
                weak.entry(target)
                    .and_modify(|value| *value = (*value).max(level))
                    .or_insert(level);
            }
        }
    }
    world
        .iter()
        .filter(|(_, block)| {
            let traits = block.redstone_traits();
            traits.conducts_weak_power || traits.conducts_strong_power
        })
        .map(|(pos, _)| {
            (
                *pos,
                PoweredBlockState {
                    weak: weak.get(pos).copied().unwrap_or(0),
                    strong: strong.get(pos).copied().unwrap_or(0),
                },
            )
        })
        .collect()
}

fn direct_level_into_dust(
    world: &World,
    dust: Pos,
    neighbor: Pos,
    block_power: &BTreeMap<Pos, PoweredBlockState>,
    devices: &DeviceOutputState,
) -> u8 {
    match world.kind_at(neighbor) {
        BlockKind::RedstoneBlock
        | BlockKind::Lever
        | BlockKind::Button
        | BlockKind::PressurePlate
        | BlockKind::RedstoneTorch => component_output_level(world, neighbor, devices),
        BlockKind::Repeater if repeater_output_pos(world, neighbor) == Some(dust) => {
            component_output_level(world, neighbor, devices)
        }
        BlockKind::Comparator if comparator_output_pos(world, neighbor) == Some(dust) => {
            component_output_level(world, neighbor, devices)
        }
        _ if world
            .get(neighbor)
            .is_some_and(|block| block.redstone_traits().strong_power_drives_dust) =>
        {
            block_power
                .get(&neighbor)
                .copied()
                .unwrap_or_default()
                .strong
        }
        _ => 0,
    }
}

pub fn solve_instantaneous(
    world: &World,
    devices: &DeviceOutputState,
    max_iterations: usize,
) -> Result<InstantaneousElectricalState, InstantaneousSolveDidNotConverge> {
    let positions: Vec<_> = world.positions().collect();
    let wires: Vec<_> = world
        .iter()
        .filter(|(_, block)| block.kind == BlockKind::RedstoneWire)
        .map(|(pos, _)| *pos)
        .collect();
    let mut signals: BTreeMap<_, _> = positions
        .iter()
        .map(|pos| (*pos, component_output_level(world, *pos, devices)))
        .collect();
    let mut block_power = BTreeMap::new();
    for iteration in 1..=max_iterations {
        let next_block_power = compute_powered_blocks(world, &signals, devices);
        let mut next_signals: BTreeMap<_, _> = positions
            .iter()
            .map(|pos| (*pos, component_output_level(world, *pos, devices)))
            .collect();
        for dust in &wires {
            let mut best = 0;
            for facing in HORIZONTAL {
                let delta = facing.horizontal_offset().expect("horizontal facing");
                for dy in -1..=1 {
                    let other = dust.offset(delta.x, dy, delta.z);
                    if world.kind_at(other) == BlockKind::RedstoneWire
                        && dust_transmits(world, other, *dust)
                    {
                        best =
                            best.max(signals.get(&other).copied().unwrap_or(0).saturating_sub(1));
                    }
                }
            }
            for dy in [-1, 1] {
                let neighbor = dust.offset(0, dy, 0);
                best = best.max(direct_level_into_dust(
                    world,
                    *dust,
                    neighbor,
                    &next_block_power,
                    devices,
                ));
            }
            for facing in HORIZONTAL {
                if !wire_has_arm(world, *dust, facing) {
                    continue;
                }
                let delta = facing.horizontal_offset().expect("horizontal facing");
                let neighbor = dust.offset(delta.x, 0, delta.z);
                best = best.max(direct_level_into_dust(
                    world,
                    *dust,
                    neighbor,
                    &next_block_power,
                    devices,
                ));
            }
            next_signals.insert(*dust, best.min(MAX_SIGNAL));
        }
        if next_signals == signals && next_block_power == block_power {
            return Ok(InstantaneousElectricalState {
                signal_levels: next_signals,
                block_power: next_block_power,
                iterations: iteration,
            });
        }
        signals = next_signals;
        block_power = next_block_power;
    }
    Err(InstantaneousSolveDidNotConverge { max_iterations })
}

#[must_use]
pub fn repeater_input_level(world: &World, pos: Pos, state: &InstantaneousElectricalState) -> u8 {
    if world
        .get(pos)
        .is_some_and(|block| block.kind.properties().repeater_reads_block_power)
    {
        state.power(pos).level()
    } else {
        state.signal(pos)
    }
}

#[must_use]
pub fn torch_support_is_powered(
    world: &World,
    pos: Pos,
    state: &InstantaneousElectricalState,
) -> bool {
    world
        .get(pos)
        .and_then(|block| block.support_pos(pos))
        .filter(|support| {
            world.get(*support).is_some_and(|block| {
                let traits = block.redstone_traits();
                traits.conducts_weak_power || traits.conducts_strong_power
            })
        })
        .is_some_and(|support| state.power(support).powered())
}

#[cfg(test)]
mod tests {
    use crate::wire::update_wire_shapes;
    use crate::world::{Block, Facing};

    use super::*;

    fn glass() -> Block {
        let mut block = Block::new(BlockKind::Transparent);
        block.observed_name = Some("minecraft:glass".to_owned());
        block
    }

    #[test]
    fn redstone_block_strongly_powers_adjacent_conductor() {
        let mut world = World::new();
        world.set(Pos::new(-1, 0, 0), Block::new(BlockKind::RedstoneBlock));
        world.set(Pos::new(0, 0, 0), Block::new(BlockKind::Solid));
        world.set(Pos::new(-1, -1, 1), Block::new(BlockKind::Solid));
        world.place(BlockKind::RedstoneWire, Pos::new(-1, 0, 1));
        update_wire_shapes(&mut world);
        let state = solve_instantaneous(&world, &DeviceOutputState::default(), 128).unwrap();
        assert_eq!(state.signal(Pos::new(-1, 0, 0)), 15);
        assert_eq!(
            state.power(Pos::new(0, 0, 0)),
            PoweredBlockState {
                weak: 0,
                strong: 15
            }
        );
        assert_eq!(state.signal(Pos::new(-1, 0, 1)), 15);
    }

    #[test]
    fn powered_lever_strongly_powers_support() {
        let mut world = World::new();
        world.set(Pos::new(0, 0, 0), Block::new(BlockKind::Solid));
        let lever = world.place(BlockKind::Lever, Pos::new(-1, 0, 0));
        lever.facing = Some(Facing::East);
        lever.powered = Some(true);
        lever.support_offset = Some(Pos::new(1, 0, 0));
        let state = solve_instantaneous(&world, &DeviceOutputState::default(), 128).unwrap();
        assert_eq!(
            state.power(Pos::new(0, 0, 0)),
            PoweredBlockState {
                weak: 0,
                strong: 15
            }
        );
    }

    #[test]
    fn strong_powered_conductor_drives_adjacent_receiver() {
        let mut world = World::new();
        world.set(Pos::new(0, 0, 0), Block::new(BlockKind::RedstoneBlock));
        world.set(Pos::new(1, 0, 0), Block::new(BlockKind::Solid));
        world.set(Pos::new(2, 0, 0), Block::new(BlockKind::RedstoneLamp));
        let state = solve_instantaneous(&world, &DeviceOutputState::default(), 128).unwrap();
        assert_eq!(state.power(Pos::new(1, 0, 0)).strong, 15);
        assert_eq!(state.power(Pos::new(2, 0, 0)).weak, 15);
    }

    #[test]
    fn weighted_pressure_plate_preserves_analog_output() {
        let mut world = World::new();
        world.fill(
            Pos::new(0, 0, 0),
            Pos::new(1, 0, 0),
            Block::new(BlockKind::Solid),
        );
        let plate = world.place(BlockKind::PressurePlate, Pos::new(0, 1, 0));
        plate.powered = Some(true);
        plate.power_level = Some(7);
        plate.support_offset = Some(Pos::new(0, -1, 0));
        world.place(BlockKind::RedstoneWire, Pos::new(1, 1, 0));
        update_wire_shapes(&mut world);
        let state = solve_instantaneous(&world, &DeviceOutputState::default(), 128).unwrap();
        assert_eq!(state.signal(Pos::new(1, 1, 0)), 7);
    }

    #[test]
    fn dust_strength_decays() {
        let mut world = World::new();
        world.fill(
            Pos::new(0, 0, 0),
            Pos::new(3, 0, 0),
            Block::new(BlockKind::Solid),
        );
        world.set(Pos::new(0, 1, 0), Block::new(BlockKind::RedstoneBlock));
        for x in 1..=3 {
            world.place(BlockKind::RedstoneWire, Pos::new(x, 1, 0));
        }
        update_wire_shapes(&mut world);
        let state = solve_instantaneous(&world, &DeviceOutputState::default(), 128).unwrap();
        assert_eq!(state.signal(Pos::new(1, 1, 0)), 15);
        assert_eq!(state.signal(Pos::new(3, 1, 0)), 13);
    }

    #[test]
    fn glass_stair_carries_strength_up_but_not_down() {
        let mut upward = World::new();
        upward.set(Pos::new(0, 0, 0), Block::new(BlockKind::Solid));
        upward.set(Pos::new(1, 1, 0), glass());
        upward.set(Pos::new(-1, 1, 0), Block::new(BlockKind::RedstoneBlock));
        upward.place(BlockKind::RedstoneWire, Pos::new(0, 1, 0));
        upward.place(BlockKind::RedstoneWire, Pos::new(1, 2, 0));
        update_wire_shapes(&mut upward);
        let state = solve_instantaneous(&upward, &DeviceOutputState::default(), 128).unwrap();
        assert_eq!(state.signal(Pos::new(0, 1, 0)), 15);
        assert_eq!(state.signal(Pos::new(1, 2, 0)), 14);

        let mut downward = World::new();
        downward.set(Pos::new(0, 0, 0), Block::new(BlockKind::Solid));
        downward.set(Pos::new(1, 1, 0), glass());
        downward.set(Pos::new(2, 2, 0), Block::new(BlockKind::RedstoneBlock));
        downward.place(BlockKind::RedstoneWire, Pos::new(0, 1, 0));
        downward.place(BlockKind::RedstoneWire, Pos::new(1, 2, 0));
        update_wire_shapes(&mut downward);
        let state = solve_instantaneous(&downward, &DeviceOutputState::default(), 128).unwrap();
        assert_eq!(state.signal(Pos::new(1, 2, 0)), 15);
        assert_eq!(state.signal(Pos::new(0, 1, 0)), 0);
    }

    #[test]
    fn repeater_reads_weakly_powered_block() {
        let mut world = World::new();
        world.set(Pos::new(-1, 0, 0), Block::new(BlockKind::Solid));
        world
            .place(BlockKind::RedstoneWire, Pos::new(-1, 1, 0))
            .wire_connections = Some(
            [
                (Facing::East, crate::world::WireConnection::Side),
                (Facing::West, crate::world::WireConnection::Side),
            ]
            .into_iter()
            .collect(),
        );
        world.set(Pos::new(0, 1, 0), Block::new(BlockKind::Solid));
        let repeater = world.place(BlockKind::Repeater, Pos::new(1, 1, 0));
        repeater.facing = Some(Facing::East);
        world.set(Pos::new(-2, 1, 0), Block::new(BlockKind::RedstoneBlock));
        let state = solve_instantaneous(&world, &DeviceOutputState::default(), 128).unwrap();
        let input = crate::connectivity::repeater_input_pos(&world, Pos::new(1, 1, 0)).unwrap();
        assert!(repeater_input_level(&world, input, &state) > 0);
    }
}
