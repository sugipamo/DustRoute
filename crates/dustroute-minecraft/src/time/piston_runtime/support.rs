//! Attachment lifetime follows native callbacks, not a post-write world sweep.
use super::electrical::write_shape_jobs;
use super::geometry::{along, delta, offset};
use super::notifications::adjacent_jobs;
use super::*;
use crate::physical::SupportTrigger;
use crate::{BlockKind, DeltaCause, PistonState};

pub(super) fn query(
    view: RuntimeView<'_>,
    pos: Pos,
    block: &Block,
    query: crate::physical::lifetime::SupportQuery,
) -> Result<bool, RuntimeError> {
    query.evaluate(block, &mut |d| view.block(offset(pos, d)?))
}

pub(super) fn delayed_update(
    view: RuntimeView<'_>,
    pos: Pos,
    block: &Block,
    shape: bool,
) -> Result<RuntimeOutcome<PistonEvent>, RuntimeError> {
    let mut out = RuntimeOutcome::default();
    if let Some(spec) = crate::physical::plants::of_block(block)
        && shape
        && !query(view, pos, block, spec.support.query)?
    {
        out.queued.push(QueueRequest::PrioritizedBlockTick {
            game_tick: next_tick(view.time().game_tick, spec.support.delay)?,
            priority: spec.support.priority,
            block: BlockIdentity::of(block),
            call: call(pos, PistonEvent::SupportTick),
        });
    }
    Ok(out)
}

pub(super) fn tick(
    view: RuntimeView<'_>,
    pos: Pos,
) -> Result<RuntimeOutcome<PistonEvent>, RuntimeError> {
    let block = view.block(pos)?;
    let Some(spec) = crate::physical::plants::of_block(&block) else {
        return Ok(RuntimeOutcome::default());
    };
    if !query(view, pos, &block, spec.support.query)? {
        return remove(view, pos, &block, false);
    }
    Ok(RuntimeOutcome::default())
}

pub(super) fn present(
    view: RuntimeView<'_>,
    pos: Pos,
    block: &Block,
) -> Result<bool, RuntimeError> {
    let Some(attachment) = block.support_offset else {
        return Err(unsupported("attachment needs an adjacent support"));
    };
    let side = crate::piston_electrical::SIDES
        .into_iter()
        .find(|d| d.offset() == attachment)
        .ok_or_else(|| unsupported("attachment support must be adjacent"))?;
    let support = view.block(offset(pos, attachment)?)?;
    let physical = crate::physical::of_kind(block.kind);
    if support.kind == BlockKind::MovingPiston {
        // A registered retracting source retains the stationary extended base
        // in its collision/support shape. A staged, unregistered carrier does
        // not yet have a block entity and provides no such support.
        if let Some(carrier) = support.piston_entity.as_deref()
            && carrier.source
            && !carrier.extending
            && carrier.pushed_block.kind == BlockKind::Piston
        {
            let mut base = (*carrier.pushed_block).clone();
            base.piston_state = Some(PistonState::Extended);
            return Ok(physical.supports_attachment(&base, side));
        }
    }
    Ok(physical.supports_attachment(&support, side))
}

pub(super) fn should_break(
    view: RuntimeView<'_>,
    pos: Pos,
    block: &Block,
    shape: bool,
    source: Option<Pos>,
) -> Result<bool, RuntimeError> {
    let Some(rule) = crate::physical::of_kind(block.kind).support_loss() else {
        return Ok(false);
    };
    let applies = if shape {
        matches!(
            rule.trigger,
            SupportTrigger::Shape | SupportTrigger::ShapeAndNeighbor
        ) && block.support_pos(pos) == source
    } else {
        matches!(
            rule.trigger,
            SupportTrigger::Neighbor | SupportTrigger::ShapeAndNeighbor
        )
    };
    Ok(applies && !present(view, pos, block)?)
}

/// World.breakBlock/removeBlock write air with flags 3. The previous state's
/// removal callback queues notifications before ordinary and shape updates.
/// All queued notifications drain after this updater entry returns.
pub(super) fn remove(
    view: RuntimeView<'_>,
    pos: Pos,
    before: &Block,
    shape: bool,
) -> Result<RuntimeOutcome<PistonEvent>, RuntimeError> {
    let after = Block::new(BlockKind::Air);
    let change = delta(
        view,
        [(pos, after.clone())],
        vec![],
        DeltaCause::NeighborUpdate,
    )?;
    let mut world = view.world().clone();
    change
        .apply(&mut world)
        .map_err(|e| RuntimeError::Invalid(e.to_string()))?;
    let changed = RuntimeView {
        world: &world,
        ..view
    };
    let mut jobs = super::command::removed_notification_jobs(changed, pos, before)?;
    if before.kind == BlockKind::RedstoneWire {
        jobs.extend(super::command::removed_wire_jobs(changed, pos, before)?);
    }
    jobs.extend(adjacent_jobs(changed, pos, false, false)?);
    jobs.extend(write_shape_jobs(changed, pos, before, &after)?);
    if !shape
        && crate::physical::of_kind(before.kind)
            .support_loss()
            .is_some_and(|r| r.notify_around_neighbors)
    {
        for side in crate::piston_electrical::SIDES {
            jobs.extend(adjacent_jobs(changed, along(pos, side, 1)?, false, false)?);
        }
    }
    Ok(RuntimeOutcome {
        delta: Some(change),
        callbacks: vec![call(pos, PistonEvent::Notify { jobs })],
        ..Default::default()
    })
}
