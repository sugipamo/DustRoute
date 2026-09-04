//! Immutable-shape and atomic world-delta primitives.
//!
//! A Minecraft world contains both geometry and signal state.  The first
//! piston subset does not yet split those into a full signal engine, but it
//! must still expose the boundary explicitly: a mechanical operation produces
//! a validated `WorldDelta`, and applying that delta yields a new shape
//! identity.  Consumers may use the dirty regions to invalidate a local
//! topology cache; the conservative initial implementation does not promise
//! that a local cache is complete.

use std::collections::BTreeSet;
use std::fmt::{Display, Formatter};

use serde::{Deserialize, Serialize};

use crate::{Block, BlockKind, World};

/// Content-derived identity for the geometric part of a world.
///
/// The identifier is intentionally opaque and generation-scoped.  Signal
/// values (`powered`, `power_level`, and ordinary `power`/`lit` observations)
/// do not participate in the identity, while piston extension and block-state
/// geometry do.  Exact before-state checks remain mandatory, so this hash is a
/// fast cache key rather than the sole stale-world proof.
#[derive(
    Clone, Copy, Debug, Default, Deserialize, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize,
)]
pub struct ShapeId(pub u64);

/// Content-derived identity for a complete observed world state.
///
/// Unlike [`ShapeId`], this identity includes signal fields such as powered,
/// lit, and dust strength. It is therefore suitable for identifying the
/// endpoints of a transition; `ShapeId` remains the cheaper geometry/cache
/// identity used by placement and movement validation.
#[derive(
    Clone, Copy, Debug, Default, Deserialize, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize,
)]
pub struct StateId(pub u64);

/// An immutable world view with a content-derived shape identity.
///
/// The wrapped `World` still contains observed signal fields for compatibility
/// with the existing snapshot API.  `ShapeId` deliberately ignores those
/// fields; a future `SignalState` can therefore be introduced without
/// changing the mechanical-delta contract.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Shape {
    world: World,
    id: ShapeId,
}

impl Shape {
    #[must_use]
    pub fn new(world: World) -> Self {
        let id = world.shape_id();
        Self { world, id }
    }

    #[must_use]
    pub const fn id(&self) -> ShapeId {
        self.id
    }

    #[must_use]
    pub const fn world(&self) -> &World {
        &self.world
    }

    #[must_use]
    pub fn into_world(self) -> World {
        self.world
    }

    /// Applies a delta to this immutable view and returns the next shape.
    pub fn apply_delta(&self, delta: &WorldDelta) -> Result<Self, WorldDeltaError> {
        let mut world = self.world.clone();
        world.apply_delta(delta)?;
        Ok(Self::new(world))
    }
}

/// A closed axis-aligned region used for conservative topology invalidation.
#[derive(Clone, Copy, Debug, Deserialize, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize)]
pub struct Region {
    pub min: crate::Pos,
    pub max: crate::Pos,
}

impl Region {
    #[must_use]
    pub fn new(a: crate::Pos, b: crate::Pos) -> Self {
        Self {
            min: crate::Pos::new(a.x.min(b.x), a.y.min(b.y), a.z.min(b.z)),
            max: crate::Pos::new(a.x.max(b.x), a.y.max(b.y), a.z.max(b.z)),
        }
    }

    #[must_use]
    pub fn around(position: crate::Pos, radius: i32) -> Self {
        let radius = radius.max(0);
        Self::new(
            position.offset(-radius, -radius, -radius),
            position.offset(radius, radius, radius),
        )
    }

    #[must_use]
    pub fn contains(&self, position: crate::Pos) -> bool {
        self.min.x <= position.x
            && position.x <= self.max.x
            && self.min.y <= position.y
            && position.y <= self.max.y
            && self.min.z <= position.z
            && position.z <= self.max.z
    }

    #[must_use]
    pub fn intersects(&self, other: &Self) -> bool {
        self.min.x <= other.max.x
            && other.min.x <= self.max.x
            && self.min.y <= other.max.y
            && other.min.y <= self.max.y
            && self.min.z <= other.max.z
            && other.min.z <= self.max.z
    }

    #[must_use]
    pub fn union(self, other: Self) -> Self {
        Self::new(
            crate::Pos::new(
                self.min.x.min(other.min.x),
                self.min.y.min(other.min.y),
                self.min.z.min(other.min.z),
            ),
            crate::Pos::new(
                self.max.x.max(other.max.x),
                self.max.y.max(other.max.y),
                self.max.z.max(other.max.z),
            ),
        )
    }
}

/// A set of dirty regions.  Regions are sorted and de-duplicated, but are not
/// aggressively merged: retaining separate local neighborhoods avoids making
/// a future incremental update scan a huge bounding box for a sparse delta.
#[derive(Clone, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
pub struct RegionSet {
    pub regions: Vec<Region>,
}

impl RegionSet {
    #[must_use]
    pub fn new(regions: impl IntoIterator<Item = Region>) -> Self {
        let mut regions = regions.into_iter().collect::<Vec<_>>();
        regions.sort_unstable();
        regions.dedup();
        Self { regions }
    }

    #[must_use]
    pub fn around_positions(positions: impl IntoIterator<Item = crate::Pos>, radius: i32) -> Self {
        Self::new(
            positions
                .into_iter()
                .map(|position| Region::around(position, radius)),
        )
    }

    #[must_use]
    pub fn contains(&self, position: crate::Pos) -> bool {
        self.regions.iter().any(|region| region.contains(position))
    }

    #[must_use]
    pub fn intersects(&self, region: &Region) -> bool {
        self.regions
            .iter()
            .any(|candidate| candidate.intersects(region))
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.regions.is_empty()
    }
}

/// A reason for one coordinate-level mutation in a delta or causal trace.
#[derive(Clone, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum ChangeReason {
    #[default]
    Unknown,
    PistonMove {
        from: crate::Pos,
        to: crate::Pos,
    },
    PistonState {
        piston: crate::Pos,
    },
    ExternalInput,
    NeighborUpdate,
    RepeaterState {
        repeater: crate::Pos,
    },
}

/// Mechanical cause of a shape transition.  Event IDs remain in the time
/// engine's trace; this enum describes the operation independently of a
/// particular scheduler implementation.
#[derive(Clone, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum DeltaCause {
    #[default]
    Unknown,
    PistonExtend {
        piston: crate::Pos,
    },
    PistonRetract {
        piston: crate::Pos,
    },
    External,
    NeighborUpdate,
    RepeaterTick {
        repeater: crate::Pos,
    },
}

/// One coordinate's expected before/after state.  Air is represented by
/// `BlockKind::Air` even though `World` stores it sparsely.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct BlockChange {
    pub position: crate::Pos,
    pub before: Block,
    pub after: Block,
    #[serde(default)]
    pub reason: ChangeReason,
}

/// A logical move retained alongside its coordinate-level changes.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct BlockMove {
    pub from: crate::Pos,
    pub to: crate::Pos,
    pub block: Block,
}

/// An atomic, validated change from one geometric shape to another.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct WorldDelta {
    pub parent_shape: ShapeId,
    pub changes: Vec<BlockChange>,
    pub moves: Vec<BlockMove>,
    pub dirty_region: RegionSet,
    pub cause: DeltaCause,
}

impl WorldDelta {
    #[must_use]
    pub fn empty(parent_shape: ShapeId, cause: DeltaCause) -> Self {
        Self {
            parent_shape,
            changes: Vec::new(),
            moves: Vec::new(),
            dirty_region: RegionSet::default(),
            cause,
        }
    }

    pub fn changed_positions(&self) -> impl Iterator<Item = crate::Pos> + '_ {
        self.changes.iter().map(|change| change.position)
    }

    /// Checks every expected source state and rejects duplicate coordinate
    /// entries before any mutation can occur.
    pub fn validate(&self, world: &World) -> Result<(), WorldDeltaError> {
        let actual_shape = world.shape_id();
        if actual_shape != self.parent_shape {
            return Err(WorldDeltaError::ParentShapeMismatch {
                expected: self.parent_shape,
                actual: actual_shape,
            });
        }
        let mut positions = BTreeSet::new();
        for change in &self.changes {
            if !positions.insert(change.position) {
                return Err(WorldDeltaError::DuplicatePosition {
                    position: change.position,
                });
            }
            let actual = world
                .get(change.position)
                .cloned()
                .unwrap_or_else(|| Block::new(BlockKind::Air));
            if actual != change.before {
                return Err(WorldDeltaError::BeforeMismatch {
                    position: change.position,
                    expected: Box::new(change.before.clone()),
                    actual: Box::new(actual),
                });
            }
        }
        for movement in &self.moves {
            let source = self
                .changes
                .iter()
                .find(|change| change.position == movement.from);
            let destination = self
                .changes
                .iter()
                .find(|change| change.position == movement.to);
            let source_match = source.is_some_and(|change| {
                represents_moved_block(&change.before, &movement.block)
                    || represents_moved_block(&change.after, &movement.block)
            });
            let destination_match = destination.is_some_and(|change| {
                represents_moved_block(&change.before, &movement.block)
                    || represents_moved_block(&change.after, &movement.block)
            });
            // At extension start (and at the first completion coordinate),
            // the source position is occupied by the moving piston head while
            // the carried block is represented at its destination. Keep this
            // one explicit exception instead of weakening ordinary move
            // validation for arbitrary deltas.
            let source_is_moving_head = source.is_some_and(|change| {
                change.before.kind == BlockKind::MovingPiston
                    || change.after.kind == BlockKind::MovingPiston
            }) && source.is_some_and(|change| {
                [&change.before, &change.after].into_iter().any(|block| {
                    block.kind == BlockKind::MovingPiston
                        && block
                            .piston_entity
                            .as_deref()
                            .is_some_and(|entity| entity.pushed_block.kind == BlockKind::PistonHead)
                })
            });
            let destination_is_moving_head = destination.is_some_and(|change| {
                [&change.before, &change.after].into_iter().any(|block| {
                    block.kind == BlockKind::MovingPiston
                        && block
                            .piston_entity
                            .as_deref()
                            .is_some_and(|entity| entity.pushed_block.kind == BlockKind::PistonHead)
                })
            });
            let represented = (source_match && destination_match)
                || (source_is_moving_head && destination_match)
                || (source_match && destination_is_moving_head);
            if !represented {
                return Err(WorldDeltaError::MoveNotRepresented {
                    from: movement.from,
                    to: movement.to,
                });
            }
        }
        Ok(())
    }

    /// Applies all changes to a staged clone and commits only after every
    /// validation and write succeeds.  This is the sole mutation primitive
    /// used by the piston event handler.
    pub fn apply(&self, world: &mut World) -> Result<(), WorldDeltaError> {
        self.validate(world)?;
        let mut staged = world.clone();
        for change in &self.changes {
            staged.set(change.position, change.after.clone());
        }
        *world = staged;
        Ok(())
    }

    pub fn target_shape(&self, world: &World) -> Result<ShapeId, WorldDeltaError> {
        let mut staged = world.clone();
        self.apply(&mut staged)?;
        Ok(staged.shape_id())
    }
}

fn represents_moved_block(actual: &Block, expected: &Block) -> bool {
    actual == expected
        || (actual.kind == BlockKind::MovingPiston
            && actual
                .piston_entity
                .as_deref()
                .is_some_and(|entity| entity.pushed_block.as_ref() == expected))
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(tag = "error", rename_all = "snake_case")]
pub enum WorldDeltaError {
    ParentShapeMismatch {
        expected: ShapeId,
        actual: ShapeId,
    },
    DuplicatePosition {
        position: crate::Pos,
    },
    BeforeMismatch {
        position: crate::Pos,
        expected: Box<Block>,
        actual: Box<Block>,
    },
    MoveNotRepresented {
        from: crate::Pos,
        to: crate::Pos,
    },
}

impl Display for WorldDeltaError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::ParentShapeMismatch { expected, actual } => {
                write!(
                    formatter,
                    "world shape changed (expected {expected:?}, found {actual:?})"
                )
            }
            Self::DuplicatePosition { position } => write!(
                formatter,
                "world delta contains duplicate position ({}, {}, {})",
                position.x, position.y, position.z
            ),
            Self::BeforeMismatch { position, .. } => write!(
                formatter,
                "world delta before-state mismatch at ({}, {}, {})",
                position.x, position.y, position.z
            ),
            Self::MoveNotRepresented { from, to } => write!(
                formatter,
                "world delta move from ({}, {}, {}) to ({}, {}, {}) is not represented",
                from.x, from.y, from.z, to.x, to.y, to.z
            ),
        }
    }
}

impl std::error::Error for WorldDeltaError {}

/// Computes the geometry identity used by `World::shape_id`.
pub(crate) fn shape_id(world: &World) -> ShapeId {
    let mut hash = 0xcbf29ce484222325_u64;
    for (position, block) in world.iter() {
        hash_value(&mut hash, position.x);
        hash_value(&mut hash, position.y);
        hash_value(&mut hash, position.z);
        hash_value(&mut hash, block.kind);
        hash_option_string(&mut hash, block.observed_name.as_deref());
        hash_value(&mut hash, block.observation_classification);
        hash_option_value(&mut hash, block.facing);
        hash_option_value(&mut hash, block.delay);
        hash_option_value(&mut hash, block.support_offset);
        if let Some(connections) = &block.wire_connections {
            hash_value(&mut hash, true);
            for (facing, connection) in connections {
                hash_value(&mut hash, *facing);
                hash_value(&mut hash, *connection);
            }
        } else {
            hash_value(&mut hash, false);
        }
        hash_option_value(&mut hash, block.piston_variant);
        hash_option_value(&mut hash, block.piston_state);
        hash_piston_parts(&mut hash, block, false);
        for (key, value) in &block.observed_properties {
            if is_signal_property(block.kind, key) {
                continue;
            }
            hash_option_string(&mut hash, Some(key));
            hash_option_string(&mut hash, Some(value));
        }
        // Delimit blocks so that concatenated values cannot change grouping.
        hash_value(&mut hash, 0xff_u8);
    }
    ShapeId(hash)
}

/// Computes the content-derived identity of every observed block state.
/// Missing coordinates remain implicit Air in the sparse world, so the
/// identity is stable for both representations of an empty cell.
pub(crate) fn state_id(world: &World) -> StateId {
    let mut hash = 0xcbf29ce484222325_u64;
    for (position, block) in world.iter() {
        hash_value(&mut hash, position.x);
        hash_value(&mut hash, position.y);
        hash_value(&mut hash, position.z);
        hash_value(&mut hash, block.kind);
        hash_option_string(&mut hash, block.observed_name.as_deref());
        hash_value(&mut hash, block.observation_classification);
        hash_option_value(&mut hash, block.facing);
        hash_option_value(&mut hash, block.powered);
        hash_option_value(&mut hash, block.power_level);
        hash_option_value(&mut hash, block.delay);
        hash_option_value(&mut hash, block.support_offset);
        if let Some(connections) = &block.wire_connections {
            hash_value(&mut hash, true);
            for (facing, connection) in connections {
                hash_value(&mut hash, *facing);
                hash_value(&mut hash, *connection);
            }
        } else {
            hash_value(&mut hash, false);
        }
        hash_option_value(&mut hash, block.piston_variant);
        hash_option_value(&mut hash, block.piston_state);
        hash_piston_parts(&mut hash, block, true);
        for (key, value) in &block.observed_properties {
            hash_option_string(&mut hash, Some(key));
            hash_option_string(&mut hash, Some(value));
        }
        hash_value(&mut hash, 0xff_u8);
    }
    StateId(hash)
}

fn hash_piston_parts(hash: &mut u64, block: &Block, include_signal: bool) {
    if let Some(head) = &block.piston_head {
        hash_value(hash, true);
        hash_value(hash, head.facing);
        hash_value(hash, head.variant);
        hash_value(hash, head.short);
    } else {
        hash_value(hash, false);
    }
    if let Some(entity) = &block.piston_entity {
        hash_value(hash, true);
        hash_value(hash, entity.facing);
        hash_value(hash, entity.extending);
        hash_value(hash, entity.source);
        hash_value(hash, entity.progress);
        hash_carried_block(hash, &entity.pushed_block, include_signal);
    } else {
        hash_value(hash, false);
    }
}

fn hash_carried_block(hash: &mut u64, block: &Block, include_signal: bool) {
    hash_value(hash, block.kind);
    hash_option_string(hash, block.observed_name.as_deref());
    hash_value(hash, block.observation_classification);
    hash_option_value(hash, block.facing);
    if include_signal {
        hash_option_value(hash, block.powered);
        hash_option_value(hash, block.power_level);
    }
    hash_option_value(hash, block.delay);
    hash_option_value(hash, block.support_offset);
    hash_option_value(hash, block.piston_variant);
    hash_option_value(hash, block.piston_state);
    if let Some(head) = &block.piston_head {
        hash_value(hash, true);
        hash_value(hash, head.facing);
        hash_value(hash, head.variant);
        hash_value(hash, head.short);
    } else {
        hash_value(hash, false);
    }
    for (key, value) in &block.observed_properties {
        if !include_signal && is_signal_property(block.kind, key) {
            continue;
        }
        hash_option_string(hash, Some(key));
        hash_option_string(hash, Some(value));
    }
}

fn is_signal_property(kind: BlockKind, key: &str) -> bool {
    matches!(
        (kind, key),
        (BlockKind::RedstoneWire, "power")
            | (BlockKind::RedstoneLamp, "lit")
            | (BlockKind::RedstoneTorch, "lit")
            | (BlockKind::Repeater, "powered")
            | (BlockKind::Comparator, "powered")
            | (BlockKind::Observer, "powered")
            | (BlockKind::Piston, "powered")
            | (BlockKind::Lever, "powered")
            | (BlockKind::Button, "powered")
            | (BlockKind::PressurePlate, "powered")
    )
}

fn hash_option_string(hash: &mut u64, value: Option<&str>) {
    hash_value(hash, value.is_some());
    if let Some(value) = value {
        for byte in value.as_bytes() {
            hash_value(hash, *byte);
        }
        hash_value(hash, 0_u8);
    }
}

fn hash_option_value<T: Copy + std::hash::Hash>(hash: &mut u64, value: Option<T>) {
    hash_value(hash, value.is_some());
    if let Some(value) = value {
        hash_value(hash, value);
    }
}

fn hash_value<T: std::hash::Hash>(hash: &mut u64, value: T) {
    let mut bytes = ValueHasher { state: *hash };
    value.hash(&mut bytes);
    *hash = bytes.state;
}

struct ValueHasher {
    state: u64,
}

impl std::hash::Hasher for ValueHasher {
    fn finish(&self) -> u64 {
        self.state
    }

    fn write(&mut self, bytes: &[u8]) {
        for byte in bytes {
            self.state ^= u64::from(*byte);
            self.state = self.state.wrapping_mul(0x100000001b3);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Facing, PistonState, PistonVariant, Pos};

    #[test]
    fn signal_only_changes_keep_shape_identity() {
        let mut world = World::new();
        let mut lever = Block::new(BlockKind::Lever);
        lever.powered = Some(false);
        world.set(Pos::new(0, 0, 0), lever);
        let before = world.shape_id();
        let state_before = world.state_id();
        world.get_mut(Pos::new(0, 0, 0)).unwrap().powered = Some(true);
        assert_eq!(world.shape_id(), before);
        assert_ne!(world.state_id(), state_before);
    }

    #[test]
    fn piston_state_changes_shape_identity() {
        let mut world = World::new();
        let mut piston = Block::new(BlockKind::Piston);
        piston.facing = Some(Facing::East);
        piston.piston_variant = Some(PistonVariant::Normal);
        piston.piston_state = Some(PistonState::Retracted);
        world.set(Pos::new(0, 0, 0), piston);
        let before = world.shape_id();
        world.get_mut(Pos::new(0, 0, 0)).unwrap().piston_state = Some(PistonState::Extended);
        assert_ne!(world.shape_id(), before);
    }

    #[test]
    fn delta_application_is_atomic_on_before_mismatch() {
        let position = Pos::new(1, 2, 3);
        let mut world = World::new();
        world.set(position, Block::new(BlockKind::Solid));
        let delta = WorldDelta {
            parent_shape: world.shape_id(),
            changes: vec![BlockChange {
                position,
                before: Block::new(BlockKind::Transparent),
                after: Block::new(BlockKind::Air),
                reason: ChangeReason::Unknown,
            }],
            moves: Vec::new(),
            dirty_region: RegionSet::around_positions([position], 1),
            cause: DeltaCause::External,
        };
        let before = world.clone();
        assert!(matches!(
            delta.apply(&mut world),
            Err(WorldDeltaError::BeforeMismatch { .. })
        ));
        assert_eq!(world, before);
    }
}
