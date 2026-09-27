//! PistonBlock.move writes each destination and drains its shape callbacks
//! before registering the captured payload. Vacated sources are cleared only
//! after all destinations. The plan is ordinary checkpointed continuation data.
use super::electrical::write_shape_jobs;
use super::geometry::delta;
use super::*;
use crate::{BlockKind, DeltaCause};

#[derive(Clone, Debug, Eq, PartialEq, serde::Serialize)]
pub struct MotionPlan {
    steps: VecDeque<MotionStep>,
    cause: DeltaCause,
    after: Option<Box<PistonEvent>>,
}

#[derive(Clone, Debug, Eq, PartialEq, serde::Serialize)]
enum MotionStep {
    Write { position: Pos, block: Box<Block> },
    Shapes { position: Pos, block: Box<Block> },
    Register { position: Pos, block: Box<Block> },
    Notify { jobs: VecDeque<NeighborJob> },
}

impl MotionPlan {
    pub(super) fn new(cause: DeltaCause, after: Option<PistonEvent>) -> Self {
        Self {
            steps: VecDeque::new(),
            cause,
            after: after.map(Box::new),
        }
    }
    pub(super) fn write(&mut self, position: Pos, block: Block, shapes: bool) {
        self.steps.push_back(MotionStep::Write {
            position,
            block: Box::new(block.clone()),
        });
        if shapes {
            self.steps.push_back(MotionStep::Shapes {
                position,
                block: Box::new(block.clone()),
            });
        }
        if block.kind == BlockKind::MovingPiston {
            self.steps.push_back(MotionStep::Register {
                position,
                block: Box::new(block),
            });
        }
    }
    pub(super) fn notify(&mut self, jobs: VecDeque<NeighborJob>) {
        self.steps.push_back(MotionStep::Notify { jobs });
    }
    pub(super) fn event(self) -> PistonEvent {
        PistonEvent::Motion {
            plan: Box::new(self),
        }
    }
}

pub(super) fn step(
    view: RuntimeView<'_>,
    pos: Pos,
    plan: &MotionPlan,
) -> Result<RuntimeOutcome<PistonEvent>, RuntimeError> {
    let mut next = plan.clone();
    let Some(stage) = next.steps.pop_front() else {
        return Ok(RuntimeOutcome {
            continuation: next.after.map(|p| *p),
            ..Default::default()
        });
    };
    let cause = next.cause.clone();
    let mut out = RuntimeOutcome {
        continuation: Some(next.event()),
        ..Default::default()
    };
    match stage {
        MotionStep::Write { position, block } => {
            let before = view.block(position)?;
            let mut after = *block;
            if after.kind == BlockKind::MovingPiston {
                out.carriers.push(CarrierEffect::Stage {
                    position,
                    block: Box::new(after.clone()),
                });
                after.piston_entity = None;
            }
            // WorldChunk invokes onStateReplaced with MOVED before the
            // enclosing setBlockState reaches its shape pass.
            if before.kind != after.kind && crate::device_program::program(before.kind).is_some() {
                out.callbacks.push(call(
                    pos,
                    PistonEvent::Notify {
                        jobs: super::devices::removed_jobs(view, position, &before)?,
                    },
                ));
            }
            // A displacement spans several writes, so it is not an atomic
            // WorldDelta::moves entry. Its captured payload is in this plan.
            out.delta = Some(delta(view, [(position, after)], vec![], cause)?);
        }
        MotionStep::Shapes { position, block } => {
            let mut after = *block;
            after.piston_entity = None;
            if view.block(position)? == after {
                // Movable payloads have no wire prepare callback. This pass
                // is evaluated after any removal notification has drained.
                out.callbacks.push(call(
                    pos,
                    PistonEvent::Notify {
                        jobs: write_shape_jobs(
                            view,
                            position,
                            &Block::new(BlockKind::Air),
                            &after,
                        )?,
                    },
                ));
            }
        }
        MotionStep::Register { position, block } => {
            if view.staged_carriers.get(&position) != Some(block.as_ref()) {
                return Err(RuntimeError::CarrierConflict(position));
            }
            out.delta = Some(delta(view, [(position, *block)], vec![], cause)?);
            out.carriers.push(CarrierEffect::Install {
                position,
                history: MotionHistory::fresh(),
            });
            out.continuation = Some(PistonEvent::Arm {
                positions: vec![position],
                after: out.continuation.map(Box::new),
            });
        }
        MotionStep::Notify { jobs } => out.callbacks.push(call(pos, PistonEvent::Notify { jobs })),
    }
    Ok(out)
}
