//! Bounded client-side piston state. Received state packets remain independently available.
//! This is a local reconstruction, never independent confirmation of server state.
mod adhesion;
mod motion;
mod rules;
mod wire_updates;
pub(crate) use motion::Action;
#[cfg(test)]
mod tests;

use super::world::World;
use crate::{NativeBlockState, Observation};
use rules::{Kind, classify, state};
use std::collections::BTreeMap;

type Pos = [i32; 3];
type ApplyResult<T> = std::result::Result<T, ReconstructionIssue>;

/// The native piston direction numbering, independent of protocol-specific packet IDs.
#[derive(Clone, Copy, Debug, Eq, PartialEq, serde::Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Direction {
    /// Negative Y.
    Down,
    /// Positive Y.
    Up,
    /// Negative Z.
    North,
    /// Positive Z.
    South,
    /// Negative X.
    West,
    /// Positive X.
    East,
}
impl Direction {
    const ALL: [Self; 6] = [
        Self::Down,
        Self::Up,
        Self::North,
        Self::South,
        Self::West,
        Self::East,
    ];
    const HORIZONTAL: [Self; 4] = [Self::North, Self::East, Self::South, Self::West];
    // AbstractBlock's shape update order, not Direction.values().
    const SHAPE_ORDER: [Self; 6] = [
        Self::West,
        Self::East,
        Self::North,
        Self::South,
        Self::Down,
        Self::Up,
    ];
    pub(crate) fn from_id(id: u8) -> Option<Self> {
        Self::ALL.get(usize::from(id)).copied()
    }
    fn from_name(name: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|d| d.name() == name)
    }
    pub(crate) fn name(self) -> &'static str {
        ["down", "up", "north", "south", "west", "east"][self as usize]
    }
    fn opposite(self) -> Self {
        Self::ALL[(self as usize) ^ 1]
    }
    fn horizontal(self) -> bool {
        !matches!(self, Self::Down | Self::Up)
    }
    fn left(self) -> Self {
        match self {
            Self::North => Self::West,
            Self::West => Self::South,
            Self::South => Self::East,
            Self::East => Self::North,
            _ => self,
        }
    }
    fn offset(self, p: Pos, n: i32) -> Pos {
        let d = [
            [0, -1, 0],
            [0, 1, 0],
            [0, 0, -1],
            [0, 0, 1],
            [-1, 0, 0],
            [1, 0, 0],
        ][self as usize];
        std::array::from_fn(|i| p[i] + d[i] * n)
    }
}

/// Exact half-step progress used by the native moving block entity.
#[derive(Clone, Copy, Debug, Eq, PartialEq, serde::Serialize)]
#[serde(rename_all = "snake_case")]
pub enum MotionProgress {
    /// Progress 0.
    Start,
    /// Progress 0.5.
    Half,
    /// Progress 1; materialization can still be pending.
    Full,
}
/// Role of a moving block entity; each carrier has its own lifetime.
#[derive(Clone, Copy, Debug, Eq, PartialEq, serde::Serialize)]
#[serde(rename_all = "snake_case")]
pub enum CarrierRole {
    /// Retracting piston body.
    Body,
    /// Extending head.
    Head,
    /// A transported block.
    Payload,
}
/// Local moving block state derived from a received piston action.
#[derive(Clone, Debug, serde::Serialize)]
pub struct MovingBlock {
    /// Carrier coordinate.
    pub position: Pos,
    /// Block carried by the moving block entity.
    pub carried: NativeBlockState,
    /// Piston facing, including during retraction.
    pub direction: Direction,
    /// Whether this carrier extends.
    pub extending: bool,
    /// Body, head or transported payload.
    pub role: CarrierRole,
    /// Current half-step.
    pub progress: MotionProgress,
    /// Previous half-step, retained separately.
    pub last_progress: MotionProgress,
    /// Client completion waits already consumed (0..5).
    pub completion_waits: u8,
    /// Receive sequence which created this carrier.
    pub action_sequence: Option<u64>,
    /// Chunk receive sequence when restored from native block-entity data.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub chunk_sequence: Option<u64>,
}
impl MovingBlock {
    fn origin(&self) -> StateOrigin {
        match self.action_sequence {
            Some(action_sequence) => StateOrigin::ClientUpdate { action_sequence },
            None => StateOrigin::ChunkUpdate {
                chunk_sequence: self.chunk_sequence.expect("carrier provenance"),
            },
        }
    }
}
/// A reason client reconstruction cannot provide a usable state.
#[derive(Clone, Debug, Eq, PartialEq, serde::Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum ReconstructionIssue {
    /// Client tick-rate/freeze semantics outside the validated 20 TPS mode.
    UnsupportedTickControl,
    /// A needed block lies in an unavailable chunk or outside the dimension.
    MissingBlock {
        /// Coordinate that could not be read.
        position: Pos,
    },
    /// No client rule has been validated for this block/state.
    UnsupportedBlock {
        /// Coordinate requiring the rule.
        position: Pos,
        /// Native identifier.
        name: String,
    },
    /// A moving state arrived without a known carried state.
    MissingCarrier {
        /// Carrier coordinate.
        position: Pos,
    },
    /// Data dependencies crossed a chunk invalidation boundary.
    ChunkInvalidated {
        /// Chunk x,z.
        chunk: [i32; 2],
    },
    /// The bounded local update could not finish.
    Limit,
    /// Unexpected state or event fields; no approximate result is adopted.
    InvalidAction,
}
/// Provenance of the chosen block state.
#[derive(Clone, Copy, Debug, serde::Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum StateOrigin {
    /// Latest explicitly received state in this cache.
    Received,
    /// Local effect of a received block action.
    ClientUpdate {
        /// Causal received action.
        action_sequence: u64,
    },
    /// Local completion/shape update from native moving-piston chunk data.
    ChunkUpdate {
        /// Causal chunk receive sequence; not a block-action sequence.
        chunk_sequence: u64,
    },
    /// Missing raw data or an unresolved local update.
    Unavailable,
}
impl From<u64> for StateOrigin {
    fn from(action_sequence: u64) -> Self {
        Self::ClientUpdate { action_sequence }
    }
}
/// Both sources for one cell; the original received state lives in `received`.
#[derive(Clone, Debug, serde::Serialize)]
pub struct ClientBlock {
    /// Absolute coordinate.
    pub position: Pos,
    /// Usable client state; absent on incomplete reconstruction.
    pub state: Option<NativeBlockState>,
    /// How the state was obtained.
    pub origin: StateOrigin,
    /// Per-carrier intermediate state when present.
    pub moving: Option<MovingBlock>,
}
/// A client view and its independently retained received-packet snapshot.
#[derive(Clone, Debug, serde::Serialize)]
pub struct ClientObservation {
    /// Original cache; never overwritten by local calculations.
    pub received: Observation,
    /// Current dimension identity.
    pub dimension: String,
    /// Local simulation frame, not a server game tick.
    pub client_tick: u64,
    /// Local world-effects revision.
    pub client_revision: u64,
    /// Client states for the same region.
    pub blocks: Vec<ClientBlock>,
    /// First unresolved effect in this dimension.
    pub issue: Option<ReconstructionIssue>,
    /// Full chunk snapshots still required before invalidated local effects can recover.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub recovery_chunks: Vec<[i32; 2]>,
}

#[derive(Clone, Default)]
pub(crate) struct Reconstruction {
    overlay: BTreeMap<Pos, (NativeBlockState, StateOrigin)>,
    moving: BTreeMap<Pos, MovingBlock>,
    // Native block entities tick in registration order, not sorted coordinate order.
    order: Vec<Pos>,
    dependencies: std::collections::BTreeSet<[i32; 2]>,
    pub recovery_chunks: std::collections::BTreeSet<[i32; 2]>,
    pub tick: u64,
    pub revision: u64,
    pub issue: Option<ReconstructionIssue>,
}
impl Reconstruction {
    pub fn cell(&self, world: &World, p: Pos) -> ClientBlock {
        let received = world.block(p).and_then(|id| super::native_state(id).ok());
        let unknown_carrier = self
            .overlay
            .get(&p)
            .map(|(s, _)| s)
            .or(received.as_ref())
            .is_some_and(|s| s.name == "minecraft:moving_piston")
            && !self.moving.contains_key(&p);
        let (state, origin) = if self.issue.is_some() || unknown_carrier {
            (None, StateOrigin::Unavailable)
        } else if let Some((s, origin)) = self.overlay.get(&p) {
            (Some(s.clone()), *origin)
        } else {
            let origin = if received.is_some() {
                StateOrigin::Received
            } else {
                StateOrigin::Unavailable
            };
            (received, origin)
        };
        let moving = state.as_ref().and_then(|_| self.moving.get(&p).cloned());
        ClientBlock {
            position: p,
            state,
            origin,
            moving,
        }
    }
    fn read(&mut self, world: &World, p: Pos) -> ApplyResult<NativeBlockState> {
        self.dependencies
            .insert([p[0].div_euclid(16), p[2].div_euclid(16)]);
        if let Some((s, _)) = self.overlay.get(&p) {
            return Ok(s.clone());
        }
        let state = world
            .block(p)
            .and_then(|id| super::native_state(id).ok())
            .ok_or(ReconstructionIssue::MissingBlock { position: p })?;
        if state.name == "minecraft:moving_piston" && !self.moving.contains_key(&p) {
            return Err(ReconstructionIssue::MissingCarrier { position: p });
        }
        Ok(state)
    }
    fn kind(&mut self, world: &World, p: Pos) -> ApplyResult<(NativeBlockState, Kind)> {
        let s = self.read(world, p)?;
        let k = classify(&s).ok_or_else(|| ReconstructionIssue::UnsupportedBlock {
            position: p,
            name: s.name.clone(),
        })?;
        Ok((s, k))
    }
    fn put(
        &mut self,
        world: &World,
        p: Pos,
        s: NativeBlockState,
        origin: impl Into<StateOrigin>,
        shape: bool,
        depth: usize,
    ) -> ApplyResult<()> {
        if depth > 128 || self.overlay.len() >= 65_536 {
            return Err(ReconstructionIssue::Limit);
        }
        let before = self.read(world, p)?;
        if before == s {
            return Ok(());
        }
        let origin = origin.into();
        self.overlay.insert(p, (s.clone(), origin));
        self.revision += 1;
        if shape {
            self.prepare(world, p, &before, origin, depth + 1)?;
            self.neighbors(world, p, origin, depth + 1)?;
            self.prepare(world, p, &s, origin, depth + 1)?;
        }
        Ok(())
    }
    fn neighbors(
        &mut self,
        world: &World,
        p: Pos,
        origin: impl Into<StateOrigin>,
        depth: usize,
    ) -> ApplyResult<()> {
        let origin = origin.into();
        for direction in Direction::SHAPE_ORDER {
            let target = direction.offset(p, 1);
            self.update_neighbor(world, target, direction.opposite(), origin, depth)?;
        }
        Ok(())
    }
    fn update_neighbor(
        &mut self,
        world: &World,
        target: Pos,
        from: Direction,
        origin: StateOrigin,
        depth: usize,
    ) -> ApplyResult<()> {
        if depth > 128 {
            return Err(ReconstructionIssue::Limit);
        }
        let (before, kind) = self.kind(world, target)?;
        let after = match kind {
            Kind::Stairs if from.horizontal() => self.stair_shape(world, target, before.clone())?,
            Kind::Lever if Self::lever_support(target, &before)?.0 == from.offset(target, 1) => {
                if self.lever_supported(world, target, &before)? {
                    before.clone()
                } else {
                    state("air", &[])
                }
            }
            Kind::Head if rules::facing(&before)?.opposite() == from => {
                if self.head_supported(world, target, &before)? {
                    before.clone()
                } else {
                    state("air", &[])
                }
            }
            Kind::Gate { .. } if from == Direction::Down => {
                if self.full_face(world, from.offset(target, 1), Direction::Up)? {
                    before.clone()
                } else {
                    state("air", &[])
                }
            }
            Kind::Wire => self.wire_update(world, target, before.clone(), from)?,
            _ => before.clone(),
        };
        if before != after {
            self.put(world, target, after, origin, true, depth)?;
        }
        Ok(())
    }
    pub fn received(&mut self, changes: &[(Pos, i32)]) {
        for (p, id) in changes {
            self.overlay.remove(p);
            if super::native_state(*id).is_ok_and(|s| s.name == "minecraft:moving_piston") {
                if !self.moving.contains_key(p) {
                    self.issue
                        .get_or_insert(ReconstructionIssue::MissingCarrier { position: *p });
                }
            } else {
                self.moving.remove(p);
                self.order.retain(|pos| pos != p);
            }
        }
        self.revision += 1;
    }
    pub fn chunk_replaced(&mut self, chunk: [i32; 2]) {
        if self.recovery_chunks.is_empty()
            && (self.dependencies.contains(&chunk)
                || matches!(self.issue, Some(ReconstructionIssue::MissingCarrier { position }) if [position[0].div_euclid(16), position[2].div_euclid(16)] == chunk))
            && (self.issue.is_none()
                || matches!(self.issue, Some(ReconstructionIssue::MissingCarrier { .. })))
        {
            self.recovery_chunks
                .extend(self.dependencies.iter().copied());
            self.recovery_chunks.insert(chunk);
            self.issue = Some(ReconstructionIssue::ChunkInvalidated { chunk });
            self.overlay.clear();
            self.moving.clear();
            self.order.clear();
        }
        if !self.recovery_chunks.is_empty() && self.dependencies.contains(&chunk) {
            self.recovery_chunks.insert(chunk);
        }
        self.overlay
            .retain(|p, _| [p[0].div_euclid(16), p[2].div_euclid(16)] != chunk);
        self.moving
            .retain(|p, _| [p[0].div_euclid(16), p[2].div_euclid(16)] != chunk);
        self.order.retain(|p| self.moving.contains_key(p));
        self.revision += 1;
    }
    pub fn chunk_loaded(
        &mut self,
        chunk: [i32; 2],
        pistons: Vec<(Pos, super::piston_nbt::PistonData)>,
        sequence: u64,
    ) {
        self.chunk_replaced(chunk);
        if self.moving.len() + pistons.len() > 4096 {
            self.recovery_chunks.clear();
            self.issue = Some(ReconstructionIssue::Limit);
            return;
        }
        for (position, data) in pistons {
            self.dependencies.insert(chunk);
            self.order.push(position);
            self.moving.insert(
                position,
                MovingBlock {
                    position,
                    carried: data.carried,
                    direction: data.direction,
                    extending: data.extending,
                    role: data.role,
                    progress: data.progress,
                    last_progress: data.progress,
                    completion_waits: 0,
                    action_sequence: None,
                    chunk_sequence: Some(sequence),
                },
            );
        }
        if self.recovery_chunks.remove(&chunk) && self.recovery_chunks.is_empty() {
            self.issue = None;
            self.dependencies = self
                .moving
                .keys()
                .map(|p| [p[0].div_euclid(16), p[2].div_euclid(16)])
                .collect();
        }
    }
    pub fn unsupported_ticking(&mut self) {
        // A fresh chunk does not restore unsupported clock semantics.
        self.recovery_chunks.clear();
        self.issue = Some(ReconstructionIssue::UnsupportedTickControl);
        self.revision += 1;
    }
    pub fn advance(&mut self, world: &World, target_tick: u64) {
        if target_tick <= self.tick {
            return;
        }
        // All active carriers settle in at most eight client frames; no server timers run here.
        let steps = (target_tick - self.tick).min(8);
        if self.issue.is_none() {
            let mut next = self.clone();
            for _ in 0..steps {
                if let Err(issue) = next.step(world) {
                    self.issue = Some(issue);
                    break;
                }
            }
            if self.issue.is_none() {
                *self = next;
            }
        }
        self.tick = target_tick;
    }
}
