//! Native lamp and observer callbacks on the shared world and tick scheduler.
use super::geometry::{along, delta};
use super::*;
use crate::device_callback_law::{DeviceEffect, ObserverCallback, builtin_laws};
use crate::{BlockKind, DeltaCause};

/// Shape preprocessing uses a requested state that is not yet in the world,
/// both for commands and for an observer arriving from a moving carrier.
pub(super) fn observer_shape_ticks(
    view: RuntimeView<'_>,
    pos: Pos,
    requested: &Block,
    front: bool,
) -> Result<Vec<QueueRequest<PistonEvent>>, RuntimeError> {
    let effect = builtin_laws().observer(
        ObserverCallback::Shape,
        requested.powered == Some(true),
        view.block_tick_queued(pos, &BlockIdentity::of(requested)),
        front,
    );
    if effect.delay == 0 {
        return Ok(vec![]);
    }
    Ok(vec![QueueRequest::PrioritizedBlockTick {
        game_tick: next_tick(view.time().game_tick, effect.delay)?,
        priority: 3,
        block: BlockIdentity::of(requested),
        call: call(pos, PistonEvent::ObserverTick),
    }])
}

fn apply(
    view: RuntimeView<'_>,
    pos: Pos,
    before: &Block,
    effect: DeviceEffect,
    tick: PistonEvent,
    shapes: bool,
) -> Result<RuntimeOutcome<PistonEvent>, RuntimeError> {
    let mut out = RuntimeOutcome::default();
    let mut jobs = VecDeque::new();
    if effect.write {
        let mut after = before.clone();
        after.powered = Some(effect.powered);
        if after.observed_name.is_some() {
            let key = if after.kind == BlockKind::RedstoneLamp {
                "lit"
            } else {
                "powered"
            };
            after
                .observed_properties
                .insert(key.into(), effect.powered.to_string());
        }
        if shapes {
            jobs.extend(super::electrical::write_shape_jobs(
                view, pos, before, &after,
            )?);
        }
        out.delta = Some(delta(
            view,
            [(pos, after)],
            vec![],
            DeltaCause::NeighborUpdate,
        )?);
    }
    if effect.notify {
        jobs.extend(super::electrical::repeater_jobs_for(view, pos, before)?);
    }
    if !jobs.is_empty() {
        out.callbacks.push(call(pos, PistonEvent::Notify { jobs }));
    }
    if effect.delay != 0 {
        out.queued.push(QueueRequest::PrioritizedBlockTick {
            game_tick: next_tick(view.time().game_tick, effect.delay)?,
            priority: 3,
            block: BlockIdentity::of(before),
            call: call(pos, tick),
        });
    }
    Ok(out)
}

pub(super) fn lamp(
    view: RuntimeView<'_>,
    pos: Pos,
    scheduled: bool,
) -> Result<RuntimeOutcome<PistonEvent>, RuntimeError> {
    let block = view.block(pos)?;
    let effect = builtin_laws().lamp(
        block.powered == Some(true),
        super::electrical::world(view)?.receiving_power(pos)?,
        scheduled,
    );
    apply(view, pos, &block, effect, PistonEvent::LampTick, true)
}

pub(super) fn observer(
    view: RuntimeView<'_>,
    pos: Pos,
    event: ObserverCallback,
    front: bool,
) -> Result<RuntimeOutcome<PistonEvent>, RuntimeError> {
    let block = view.block(pos)?;
    let mut effect = builtin_laws().observer(
        event,
        block.powered == Some(true),
        view.block_tick_queued(pos, &BlockIdentity::of(&block)),
        front,
    );
    if event == ObserverCallback::Tick {
        // Java schedules the off tick after the flags-2 write has drained its
        // shape callbacks, then emits the ordinary output notifications.
        let delay = effect.delay;
        effect.delay = 0;
        effect.notify = false;
        let mut out = apply(view, pos, &block, effect, PistonEvent::ObserverTick, true)?;
        out.continuation = Some(PistonEvent::ObserverAfterTick {
            block: Box::new(block),
            delay,
        });
        Ok(out)
    } else {
        apply(
            view,
            pos,
            &block,
            effect,
            PistonEvent::ObserverTick,
            event != ObserverCallback::Added,
        )
    }
}

pub(super) fn observer_after_tick(
    view: RuntimeView<'_>,
    pos: Pos,
    before: &Block,
    delay: u64,
) -> Result<RuntimeOutcome<PistonEvent>, RuntimeError> {
    apply(
        view,
        pos,
        before,
        DeviceEffect {
            write: false,
            powered: false,
            delay,
            notify: true,
        },
        PistonEvent::ObserverTick,
        false,
    )
}

pub(super) fn removed_jobs(
    view: RuntimeView<'_>,
    pos: Pos,
    before: &Block,
) -> Result<VecDeque<NeighborJob>, RuntimeError> {
    let effect = builtin_laws().observer(
        ObserverCallback::Removed,
        before.powered == Some(true),
        view.block_tick_queued(pos, &BlockIdentity::of(before)),
        false,
    );
    if effect.notify {
        super::electrical::repeater_jobs_for(view, pos, before)
    } else {
        Ok(VecDeque::new())
    }
}

pub(super) fn notify(
    view: RuntimeView<'_>,
    job: &NeighborJob,
    block: &Block,
) -> Result<RuntimeOutcome<PistonEvent>, RuntimeError> {
    match (block.kind, job.shape) {
        (BlockKind::RedstoneLamp, false) => lamp(view, job.target, false),
        (BlockKind::Observer, true) => {
            let output = block
                .facing
                .ok_or_else(|| unsupported("observer output required"))?;
            observer(
                view,
                job.target,
                ObserverCallback::Shape,
                along(job.target, output, -1)? == job.source,
            )
        }
        _ => Ok(RuntimeOutcome::default()),
    }
}
