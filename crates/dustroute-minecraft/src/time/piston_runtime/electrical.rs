//! Expanded electrical adapter and source-ordered write/shape callbacks.
use super::geometry::{along, delta, offset};
use super::notifications::{adjacent_jobs, shape_jobs};
use super::*;
use crate::piston_electrical::{ElectricalWorld, full_face, wire_notification_centers};
use crate::{BlockKind, DeltaCause, Facing, WireConnection};

pub(super) fn world(view: RuntimeView<'_>) -> Result<ElectricalWorld<'_>, RuntimeError> {
    ElectricalWorld::for_runtime(view)
}

pub(super) fn wire_offset_jobs(
    view: RuntimeView<'_>,
    pos: Pos,
) -> Result<VecDeque<NeighborJob>, RuntimeError> {
    let mut jobs = VecDeque::new();
    // RedstoneWireBlock.updateOffsetNeighbors; its private helper
    // notifies around a wire and each of its six adjacent centers.
    let wire_updates = |target| -> Result<VecDeque<NeighborJob>, RuntimeError> {
        let mut jobs = VecDeque::new();
        if view.block(target)?.kind == BlockKind::RedstoneWire {
            jobs.extend(adjacent_jobs(view, target, false, false)?);
            for side in crate::piston_electrical::SIDES {
                jobs.extend(adjacent_jobs(view, along(target, side, 1)?, false, false)?);
            }
        }
        Ok(jobs)
    };
    for side in crate::piston_electrical::HORIZONTAL {
        jobs.extend(wire_updates(along(pos, side, 1)?)?);
    }
    for side in crate::piston_electrical::HORIZONTAL {
        let beside = along(pos, side, 1)?;
        let vertical = if crate::piston_electrical::conducts(&view.block(beside)?) {
            Facing::Up
        } else {
            Facing::Down
        };
        jobs.extend(wire_updates(along(beside, vertical, 1)?)?);
    }
    Ok(jobs)
}

pub(super) fn validate_scope(view: RuntimeView<'_>, fresh: bool) -> Result<(), RuntimeError> {
    let electrical = world(view)?;
    for (pos, block) in view.world().iter() {
        if let Some(program) = crate::device_program::program(block) {
            let definition = &program.definition;
            if fresh && definition.fresh_powered_requires_history && block.powered == Some(true) {
                return Err(unsupported(
                    "powered device needs its pending history; resume a checkpoint",
                ));
            }
            for handler in definition.handlers.values() {
                for input in &handler.inputs {
                    use crate::device_program::Query;
                    match input.sample {
                        Query::ReceivingPower => {
                            electrical.receiving_power(*pos)?;
                        }
                        Query::ReceivingLevel => {
                            electrical.receiving_level(*pos)?;
                        }
                        Query::SideLevel { side } => {
                            electrical.emitted(along(*pos, side, 1)?, side, true)?;
                        }
                        Query::GateInputPowered => {
                            electrical.gate_input_powered(*pos)?;
                        }
                        Query::SideGatePowered => {
                            electrical.side_gate_powered(*pos)?;
                        }
                        Query::OutputGateMisaligned => {
                            electrical.output_gate_misaligned(*pos)?;
                        }
                        _ => {}
                    }
                }
                for effect in &handler.effects {
                    if let crate::device_program::Effect::Notify { targets, .. } = effect {
                        super::devices::notification_jobs(view, *pos, block, *targets)?;
                    }
                }
            }
        }
        match block.kind {
            BlockKind::Piston => {
                electrical.piston_powered(*pos)?;
            }
            BlockKind::RedstoneWire => {
                if fresh && Some(&electrical.wire_shape(*pos)?) != block.wire_connections.as_ref() {
                    return Err(unsupported(format!(
                        "initial wire shape at {pos:?} disagrees with the arrangement"
                    )));
                }
                electrical.wire_power(*pos)?;
                for center in wire_notification_centers(*pos)? {
                    adjacent_jobs(view, center, false, false)?;
                }
            }
            _ => {}
        }
        if crate::physical::of_kind(block.kind).support() != crate::physical::Support::None {
            let offset = block
                .support_offset
                .ok_or_else(|| unsupported("electrical support required"))?;
            let support = super::geometry::offset(*pos, offset)?;
            let side = crate::piston_electrical::SIDES
                .into_iter()
                .find(|d| d.offset() == offset)
                .ok_or_else(|| unsupported("adjacent support required"))?;
            let support_block = view
                .staged_carriers
                .get(&support)
                .cloned()
                .unwrap_or(view.block(support)?);
            let retained_body_back = support_block.kind == BlockKind::MovingPiston
                && support_block
                    .piston_entity
                    .as_deref()
                    .is_some_and(|carrier| {
                        carrier.source
                            && !carrier.extending
                            && carrier.pushed_block.kind == BlockKind::Piston
                            && carrier.pushed_block.facing == Some(side)
                    });
            if !full_face(&support_block, side.opposite()) && !retained_body_back {
                return Err(unsupported(format!(
                    "unsupported electrical support at {pos:?}"
                )));
            }
        }
    }
    Ok(())
}

fn side_from_source(job: &NeighborJob) -> Result<Facing, RuntimeError> {
    crate::piston_electrical::SIDES
        .into_iter()
        .find(|side| along(job.target, *side, 1).ok() == Some(job.source))
        .ok_or_else(|| unsupported("shape callback source must be adjacent"))
}

/// RedstoneWireBlock.prepare: diagonal shapes around connected arms, in
/// Direction.Type.HORIZONTAL order. Components cannot be moved/destroyed in
/// this profile, so the target identities stay fixed during these callbacks.
fn prepare_jobs(
    view: RuntimeView<'_>,
    pos: Pos,
    block: &Block,
) -> Result<VecDeque<NeighborJob>, RuntimeError> {
    let mut jobs = VecDeque::new();
    if block.kind != BlockKind::RedstoneWire {
        return Ok(jobs);
    }
    let arms = block
        .wire_connections
        .as_ref()
        .ok_or_else(|| unsupported("wire arms required"))?;
    for side in crate::piston_electrical::HORIZONTAL {
        let beside = along(pos, side, 1)?;
        if arms[&side] == WireConnection::None
            || view.block(beside)?.kind == BlockKind::RedstoneWire
        {
            continue;
        }
        for vertical in [Facing::Down, Facing::Up] {
            let target = along(beside, vertical, 1)?;
            if view.block(target)?.kind == BlockKind::RedstoneWire {
                jobs.push_back(NeighborJob {
                    target,
                    source: along(pos, vertical, 1)?,
                    shape: true,
                });
            }
        }
    }
    Ok(jobs)
}

pub(super) fn write_shape_jobs(
    view: RuntimeView<'_>,
    pos: Pos,
    before: &Block,
    after: &Block,
) -> Result<VecDeque<NeighborJob>, RuntimeError> {
    // World.setBlockState flags 2: old.prepare, new.updateNeighbors (shapes),
    // new.prepare. Ordinary power notifications are separate method calls.
    let mut jobs = prepare_jobs(view, pos, before)?;
    jobs.extend(shape_jobs(view, pos)?);
    jobs.extend(prepare_jobs(view, pos, after)?);
    Ok(jobs)
}

pub(super) fn input_jobs(
    view: RuntimeView<'_>,
    pos: Pos,
) -> Result<VecDeque<NeighborJob>, RuntimeError> {
    // Lever.togglePower uses flags 3, then explicitly notifies source/support.
    let mut jobs = adjacent_jobs(view, pos, false, false)?;
    jobs.extend(shape_jobs(view, pos)?);
    let support = view
        .block(pos)?
        .support_offset
        .ok_or_else(|| unsupported("input needs explicit lever support"))?;
    if !crate::piston_electrical::SIDES
        .iter()
        .any(|side| side.offset() == support)
    {
        return Err(unsupported("input needs adjacent lever support"));
    }
    jobs.extend(adjacent_jobs(view, pos, false, false)?);
    jobs.extend(adjacent_jobs(view, offset(pos, support)?, false, false)?);
    Ok(jobs)
}

pub(super) fn notify_wire(
    view: RuntimeView<'_>,
    job: &NeighborJob,
    block: &Block,
) -> Result<RuntimeOutcome<PistonEvent>, RuntimeError> {
    let pos = job.target;
    let mut out = RuntimeOutcome::default();
    let electrical = world(view)?;
    if job.shape {
        let side = side_from_source(job)?;
        if block.kind == BlockKind::RedstoneWire {
            let shape = electrical.wire_neighbor_shape(pos, side)?;
            if Some(&shape) != block.wire_connections.as_ref() {
                let mut after = block.clone();
                if after.observed_name.is_some() {
                    for (direction, connection) in &shape {
                        after.observed_properties.insert(
                            super::geometry::facing_name(*direction).into(),
                            match connection {
                                WireConnection::None => "none",
                                WireConnection::Side => "side",
                                WireConnection::Up => "up",
                            }
                            .into(),
                        );
                    }
                }
                after.wire_connections = Some(shape);
                let jobs = write_shape_jobs(view, pos, block, &after)?;
                out.delta = Some(delta(
                    view,
                    [(pos, after)],
                    vec![],
                    DeltaCause::NeighborUpdate,
                )?);
                out.callbacks.push(call(pos, PistonEvent::Notify { jobs }));
            }
        }
        return Ok(out);
    }
    if block.kind == BlockKind::RedstoneWire {
        let power = electrical.wire_power(pos)?;
        if block.power_level != Some(power) {
            let mut after = block.clone();
            after.power_level = Some(power);
            if after.observed_name.is_some() {
                after
                    .observed_properties
                    .insert("power".into(), power.to_string());
            }
            let mut jobs = write_shape_jobs(view, pos, block, &after)?;
            out.delta = Some(delta(
                view,
                [(pos, after)],
                vec![],
                DeltaCause::NeighborUpdate,
            )?);
            for center in wire_notification_centers(pos)? {
                jobs.extend(adjacent_jobs(view, center, false, false)?);
            }
            out.callbacks.push(call(pos, PistonEvent::Notify { jobs }));
        }
    }
    Ok(out)
}
