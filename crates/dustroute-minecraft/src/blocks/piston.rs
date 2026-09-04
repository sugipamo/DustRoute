use std::collections::{BTreeMap, BTreeSet};
use std::error::Error;
use std::fmt::{Display, Formatter};

use serde::{Deserialize, Serialize};

use super::{BlockBehaviorProfile, UpdateModel};
use crate::{
    Block, BlockChange, BlockKind, BlockMove, BlockProperties, ChangeReason, DeltaCause, Facing,
    ObservationClassification, PistonState, PistonVariant, Pos, Region, RegionSet, ShapeId, World,
    WorldDelta, WorldDeltaError,
};

pub(super) const PROFILE: BlockBehaviorProfile = BlockBehaviorProfile {
    properties: BlockProperties::support_only(true),
    update_model: UpdateModel::BlockEvent,
    order_sensitive: true,
};

/// The Java piston push limit. A plan fails closed when the contiguous set
/// would exceed this limit instead of silently moving a prefix.
pub const PISTON_PUSH_LIMIT: usize = 12;

/// A phase-aware piston motion profile.  The initial delay is a range because
/// Java can start an actuation in the current game tick or the next one,
/// depending on which scheduler phase delivered the power update.  Once the
/// block event starts, the stable movement completion is modeled separately.
/// Zero is a valid lower bound and is intentionally not special-cased.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct PistonMotionProfile {
    pub initial_delay_min_game_ticks: u64,
    pub initial_delay_max_game_ticks: u64,
    /// Compatibility field name.  This is the delay from the already
    /// delivered Block Event to the stable block-state transition; it is not
    /// the continuous piston animation duration.
    pub movement_game_ticks: u64,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum PistonMotionProfileError {
    InvalidInitialDelayRange {
        minimum_game_ticks: u64,
        maximum_game_ticks: u64,
    },
}

impl Display for PistonMotionProfileError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::InvalidInitialDelayRange {
                minimum_game_ticks,
                maximum_game_ticks,
            } => write!(
                formatter,
                "piston initial delay range is invalid: minimum {minimum_game_ticks} exceeds maximum {maximum_game_ticks}"
            ),
        }
    }
}

impl Error for PistonMotionProfileError {}

impl Default for PistonMotionProfile {
    fn default() -> Self {
        Self {
            initial_delay_min_game_ticks: 0,
            initial_delay_max_game_ticks: 1,
            movement_game_ticks: 2,
        }
    }
}

/// The initial Java profile used by the event engine. One redstone tick is
/// two game ticks; the profile keeps the phase range separate from the
/// movement interval so a later measured version can replace either value.
pub const DEFAULT_PISTON_MOTION_PROFILE: PistonMotionProfile = PistonMotionProfile {
    initial_delay_min_game_ticks: 0,
    initial_delay_max_game_ticks: 1,
    movement_game_ticks: 2,
};

impl PistonMotionProfile {
    /// Validates interval invariants before a profile is used by the event
    /// engine. Zero is valid for either bound and for the stable transition.
    pub fn validate(self) -> Result<(), PistonMotionProfileError> {
        if self.initial_delay_min_game_ticks > self.initial_delay_max_game_ticks {
            return Err(PistonMotionProfileError::InvalidInitialDelayRange {
                minimum_game_ticks: self.initial_delay_min_game_ticks,
                maximum_game_ticks: self.initial_delay_max_game_ticks,
            });
        }
        Ok(())
    }

    /// Returns the activation-side interval. The current Block Event runner
    /// does not consume it because it starts after activation has already
    /// been resolved by an upper scheduler.
    #[must_use]
    pub const fn activation_delay_game_ticks(self) -> (u64, u64) {
        (
            self.initial_delay_min_game_ticks,
            self.initial_delay_max_game_ticks,
        )
    }

    /// Returns the stable block-state delay after a Block Event starts. This
    /// name is preferred over the legacy `movement_game_ticks` field.
    #[must_use]
    pub const fn stable_completion_delay_game_ticks(self) -> u64 {
        self.movement_game_ticks
    }
}

/// A caller-provided completeness boundary for piston planning.
///
/// Every coordinate in `known_region` must have been observed in one static
/// snapshot. Coordinates outside it are treated as `Unknown`, never as Air,
/// and a plan fails closed before it can under-count a push chain. The
/// unchecked [`plan_piston`] function remains available for synthetic worlds;
/// live callers should use this context.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct PistonPlanningContext {
    pub known_region: Region,
}

impl PistonPlanningContext {
    #[must_use]
    pub const fn new(known_region: Region) -> Self {
        Self { known_region }
    }

    pub fn plan(
        self,
        world: &World,
        position: Pos,
        action: PistonAction,
    ) -> Result<PistonPlan, PistonError> {
        plan_piston_in_region(world, self.known_region, position, action)
    }
}

/// Blocks that are known not to move in the targeted Java Edition subset.
/// Unknown/coarse observations are rejected separately; this list prevents a
/// known `bedrock` observation from being mistaken for an ordinary `Solid`.
#[must_use]
pub fn observed_name_is_immovable(name: &str) -> bool {
    let short_name = name.strip_prefix("minecraft:").unwrap_or(name);
    matches!(
        short_name,
        "bedrock"
            | "obsidian"
            | "crying_obsidian"
            | "reinforced_deepslate"
            | "end_portal_frame"
            | "end_portal"
            | "nether_portal"
            | "moving_piston"
            | "piston_head"
            | "barrier"
            | "structure_block"
            | "jigsaw"
            | "command_block"
            | "chain_command_block"
            | "repeating_command_block"
    )
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, Hash, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum PistonAction {
    Extend,
    Retract,
}

/// Compatibility name for the generic move relation emitted in a
/// `WorldDelta`. New code may use [`crate::BlockMove`] directly.
pub type PistonBlockMove = BlockMove;

/// A validated, read-only movement plan. Creating a plan does not mutate the
/// world; `apply` performs an all-or-nothing stale-plan check before changing
/// it. The initial supported subset deliberately models stable piston states
/// and ordinary movable blocks only. The plan's `delta` is the stable
/// completion transition; [`Self::start_delta`] exposes the separate moving
/// state used by the event engine. Stable piston heads and transient moving
/// block entities are retained in the two deltas rather than collapsed into
/// a single body-state change.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct PistonPlan {
    pub parent_shape: ShapeId,
    pub piston: Pos,
    pub variant: PistonVariant,
    pub facing: Facing,
    pub action: PistonAction,
    pub state_before: PistonState,
    pub state_after: PistonState,
    pub moved: Vec<PistonBlockMove>,
    pub piston_before: Block,
    pub piston_after: Block,
    /// Transient moving-piston transition. Plans deserialized from an older
    /// schema may omit it and use the legacy body-only start transition.
    #[serde(default)]
    pub moving_delta: Option<WorldDelta>,
    pub delta: WorldDelta,
}

impl PistonPlan {
    #[must_use]
    pub fn moved_count(&self) -> usize {
        self.moved.len()
    }

    /// Returns a compatibility view of the final coordinate states.  New
    /// callers should consume [`Self::world_delta`] so source/destination move
    /// relations and dirty-region metadata are not lost.
    #[must_use]
    pub fn world_changes(&self) -> Vec<(Pos, Block)> {
        self.delta
            .changes
            .iter()
            .map(|change| (change.position, change.after.clone()))
            .collect()
    }

    /// Returns the immutable shape transition represented by this plan.
    #[must_use]
    pub const fn world_delta(&self) -> &WorldDelta {
        &self.delta
    }

    /// Returns the state transition at the beginning of an actuation. The
    /// coordinate changes install Vanilla-like moving carriers (and retain
    /// logical move relations) so observers and a later completion event
    /// cannot be collapsed into the trigger tick.
    #[must_use]
    pub fn start_delta(&self) -> WorldDelta {
        if let Some(delta) = &self.moving_delta {
            return delta.clone();
        }
        let mut piston_after = self.piston_before.clone();
        set_piston_state(
            &mut piston_after,
            match self.action {
                PistonAction::Extend => PistonState::Extending,
                PistonAction::Retract => PistonState::Retracting,
            },
        );
        WorldDelta {
            parent_shape: self.parent_shape,
            changes: vec![BlockChange {
                position: self.piston,
                before: self.piston_before.clone(),
                after: piston_after,
                reason: ChangeReason::PistonState {
                    piston: self.piston,
                },
            }],
            moves: Vec::new(),
            dirty_region: RegionSet::around_positions([self.piston], 1),
            cause: delta_cause(self.action, self.piston),
        }
    }

    /// Rebases the stable completion plan after [`Self::start_delta`] has
    /// been applied.  The moved blocks are still checked against their
    /// original states, so an external mutation during the moving interval
    /// fails closed instead of applying a stale chain.
    pub fn completion_plan(&self, world: &World) -> Result<Self, PistonError> {
        let Some(piston) = world.get(self.piston).cloned() else {
            return Err(PistonError::StalePlan {
                position: self.piston,
            });
        };
        let expected_state = match self.action {
            PistonAction::Extend => PistonState::Extending,
            PistonAction::Retract => PistonState::Retracting,
        };
        if piston_state(&piston) != expected_state {
            return Err(PistonError::StalePlan {
                position: self.piston,
            });
        }
        let mut expected_piston = self.piston_before.clone();
        set_piston_state(&mut expected_piston, expected_state);
        if !piston_geometry_matches(&piston, &expected_piston) {
            return Err(PistonError::StalePlan {
                position: self.piston,
            });
        }
        let start = self.start_delta();
        for change in &start.changes {
            if change.position == self.piston {
                continue;
            }
            let actual = world
                .get(change.position)
                .cloned()
                .unwrap_or_else(|| Block::new(BlockKind::Air));
            if actual != change.after {
                return Err(PistonError::StalePlan {
                    position: change.position,
                });
            }
        }
        let mut completion = self.clone();
        completion.parent_shape = world.shape_id();
        completion.piston_before = piston.clone();
        completion.piston_after = piston;
        set_piston_state(&mut completion.piston_after, completion.state_after);
        completion.delta = piston_delta(world, &completion);
        Ok(completion)
    }

    /// Applies the plan atomically with respect to the in-memory world. All
    /// preconditions are checked before the first mutation, so a stale plan or
    /// changed destination cannot leave a half-moved circuit behind.
    pub fn apply(&self, world: &mut World) -> Result<(), PistonError> {
        validate_plan(self, world)?;
        self.delta
            .apply(world)
            .map_err(|error| PistonError::DeltaApplication(Box::new(error)))
    }
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(tag = "error", rename_all = "snake_case")]
pub enum PistonError {
    MissingPiston {
        position: Pos,
    },
    WrongBlock {
        position: Pos,
        actual: BlockKind,
    },
    UnsupportedFacing {
        position: Pos,
        facing: Facing,
    },
    InvalidState {
        position: Pos,
        action: PistonAction,
        state: PistonState,
    },
    Obstructed {
        position: Pos,
        kind: BlockKind,
    },
    UnsupportedMovingBlock {
        position: Pos,
        kind: BlockKind,
        reason: String,
    },
    UnknownSpace {
        position: Pos,
    },
    PushLimitExceeded {
        limit: usize,
        attempted: usize,
    },
    DestinationOccupied {
        position: Pos,
        kind: BlockKind,
    },
    StalePlan {
        position: Pos,
    },
    DeltaApplication(Box<WorldDeltaError>),
}

impl Display for PistonError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::MissingPiston { position } => write!(
                formatter,
                "no piston exists at ({}, {}, {})",
                position.x, position.y, position.z
            ),
            Self::WrongBlock { position, actual } => write!(
                formatter,
                "expected a piston at ({}, {}, {}), found {actual:?}",
                position.x, position.y, position.z
            ),
            Self::UnsupportedFacing { position, facing } => write!(
                formatter,
                "piston at ({}, {}, {}) has unsupported facing {facing:?}",
                position.x, position.y, position.z
            ),
            Self::InvalidState {
                position,
                action,
                state,
            } => write!(
                formatter,
                "cannot {action:?} piston at ({}, {}, {}) from {state:?}",
                position.x, position.y, position.z
            ),
            Self::Obstructed { position, kind } => write!(
                formatter,
                "piston movement is obstructed at ({}, {}, {}) by {kind:?}",
                position.x, position.y, position.z
            ),
            Self::UnsupportedMovingBlock {
                position,
                kind,
                reason,
            } => write!(
                formatter,
                "block {kind:?} at ({}, {}, {}) is outside the piston subset: {reason}",
                position.x, position.y, position.z
            ),
            Self::UnknownSpace { position } => write!(
                formatter,
                "piston planning reached an unknown coordinate at ({}, {}, {}); the observed region is incomplete",
                position.x, position.y, position.z
            ),
            Self::PushLimitExceeded { limit, attempted } => write!(
                formatter,
                "piston push limit {limit} exceeded by {attempted} blocks"
            ),
            Self::DestinationOccupied { position, kind } => write!(
                formatter,
                "piston pull destination at ({}, {}, {}) is occupied by {kind:?}",
                position.x, position.y, position.z
            ),
            Self::StalePlan { position } => write!(
                formatter,
                "piston plan is stale at ({}, {}, {})",
                position.x, position.y, position.z
            ),
            Self::DeltaApplication(error) => {
                write!(formatter, "piston delta could not apply: {error}")
            }
        }
    }
}

impl Error for PistonError {}

/// Returns the physical variant while preserving compatibility with legacy
/// blocks that only retain the observed Minecraft identifier.
#[must_use]
pub fn piston_variant(block: &Block) -> PistonVariant {
    block.piston_variant.unwrap_or_else(|| {
        if block
            .observed_name
            .as_deref()
            .is_some_and(|name| name.trim_start_matches("minecraft:") == "sticky_piston")
        {
            PistonVariant::Sticky
        } else {
            PistonVariant::Normal
        }
    })
}

/// Returns the stable/transient state while preserving compatibility with
/// snapshots produced before the typed piston fields existed.
#[must_use]
pub fn piston_state(block: &Block) -> PistonState {
    block.piston_state.unwrap_or_else(|| {
        block
            .observed_properties
            .get("extended")
            .and_then(|value| value.parse::<bool>().ok())
            .filter(|extended| *extended)
            .map_or(PistonState::Retracted, |_| PistonState::Extended)
    })
}

/// Builds a movement plan without mutating `world`.
pub fn plan_piston(
    world: &World,
    position: Pos,
    action: PistonAction,
) -> Result<PistonPlan, PistonError> {
    plan_piston_with_region(world, None, position, action)
}

/// Builds a piston plan only inside a complete, static observation region.
/// Unlike [`plan_piston`], an absent block is interpreted as Air only when the
/// coordinate is inside `known_region`; leaving that region returns
/// [`PistonError::UnknownSpace`].
pub fn plan_piston_in_region(
    world: &World,
    known_region: Region,
    position: Pos,
    action: PistonAction,
) -> Result<PistonPlan, PistonError> {
    plan_piston_with_region(world, Some(known_region), position, action)
}

fn plan_piston_with_region(
    world: &World,
    known_region: Option<Region>,
    position: Pos,
    action: PistonAction,
) -> Result<PistonPlan, PistonError> {
    ensure_known(known_region, position)?;
    let piston = world
        .get(position)
        .cloned()
        .ok_or(PistonError::MissingPiston { position })?;
    if piston.kind != BlockKind::Piston {
        return Err(PistonError::WrongBlock {
            position,
            actual: piston.kind,
        });
    }
    let facing = piston.facing.ok_or(PistonError::UnsupportedFacing {
        position,
        facing: Facing::North,
    })?;
    let Some(offset) = facing.horizontal_offset() else {
        return Err(PistonError::UnsupportedFacing { position, facing });
    };
    let before = piston_state(&piston);
    if !before.is_stable()
        || matches!(
            (action, before),
            (PistonAction::Extend, PistonState::Extended)
                | (PistonAction::Retract, PistonState::Retracted)
        )
    {
        return Err(PistonError::InvalidState {
            position,
            action,
            state: before,
        });
    }
    let variant = piston_variant(&piston);
    let moved = match action {
        PistonAction::Extend => extension_moves(world, known_region, position, offset)?,
        PistonAction::Retract if variant == PistonVariant::Sticky => {
            retraction_moves(world, known_region, position, offset)?
        }
        PistonAction::Retract => Vec::new(),
    };
    let after = match action {
        PistonAction::Extend => PistonState::Extended,
        PistonAction::Retract => PistonState::Retracted,
    };
    let mut piston_after = piston.clone();
    set_piston_state(&mut piston_after, after);
    let parent_shape = world.shape_id();
    let mut plan = PistonPlan {
        parent_shape,
        piston: position,
        variant,
        facing,
        action,
        state_before: before,
        state_after: after,
        moved,
        piston_before: piston,
        piston_after,
        moving_delta: None,
        delta: WorldDelta::empty(
            parent_shape,
            match action {
                PistonAction::Extend => DeltaCause::PistonExtend { piston: position },
                PistonAction::Retract => DeltaCause::PistonRetract { piston: position },
            },
        ),
    };
    plan.delta = piston_delta(world, &plan);
    plan.moving_delta = Some(piston_start_delta(world, &plan));
    Ok(plan)
}

fn piston_delta(world: &World, plan: &PistonPlan) -> WorldDelta {
    // A coordinate can be both the source of one block and the destination of
    // another in a push chain.  WorldDelta is a final-state diff, so collapse
    // those two operations into one before/after entry while retaining every
    // logical relation in `moves`.
    let mut by_position = BTreeMap::<Pos, BlockChange>::new();
    for movement in &plan.moved {
        let source_before = world
            .get(movement.from)
            .cloned()
            .unwrap_or_else(|| Block::new(BlockKind::Air));
        let source = by_position
            .entry(movement.from)
            .or_insert_with(|| BlockChange {
                position: movement.from,
                before: source_before,
                after: Block::new(BlockKind::Air),
                reason: ChangeReason::PistonMove {
                    from: movement.from,
                    to: movement.to,
                },
            });
        source.after = Block::new(BlockKind::Air);
        source.reason = ChangeReason::PistonMove {
            from: movement.from,
            to: movement.to,
        };

        let destination = by_position
            .entry(movement.to)
            .or_insert_with(|| BlockChange {
                position: movement.to,
                before: world
                    .get(movement.to)
                    .cloned()
                    .unwrap_or_else(|| Block::new(BlockKind::Air)),
                after: movement.block.clone(),
                reason: ChangeReason::PistonMove {
                    from: movement.from,
                    to: movement.to,
                },
            });
        destination.after = movement.block.clone();
        destination.reason = ChangeReason::PistonMove {
            from: movement.from,
            to: movement.to,
        };
    }
    by_position.insert(
        plan.piston,
        BlockChange {
            position: plan.piston,
            before: plan.piston_before.clone(),
            after: plan.piston_after.clone(),
            reason: ChangeReason::PistonState {
                piston: plan.piston,
            },
        },
    );

    let head_position = plan.piston.offset(
        plan.facing.offset().x,
        plan.facing.offset().y,
        plan.facing.offset().z,
    );
    let head_before = world
        .get(head_position)
        .cloned()
        .unwrap_or_else(|| Block::new(BlockKind::Air));
    let head_after = match plan.action {
        PistonAction::Extend => piston_head_block(&plan.piston_before, plan.facing, plan.variant),
        PistonAction::Retract => plan
            .moved
            .iter()
            .find(|movement| movement.to == head_position)
            .map(|movement| movement.block.clone())
            .unwrap_or_else(|| Block::new(BlockKind::Air)),
    };
    if head_before != head_after {
        let entry = by_position
            .entry(head_position)
            .or_insert_with(|| BlockChange {
                position: head_position,
                before: head_before,
                after: head_after.clone(),
                reason: if plan.action == PistonAction::Retract && !plan.moved.is_empty() {
                    ChangeReason::PistonMove {
                        from: plan.moved[0].from,
                        to: plan.moved[0].to,
                    }
                } else {
                    ChangeReason::PistonState {
                        piston: plan.piston,
                    }
                },
            });
        entry.after = head_after;
        if plan.action == PistonAction::Extend || plan.moved.is_empty() {
            entry.reason = ChangeReason::PistonState {
                piston: plan.piston,
            };
        }
    }

    let mut changes = Vec::with_capacity(by_position.len());
    let mut added = BTreeSet::new();
    for movement in &plan.moved {
        for position in [movement.from, movement.to] {
            if added.insert(position) {
                changes.push(by_position[&position].clone());
            }
        }
    }
    if added.insert(plan.piston) {
        changes.push(by_position[&plan.piston].clone());
    }
    if added.insert(head_position) {
        changes.push(by_position[&head_position].clone());
    }
    let mut affected = Vec::with_capacity(plan.moved.len() * 2 + 2);
    affected.push(plan.piston);
    affected.push(head_position);
    for movement in &plan.moved {
        affected.push(movement.from);
        affected.push(movement.to);
    }
    WorldDelta {
        parent_shape: plan.parent_shape,
        changes,
        moves: plan
            .moved
            .iter()
            .map(|movement| BlockMove {
                from: movement.from,
                to: movement.to,
                block: movement.block.clone(),
            })
            .collect(),
        dirty_region: RegionSet::around_positions(affected, 1),
        cause: delta_cause(plan.action, plan.piston),
    }
}

/// Builds the transient world state installed when the block event starts.
/// Each moved destination becomes a `MovingPiston` carrying the original
/// block, while the head coordinate carries a typed piston-head state. This
/// keeps stable block identity and block-entity metadata distinct without
/// pretending to model the continuous animation progress.
fn piston_start_delta(world: &World, plan: &PistonPlan) -> WorldDelta {
    let mut by_position = BTreeMap::<Pos, BlockChange>::new();
    for movement in &plan.moved {
        let source_before = world
            .get(movement.from)
            .cloned()
            .unwrap_or_else(|| Block::new(BlockKind::Air));
        by_position.insert(
            movement.from,
            BlockChange {
                position: movement.from,
                before: source_before,
                after: if plan.action == PistonAction::Retract {
                    moving_block(movement.block.clone(), plan.facing, false, false, 1)
                } else {
                    Block::new(BlockKind::Air)
                },
                reason: ChangeReason::PistonMove {
                    from: movement.from,
                    to: movement.to,
                },
            },
        );
        let destination_before = world
            .get(movement.to)
            .cloned()
            .unwrap_or_else(|| Block::new(BlockKind::Air));
        by_position.insert(
            movement.to,
            BlockChange {
                position: movement.to,
                before: destination_before,
                after: moving_block(
                    movement.block.clone(),
                    plan.facing,
                    plan.action == PistonAction::Extend,
                    false,
                    if plan.action == PistonAction::Extend {
                        0
                    } else {
                        1
                    },
                ),
                reason: ChangeReason::PistonMove {
                    from: movement.from,
                    to: movement.to,
                },
            },
        );
    }

    let head_position = plan.piston.offset(
        plan.facing.offset().x,
        plan.facing.offset().y,
        plan.facing.offset().z,
    );
    let head_before = world
        .get(head_position)
        .cloned()
        .unwrap_or_else(|| Block::new(BlockKind::Air));
    by_position.insert(
        head_position,
        BlockChange {
            position: head_position,
            before: head_before,
            after: moving_block(
                piston_head_block(&plan.piston_before, plan.facing, plan.variant),
                plan.facing,
                plan.action == PistonAction::Extend,
                true,
                if plan.action == PistonAction::Extend {
                    0
                } else {
                    1
                },
            ),
            reason: ChangeReason::PistonState {
                piston: plan.piston,
            },
        },
    );

    let mut piston_after = plan.piston_before.clone();
    set_piston_state(
        &mut piston_after,
        match plan.action {
            PistonAction::Extend => PistonState::Extending,
            PistonAction::Retract => PistonState::Retracting,
        },
    );
    by_position.insert(
        plan.piston,
        BlockChange {
            position: plan.piston,
            before: plan.piston_before.clone(),
            after: piston_after,
            reason: ChangeReason::PistonState {
                piston: plan.piston,
            },
        },
    );

    let mut changes = Vec::with_capacity(by_position.len());
    let mut added = BTreeSet::new();
    if added.insert(plan.piston) {
        changes.push(by_position[&plan.piston].clone());
    }
    for movement in &plan.moved {
        for position in [movement.from, movement.to] {
            if added.insert(position) {
                changes.push(by_position[&position].clone());
            }
        }
    }
    if added.insert(head_position) {
        changes.push(by_position[&head_position].clone());
    }
    let mut affected = Vec::with_capacity(changes.len());
    affected.extend(changes.iter().map(|change| change.position));
    WorldDelta {
        parent_shape: plan.parent_shape,
        changes,
        moves: plan
            .moved
            .iter()
            .map(|movement| BlockMove {
                from: movement.from,
                to: movement.to,
                block: movement.block.clone(),
            })
            .collect(),
        dirty_region: RegionSet::around_positions(affected, 1),
        cause: delta_cause(plan.action, plan.piston),
    }
}

fn moving_block(
    pushed_block: Block,
    facing: Facing,
    extending: bool,
    source: bool,
    progress: u8,
) -> Block {
    Block::moving_piston(crate::PistonBlockEntityState {
        pushed_block: Box::new(pushed_block),
        facing,
        extending,
        source,
        progress,
    })
}

fn piston_head_block(piston: &Block, facing: Facing, variant: PistonVariant) -> Block {
    let mut head = Block::piston_head(facing, variant, false);
    if piston.observed_name.is_some() {
        head.observed_name = Some("minecraft:piston_head".to_owned());
        head.observation_classification = piston.observation_classification;
        head.observed_properties
            .insert("facing".to_owned(), facing_name(facing).to_owned());
        head.observed_properties
            .insert("type".to_owned(), variant_name(variant).to_owned());
        head.observed_properties
            .insert("short".to_owned(), "false".to_owned());
    }
    head
}

fn facing_name(facing: Facing) -> &'static str {
    match facing {
        Facing::North => "north",
        Facing::East => "east",
        Facing::South => "south",
        Facing::West => "west",
        Facing::Up => "up",
        Facing::Down => "down",
    }
}

fn variant_name(variant: PistonVariant) -> &'static str {
    match variant {
        PistonVariant::Normal => "normal",
        PistonVariant::Sticky => "sticky",
    }
}

fn delta_cause(action: PistonAction, piston: Pos) -> DeltaCause {
    match action {
        PistonAction::Extend => DeltaCause::PistonExtend { piston },
        PistonAction::Retract => DeltaCause::PistonRetract { piston },
    }
}

fn extension_moves(
    world: &World,
    known_region: Option<Region>,
    piston: Pos,
    offset: Pos,
) -> Result<Vec<PistonBlockMove>, PistonError> {
    let mut cursor = piston.offset(offset.x, offset.y, offset.z);
    let mut contiguous = Vec::new();
    loop {
        ensure_known(known_region, cursor)?;
        let Some(block) = world.get(cursor).cloned() else {
            break;
        };
        if contiguous.len() == PISTON_PUSH_LIMIT {
            return Err(PistonError::PushLimitExceeded {
                limit: PISTON_PUSH_LIMIT,
                attempted: contiguous.len() + 1,
            });
        }
        ensure_movable(cursor, &block)?;
        contiguous.push((cursor, block));
        cursor = cursor.offset(offset.x, offset.y, offset.z);
    }
    Ok(contiguous
        .into_iter()
        .rev()
        .map(|(from, block)| PistonBlockMove {
            from,
            to: from.offset(offset.x, offset.y, offset.z),
            block,
        })
        .collect())
}

fn retraction_moves(
    world: &World,
    known_region: Option<Region>,
    piston: Pos,
    offset: Pos,
) -> Result<Vec<PistonBlockMove>, PistonError> {
    let destination = piston.offset(offset.x, offset.y, offset.z);
    ensure_known(known_region, destination)?;
    if let Some(block) = world.get(destination)
        && !is_piston_head(block)
    {
        return Err(PistonError::DestinationOccupied {
            position: destination,
            kind: block.kind,
        });
    }
    let source = destination.offset(offset.x, offset.y, offset.z);
    ensure_known(known_region, source)?;
    let Some(block) = world.get(source).cloned() else {
        return Ok(Vec::new());
    };
    ensure_movable(source, &block)?;
    Ok(vec![PistonBlockMove {
        from: source,
        to: destination,
        block,
    }])
}

fn is_piston_head(block: &Block) -> bool {
    block.kind == BlockKind::PistonHead
        || block
            .observed_name
            .as_deref()
            .is_some_and(|name| name.trim_start_matches("minecraft:") == "piston_head")
}

fn ensure_known(known_region: Option<Region>, position: Pos) -> Result<(), PistonError> {
    if known_region.is_some_and(|region| !region.contains(position)) {
        return Err(PistonError::UnknownSpace { position });
    }
    Ok(())
}

fn ensure_movable(position: Pos, block: &Block) -> Result<(), PistonError> {
    if !matches!(block.kind, BlockKind::Solid | BlockKind::Transparent) {
        return Err(PistonError::UnsupportedMovingBlock {
            position,
            kind: block.kind,
            reason: "the first piston subset moves ordinary non-redstone blocks only".to_owned(),
        });
    }
    if block.observation_classification == ObservationClassification::Coarse {
        return Err(PistonError::UnsupportedMovingBlock {
            position,
            kind: block.kind,
            reason: "coarse observed block identity is not safe to move".to_owned(),
        });
    }
    if block.requires_live_observation() {
        return Err(PistonError::UnsupportedMovingBlock {
            position,
            kind: block.kind,
            reason: "the block requires live behavior outside this block-only planner".to_owned(),
        });
    }
    if block
        .observed_name
        .as_deref()
        .is_some_and(observed_name_is_immovable)
    {
        return Err(PistonError::UnsupportedMovingBlock {
            position,
            kind: block.kind,
            reason: "the observed block is immovable in the supported Java piston subset"
                .to_owned(),
        });
    }
    Ok(())
}

fn set_piston_state(block: &mut Block, state: PistonState) {
    block.piston_state = Some(state);
    if block.observed_name.is_some() {
        block
            .observed_properties
            .insert("extended".to_owned(), state.is_extended().to_string());
    }
}

/// A piston may receive a signal-only update while its moving block entity is
/// alive.  Such an update must not invalidate the completion plan, but a
/// facing/variant/state-property mutation must: otherwise a stale plan could
/// move the original chain using geometry that no longer belongs to the
/// piston.  Keep the comparison explicit rather than relying on `ShapeId` so
/// the stale-plan guard remains exact (the hash is only a cache key).
fn piston_geometry_matches(actual: &Block, expected: &Block) -> bool {
    actual.kind == expected.kind
        && actual.observed_name == expected.observed_name
        && actual.observation_classification == expected.observation_classification
        && actual.facing == expected.facing
        && actual.delay == expected.delay
        && actual.support_offset == expected.support_offset
        && actual.wire_connections == expected.wire_connections
        && actual.piston_variant == expected.piston_variant
        && non_signal_properties(&actual.observed_properties)
            == non_signal_properties(&expected.observed_properties)
}

fn non_signal_properties(properties: &BTreeMap<String, String>) -> BTreeMap<String, String> {
    properties
        .iter()
        .filter(|(key, _)| !matches!(key.as_str(), "powered" | "extended" | "power" | "lit"))
        .map(|(key, value)| (key.clone(), value.clone()))
        .collect()
}

fn validate_plan(plan: &PistonPlan, world: &World) -> Result<(), PistonError> {
    if world.shape_id() != plan.delta.parent_shape {
        return Err(PistonError::StalePlan {
            position: plan.piston,
        });
    }
    for change in &plan.delta.changes {
        let actual = world
            .get(change.position)
            .cloned()
            .unwrap_or_else(|| Block::new(BlockKind::Air));
        if actual != change.before {
            return Err(PistonError::StalePlan {
                position: change.position,
            });
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn retracted_piston(variant: PistonVariant) -> (World, Pos) {
        let piston_pos = Pos::new(0, 1, 0);
        let mut world = World::new();
        world.set(Pos::new(0, 0, 0), Block::new(BlockKind::Solid));
        let mut piston = Block::new(BlockKind::Piston);
        piston.facing = Some(Facing::East);
        piston.piston_variant = Some(variant);
        piston.piston_state = Some(PistonState::Retracted);
        world.set(piston_pos, piston);
        (world, piston_pos)
    }

    #[test]
    fn extension_plans_and_applies_a_single_ordinary_block_atomically() {
        let (mut world, piston) = retracted_piston(PistonVariant::Normal);
        world.set(Pos::new(1, 1, 0), Block::new(BlockKind::Solid));
        let plan = plan_piston(&world, piston, PistonAction::Extend).unwrap();
        assert_eq!(plan.moved_count(), 1);
        assert_eq!(plan.moved[0].from, Pos::new(1, 1, 0));
        assert_eq!(plan.moved[0].to, Pos::new(2, 1, 0));
        plan.apply(&mut world).unwrap();
        assert_eq!(world.kind_at(Pos::new(1, 1, 0)), BlockKind::PistonHead);
        assert_eq!(world.kind_at(Pos::new(2, 1, 0)), BlockKind::Solid);
        assert_eq!(
            piston_state(world.get(piston).unwrap()),
            PistonState::Extended
        );
    }

    #[test]
    fn extension_refuses_an_unsupported_redstone_block_without_mutation() {
        let (mut world, piston) = retracted_piston(PistonVariant::Normal);
        world.set(Pos::new(1, 1, 0), Block::new(BlockKind::RedstoneWire));
        let before = world.clone();
        let error = plan_piston(&world, piston, PistonAction::Extend).unwrap_err();
        assert!(matches!(error, PistonError::UnsupportedMovingBlock { .. }));
        assert_eq!(world, before);
    }

    #[test]
    fn bounded_planner_rejects_unknown_space_after_a_known_prefix() {
        let (mut world, piston) = retracted_piston(PistonVariant::Normal);
        world.set(Pos::new(1, 1, 0), Block::new(BlockKind::Solid));
        let known_region = Region::new(Pos::new(-1, 0, -1), Pos::new(1, 2, 1));
        assert!(matches!(
            plan_piston_in_region(&world, known_region, piston, PistonAction::Extend),
            Err(PistonError::UnknownSpace { position }) if position == Pos::new(2, 1, 0)
        ));
    }

    #[test]
    fn bounded_planner_treats_an_empty_coordinate_as_air_when_it_is_known() {
        let (mut world, piston) = retracted_piston(PistonVariant::Normal);
        world.set(Pos::new(1, 1, 0), Block::new(BlockKind::Solid));
        let known_region = Region::new(Pos::new(-1, 0, -1), Pos::new(2, 2, 1));
        let plan = PistonPlanningContext::new(known_region)
            .plan(&world, piston, PistonAction::Extend)
            .unwrap();
        assert_eq!(plan.moved_count(), 1);
        assert_eq!(plan.moved[0].to, Pos::new(2, 1, 0));
    }

    #[test]
    fn extension_refuses_a_known_immovable_block() {
        let (mut world, piston) = retracted_piston(PistonVariant::Normal);
        let mut bedrock = Block::new(BlockKind::Solid);
        bedrock.observed_name = Some("minecraft:bedrock".to_owned());
        bedrock.observation_classification = ObservationClassification::Exact;
        world.set(Pos::new(1, 1, 0), bedrock);
        assert!(matches!(
            plan_piston(&world, piston, PistonAction::Extend),
            Err(PistonError::UnsupportedMovingBlock { .. })
        ));
    }

    #[test]
    fn extension_rejects_more_than_the_push_limit() {
        let (mut world, piston) = retracted_piston(PistonVariant::Normal);
        for x in 1..=(PISTON_PUSH_LIMIT as i32 + 1) {
            world.set(Pos::new(x, 1, 0), Block::new(BlockKind::Solid));
        }
        assert!(matches!(
            plan_piston(&world, piston, PistonAction::Extend),
            Err(PistonError::PushLimitExceeded { .. })
        ));
    }

    #[test]
    fn extension_delta_collapses_a_multi_block_push_into_final_coordinate_states() {
        let (mut world, piston) = retracted_piston(PistonVariant::Normal);
        world.set(Pos::new(1, 1, 0), Block::new(BlockKind::Solid));
        world.set(Pos::new(2, 1, 0), Block::new(BlockKind::Transparent));
        let plan = plan_piston(&world, piston, PistonAction::Extend).unwrap();
        assert_eq!(plan.delta.changes.len(), 4);
        assert_eq!(plan.delta.moves.len(), 2);
        plan.apply(&mut world).unwrap();
        assert_eq!(world.kind_at(Pos::new(1, 1, 0)), BlockKind::PistonHead);
        assert_eq!(world.kind_at(Pos::new(2, 1, 0)), BlockKind::Solid);
        assert_eq!(world.kind_at(Pos::new(3, 1, 0)), BlockKind::Transparent);
    }

    #[test]
    fn sticky_retraction_pulls_one_ordinary_block_and_preserves_variant() {
        let (mut world, piston) = retracted_piston(PistonVariant::Sticky);
        let body = world.get(piston).cloned().unwrap();
        let mut extended = body;
        extended.piston_state = Some(PistonState::Extended);
        world.set(piston, extended);
        world.set(Pos::new(2, 1, 0), Block::new(BlockKind::Solid));
        let plan = plan_piston(&world, piston, PistonAction::Retract).unwrap();
        assert_eq!(plan.moved[0].from, Pos::new(2, 1, 0));
        assert_eq!(plan.moved[0].to, Pos::new(1, 1, 0));
        plan.apply(&mut world).unwrap();
        assert_eq!(world.kind_at(Pos::new(1, 1, 0)), BlockKind::Solid);
        assert_eq!(world.kind_at(Pos::new(2, 1, 0)), BlockKind::Air);
        assert_eq!(
            piston_variant(world.get(piston).unwrap()),
            PistonVariant::Sticky
        );
        assert_eq!(
            piston_state(world.get(piston).unwrap()),
            PistonState::Retracted
        );
    }

    #[test]
    fn sticky_retraction_transitions_through_typed_head_and_block_entities() {
        let (mut world, piston) = retracted_piston(PistonVariant::Sticky);
        let body = world.get(piston).cloned().unwrap();
        let mut extended = body;
        extended.piston_state = Some(PistonState::Extended);
        world.set(piston, extended.clone());
        let head = Pos::new(1, 1, 0);
        world.set(
            head,
            Block::piston_head(Facing::East, PistonVariant::Sticky, false),
        );
        world.set(Pos::new(2, 1, 0), Block::new(BlockKind::Solid));

        let plan = plan_piston(&world, piston, PistonAction::Retract).unwrap();
        let start = plan.start_delta();
        start.apply(&mut world).unwrap();
        assert_eq!(world.kind_at(head), BlockKind::MovingPiston);
        assert_eq!(world.kind_at(Pos::new(2, 1, 0)), BlockKind::MovingPiston);
        assert_eq!(
            world
                .get(head)
                .and_then(|block| block.piston_entity.as_deref())
                .map(|entity| entity.extending),
            Some(false)
        );

        let completion = plan.completion_plan(&world).unwrap();
        completion.apply(&mut world).unwrap();
        assert_eq!(world.kind_at(head), BlockKind::Solid);
        assert_eq!(world.kind_at(Pos::new(2, 1, 0)), BlockKind::Air);
        assert_eq!(
            piston_state(world.get(piston).unwrap()),
            PistonState::Retracted
        );
    }

    #[test]
    fn normal_retraction_removes_typed_head_through_a_moving_state() {
        let (mut world, piston) = retracted_piston(PistonVariant::Normal);
        let mut extended = world.get(piston).cloned().unwrap();
        extended.piston_state = Some(PistonState::Extended);
        world.set(piston, extended);
        let head = Pos::new(1, 1, 0);
        world.set(
            head,
            Block::piston_head(Facing::East, PistonVariant::Normal, false),
        );

        let plan = plan_piston(&world, piston, PistonAction::Retract).unwrap();
        plan.start_delta().apply(&mut world).unwrap();
        assert_eq!(world.kind_at(head), BlockKind::MovingPiston);
        let completion = plan.completion_plan(&world).unwrap();
        completion.apply(&mut world).unwrap();
        assert_eq!(world.kind_at(head), BlockKind::Air);
        assert_eq!(
            piston_state(world.get(piston).unwrap()),
            PistonState::Retracted
        );
    }

    #[test]
    fn extension_start_exposes_head_and_moving_block_metadata() {
        let (mut world, piston) = retracted_piston(PistonVariant::Normal);
        let source = Pos::new(1, 1, 0);
        let destination = Pos::new(2, 1, 0);
        world.set(source, Block::new(BlockKind::Solid));
        let plan = plan_piston(&world, piston, PistonAction::Extend).unwrap();
        let start = plan.start_delta();
        start.apply(&mut world).unwrap();
        let head = world.get(source).unwrap();
        assert_eq!(head.kind, BlockKind::MovingPiston);
        assert!(head.piston_entity.as_deref().is_some_and(|entity| {
            entity.source && entity.extending && entity.pushed_block.kind == BlockKind::PistonHead
        }));
        let moved = world.get(destination).unwrap();
        assert!(moved.piston_entity.as_deref().is_some_and(|entity| {
            !entity.source && entity.extending && entity.pushed_block.kind == BlockKind::Solid
        }));
        let completion = plan.completion_plan(&world).unwrap();
        completion.apply(&mut world).unwrap();
        assert_eq!(world.kind_at(source), BlockKind::PistonHead);
        assert_eq!(world.kind_at(destination), BlockKind::Solid);
    }

    #[test]
    fn stale_plan_does_not_leave_a_partial_move() {
        let (mut world, piston) = retracted_piston(PistonVariant::Normal);
        world.set(Pos::new(1, 1, 0), Block::new(BlockKind::Solid));
        let plan = plan_piston(&world, piston, PistonAction::Extend).unwrap();
        world.set(Pos::new(2, 1, 0), Block::new(BlockKind::Solid));
        let before = world.clone();
        assert!(matches!(
            plan.apply(&mut world),
            Err(PistonError::StalePlan { .. })
        ));
        assert_eq!(world, before);
    }

    #[test]
    fn completion_rejects_piston_geometry_changed_during_motion() {
        let (mut world, piston) = retracted_piston(PistonVariant::Normal);
        world.set(Pos::new(1, 1, 0), Block::new(BlockKind::Solid));
        let plan = plan_piston(&world, piston, PistonAction::Extend).unwrap();
        let start = plan.start_delta();
        start.apply(&mut world).unwrap();
        world.get_mut(piston).unwrap().facing = Some(Facing::West);

        assert!(matches!(
            plan.completion_plan(&world),
            Err(PistonError::StalePlan { position }) if position == piston
        ));
    }
}
