//! Bounded redstone signal propagation helpers.
//!
//! This module intentionally models only the steady-state, block-only subset
//! needed by the world-driven runner.  It does not attempt to reproduce the
//! complete Java update engine (quasi-connectivity, repeater timing, observer
//! pulses, or comparator calculation).  Unknown observed state is returned as
//! an error instead of being silently treated as an unpowered source.

use std::collections::BTreeSet;
use std::error::Error;
use std::fmt::{Display, Formatter};

use serde::{Deserialize, Serialize};

use crate::{
    Block, BlockChange, BlockKind, ChangeReason, DeltaCause, Facing, Pos, Region, RegionSet,
    WireConnection, World, WorldDelta,
};

const HORIZONTAL_FACINGS: [Facing; 4] = [Facing::North, Facing::East, Facing::South, Facing::West];
const ALL_FACINGS: [Facing; 6] = [
    Facing::North,
    Facing::East,
    Facing::South,
    Facing::West,
    Facing::Up,
    Facing::Down,
];

/// An observed redstone state cannot be evaluated conservatively.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(tag = "error", rename_all = "snake_case")]
pub enum RedstonePropagationError {
    UnknownState {
        position: Pos,
        kind: BlockKind,
        reason: String,
    },
    UnsupportedComponent {
        position: Pos,
        kind: BlockKind,
        reason: String,
    },
}

impl Display for RedstonePropagationError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::UnknownState {
                position,
                kind,
                reason,
            } => write!(
                formatter,
                "redstone state for {kind:?} at ({}, {}, {}) is unavailable: {reason}",
                position.x, position.y, position.z
            ),
            Self::UnsupportedComponent {
                position,
                kind,
                reason,
            } => write!(
                formatter,
                "redstone propagation for {kind:?} at ({}, {}, {}) is outside the MVP: {reason}",
                position.x, position.y, position.z
            ),
        }
    }
}

impl Error for RedstonePropagationError {}

/// Returns the positions that can receive a neighbor update after a block
/// transition.  Sorting and de-duplicating here makes event insertion
/// deterministic even when several changed blocks share a neighbor.
pub(crate) fn redstone_update_positions(position: Pos, include_self: bool) -> Vec<Pos> {
    let mut positions = BTreeSet::new();
    if include_self {
        positions.insert(position);
    }
    for facing in ALL_FACINGS {
        let offset = facing.offset();
        positions.insert(position.offset(offset.x, offset.y, offset.z));
    }
    positions.into_iter().collect()
}

/// Validates that a propagation lookup is inside the caller's complete
/// observation boundary. A missing block is Air only after this check has
/// succeeded; coordinates outside the boundary remain unknown.
pub(crate) fn redstone_position_known(
    known_region: Option<Region>,
    position: Pos,
) -> Result<(), RedstonePropagationError> {
    ensure_known(known_region, position, BlockKind::Air)
}

/// Builds an externally supplied World transition.  `World::set` remains a
/// low-level snapshot mutator; callers that want propagation must enter
/// through this explicit event boundary so before-state validation and causal
/// trace records are retained.
pub(crate) fn external_world_delta(
    world: &World,
    position: Pos,
    after: &Block,
) -> Option<WorldDelta> {
    let before = world
        .get(position)
        .cloned()
        .unwrap_or_else(|| Block::new(BlockKind::Air));
    if before == *after {
        return None;
    }
    Some(WorldDelta {
        parent_shape: world.shape_id(),
        changes: vec![BlockChange {
            position,
            before,
            after: after.clone(),
            reason: ChangeReason::ExternalInput,
        }],
        moves: Vec::new(),
        dirty_region: RegionSet::around_positions([position], 1),
        cause: DeltaCause::External,
    })
}

/// Recomputes one wire's steady-state power from adjacent supported sources
/// and wires.  Wire power is the maximum adjacent input, with one level lost
/// when the input is another wire.  The event runner repeatedly applies these
/// local deltas until the queue reaches a fixed point.
pub(crate) fn redstone_wire_delta(
    world: &World,
    position: Pos,
    known_region: Option<Region>,
) -> Result<Option<WorldDelta>, RedstonePropagationError> {
    ensure_known(known_region, position, BlockKind::RedstoneWire)?;
    let Some(block) = world.get(position) else {
        return Ok(None);
    };
    if block.kind != BlockKind::RedstoneWire {
        return Err(RedstonePropagationError::UnsupportedComponent {
            position,
            kind: block.kind,
            reason: "wire evaluator received a non-wire target".to_owned(),
        });
    }

    let current = wire_level(block, position)?;
    let mut expected = 0_u8;
    for facing in HORIZONTAL_FACINGS {
        let offset = facing.offset();
        let source_position = position.offset(offset.x, offset.y, offset.z);
        ensure_known(known_region, source_position, BlockKind::Air)?;
        let Some(source) = world.get(source_position) else {
            continue;
        };
        if !wire_connects(block, position, facing)? {
            continue;
        }
        let signal = if source.kind == BlockKind::RedstoneWire {
            if !wire_connects(source, source_position, facing.opposite())? {
                0
            } else {
                wire_level(source, source_position)?.saturating_sub(1)
            }
        } else {
            source_signal(source, source_position)?
        };
        expected = expected.max(signal);
    }

    if current == expected {
        return Ok(None);
    }
    let mut after = block.clone();
    after.power_level = Some(expected);
    if after.observed_name.is_some() {
        after
            .observed_properties
            .insert("power".to_owned(), expected.to_string());
    }
    Ok(Some(signal_delta(world, position, block.clone(), after)))
}

/// Recomputes a lamp's discrete lit/powered state from adjacent signals.  The
/// lamp is included because it provides a useful observable sink for the
/// propagation tests; it never emits power to other blocks.
pub(crate) fn redstone_lamp_delta(
    world: &World,
    position: Pos,
    known_region: Option<Region>,
) -> Result<Option<WorldDelta>, RedstonePropagationError> {
    ensure_known(known_region, position, BlockKind::RedstoneLamp)?;
    let Some(block) = world.get(position) else {
        return Ok(None);
    };
    if block.kind != BlockKind::RedstoneLamp {
        return Err(RedstonePropagationError::UnsupportedComponent {
            position,
            kind: block.kind,
            reason: "lamp evaluator received a non-lamp target".to_owned(),
        });
    }
    let current = block
        .powered
        .or_else(|| observed_bool_property(block, "lit"))
        .or_else(|| block.observed_name.is_none().then_some(false))
        .ok_or_else(|| RedstonePropagationError::UnknownState {
            position,
            kind: block.kind,
            reason: "lamp lit state is missing".to_owned(),
        })?;
    let expected = received_signal(world, position, known_region)? > 0;
    if current == expected {
        return Ok(None);
    }
    let mut after = block.clone();
    after.powered = Some(expected);
    if after.observed_name.is_some() {
        after
            .observed_properties
            .insert("lit".to_owned(), expected.to_string());
    }
    Ok(Some(signal_delta(world, position, block.clone(), after)))
}

fn signal_delta(world: &World, position: Pos, before: Block, after: Block) -> WorldDelta {
    WorldDelta {
        parent_shape: world.shape_id(),
        changes: vec![BlockChange {
            position,
            before,
            after,
            reason: ChangeReason::NeighborUpdate,
        }],
        moves: Vec::new(),
        dirty_region: RegionSet::around_positions([position], 1),
        cause: DeltaCause::NeighborUpdate,
    }
}

fn received_signal(
    world: &World,
    target: Pos,
    known_region: Option<Region>,
) -> Result<u8, RedstonePropagationError> {
    let mut signal = 0_u8;
    for facing in HORIZONTAL_FACINGS {
        let offset = facing.offset();
        let source_position = target.offset(offset.x, offset.y, offset.z);
        ensure_known(known_region, source_position, BlockKind::Air)?;
        let Some(source) = world.get(source_position) else {
            continue;
        };
        let Some(target_block) = world.get(target) else {
            continue;
        };
        if target_block.kind == BlockKind::RedstoneWire
            && !wire_connects(target_block, target, facing)?
        {
            continue;
        }
        let candidate = if source.kind == BlockKind::RedstoneWire {
            if !wire_connects(source, source_position, facing.opposite())? {
                0
            } else {
                wire_level(source, source_position)?.saturating_sub(1)
            }
        } else {
            source_signal(source, source_position)?
        };
        signal = signal.max(candidate);
    }
    Ok(signal)
}

fn source_signal(block: &Block, position: Pos) -> Result<u8, RedstonePropagationError> {
    let signal = match block.kind {
        BlockKind::RedstoneBlock => block
            .powered
            .or_else(|| block.observed_name.is_none().then_some(true))
            .or_else(|| observed_bool_property(block, "powered"))
            .map(|powered| u8::from(powered) * 15),
        BlockKind::RedstoneWire => Some(wire_level(block, position)?),
        BlockKind::RedstoneTorch => block
            .powered
            .or_else(|| observed_bool_property(block, "lit"))
            .or_else(|| block.observed_name.is_none().then_some(true))
            .map(|powered| u8::from(powered) * 15),
        BlockKind::Lever | BlockKind::Button | BlockKind::PressurePlate => block
            .powered
            .or_else(|| observed_bool_property(block, "powered"))
            .or_else(|| block.observed_name.is_none().then_some(false))
            .map(|powered| u8::from(powered) * 15),
        // These components can be observed as already powered, but their
        // upstream timing/calculation is intentionally a later goal.
        BlockKind::Repeater | BlockKind::Comparator | BlockKind::Observer => block
            .power_level
            .or_else(|| {
                block
                    .powered
                    .or_else(|| observed_bool_property(block, "powered"))
                    .map(|powered| u8::from(powered) * 15)
            })
            .or_else(|| block.observed_name.is_none().then_some(0)),
        _ => Some(0),
    };
    signal.ok_or_else(|| RedstonePropagationError::UnknownState {
        position,
        kind: block.kind,
        reason: "source power state is missing".to_owned(),
    })
}

fn wire_level(block: &Block, position: Pos) -> Result<u8, RedstonePropagationError> {
    block
        .power_level
        .or_else(|| observed_u8_property(block, "power"))
        .or_else(|| block.observed_name.is_none().then_some(0))
        .ok_or_else(|| RedstonePropagationError::UnknownState {
            position,
            kind: block.kind,
            reason: "wire power level is missing".to_owned(),
        })
}

fn wire_connects(
    block: &Block,
    position: Pos,
    toward: Facing,
) -> Result<bool, RedstonePropagationError> {
    match &block.wire_connections {
        Some(connections) => Ok(connections
            .get(&toward)
            .is_some_and(|connection| *connection != WireConnection::None)),
        None if block.observed_name.is_some() => Err(RedstonePropagationError::UnknownState {
            position,
            kind: block.kind,
            reason: "observed wire connection shape is missing".to_owned(),
        }),
        None => Ok(true),
    }
}

fn ensure_known(
    known_region: Option<Region>,
    position: Pos,
    kind: BlockKind,
) -> Result<(), RedstonePropagationError> {
    if known_region.is_some_and(|region| !region.contains(position)) {
        return Err(RedstonePropagationError::UnknownState {
            position,
            kind,
            reason: "propagation reached outside the complete observed region".to_owned(),
        });
    }
    Ok(())
}

fn observed_bool_property(block: &Block, key: &str) -> Option<bool> {
    block
        .observed_properties
        .get(key)
        .and_then(|value| value.parse::<bool>().ok())
}

fn observed_u8_property(block: &Block, key: &str) -> Option<u8> {
    block
        .observed_properties
        .get(key)
        .and_then(|value| value.parse::<u8>().ok())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn update_positions_are_sorted_and_include_all_six_neighbors() {
        let position = Pos::new(2, 3, 4);
        let positions = redstone_update_positions(position, true);
        assert_eq!(positions.len(), 7);
        assert!(positions.contains(&position.offset(0, 1, 0)));
        assert!(positions.contains(&position.offset(0, -1, 0)));
    }

    #[test]
    fn synthetic_wire_reaches_fixed_point_one_level_at_a_time() {
        let source_pos = Pos::new(0, 1, 0);
        let first_wire = Pos::new(1, 1, 0);
        let second_wire = Pos::new(2, 1, 0);
        let mut world = World::new();
        let mut source = Block::new(BlockKind::Lever);
        source.powered = Some(true);
        world.set(source_pos, source);
        world.set(first_wire, Block::new(BlockKind::RedstoneWire));
        world.set(second_wire, Block::new(BlockKind::RedstoneWire));

        let first = redstone_wire_delta(&world, first_wire, None)
            .unwrap()
            .expect("source should power first wire");
        first.apply(&mut world).unwrap();
        assert_eq!(world.get(first_wire).unwrap().power_level, Some(15));
        let second = redstone_wire_delta(&world, second_wire, None)
            .unwrap()
            .expect("first wire should power second wire");
        second.apply(&mut world).unwrap();
        assert_eq!(world.get(second_wire).unwrap().power_level, Some(14));
    }
}
