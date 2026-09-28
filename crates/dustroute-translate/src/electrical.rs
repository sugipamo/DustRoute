use std::collections::BTreeMap;
use std::error::Error;
use std::fmt::{Display, Formatter};

use crate::dust_law::{DustLaw, builtin_dust_law};
use crate::wire::resolved_wire_connection;
use crate::wire::{HORIZONTAL, dust_transmits, wire_has_arm};
use crate::world::{BlockKind, Pos, World};
use dustroute_minecraft::spatial::{DirectPower, WeakTarget, builtin_spatial_laws};

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
    pub observer_powered: BTreeMap<Pos, bool>,
}

impl DeviceOutputState {
    #[must_use]
    pub fn initially_lit(world: &World) -> Self {
        Self {
            repeater_powered: world
                .iter()
                .filter(|(_, block)| block.kind == BlockKind::Repeater)
                .map(|(pos, block)| (*pos, block.powered.unwrap_or(false)))
                .collect(),
            torch_lit: world
                .iter()
                .filter(|(_, block)| block.kind == BlockKind::RedstoneTorch)
                .map(|(pos, block)| (*pos, block.powered.unwrap_or(true)))
                .collect(),
            comparator_output: world
                .iter()
                .filter(|(_, block)| block.kind == BlockKind::Comparator)
                .map(|(pos, block)| {
                    (
                        *pos,
                        block
                            .power_level
                            .unwrap_or_else(|| u8::from(block.powered.unwrap_or(false)) * 15),
                    )
                })
                .collect(),
            observer_powered: world
                .iter()
                .filter(|(_, block)| block.kind == BlockKind::Observer)
                .map(|(pos, block)| (*pos, block.powered.unwrap_or(false)))
                .collect(),
        }
    }
}

/// Immutable world topology reused by the tick simulator.
///
/// Device state changes on every tick, but the set of observed positions,
/// wires, conductors, source targets, and wire weak-power targets does not.
/// Keeping those derived lists outside `solve_instantaneous` avoids rebuilding
/// them for every fixed-point solve while preserving the existing public
/// `solve_instantaneous` entry point for one-shot callers.
#[derive(Clone, Debug)]
pub(crate) struct ElectricalTopology {
    positions: Vec<Pos>,
    wires: Vec<Pos>,
    conductive_positions: Vec<Pos>,
    power_sources: Vec<PowerSource>,
    wire_weak_targets: Vec<Vec<Pos>>,
}

#[derive(Clone, Debug)]
struct PowerSource {
    pos: Pos,
    strong_targets: Vec<Pos>,
}

impl ElectricalTopology {
    pub(crate) fn from_world(world: &World) -> Self {
        let positions: Vec<_> = world.positions().collect();
        let wires: Vec<_> = positions
            .iter()
            .copied()
            .filter(|pos| world.kind_at(*pos) == BlockKind::RedstoneWire)
            .collect();
        let conductive_positions: Vec<_> = positions
            .iter()
            .copied()
            .filter(|pos| {
                world.get(*pos).is_some_and(|block| {
                    let traits = block.redstone_traits();
                    traits.conducts_weak_power || traits.conducts_strong_power
                })
            })
            .collect();
        let power_sources: Vec<_> = positions
            .iter()
            .filter_map(|pos| {
                let targets = strong_output_targets(world, *pos)?;
                let strong_targets = targets
                    .into_iter()
                    .filter(|target| {
                        world
                            .get(*target)
                            .is_some_and(|block| block.redstone_traits().conducts_strong_power)
                    })
                    .collect();
                Some(PowerSource {
                    pos: *pos,
                    strong_targets,
                })
            })
            .collect();
        let wire_weak_targets: Vec<_> = wires
            .iter()
            .map(|pos| {
                dust_weak_power_targets(world, *pos)
                    .into_iter()
                    .filter(|target| {
                        world
                            .get(*target)
                            .is_some_and(|block| block.redstone_traits().conducts_weak_power)
                    })
                    .collect()
            })
            .collect();
        Self {
            positions,
            wires,
            conductive_positions,
            power_sources,
            wire_weak_targets,
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
pub enum ElectricalSolveError {
    DidNotConverge { max_iterations: usize },
    UnsupportedBlock { position: Pos, kind: BlockKind },
}

impl Display for ElectricalSolveError {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::DidNotConverge { max_iterations } => write!(
                f,
                "instantaneous network did not converge in {max_iterations} iterations"
            ),
            Self::UnsupportedBlock { position, kind } => write!(
                f,
                "compatibility electrical model does not admit {kind:?} at {position:?}"
            ),
        }
    }
}

pub(crate) fn validate_compatibility_kinds(world: &World) -> Result<(), ElectricalSolveError> {
    for (position, block) in world.iter() {
        if !dustroute_minecraft::execution_context::WorldExecutionProfile::RedstoneCompatibilityBoundaryV1.admits_kind(block.kind) {
            return Err(ElectricalSolveError::UnsupportedBlock { position: *position, kind: block.kind });
        }
    }
    Ok(())
}

impl Error for ElectricalSolveError {}

/// Potential strong-power targets shared by simulation and physical route checks.
/// Current ON/OFF state controls the signal level, not connectivity.
pub(crate) fn strong_output_targets(world: &World, pos: Pos) -> Option<Vec<Pos>> {
    builtin_spatial_laws().strong_output_targets(world, pos)
}

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
        BlockKind::Observer if devices.observer_powered.get(&pos).copied().unwrap_or(false) => {
            MAX_SIGNAL
        }
        _ => 0,
    }
}

/// Potential weak-power targets, independent of the wire's current level.
/// Route checks and electrical execution must use the same arm geometry.
pub(crate) fn dust_weak_power_targets(world: &World, pos: Pos) -> Vec<Pos> {
    let laws = builtin_spatial_laws();
    let mut targets = Vec::new();
    if laws.weakly_powers(WeakTarget::Below, crate::world::WireConnection::None) {
        targets.push(pos.offset(0, -1, 0));
    }
    for facing in HORIZONTAL {
        if laws.weakly_powers(
            WeakTarget::Horizontal,
            resolved_wire_connection(world, pos, facing),
        ) {
            let delta = facing.horizontal_offset().expect("horizontal facing");
            targets.push(pos.offset(delta.x, 0, delta.z));
        }
    }
    targets
}

fn compute_powered_blocks(
    world: &World,
    topology: &ElectricalTopology,
    signals: &BTreeMap<Pos, u8>,
    devices: &DeviceOutputState,
) -> BTreeMap<Pos, PoweredBlockState> {
    let mut weak = BTreeMap::<Pos, u8>::new();
    let mut strong = BTreeMap::<Pos, u8>::new();
    for source in &topology.power_sources {
        let output = component_output_level(world, source.pos, devices);
        if output == 0 {
            continue;
        }
        for target in &source.strong_targets {
            strong
                .entry(*target)
                .and_modify(|value| *value = (*value).max(output))
                .or_insert(output);
        }
    }
    for (source, level) in &strong {
        for delta in ADJACENT {
            let target = source.offset(delta.x, delta.y, delta.z);
            if world.get(target).is_some_and(|block| {
                builtin_spatial_laws().accepts_relayed_weak_power(block.kind)
                    && block.redstone_traits().conducts_weak_power
            }) {
                weak.entry(target)
                    .and_modify(|value| *value = (*value).max(*level))
                    .or_insert(*level);
            }
        }
    }
    for (pos, targets) in topology.wires.iter().zip(&topology.wire_weak_targets) {
        let level = signals.get(pos).copied().unwrap_or(0);
        if level == 0 {
            continue;
        }
        for target in targets {
            weak.entry(*target)
                .and_modify(|value| *value = (*value).max(level))
                .or_insert(level);
        }
    }
    topology
        .conductive_positions
        .iter()
        .map(|pos| {
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
    let Some(block) = world.get(neighbor) else {
        return 0;
    };
    let direct = match builtin_spatial_laws().direct_power(block.kind) {
        DirectPower::Adjacent => true,
        DirectPower::ForwardHorizontal => block
            .facing
            .and_then(|f| f.horizontal_offset())
            .is_some_and(|d| neighbor.offset(d.x, d.y, d.z) == dust),
        DirectPower::ForwardAny => block
            .facing
            .map(|f| f.offset())
            .is_some_and(|d| neighbor.offset(d.x, d.y, d.z) == dust),
        DirectPower::None => false,
    };
    if direct {
        component_output_level(world, neighbor, devices)
    } else if block.redstone_traits().strong_power_drives_dust {
        block_power
            .get(&neighbor)
            .copied()
            .unwrap_or_default()
            .strong
    } else {
        0
    }
}

pub fn solve_instantaneous(
    world: &World,
    devices: &DeviceOutputState,
    max_iterations: usize,
) -> Result<InstantaneousElectricalState, ElectricalSolveError> {
    validate_compatibility_kinds(world)?;
    let topology = ElectricalTopology::from_world(world);
    solve_instantaneous_with_topology(world, devices, max_iterations, &topology)
}

pub(crate) fn solve_instantaneous_with_topology(
    world: &World,
    devices: &DeviceOutputState,
    max_iterations: usize,
    topology: &ElectricalTopology,
) -> Result<InstantaneousElectricalState, ElectricalSolveError> {
    solve_instantaneous_with_law(world, devices, max_iterations, topology, builtin_dust_law())
}

pub(crate) fn solve_instantaneous_with_law(
    world: &World,
    devices: &DeviceOutputState,
    max_iterations: usize,
    topology: &ElectricalTopology,
    dust_law: &DustLaw,
) -> Result<InstantaneousElectricalState, ElectricalSolveError> {
    validate_compatibility_kinds(world)?;
    let mut signals: BTreeMap<_, _> = topology
        .positions
        .iter()
        .map(|pos| (*pos, component_output_level(world, *pos, devices)))
        .collect();
    let mut block_power = BTreeMap::new();
    for iteration in 1..=max_iterations {
        let next_block_power = compute_powered_blocks(world, topology, &signals, devices);
        let mut next_signals: BTreeMap<_, _> = topology
            .positions
            .iter()
            .map(|pos| (*pos, component_output_level(world, *pos, devices)))
            .collect();
        for dust in &topology.wires {
            let mut neighboring = 0;
            let mut direct = 0;
            for facing in HORIZONTAL {
                let delta = facing.horizontal_offset().expect("horizontal facing");
                for dy in -1..=1 {
                    let other = dust.offset(delta.x, dy, delta.z);
                    if world.kind_at(other) == BlockKind::RedstoneWire
                        && dust_transmits(world, other, *dust)
                    {
                        neighboring = neighboring.max(signals.get(&other).copied().unwrap_or(0));
                    }
                }
            }
            for dy in [-1, 1] {
                let neighbor = dust.offset(0, dy, 0);
                direct = direct.max(direct_level_into_dust(
                    world,
                    *dust,
                    neighbor,
                    &next_block_power,
                    devices,
                ));
            }
            for facing in HORIZONTAL {
                let delta = facing.horizontal_offset().expect("horizontal facing");
                let neighbor = dust.offset(delta.x, 0, delta.z);
                let receives_through_arm = wire_has_arm(world, *dust, facing);
                let receives_from_strong_block = world.get(neighbor).is_some_and(|block| {
                    block.redstone_traits().strong_power_drives_dust
                        && next_block_power
                            .get(&neighbor)
                            .is_some_and(|power| power.strong > 0)
                });
                if !receives_through_arm && !receives_from_strong_block {
                    continue;
                }
                direct = direct.max(direct_level_into_dust(
                    world,
                    *dust,
                    neighbor,
                    &next_block_power,
                    devices,
                ));
            }
            next_signals.insert(*dust, dust_law.strength(direct, neighboring));
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
    Err(ElectricalSolveError::DidNotConverge { max_iterations })
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
    fn lit_torch_strongly_powers_block_above_and_dust_on_top() {
        let mut world = World::new();
        world.set(Pos::new(1, 0, 0), Block::new(BlockKind::Solid));
        let torch = world.place(BlockKind::RedstoneTorch, Pos::new(0, 0, 0));
        torch.facing = Some(Facing::West);
        torch.support_offset = Some(Pos::new(1, 0, 0));
        world.set(Pos::new(0, 1, 0), Block::new(BlockKind::Solid));
        world.place(BlockKind::RedstoneWire, Pos::new(0, 2, 0));
        update_wire_shapes(&mut world);

        let devices = DeviceOutputState::initially_lit(&world);
        let state = solve_instantaneous(&world, &devices, 128).unwrap();

        assert_eq!(state.power(Pos::new(0, 1, 0)).strong, 15);
        assert_eq!(state.signal(Pos::new(0, 2, 0)), 15);
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
    fn strong_power_does_not_chain_into_an_adjacent_solid_conductor() {
        let mut world = World::new();
        world.set(Pos::new(0, 0, 0), Block::new(BlockKind::RedstoneBlock));
        world.set(Pos::new(1, 0, 0), Block::new(BlockKind::Solid));
        world.set(Pos::new(2, 0, 0), Block::new(BlockKind::Solid));
        let state = solve_instantaneous(&world, &DeviceOutputState::default(), 128).unwrap();
        assert_eq!(state.power(Pos::new(1, 0, 0)).strong, 15);
        assert_eq!(state.power(Pos::new(2, 0, 0)), PoweredBlockState::default());
    }

    #[test]
    fn dust_reads_adjacent_strong_block_without_visual_arm() {
        let mut world = World::new();
        world.set(Pos::new(0, -1, 0), Block::new(BlockKind::Solid));
        world.place(BlockKind::RedstoneWire, Pos::new(0, 0, 0));
        world.set(Pos::new(1, 0, 0), Block::new(BlockKind::Solid));
        world.set(Pos::new(1, -1, 0), Block::new(BlockKind::RedstoneTorch));
        update_wire_shapes(&mut world);
        assert!(!wire_has_arm(&world, Pos::new(0, 0, 0), Facing::East));

        let devices = DeviceOutputState::initially_lit(&world);
        let state = solve_instantaneous(&world, &devices, 128).unwrap();
        assert_eq!(state.power(Pos::new(1, 0, 0)).strong, 15);
        assert_eq!(state.signal(Pos::new(0, 0, 0)), 15);
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
