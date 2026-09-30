//! Observed environment facts. Enclosed water is an explicit fixed boundary,
//! not a fluid executor or a solid fallback.
use super::native_state::NativeState;
use super::{CheckedPhysical, EMPTY, Faces, PhysicalSpec, PistonReaction, SOLID, Shape};
use crate::{Block, BlockKind};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum EnvironmentTag {
    CaneSoil,
    Water,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum EnvironmentRole {
    Soil,
    EnclosedWaterSource,
}

#[derive(Clone, Copy, Debug)]
pub struct EnvironmentSpec {
    pub names: &'static [&'static str],
    pub kind: BlockKind,
    pub physical: CheckedPhysical,
    pub state: NativeState,
    pub role: EnvironmentRole,
}
impl EnvironmentSpec {
    pub const fn checked(self) -> Self {
        assert!(!self.names.is_empty(), "environment identity required");
        self.state.checked();
        match self.role {
            EnvironmentRole::Soil => {
                assert!(
                    matches!(self.kind, BlockKind::Solid)
                        && matches!(self.physical.spec().shape, Shape::FullCube),
                    "soil requires solid cube geometry"
                );
                assert!(
                    matches!(self.state, NativeState::Empty),
                    "initial soil scope requires stateless identities"
                );
            }
            EnvironmentRole::EnclosedWaterSource => {
                assert!(
                    matches!(self.state, NativeState::Exact { .. }),
                    "fixed water requires an explicit native source-state constraint"
                );
                assert!(
                    matches!(self.kind, BlockKind::Transparent)
                        && matches!(self.physical.spec().shape, Shape::Empty)
                        && matches!(self.physical.spec().conducting, Faces::None)
                        && matches!(self.physical.spec().supporting, Faces::None),
                    "water must not become a solid or supporting face"
                );
            }
        }
        self
    }
    pub fn matches(&self, block: &Block) -> bool {
        block.kind == self.kind
            && self.state.matches(block)
            && block
                .observed_name
                .as_deref()
                .is_some_and(|n| self.names.contains(&short_name(n)))
    }
}

pub(super) fn short_name(name: &str) -> &str {
    name.strip_prefix("minecraft:").unwrap_or(name)
}

pub const BUILTINS: [EnvironmentSpec; 2] = [
    EnvironmentSpec {
        names: &["dirt", "coarse_dirt", "rooted_dirt"],
        kind: BlockKind::Solid,
        physical: SOLID,
        state: NativeState::Empty,
        role: EnvironmentRole::Soil,
    }
    .checked(),
    EnvironmentSpec {
        names: &["water"],
        kind: BlockKind::Transparent,
        physical: PhysicalSpec {
            piston_reaction: PistonReaction::Destroy,
            ..EMPTY
        }
        .checked(),
        state: NativeState::Exact {
            property: "level",
            value: "0",
        },
        role: EnvironmentRole::EnclosedWaterSource,
    }
    .checked(),
];

pub fn named(name: &str) -> Option<&'static EnvironmentSpec> {
    BUILTINS
        .iter()
        .find(|s| s.names.contains(&short_name(name)))
}
pub fn of_block(block: &Block) -> Option<&'static EnvironmentSpec> {
    named(block.observed_name.as_deref()?).filter(|s| s.matches(block))
}
pub fn has_tag(block: &Block, tag: EnvironmentTag) -> bool {
    of_block(block).is_some_and(|s| {
        matches!(
            (s.role, tag),
            (EnvironmentRole::Soil, EnvironmentTag::CaneSoil)
                | (EnvironmentRole::EnclosedWaterSource, EnvironmentTag::Water)
        )
    })
}
pub fn is_water(block: &Block) -> bool {
    has_tag(block, EnvironmentTag::Water)
}
