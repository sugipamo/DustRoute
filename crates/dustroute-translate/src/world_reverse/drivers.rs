//! Inferred input-driver selection and explicit model mutations.
use super::truth_table::TruthTableError;
use super::{InferredTerminal, RegionAnalysis};
use crate::world::{Block, BlockKind, Pos, World};
use std::collections::BTreeSet;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum InferredInputDriver {
    Lever(Pos),
    Button(Pos),
    PressurePlate(Pos),
    External(Pos),
}

/// Applies the same physical input driver used by truth-table inference and
/// transition verification.  Keeping this operation typed prevents a wire or
/// an arbitrary block from being silently mutated as an input.
pub fn apply_inferred_input_driver(
    world: &mut World,
    driver: InferredInputDriver,
    powered: bool,
) -> Result<(), TruthTableError> {
    match driver {
        InferredInputDriver::Lever(pos) => {
            set_stateful_input(world, pos, BlockKind::Lever, powered)
        }
        InferredInputDriver::Button(pos) => {
            set_stateful_input(world, pos, BlockKind::Button, powered)
        }
        InferredInputDriver::PressurePlate(pos) => {
            let Some(block) = world.get(pos).cloned() else {
                return Err(TruthTableError::InvalidDriver {
                    position: pos,
                    expected: "pressure_plate",
                    actual: BlockKind::Air,
                });
            };
            if block.kind != BlockKind::PressurePlate {
                return Err(TruthTableError::InvalidDriver {
                    position: pos,
                    expected: "pressure_plate",
                    actual: block.kind,
                });
            }
            let mut changed = block;
            changed.powered = Some(powered);
            changed.power_level = Some(if powered { 15 } else { 0 });
            world.set(pos, changed);
            Ok(())
        }
        InferredInputDriver::External(pos) => {
            let current = world.kind_at(pos);
            if !matches!(current, BlockKind::Air | BlockKind::RedstoneBlock) {
                return Err(TruthTableError::InvalidDriver {
                    position: pos,
                    expected: "air or redstone_block for an external driver",
                    actual: current,
                });
            }
            if powered {
                world.set(pos, Block::new(BlockKind::RedstoneBlock));
            } else if world.kind_at(pos) == BlockKind::RedstoneBlock {
                world.remove(pos);
            }
            Ok(())
        }
    }
}

pub(super) const fn inferred_driver_position(driver: InferredInputDriver) -> Pos {
    match driver {
        InferredInputDriver::Lever(pos)
        | InferredInputDriver::Button(pos)
        | InferredInputDriver::PressurePlate(pos)
        | InferredInputDriver::External(pos) => pos,
    }
}

fn set_stateful_input(
    world: &mut World,
    pos: Pos,
    expected: BlockKind,
    powered: bool,
) -> Result<(), TruthTableError> {
    let Some(block) = world.get(pos).cloned() else {
        return Err(TruthTableError::InvalidDriver {
            position: pos,
            expected: match expected {
                BlockKind::Lever => "lever",
                BlockKind::Button => "button",
                _ => "stateful input",
            },
            actual: BlockKind::Air,
        });
    };
    if block.kind != expected {
        return Err(TruthTableError::InvalidDriver {
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
    world.set(pos, changed);
    Ok(())
}

pub fn inferred_input_driver(
    world: &World,
    analysis: &RegionAnalysis,
    terminal: &InferredTerminal,
) -> Result<InferredInputDriver, TruthTableError> {
    match world.kind_at(terminal.anchor) {
        BlockKind::Lever => return Ok(InferredInputDriver::Lever(terminal.anchor)),
        BlockKind::Button => return Ok(InferredInputDriver::Button(terminal.anchor)),
        BlockKind::PressurePlate => {
            return Ok(InferredInputDriver::PressurePlate(terminal.anchor));
        }
        _ => {}
    }
    let horizontal = [
        Pos::new(-1, 0, 0),
        Pos::new(1, 0, 0),
        Pos::new(0, 0, -1),
        Pos::new(0, 0, 1),
    ];
    if let Some(pos) = horizontal
        .iter()
        .map(|delta| terminal.anchor.offset(delta.x, delta.y, delta.z))
        .find(|pos| !analysis.bounds.contains(*pos) && world.kind_at(*pos) == BlockKind::Air)
    {
        return Ok(InferredInputDriver::External(pos));
    }
    let component = &analysis.components[terminal.component];
    let downstream: BTreeSet<_> = component
        .outgoing
        .iter()
        .flat_map(|id| analysis.components[*id].positions.iter().copied())
        .collect();
    for next in downstream {
        let dx = next.x - terminal.anchor.x;
        let dz = next.z - terminal.anchor.z;
        if next.y == terminal.anchor.y && dx.abs() + dz.abs() == 1 {
            let pos = terminal.anchor.offset(-dx, 0, -dz);
            if world.kind_at(pos) == BlockKind::Air {
                return Ok(InferredInputDriver::External(pos));
            }
        }
    }
    Err(TruthTableError::NoDriverPosition(terminal.anchor))
}
