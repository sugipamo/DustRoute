use std::collections::BTreeMap;
use std::error::Error;
use std::fmt::{Display, Formatter};

use crate::connectivity::repeater_output_pos;
use crate::wire::{HORIZONTAL, dust_connected, wire_has_arm};
use crate::world::{BlockKind, Pos, World};

pub const MAX_SIGNAL: u8 = 15;

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
        BlockKind::Lever if block.powered.unwrap_or(false) => MAX_SIGNAL,
        BlockKind::RedstoneTorch if devices.torch_lit.get(&pos).copied().unwrap_or(false) => {
            MAX_SIGNAL
        }
        BlockKind::Repeater if devices.repeater_powered.get(&pos).copied().unwrap_or(false) => {
            MAX_SIGNAL
        }
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
        let target = match block.kind {
            BlockKind::Lever => block.support_pos(*pos),
            BlockKind::Repeater => repeater_output_pos(world, *pos),
            _ => None,
        };
        if let Some(target) =
            target.filter(|target| world.kind_at(*target).properties().receives_strong_power)
        {
            strong
                .entry(target)
                .and_modify(|value| *value = (*value).max(output))
                .or_insert(output);
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
            if world.kind_at(target).properties().receives_weak_power {
                weak.entry(target)
                    .and_modify(|value| *value = (*value).max(level))
                    .or_insert(level);
            }
        }
    }
    world
        .iter()
        .filter(|(_, block)| block.kind.properties().can_be_powered())
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
        BlockKind::RedstoneBlock | BlockKind::Lever | BlockKind::RedstoneTorch => {
            component_output_level(world, neighbor, devices)
        }
        BlockKind::Repeater if repeater_output_pos(world, neighbor) == Some(dust) => {
            component_output_level(world, neighbor, devices)
        }
        kind if kind.properties().strong_power_drives_dust => {
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
                        && dust_connected(world, *dust, other)
                    {
                        best =
                            best.max(signals.get(&other).copied().unwrap_or(0).saturating_sub(1));
                    }
                }
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
    if world.kind_at(pos).properties().repeater_reads_block_power {
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
        .filter(|support| world.kind_at(*support).properties().can_be_powered())
        .is_some_and(|support| state.power(support).powered())
}

#[cfg(test)]
mod tests {
    use crate::wire::update_wire_shapes;
    use crate::world::{Block, Facing};

    use super::*;

    #[test]
    fn redstone_block_is_direct_source_not_stored_power() {
        let mut world = World::new();
        world.set(Pos::new(-1, 0, 0), Block::new(BlockKind::RedstoneBlock));
        world.set(Pos::new(0, 0, 0), Block::new(BlockKind::Solid));
        world.set(Pos::new(-1, -1, 1), Block::new(BlockKind::Solid));
        world.place(BlockKind::RedstoneWire, Pos::new(-1, 0, 1));
        update_wire_shapes(&mut world);
        let state = solve_instantaneous(&world, &DeviceOutputState::default(), 128).unwrap();
        assert_eq!(state.signal(Pos::new(-1, 0, 0)), 15);
        assert_eq!(state.power(Pos::new(0, 0, 0)), PoweredBlockState::default());
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
