use super::geometry::*;
use super::*;
use crate::piston_motion_law::builtin_laws;
use crate::{Block, BlockKind, DeltaCause, Pos, piston_state};

pub(super) fn adjacent_jobs(
    view: RuntimeView<'_>,
    source: Pos,
    include_self: bool,
    shape: bool,
) -> Result<VecDeque<NeighborJob>, RuntimeError> {
    let mut jobs = VecDeque::new();
    if include_self {
        jobs.push_back(NeighborJob {
            target: source,
            source,
            shape: false,
        });
    }
    for index in 0..6 {
        let v = builtin_laws()
            .geometry(false, false, view.time().section, index)
            .expect("bounded neighbor index")
            .neighbor_offset;
        let target = offset(source, v)?;
        view.block(target)?;
        jobs.push_back(NeighborJob {
            target,
            source,
            shape: false,
        });
    }
    if shape {
        jobs.extend(shape_jobs(view, source)?);
    }
    Ok(jobs)
}

pub(super) fn shape_jobs(
    view: RuntimeView<'_>,
    source: Pos,
) -> Result<VecDeque<NeighborJob>, RuntimeError> {
    (0..6)
        .map(|index| {
            let v = builtin_laws()
                .geometry(false, false, view.time().section, index)
                .expect("bounded shape index")
                .shape_offset;
            let target = offset(source, v)?;
            view.block(target)?;
            Ok(NeighborJob {
                target,
                source,
                shape: true,
            })
        })
        .collect()
}

pub(super) fn notification(
    view: RuntimeView<'_>,
    target: Pos,
    sources: &[Pos],
    include_self: bool,
) -> Result<RuntimeCall<PistonEvent>, RuntimeError> {
    let mut jobs = VecDeque::new();
    for source in sources {
        jobs.extend(adjacent_jobs(view, *source, include_self, true)?);
    }
    if jobs.len() > view.limits().max_pending {
        return Err(RuntimeError::Limit("notification batch"));
    }
    Ok(call(target, PistonEvent::Notify { jobs }))
}

/// One Java updater entry returns before its nested notifications are drained.
/// Pending entries are values in the continuation, so their order and context
/// survive checkpoints and cannot admit an external input halfway through.
pub(super) fn notify(
    view: RuntimeView<'_>,
    mut jobs: VecDeque<NeighborJob>,
) -> Result<RuntimeOutcome<PistonEvent>, RuntimeError> {
    let mut out = RuntimeOutcome::default();
    let Some(job) = jobs.pop_front() else {
        return Ok(out);
    };
    let block = view.block(job.target)?;
    if matches!(
        block.kind,
        BlockKind::RedstoneWire
            | BlockKind::Repeater
            | BlockKind::Observer
            | BlockKind::RedstoneLamp
    ) {
        let mut out = match block.kind {
            BlockKind::Observer | BlockKind::RedstoneLamp => {
                super::devices::notify(view, &job, &block)?
            }
            _ => super::electrical::notify_device(view, &job, &block)?,
        };
        // Device callbacks drain before the enclosing neighbor sequence resumes.
        if !jobs.is_empty() {
            out.continuation = Some(PistonEvent::Notify { jobs });
        }
        return Ok(out);
    }
    let mut nested = VecDeque::new();
    if job.shape {
        if block.kind == BlockKind::PistonHead {
            let dir = block
                .facing
                .ok_or_else(|| unsupported("head facing is missing"))?;
            if along(job.target, dir, -1)? == job.source
                && !head_supported(view, job.target, &block)?
            {
                out.delta = Some(delta(
                    view,
                    [(job.target, Block::new(BlockKind::Air))],
                    Vec::new(),
                    DeltaCause::NeighborUpdate,
                )?);
                nested = adjacent_jobs(view, job.target, false, true)?;
            }
        }
    } else {
        match block.kind {
            BlockKind::Piston => {
                let mut facts = control_facts(view, job.target, &block)?;
                if facts.powered && !piston_state(&block).is_extended() {
                    facts.can_push = push_moves(view, job.target, &block)?.is_some();
                }
                if let Some(event) = builtin_laws().control(facts).request {
                    out.queued.push(QueueRequest::BlockEvent {
                        block: BlockIdentity::of(&block),
                        call: call(
                            job.target,
                            PistonEvent::Block {
                                event,
                                facing: facing(&block)?,
                            },
                        ),
                    });
                }
            }
            BlockKind::PistonHead => {
                if head_supported(view, job.target, &block)? {
                    nested.push_back(NeighborJob {
                        target: along(job.target, block.facing.expect("checked head"), -1)?,
                        source: job.source,
                        shape: false,
                    });
                }
            }
            BlockKind::Air
            | BlockKind::Solid
            | BlockKind::Transparent
            | BlockKind::Lever
            | BlockKind::RedstoneBlock
            | BlockKind::MovingPiston => {}
            other => {
                return Err(unsupported(format!(
                    "unsupported notification target {other:?}"
                )));
            }
        }
    }
    nested.append(&mut jobs);
    if nested.len() > view.limits().max_pending {
        return Err(RuntimeError::Limit("notification batch"));
    }
    if !nested.is_empty() {
        out.continuation = Some(PistonEvent::Notify { jobs: nested });
    }
    Ok(out)
}
