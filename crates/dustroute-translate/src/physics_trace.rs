use std::collections::{BTreeMap, BTreeSet};

use serde::{Deserialize, Serialize};

use crate::{BlockKind, PhysicalCell, Pos, TickState, World};

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum TraceSource {
    MinecraftJava,
    DustRouteSimulator,
}

/// One normalized observation of a physical block. `None` means that the
/// source cannot observe that property; it does not mean zero or false.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct PhysicalBlockObservation {
    pub redstone_tick: u64,
    pub position: Pos,
    pub block_kind: BlockKind,
    #[serde(default)]
    pub wire_connections: Option<BTreeMap<String, String>>,
    pub dust_strength: Option<u8>,
    pub powered: Option<bool>,
    pub torch_lit: Option<bool>,
    pub weak_power: Option<u8>,
    pub strong_power: Option<u8>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct PhysicalTrace {
    pub source: TraceSource,
    pub observations: Vec<PhysicalBlockObservation>,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, Ord, PartialEq, PartialOrd, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum PhysicalProperty {
    BlockKind,
    WireConnections,
    DustStrength,
    Powered,
    TorchLit,
    WeakPower,
    StrongPower,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(untagged)]
pub enum PhysicalValue {
    BlockKind(BlockKind),
    Connections(BTreeMap<String, String>),
    Bool(bool),
    Level(u8),
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct PhysicalTraceMismatch {
    pub redstone_tick: u64,
    pub position: Pos,
    pub property: PhysicalProperty,
    pub minecraft: PhysicalValue,
    pub simulator: PhysicalValue,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct PhysicalTraceComparison {
    pub compared_observations: usize,
    pub first_mismatch: Option<PhysicalTraceMismatch>,
}

#[must_use]
pub fn simulator_observations(
    world: &World,
    state: &TickState,
    positions: &BTreeSet<Pos>,
) -> Vec<PhysicalBlockObservation> {
    positions
        .iter()
        .filter_map(|position| {
            let block = world.get(*position)?;
            let power = state.power(*position);
            Some(PhysicalBlockObservation {
                redstone_tick: state.tick,
                position: *position,
                block_kind: block.kind,
                wire_connections: (block.kind == BlockKind::RedstoneWire).then(|| {
                    [
                        crate::Facing::North,
                        crate::Facing::East,
                        crate::Facing::South,
                        crate::Facing::West,
                    ]
                    .into_iter()
                    .map(|facing| {
                        let connection = block
                            .wire_connections
                            .as_ref()
                            .and_then(|connections| connections.get(&facing))
                            .copied()
                            .unwrap_or(crate::WireConnection::None);
                        (
                            format!("{facing:?}").to_ascii_lowercase(),
                            format!("{connection:?}").to_ascii_lowercase(),
                        )
                    })
                    .collect()
                }),
                dust_strength: (block.kind == BlockKind::RedstoneWire)
                    .then(|| state.strength(*position)),
                powered: match block.kind {
                    BlockKind::Lever | BlockKind::Button | BlockKind::PressurePlate => {
                        block.powered
                    }
                    BlockKind::Repeater => state.repeater_powered.get(position).copied(),
                    BlockKind::Comparator => state
                        .comparator_output
                        .get(position)
                        .map(|level| *level > 0),
                    BlockKind::RedstoneLamp => state.lamp_lit.get(position).copied(),
                    BlockKind::Observer => state.observer_powered.get(position).copied(),
                    _ => None,
                },
                torch_lit: (block.kind == BlockKind::RedstoneTorch)
                    .then(|| state.torch_lit.get(position).copied())
                    .flatten(),
                weak_power: Some(power.weak),
                strong_power: Some(power.strong),
            })
        })
        .collect()
}

pub fn simulate_world_trace(
    world: World,
    positions: &BTreeSet<Pos>,
    settle_redstone_ticks: usize,
    duration_redstone_ticks: u64,
) -> Result<PhysicalTrace, String> {
    let observed_world = world.clone();
    let mut simulator =
        crate::RedstoneTickSimulator::new(world).map_err(|error| error.to_string())?;
    let mut state = simulator
        .settle_ticks(settle_redstone_ticks)
        .map_err(|error| error.to_string())?;
    let mut observations = Vec::new();
    for relative_tick in 0..=duration_redstone_ticks {
        let mut tick_observations = simulator_observations(&observed_world, &state, positions);
        for observation in &mut tick_observations {
            observation.redstone_tick = relative_tick;
        }
        observations.extend(tick_observations);
        if relative_tick < duration_redstone_ticks {
            state = simulator
                .advance_tick()
                .map_err(|error| error.to_string())?;
        }
    }
    Ok(PhysicalTrace {
        source: TraceSource::DustRouteSimulator,
        observations,
    })
}

pub fn simulate_cell_trace(
    cell: &PhysicalCell,
    inputs: &[bool],
    positions: &BTreeSet<Pos>,
    settle_redstone_ticks: usize,
    duration_redstone_ticks: u64,
) -> Result<PhysicalTrace, String> {
    if inputs.len() != cell.inputs.len() {
        return Err(format!(
            "expected {} cell inputs, got {}",
            cell.inputs.len(),
            inputs.len()
        ));
    }
    let mut world = cell.world.clone();
    for (port, value) in cell.inputs.iter().zip(inputs) {
        crate::cell_library::drive_input(&mut world, port, *value);
    }
    crate::wire::update_wire_shapes(&mut world);
    simulate_world_trace(
        world,
        positions,
        settle_redstone_ticks,
        duration_redstone_ticks,
    )
}

/// Compares only properties observed by both sources. Missing observations are
/// intentionally not converted to defaults.
#[must_use]
pub fn compare_physical_traces(
    minecraft: &PhysicalTrace,
    simulator: &PhysicalTrace,
) -> PhysicalTraceComparison {
    let simulator_by_key: BTreeMap<_, _> = simulator
        .observations
        .iter()
        .map(|item| ((item.redstone_tick, item.position), item))
        .collect();
    let minecraft_by_key: BTreeMap<_, _> = minecraft
        .observations
        .iter()
        .map(|item| ((item.redstone_tick, item.position), item))
        .collect();
    let mut compared = 0;
    for (key, expected) in minecraft_by_key {
        let Some(actual) = simulator_by_key.get(&key) else {
            continue;
        };
        compared += 1;
        let fields = [
            (
                PhysicalProperty::BlockKind,
                Some(PhysicalValue::BlockKind(expected.block_kind)),
                Some(PhysicalValue::BlockKind(actual.block_kind)),
            ),
            (
                PhysicalProperty::WireConnections,
                expected
                    .wire_connections
                    .clone()
                    .map(PhysicalValue::Connections),
                actual
                    .wire_connections
                    .clone()
                    .map(PhysicalValue::Connections),
            ),
            (
                PhysicalProperty::DustStrength,
                expected.dust_strength.map(PhysicalValue::Level),
                actual.dust_strength.map(PhysicalValue::Level),
            ),
            (
                PhysicalProperty::Powered,
                expected.powered.map(PhysicalValue::Bool),
                actual.powered.map(PhysicalValue::Bool),
            ),
            (
                PhysicalProperty::TorchLit,
                expected.torch_lit.map(PhysicalValue::Bool),
                actual.torch_lit.map(PhysicalValue::Bool),
            ),
            (
                PhysicalProperty::WeakPower,
                expected.weak_power.map(PhysicalValue::Level),
                actual.weak_power.map(PhysicalValue::Level),
            ),
            (
                PhysicalProperty::StrongPower,
                expected.strong_power.map(PhysicalValue::Level),
                actual.strong_power.map(PhysicalValue::Level),
            ),
        ];
        for (property, minecraft_value, simulator_value) in fields {
            if let (Some(minecraft), Some(simulator)) = (minecraft_value, simulator_value)
                && minecraft != simulator
            {
                return PhysicalTraceComparison {
                    compared_observations: compared,
                    first_mismatch: Some(PhysicalTraceMismatch {
                        redstone_tick: expected.redstone_tick,
                        position: expected.position,
                        property,
                        minecraft,
                        simulator,
                    }),
                };
            }
        }
    }
    PhysicalTraceComparison {
        compared_observations: compared,
        first_mismatch: None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn observation(tick: u64, position: Pos, strength: Option<u8>) -> PhysicalBlockObservation {
        PhysicalBlockObservation {
            redstone_tick: tick,
            position,
            block_kind: BlockKind::RedstoneWire,
            wire_connections: None,
            dust_strength: strength,
            powered: None,
            torch_lit: None,
            weak_power: None,
            strong_power: None,
        }
    }

    #[test]
    fn reports_first_shared_observable_mismatch() {
        let pos = Pos::new(1, 2, 3);
        let minecraft = PhysicalTrace {
            source: TraceSource::MinecraftJava,
            observations: vec![observation(0, pos, Some(0)), observation(1, pos, Some(15))],
        };
        let simulator = PhysicalTrace {
            source: TraceSource::DustRouteSimulator,
            observations: vec![observation(0, pos, Some(0)), observation(1, pos, Some(14))],
        };
        let comparison = compare_physical_traces(&minecraft, &simulator);
        assert_eq!(comparison.compared_observations, 2);
        let mismatch = comparison.first_mismatch.unwrap();
        assert_eq!(mismatch.redstone_tick, 1);
        assert_eq!(mismatch.property, PhysicalProperty::DustStrength);
        assert_eq!(mismatch.minecraft, PhysicalValue::Level(15));
        assert_eq!(mismatch.simulator, PhysicalValue::Level(14));
    }

    #[test]
    fn unavailable_minecraft_power_is_not_treated_as_zero() {
        let pos = Pos::new(0, 0, 0);
        let minecraft = PhysicalTrace {
            source: TraceSource::MinecraftJava,
            observations: vec![observation(0, pos, None)],
        };
        let mut simulated = observation(0, pos, None);
        simulated.strong_power = Some(15);
        let simulator = PhysicalTrace {
            source: TraceSource::DustRouteSimulator,
            observations: vec![simulated],
        };
        assert!(
            compare_physical_traces(&minecraft, &simulator)
                .first_mismatch
                .is_none()
        );
    }

    #[test]
    fn simulator_trace_uses_relative_ticks() {
        let mut world = World::new();
        let pos = Pos::new(0, 0, 0);
        world.place(BlockKind::RedstoneBlock, pos);
        let trace = simulate_world_trace(world, &BTreeSet::from([pos]), 3, 2).unwrap();
        assert_eq!(
            trace
                .observations
                .iter()
                .map(|item| item.redstone_tick)
                .collect::<Vec<_>>(),
            vec![0, 1, 2]
        );
    }

    #[test]
    fn cell_trace_rejects_wrong_input_arity() {
        let error = simulate_cell_trace(
            &crate::cells::external_xor_cell(),
            &[false],
            &BTreeSet::new(),
            0,
            0,
        )
        .unwrap_err();
        assert!(error.contains("expected 2 cell inputs"));
    }
}
