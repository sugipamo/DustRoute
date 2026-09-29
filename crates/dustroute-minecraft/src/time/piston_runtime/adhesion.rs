//! Ordered PistonHandler traversal, shared by extension and sticky retraction.
//! A side contact which cannot adhere is distinct from an obstructed destination.
use super::geometry::{along, facing, movable};
use super::*;
use crate::physical::{self, Adhesion};
use crate::{BlockKind, BlockMove};

fn adhesion(block: &Block) -> Adhesion {
    physical::of_block(block).map_or(Adhesion::None, |p| p.spec().adhesion)
}

// DESTROY blocks never join a side/back attachment.
fn destroys(block: &Block) -> bool {
    physical::of_block(block)
        .is_some_and(|p| p.spec().piston_reaction == physical::PistonReaction::Destroy)
}

/// Collection order is retained independently for moves and destruction.
/// Destroyed blocks do not count toward the movement limit.
pub(super) struct CollectedMotion {
    pub moves: Vec<BlockMove>,
    pub destroyed: Vec<Pos>,
}

fn require_passive_destruction(block: &Block) -> Result<(), RuntimeError> {
    if block
        .observed_name
        .as_deref()
        .and_then(physical::passive::named)
        .is_none()
        || !destroys(block)
    {
        return Err(unsupported(
            "piston destruction callbacks for this block are not modeled",
        ));
    }
    Ok(())
}

pub(super) fn attachable(pos: Pos, block: &Block) -> Result<bool, RuntimeError> {
    if destroys(block) {
        Ok(false)
    } else {
        movable(pos, block)
    }
}

pub(super) fn collect(
    view: RuntimeView<'_>,
    body_pos: Pos,
    body: &Block,
    extending: bool,
) -> Result<Option<CollectedMotion>, RuntimeError> {
    let dir = facing(body)?;
    let geometry = crate::piston_motion_law::builtin_laws()
        .geometry(false, false, view.time().section, 0)
        .expect("fixed geometry facts");
    let start = along(
        body_pos,
        dir,
        if extending {
            geometry.head
        } else {
            geometry.payload
        },
    )?;
    let motion = if extending { dir } else { dir.opposite() };
    let block = view.block(start)?;
    if block.kind != BlockKind::Air && !attachable(start, &block)? {
        if extending && destroys(&block) {
            require_passive_destruction(&block)?;
            return Ok(Some(CollectedMotion {
                moves: Vec::new(),
                destroyed: vec![start],
            }));
        }
        return Ok(None);
    }
    let mut collector = Collector {
        view,
        body_pos,
        front: along(body_pos, dir, geometry.head)?,
        limit: geometry.limit,
        motion,
        extending,
        moved: Vec::new(),
        destroyed: Vec::new(),
    };
    if !collector.line(start)? {
        return Ok(None);
    }
    let mut i = 0;
    while i < collector.moved.len() {
        let pos = collector.moved[i];
        if adhesion(&collector.block(pos)?) != Adhesion::None && !collector.sides(pos)? {
            return Ok(None);
        }
        i += 1;
    }
    let moves = collector
        .moved
        .into_iter()
        .rev()
        .map(|from| {
            Ok(BlockMove {
                from,
                to: along(from, motion, 1)?,
                block: view.block(from)?,
            })
        })
        .collect::<Result<Vec<_>, RuntimeError>>()?;
    Ok(Some(CollectedMotion {
        moves,
        destroyed: collector.destroyed.into_iter().rev().collect(),
    }))
}

struct Collector<'a> {
    view: RuntimeView<'a>,
    body_pos: Pos,
    front: Pos,
    motion: Facing,
    extending: bool,
    moved: Vec<Pos>,
    destroyed: Vec<Pos>,
    limit: usize,
}
impl Collector<'_> {
    fn block(&self, pos: Pos) -> Result<Block, RuntimeError> {
        let b = self.view.block(pos)?;
        // Native move(false) removes the old head before collecting. No shape
        // callbacks run for that flags-276 write, so the read overlay is exact.
        Ok(
            if !self.extending && pos == self.front && b.kind == BlockKind::PistonHead {
                Block::new(BlockKind::Air)
            } else {
                b
            },
        )
    }
    fn line(&mut self, origin: Pos) -> Result<bool, RuntimeError> {
        let mut block = self.block(origin)?;
        if block.kind == BlockKind::Air
            || !attachable(origin, &block)?
            || origin == self.body_pos
            || self.moved.contains(&origin)
        {
            return Ok(true);
        }
        let mut behind = 1;
        if behind + self.moved.len() > self.limit {
            return Ok(false);
        }
        while adhesion(&block) != Adhesion::None {
            let pos = along(origin, self.motion, -(behind as i32))?;
            let next = self.block(pos)?;
            if next.kind == BlockKind::Air
                || !adhesion(&block).sticks_to(adhesion(&next))
                || !attachable(pos, &next)?
                || pos == self.body_pos
            {
                break;
            }
            block = next;
            behind += 1;
            if behind + self.moved.len() > self.limit {
                return Ok(false);
            }
        }
        for n in (0..behind).rev() {
            self.moved.push(along(origin, self.motion, -(n as i32))?);
        }
        let mut added = behind;
        let mut ahead = 1;
        loop {
            let pos = along(origin, self.motion, ahead)?;
            if let Some(index) = self.moved.iter().position(|p| *p == pos) {
                let tail = self.moved.len() - added;
                self.moved[index..].rotate_right(added);
                debug_assert!(index <= tail);
                for i in 0..=index + added {
                    let p = self.moved[i];
                    if adhesion(&self.block(p)?) != Adhesion::None && !self.sides(p)? {
                        return Ok(false);
                    }
                }
                return Ok(true);
            }
            let block = self.block(pos)?;
            if block.kind == BlockKind::Air {
                return Ok(true);
            }
            if destroys(&block) {
                require_passive_destruction(&block)?;
                self.destroyed.push(pos);
                return Ok(true);
            }
            if !movable(pos, &block)? || pos == self.body_pos {
                return Ok(false);
            }
            if self.moved.len() >= self.limit {
                return Ok(false);
            }
            self.moved.push(pos);
            added += 1;
            ahead += 1;
        }
    }
    fn sides(&mut self, pos: Pos) -> Result<bool, RuntimeError> {
        let own = adhesion(&self.block(pos)?);
        for side in crate::piston_electrical::SIDES {
            if side == self.motion || side == self.motion.opposite() {
                continue;
            }
            let neighbor = along(pos, side, 1)?;
            if own.sticks_to(adhesion(&self.block(neighbor)?)) && !self.line(neighbor)? {
                return Ok(false);
            }
        }
        Ok(true)
    }
}

/// Native HashMap<BlockPos> iteration after destination/head keys are removed.
/// At most twelve keys: preserve insertion order within each Java hash bucket.
pub(super) fn vacated(moves: &[BlockMove], head: Option<Pos>) -> Result<Vec<Pos>, RuntimeError> {
    let mut sources: Vec<_> = moves.iter().rev().map(|m| m.from).collect();
    let hash = |p: &Pos| {
        let h =
            p.y.wrapping_add(p.z.wrapping_mul(31))
                .wrapping_mul(31)
                .wrapping_add(p.x) as u32;
        h ^ (h >> 16)
    };
    // Do not invent iteration for the rare Java collision-triggered tree/resize
    // path. The ordinary capacity remains 16 through the twelfth insertion.
    let mut buckets = [0; 16];
    for p in &sources {
        let count = &mut buckets[(hash(p) & 15) as usize];
        *count += 1;
        if *count > 8 {
            return Err(unsupported("piston source-map hash collision limit"));
        }
    }
    sources.sort_by_key(|p| hash(p) & 15);
    sources.retain(|p| Some(*p) != head && !moves.iter().any(|m| m.to == *p));
    Ok(sources)
}
