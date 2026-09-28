//! Pure passive state transforms share the existing synchronous shape queue.
use super::electrical::{world, write_shape_jobs};
use super::geometry::{along, delta};
use super::*;
use crate::DeltaCause;
use crate::physical::stairs;

pub(super) fn transformed(
    view: RuntimeView<'_>,
    pos: Pos,
    block: &Block,
) -> Result<Block, RuntimeError> {
    let Some(state) = stairs::state(block) else {
        return Ok(block.clone());
    };
    let query = world(view)?;
    let shape = state
        .neighbor_shape(|side| query.block(along(pos, side, 1)?).map(|b| stairs::state(&b)))?;
    let mut after = block.clone();
    after
        .observed_properties
        .insert("shape".into(), shape.name().into());
    Ok(after)
}

pub(super) fn notify(
    view: RuntimeView<'_>,
    job: &NeighborJob,
    before: &Block,
) -> Result<RuntimeOutcome<PistonEvent>, RuntimeError> {
    let mut out = RuntimeOutcome::default();
    // Vertical shape updates leave dry stairs unchanged. Ordinary neighbor
    // callbacks do not poll/recompute their shape.
    if !job.shape || job.source.y != job.target.y {
        return Ok(out);
    }
    let after = transformed(view, job.target, before)?;
    if &after != before {
        let jobs = write_shape_jobs(view, job.target, before, &after)?;
        out.delta = Some(delta(
            view,
            [(job.target, after)],
            vec![],
            DeltaCause::NeighborUpdate,
        )?);
        out.callbacks
            .push(call(job.target, PistonEvent::Notify { jobs }));
    }
    Ok(out)
}
