//! Immutable evidence that a complete world passed the shared placement checks.
//! This proves supported initial placement, not temporal correctness or live
//! equivalence. Raw worlds remain available for observation and diagnostics.

use std::fmt::{Display, Formatter};
use std::ops::Deref;

use serde::Serialize;

use crate::{BlockKind, CapabilityLevel, Facing, Pos, World};

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(tag = "code", rename_all = "snake_case")]
pub enum WorldValidationIssue {
    InvalidSupport {
        position: Pos,
        kind: BlockKind,
        support: Option<Pos>,
    },
    UnsupportedPlacement {
        position: Pos,
        kind: BlockKind,
    },
    InvalidState {
        position: Pos,
        reason: String,
    },
    SyntheticInputDriver {
        reason: String,
    },
    DuplicateChange {
        position: Pos,
    },
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct WorldValidationError {
    pub issues: Vec<WorldValidationIssue>,
}

impl Display for WorldValidationError {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "world failed placement validation ({} issues)",
            self.issues.len()
        )
    }
}

impl std::error::Error for WorldValidationError {}

/// A validated, immutable initial placement. Mutation or deserialization cannot
/// manufacture this evidence. Converting back to `World` discards the proof.
///
/// ```compile_fail
/// use dustroute_minecraft::{ValidatedWorld, World, Pos, Block, BlockKind};
/// let mut checked = ValidatedWorld::try_from(World::new()).unwrap();
/// checked.set(Pos::new(0, 0, 0), Block::new(BlockKind::Repeater));
/// ```
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ValidatedWorld(World);

impl ValidatedWorld {
    #[must_use]
    pub fn into_world(self) -> World {
        self.0
    }
}

impl Deref for ValidatedWorld {
    type Target = World;
    fn deref(&self) -> &World {
        &self.0
    }
}

impl TryFrom<World> for ValidatedWorld {
    type Error = WorldValidationError;

    fn try_from(world: World) -> Result<Self, Self::Error> {
        let issues = world.placement_issues();
        if issues.is_empty() {
            Ok(Self(world))
        } else {
            Err(WorldValidationError { issues })
        }
    }
}

impl World {
    /// Reuses the canonical support and per-block capability rules. No support
    /// positions or block-state properties are invented to make a world pass.
    #[must_use]
    pub fn placement_issues(&self) -> Vec<WorldValidationIssue> {
        let mut issues: Vec<_> = self
            .support_issues()
            .into_iter()
            .map(
                |(position, kind, support)| WorldValidationIssue::InvalidSupport {
                    position,
                    kind,
                    support,
                },
            )
            .collect();
        for (position, block) in self.iter() {
            if block.capabilities().placement != CapabilityLevel::Full {
                issues.push(WorldValidationIssue::UnsupportedPlacement {
                    position: *position,
                    kind: block.kind,
                });
            }
            let mut invalid = |reason: &str| {
                issues.push(WorldValidationIssue::InvalidState {
                    position: *position,
                    reason: reason.to_owned(),
                })
            };
            if let Some(offset) = block.support_offset {
                let adjacent = matches!(
                    (offset.x, offset.y, offset.z),
                    (0, -1, 0) | (0, 1, 0) | (-1, 0, 0) | (1, 0, 0) | (0, 0, -1) | (0, 0, 1)
                );
                if !adjacent {
                    invalid("support must be immediately adjacent");
                }
                if matches!(
                    block.kind,
                    BlockKind::RedstoneWire
                        | BlockKind::Repeater
                        | BlockKind::Comparator
                        | BlockKind::PressurePlate
                ) && offset != Pos::new(0, -1, 0)
                {
                    invalid("this component requires support directly below");
                }
            }
            if block.kind == BlockKind::RedstoneBlock && block.powered == Some(false) {
                invalid("Java redstone blocks cannot be switched off; use a physical input device");
            }
            if matches!(block.kind, BlockKind::Repeater | BlockKind::Comparator)
                && !matches!(
                    block.facing,
                    Some(Facing::North | Facing::South | Facing::East | Facing::West)
                )
            {
                invalid("directional device needs an explicit horizontal facing");
            }
            if block.kind == BlockKind::Repeater && !matches!(block.delay, Some(1..=4)) {
                invalid("repeater needs an explicit delay from 1 to 4");
            }
            if block.kind == BlockKind::Observer && block.facing.is_none() {
                invalid("observer needs an explicit facing");
            }
            if block.power_level.is_some_and(|level| level > 15) {
                invalid("power level must be between 0 and 15");
            }
        }
        issues
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Block;

    #[test]
    fn editing_a_copy_requires_new_validation() {
        let mut world = World::new();
        world.place(BlockKind::Solid, Pos::new(0, 0, 0));
        world.place(BlockKind::RedstoneWire, Pos::new(0, 1, 0));
        let checked = ValidatedWorld::try_from(world).unwrap();
        let mut edited = checked.clone().into_world();
        edited.remove(Pos::new(0, 0, 0));
        assert!(ValidatedWorld::try_from(edited).is_err());
        assert!(checked.placement_issues().is_empty());
    }

    #[test]
    fn aggregates_support_capability_and_synthetic_state_failures() {
        let mut world = World::new();
        world.place(BlockKind::Repeater, Pos::new(0, 1, 0));
        world.set(Pos::new(2, 1, 0), Block::new(BlockKind::Piston));
        let mut source = Block::new(BlockKind::RedstoneBlock);
        source.powered = Some(false);
        world.set(Pos::new(3, 1, 0), source);
        let error = ValidatedWorld::try_from(world).unwrap_err();
        assert!(
            error
                .issues
                .iter()
                .any(|i| matches!(i, WorldValidationIssue::InvalidSupport { .. }))
        );
        assert!(
            error
                .issues
                .iter()
                .any(|i| matches!(i, WorldValidationIssue::UnsupportedPlacement { .. }))
        );
        assert!(error.issues.iter().any(|i| matches!(i, WorldValidationIssue::InvalidState { position, .. } if *position == Pos::new(3, 1, 0))));
    }
}
