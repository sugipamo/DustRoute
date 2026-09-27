//! Expanded electrical adapter and source-ordered write/shape callbacks.
use super::geometry::{along, delta, facing, offset};
use super::notifications::{adjacent_jobs, shape_jobs};
use super::*;
use crate::piston_electrical::{ElectricalWorld, full_face, wire_notification_centers};
use crate::piston_electrical_law::{RepeaterFacts, builtin_laws};
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
        if let Some(program) = crate::device_program::program(block.kind) {
            let definition = &program.definition;
            if fresh && definition.fresh_powered_requires_history && block.powered == Some(true) {
                return Err(unsupported(
                    "powered device needs its pending history; resume a checkpoint",
                ));
            }
            if definition.handlers.values().any(|h| {
                h.inputs
                    .iter()
                    .any(|i| matches!(i.sample, crate::device_program::Query::ReceivingPower))
            }) {
                electrical.receiving_power(*pos)?;
            }
            for handler in definition.handlers.values() {
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
            BlockKind::Repeater => {
                electrical.repeater_input(*pos)?;
                electrical.repeater_locked(*pos)?;
                repeater_jobs(view, *pos)?;
            }
            _ => {}
        }
        if matches!(
            block.kind,
            BlockKind::Lever | BlockKind::RedstoneWire | BlockKind::Repeater
        ) || crate::device_program::program(block.kind).is_some_and(|p| {
            p.definition.orientation == crate::device_program::Orientation::Attached
        }) {
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

fn decision(
    view: RuntimeView<'_>,
    pos: Pos,
) -> Result<crate::piston_electrical_law::RepeaterDecision, RuntimeError> {
    let electrical = world(view)?;
    let block = view.block(pos)?;
    let direction = facing(&block)?;
    let target = view.block(along(pos, direction, 1)?)?;
    builtin_laws()
        .repeater(RepeaterFacts {
            powered: block
                .powered
                .ok_or_else(|| unsupported("repeater power required"))?,
            input: electrical.repeater_input(pos)?,
            locked: electrical.repeater_locked(pos)?,
            target_misaligned: target.kind == BlockKind::Repeater
                && target.facing != Some(direction.opposite()),
            delay: block
                .delay
                .ok_or_else(|| unsupported("repeater delay required"))?,
        })
        .ok_or_else(|| unsupported("invalid repeater facts"))
}

fn queue(
    view: RuntimeView<'_>,
    pos: Pos,
    delay: u64,
    priority: u8,
) -> Result<QueueRequest<PistonEvent>, RuntimeError> {
    Ok(QueueRequest::PrioritizedBlockTick {
        game_tick: next_tick(view.time().game_tick, delay)?,
        priority,
        block: BlockIdentity::of(&view.block(pos)?),
        call: call(pos, PistonEvent::ElectricalRepeaterTick),
    })
}

pub(super) fn repeater_jobs(
    view: RuntimeView<'_>,
    pos: Pos,
) -> Result<VecDeque<NeighborJob>, RuntimeError> {
    repeater_jobs_for(view, pos, &view.block(pos)?)
}

pub(super) fn repeater_jobs_for(
    view: RuntimeView<'_>,
    pos: Pos,
    block: &Block,
) -> Result<VecDeque<NeighborJob>, RuntimeError> {
    let direction = facing(block)?;
    let target = along(pos, direction, 1)?;
    view.block(target)?;
    let mut jobs = VecDeque::from([NeighborJob {
        target,
        source: pos,
        shape: false,
    }]);
    jobs.extend(
        adjacent_jobs(view, target, false, false)?
            .into_iter()
            .filter(|j| j.target != pos),
    );
    Ok(jobs)
}

pub(super) fn repeater_tick(
    view: RuntimeView<'_>,
    pos: Pos,
) -> Result<RuntimeOutcome<PistonEvent>, RuntimeError> {
    let before = view.block(pos)?;
    if before.kind != BlockKind::Repeater {
        return Ok(RuntimeOutcome::default());
    }
    let decision = decision(view, pos)?;
    if !decision.apply {
        return Ok(RuntimeOutcome::default());
    }
    let mut after = before.clone();
    after.powered = Some(decision.next_powered);
    if after.observed_name.is_some() {
        after
            .observed_properties
            .insert("powered".into(), decision.next_powered.to_string());
    }
    // WorldChunk.onBlockAdded invokes the gate output before World performs
    // shape replacement, including same-block powered/locked state changes.
    let mut jobs = repeater_jobs(view, pos)?;
    jobs.extend(write_shape_jobs(view, pos, &before, &after)?);
    Ok(RuntimeOutcome {
        delta: Some(delta(
            view,
            [(pos, after)],
            vec![],
            DeltaCause::RepeaterTick { repeater: pos },
        )?),
        callbacks: vec![call(pos, PistonEvent::Notify { jobs })],
        queued: if decision.schedule_off {
            vec![queue(view, pos, decision.delay_game_ticks, 1)?]
        } else {
            vec![]
        },
        ..Default::default()
    })
}

pub(super) fn notify_device(
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
        } else if side != facing(block)? && side != facing(block)?.opposite() {
            let locked = electrical.repeater_locked(pos)?;
            let previous = block
                .observed_properties
                .get("locked")
                .map(String::as_str)
                .unwrap_or("false");
            if previous != locked.to_string() {
                let mut after = block.clone();
                after
                    .observed_properties
                    .insert("locked".into(), locked.to_string());
                let mut jobs = repeater_jobs(view, pos)?;
                jobs.extend(write_shape_jobs(view, pos, block, &after)?);
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
    } else {
        let decision = decision(view, pos)?;
        if decision.request && !view.block_tick_ticking(pos, &BlockIdentity::of(block)) {
            out.queued.push(queue(
                view,
                pos,
                decision.delay_game_ticks,
                decision.priority,
            )?);
        }
    }
    Ok(out)
}
