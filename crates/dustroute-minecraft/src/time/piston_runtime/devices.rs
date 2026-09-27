//! Shared device query/effect interpreter. No concrete device owns delivery.
use super::geometry::{along, delta, offset};
use super::*;
use crate::DeltaCause;
use crate::device_program::{
    self, Callback, DeviceRun, NotifyTargets, Query, ResolvedEffect, WriteNotifications,
};

fn prepare(
    view: RuntimeView<'_>,
    pos: Pos,
    before: &Block,
    callback: Callback,
    source: Option<Pos>,
) -> Result<Option<DeviceRun>, RuntimeError> {
    let Some(program) = device_program::program(before.kind) else {
        return if callback == Callback::Use {
            Err(unsupported("device has no use handler"))
        } else {
            Ok(None)
        };
    };
    if callback == Callback::Use && !program.definition.handlers.contains_key(&callback) {
        return Err(unsupported("device has no use handler"));
    }
    program
        .prepare(callback, before, |query| {
            let read = || -> Result<u16, RuntimeError> {
                Ok(match query {
                    Query::Constant { value } => *value,
                    Query::Powered => before
                        .powered
                        .ok_or_else(|| unsupported("device power required"))?
                        .into(),
                    Query::ReceivingPower => {
                        super::electrical::world(view)?.receiving_power(pos)?.into()
                    }
                    Query::TickQueued => view
                        .block_tick_queued(pos, &BlockIdentity::of(before))
                        .into(),
                    Query::SourceAtFront => {
                        let facing = before
                            .facing
                            .ok_or_else(|| unsupported("device output required"))?;
                        (source == Some(along(pos, facing, -1)?)).into()
                    }
                })
            };
            read().map_err(|e| e.to_string())
        })
        .map_err(unsupported)
}

pub(super) fn start(
    view: RuntimeView<'_>,
    pos: Pos,
    callback: Callback,
    source: Option<Pos>,
    captured: Option<&Block>,
) -> Result<RuntimeOutcome<PistonEvent>, RuntimeError> {
    let current;
    let before = match captured {
        Some(block) => block,
        None => {
            current = view.block(pos)?;
            &current
        }
    };
    let Some(run) = prepare(view, pos, before, callback, source)? else {
        return Ok(RuntimeOutcome::default());
    };
    step(view, pos, run)
}

fn tick(
    view: RuntimeView<'_>,
    pos: Pos,
    before: &Block,
    delay: u64,
    priority: u8,
) -> Result<QueueRequest<PistonEvent>, RuntimeError> {
    Ok(QueueRequest::PrioritizedBlockTick {
        game_tick: next_tick(view.time().game_tick, delay)?,
        priority,
        block: BlockIdentity::of(before),
        call: call(
            pos,
            PistonEvent::Device {
                callback: Callback::Tick,
                source: None,
                captured: None,
            },
        ),
    })
}

pub(super) fn notification_jobs(
    view: RuntimeView<'_>,
    pos: Pos,
    before: &Block,
    targets: NotifyTargets,
) -> Result<VecDeque<NeighborJob>, RuntimeError> {
    match targets {
        NotifyTargets::Output => super::electrical::repeater_jobs_for(view, pos, before),
        NotifyTargets::SelfAndSupport => {
            let support = before
                .support_offset
                .ok_or_else(|| unsupported("device support required"))?;
            let mut jobs = super::notifications::adjacent_jobs(view, pos, false, false)?;
            jobs.extend(super::notifications::adjacent_jobs(
                view,
                offset(pos, support)?,
                false,
                false,
            )?);
            Ok(jobs)
        }
    }
}

pub(super) fn step(
    view: RuntimeView<'_>,
    pos: Pos,
    mut run: DeviceRun,
) -> Result<RuntimeOutcome<PistonEvent>, RuntimeError> {
    let program = device_program::program(run.before.kind)
        .ok_or_else(|| unsupported("missing continuation device"))?;
    if run.revision != device_program::REVISION || run.definition != program.definition.id {
        return Err(unsupported("device continuation revision mismatch"));
    }
    let mut out = RuntimeOutcome::default();
    let Some(effect) = run.effects.get(run.next).cloned() else {
        return Ok(out);
    };
    run.next += 1;
    match effect {
        ResolvedEffect::WritePower {
            powered,
            notifications,
        } => {
            // A definition admits at most one write, before its nested effects.
            // The captured state cannot overwrite an intervening replacement.
            if view.block(pos)? != *run.before {
                return Err(unsupported("device changed before its write"));
            }
            let mut after = *run.before.clone();
            after.powered = Some(powered);
            if after.observed_name.is_some() {
                after.observed_properties.insert(
                    program.definition.power_property.clone(),
                    powered.to_string(),
                );
            }
            let mut jobs = VecDeque::new();
            if notifications == WriteNotifications::NeighborsAndShapes {
                jobs.extend(super::notifications::adjacent_jobs(
                    view, pos, false, false,
                )?);
            }
            if notifications != WriteNotifications::None {
                jobs.extend(super::electrical::write_shape_jobs(
                    view,
                    pos,
                    &run.before,
                    &after,
                )?);
            }
            out.delta = Some(delta(
                view,
                [(pos, after)],
                vec![],
                DeltaCause::NeighborUpdate,
            )?);
            if !jobs.is_empty() {
                out.callbacks.push(call(pos, PistonEvent::Notify { jobs }));
            }
        }
        ResolvedEffect::Schedule { delay, priority } => {
            out.queued
                .push(tick(view, pos, &run.before, delay, priority)?)
        }
        ResolvedEffect::Notify { targets } => {
            let jobs = notification_jobs(view, pos, &run.before, targets)?;
            if !jobs.is_empty() {
                out.callbacks.push(call(pos, PistonEvent::Notify { jobs }));
            }
        }
    }
    if run.next < run.effects.len() {
        out.continuation = Some(PistonEvent::DeviceContinue { run: Box::new(run) });
    }
    Ok(out)
}

/// Shape queries against requested state before insertion. Compilation restricts
/// these programs to queue operations; no temporary world mutation is allowed.
pub(super) fn preprocess(
    view: RuntimeView<'_>,
    pos: Pos,
    requested: &Block,
    source: Pos,
) -> Result<Vec<QueueRequest<PistonEvent>>, RuntimeError> {
    if !device_program::program(requested.kind).is_some_and(|p| p.definition.preprocess_shapes) {
        return Ok(vec![]);
    }
    let Some(run) = prepare(view, pos, requested, Callback::Shape, Some(source))? else {
        return Ok(vec![]);
    };
    run.effects
        .into_iter()
        .map(|effect| match effect {
            ResolvedEffect::Schedule { delay, priority } => {
                tick(view, pos, requested, delay, priority)
            }
            _ => Err(unsupported("pre-write device attempted a world mutation")),
        })
        .collect()
}

/// Movement must deliver removal effects before its next write/shape step.
/// The generic Removed contract is notification-only and samples the old state.
pub(super) fn removed_jobs(
    view: RuntimeView<'_>,
    pos: Pos,
    before: &Block,
) -> Result<VecDeque<NeighborJob>, RuntimeError> {
    let mut jobs = VecDeque::new();
    if let Some(run) = prepare(view, pos, before, Callback::Removed, None)? {
        for effect in run.effects {
            match effect {
                ResolvedEffect::Notify { targets } => {
                    jobs.extend(notification_jobs(view, pos, before, targets)?)
                }
                _ => return Err(unsupported("removal program attempted a world mutation")),
            }
        }
    }
    Ok(jobs)
}

pub(super) fn event(pos: Pos, callback: Callback, source: Option<Pos>) -> RuntimeCall<PistonEvent> {
    call(
        pos,
        PistonEvent::Device {
            callback,
            source,
            captured: None,
        },
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{BlockKind, Facing};

    #[test]
    fn removing_a_pressed_device_notifies_its_support_and_guards_the_old_tick() {
        let pos = Pos::new(0, 4, 0);
        let lamp = pos.offset(1, 0, 0);
        let mut world = World::new();
        world.place(BlockKind::Solid, pos.offset(0, -1, 0));
        let button = world.place(BlockKind::Button, pos);
        button.powered = Some(false);
        button.support_offset = Some(Facing::Down.offset());
        world.place(BlockKind::RedstoneLamp, lamp).powered = Some(false);
        let mut runtime = new_piston_runtime(
            world,
            Region::new(Pos::new(-6, -4, -6), Pos::new(6, 12, 6)),
            Default::default(),
        )
        .unwrap();
        schedule_device_use_after_tick(&mut runtime, 1, pos).unwrap();
        // Exercise the already modeled command callback at an external root.
        // Public construction deliberately requires idle state.
        runtime
            .0
            .enqueue(QueueRequest::AfterWorldTick {
                game_tick: 5,
                call: call(pos, PistonEvent::ElectricalRemove),
            })
            .unwrap();
        runtime.run_until_idle().unwrap();
        assert_eq!(runtime.view().block(pos).unwrap().kind, BlockKind::Air);
        assert_eq!(runtime.view().block(lamp).unwrap().powered, Some(false));
        assert!(
            runtime
                .trace()
                .iter()
                .any(|record| record.invocation.time.game_tick == 21
                    && record.result == DeliveryResult::BlockReplaced)
        );
    }
}
