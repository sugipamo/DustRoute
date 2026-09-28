//! Exact-state Java command insertion/removal, including pre-write work.
//! Queue payloads remain in PistonEvent; module boundaries do not change timing.
use super::electrical::{wire_offset_jobs, world, write_shape_jobs};
use super::geometry::{along, delta, offset};
use super::notifications::adjacent_jobs;
use super::*;
use crate::piston_electrical::wire_notification_centers;
use crate::{BlockKind, DeltaCause, Facing};

pub(super) fn install(
    view: RuntimeView<'_>,
    pos: Pos,
    block: &Block,
) -> Result<RuntimeOutcome<PistonEvent>, RuntimeError> {
    if view.block(pos)?.kind != BlockKind::Air {
        return Err(unsupported("electrical construction requires known air"));
    }
    crate::piston_electrical::validate_evidence(block)?;
    if matches!(
        block.kind,
        BlockKind::Air | BlockKind::PistonHead | BlockKind::MovingPiston
    ) || (block.kind == BlockKind::Piston
        && block.piston_state != Some(crate::PistonState::Retracted))
    {
        return Err(unsupported(
            "construction installs stable components and retracted bodies",
        ));
    }
    if let Some(support) = block.support_offset {
        let side = crate::piston_electrical::SIDES
            .into_iter()
            .find(|side| side.offset() == support)
            .ok_or_else(|| unsupported("adjacent construction support required"))?;
        if !crate::physical::of_kind(block.kind)
            .supports_attachment(&view.block(offset(pos, support)?)?, side)
        {
            return Err(unsupported("unsupported command placement support"));
        }
    }
    Ok(RuntimeOutcome {
        continuation: Some(PistonEvent::ElectricalPreprocess {
            block: Box::new(block.clone()),
            side: 0,
        }),
        ..Default::default()
    })
}

pub(super) fn preprocess(
    view: RuntimeView<'_>,
    pos: Pos,
    block: &Block,
    side: usize,
) -> Result<RuntimeOutcome<PistonEvent>, RuntimeError> {
    // Block.postProcessState: AbstractBlock.DIRECTIONS, not neighbor-update
    // order. The requested state is NOT yet in the world. Construction exports
    // every property explicitly, so BlockStateArgument.copyPropertiesTo restores
    // wire arms/repeater lock after their pure shape transforms. Support loss is
    // excluded above. Of the admitted stable kinds only observer shapes enqueue
    // work. Keep each callback boundary so queued identity and pre-write state
    // survive checkpoints; do not use a temporary world write.
    const SIDES: [Facing; 6] = [
        Facing::West,
        Facing::East,
        Facing::North,
        Facing::South,
        Facing::Down,
        Facing::Up,
    ];
    let Some(direction) = SIDES.get(side) else {
        return Ok(RuntimeOutcome {
            continuation: Some(PistonEvent::ElectricalWrite {
                block: Box::new(block.clone()),
            }),
            ..Default::default()
        });
    };
    view.block(along(pos, *direction, 1)?)?;
    let mut out = RuntimeOutcome {
        continuation: Some(PistonEvent::ElectricalPreprocess {
            block: Box::new(block.clone()),
            side: side + 1,
        }),
        ..Default::default()
    };
    out.queued = super::devices::preprocess(view, pos, block, along(pos, *direction, 1)?)?;
    Ok(out)
}

pub(super) fn write(
    view: RuntimeView<'_>,
    pos: Pos,
    block: &Block,
) -> Result<RuntimeOutcome<PistonEvent>, RuntimeError> {
    Ok(RuntimeOutcome {
        delta: Some(delta(
            view,
            [(pos, block.clone())],
            vec![],
            DeltaCause::NeighborUpdate,
        )?),
        callbacks: vec![call(pos, PistonEvent::ElectricalAdded)],
        ..Default::default()
    })
}

pub(super) fn added(
    view: RuntimeView<'_>,
    pos: Pos,
) -> Result<RuntimeOutcome<PistonEvent>, RuntimeError> {
    let block = view.block(pos)?;
    let mut jobs = VecDeque::new();
    if crate::device_program::program(&block).is_some() {
        return Ok(RuntimeOutcome {
            callbacks: vec![super::devices::event(
                pos,
                crate::device_program::Callback::Added,
                None,
            )],
            continuation: Some(PistonEvent::ElectricalAfterAdded {
                block: Box::new(block),
            }),
            ..Default::default()
        });
    }
    match block.kind {
        BlockKind::Piston => jobs.push_back(NeighborJob {
            target: pos,
            source: pos,
            shape: false,
        }),
        BlockKind::RedstoneWire => {
            jobs.push_back(NeighborJob {
                target: pos,
                source: pos,
                shape: false,
            });
            for vertical in [Facing::Up, Facing::Down] {
                jobs.extend(adjacent_jobs(view, along(pos, vertical, 1)?, false, false)?);
            }
            jobs.extend(wire_offset_jobs(view, pos)?);
        }
        _ => {}
    }
    Ok(RuntimeOutcome {
        callbacks: vec![call(pos, PistonEvent::Notify { jobs })],
        continuation: Some(PistonEvent::ElectricalAfterAdded {
            block: Box::new(block),
        }),
        ..Default::default()
    })
}

pub(super) fn after_added(
    view: RuntimeView<'_>,
    pos: Pos,
    block: &Block,
) -> Result<RuntimeOutcome<PistonEvent>, RuntimeError> {
    // A nested onBlockAdded write may replace the requested state. World only
    // runs flags 258 shape callbacks when that requested state remains.
    // SetBlockCommand's explicit ordinary notifications follow regardless.
    let mut jobs = VecDeque::new();
    if &view.block(pos)? == block {
        jobs.extend(write_shape_jobs(
            view,
            pos,
            &Block::new(BlockKind::Air),
            block,
        )?);
    }
    Ok(RuntimeOutcome {
        callbacks: vec![call(pos, PistonEvent::Notify { jobs })],
        continuation: Some(PistonEvent::ElectricalCommandNeighbors),
        ..Default::default()
    })
}

pub(super) fn remove(
    view: RuntimeView<'_>,
    pos: Pos,
) -> Result<RuntimeOutcome<PistonEvent>, RuntimeError> {
    let before = view.block(pos)?;
    if matches!(
        before.kind,
        BlockKind::Air | BlockKind::MovingPiston | BlockKind::PistonHead
    ) {
        return Err(unsupported(
            "teardown removes settled bodies before their heads",
        ));
    }
    Ok(RuntimeOutcome {
        delta: Some(delta(
            view,
            [(pos, Block::new(BlockKind::Air))],
            vec![],
            DeltaCause::NeighborUpdate,
        )?),
        callbacks: vec![call(
            pos,
            PistonEvent::ElectricalRemoveShapes {
                block: Box::new(before),
            },
        )],
        ..Default::default()
    })
}

pub(super) fn remove_shapes(
    view: RuntimeView<'_>,
    pos: Pos,
    before: &Block,
) -> Result<RuntimeOutcome<PistonEvent>, RuntimeError> {
    // flags 258 defer old.onStateReplaced until command postprocessing, after
    // the world's shape callbacks. Air's pre-write shape transforms are pure.
    Ok(RuntimeOutcome {
        callbacks: vec![call(
            pos,
            PistonEvent::Notify {
                jobs: write_shape_jobs(view, pos, before, &Block::new(BlockKind::Air))?,
            },
        )],
        continuation: Some(PistonEvent::ElectricalRemoved {
            block: Box::new(before.clone()),
        }),
        ..Default::default()
    })
}

pub(super) fn removed(
    view: RuntimeView<'_>,
    pos: Pos,
    before: &Block,
) -> Result<RuntimeOutcome<PistonEvent>, RuntimeError> {
    let jobs = removed_notification_jobs(view, pos, before)?;
    Ok(RuntimeOutcome {
        callbacks: vec![call(pos, PistonEvent::Notify { jobs })],
        continuation: Some(if before.kind == BlockKind::RedstoneWire {
            PistonEvent::ElectricalRemovedWireUpdate {
                block: Box::new(before.clone()),
            }
        } else {
            PistonEvent::ElectricalCommandNeighbors
        }),
        ..Default::default()
    })
}

pub(super) fn removed_notification_jobs(
    view: RuntimeView<'_>,
    pos: Pos,
    before: &Block,
) -> Result<VecDeque<NeighborJob>, RuntimeError> {
    let mut jobs = VecDeque::new();
    jobs.extend(super::devices::removed_jobs(view, pos, before)?);
    match before.kind {
        BlockKind::Lever if before.powered == Some(true) => {
            jobs.extend(adjacent_jobs(view, pos, false, false)?);
            let support = super::geometry::offset(
                pos,
                before
                    .support_offset
                    .ok_or_else(|| unsupported("lever support required"))?,
            )?;
            jobs.extend(adjacent_jobs(view, support, false, false)?);
        }
        BlockKind::RedstoneWire => {
            for side in crate::piston_electrical::SIDES {
                jobs.extend(adjacent_jobs(view, along(pos, side, 1)?, false, false)?);
            }
        }
        _ => {}
    }
    Ok(jobs)
}

pub(super) fn removed_wire_update(
    view: RuntimeView<'_>,
    pos: Pos,
    before: &Block,
) -> Result<RuntimeOutcome<PistonEvent>, RuntimeError> {
    Ok(RuntimeOutcome {
        callbacks: vec![call(
            pos,
            PistonEvent::Notify {
                jobs: removed_wire_jobs(view, pos, before)?,
            },
        )],
        continuation: Some(PistonEvent::ElectricalCommandNeighbors),
        ..Default::default()
    })
}

pub(super) fn removed_wire_jobs(
    view: RuntimeView<'_>,
    pos: Pos,
    before: &Block,
) -> Result<VecDeque<NeighborJob>, RuntimeError> {
    let mut jobs = VecDeque::new();
    if Some(world(view)?.wire_power_at(pos)?) != before.power_level {
        for center in wire_notification_centers(pos)? {
            jobs.extend(adjacent_jobs(view, center, false, false)?);
        }
    }
    jobs.extend(wire_offset_jobs(view, pos)?);
    Ok(jobs)
}
