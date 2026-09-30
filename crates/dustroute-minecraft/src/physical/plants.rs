//! Plants with native state, piston destruction and compiled support lifetime.
use super::environment::{EnvironmentTag, short_name};
use super::lifetime::{DelayedSupport, SupportFact, SupportQuery};
use super::native_state::NativeState;
use super::{
    Adhesion, CheckedPhysical, EMPTY, Faces, Orientation, PhysicalSpec, PistonReaction, Shape,
    Support, WireConnectionRule,
};
use crate::{Block, BlockKind, Pos};

#[derive(Clone, Copy, Debug)]
pub struct PlantSpec {
    pub names: &'static [&'static str],
    pub kind: BlockKind,
    pub physical: CheckedPhysical,
    pub state: NativeState,
    pub support: DelayedSupport,
}
impl PlantSpec {
    pub const fn checked(self) -> Self {
        assert!(
            !self.names.is_empty() && matches!(self.kind, BlockKind::Transparent),
            "plant identity required"
        );
        assert!(
            matches!(self.physical.spec().shape, Shape::Partial)
                && matches!(
                    self.physical.spec().piston_reaction,
                    PistonReaction::Destroy
                )
                && matches!(self.physical.spec().conducting, Faces::None)
                && matches!(self.physical.spec().supporting, Faces::None)
                && matches!(self.physical.spec().support, Support::None)
                && matches!(self.physical.spec().orientation, Orientation::None)
                && matches!(self.physical.spec().adhesion, Adhesion::None)
                && matches!(
                    self.physical.spec().wire_connection,
                    WireConnectionRule::None
                ),
            "initial plant scope requires partial geometry and piston destruction"
        );
        self.state.checked();
        self.support.checked();
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
const BELOW: Pos = Pos::new(0, -1, 0);
const ROOT: SupportQuery = SupportQuery::All(&[
    SupportQuery::At {
        offset: BELOW,
        fact: SupportFact::Tag(EnvironmentTag::CaneSoil),
    },
    SupportQuery::Any(&[
        SupportQuery::At {
            offset: Pos::new(-1, -1, 0),
            fact: SupportFact::Tag(EnvironmentTag::Water),
        },
        SupportQuery::At {
            offset: Pos::new(1, -1, 0),
            fact: SupportFact::Tag(EnvironmentTag::Water),
        },
        SupportQuery::At {
            offset: Pos::new(0, -1, -1),
            fact: SupportFact::Tag(EnvironmentTag::Water),
        },
        SupportQuery::At {
            offset: Pos::new(0, -1, 1),
            fact: SupportFact::Tag(EnvironmentTag::Water),
        },
    ]),
]);
pub const BUILTINS: [PlantSpec; 1] = [PlantSpec {
    names: &["sugar_cane"],
    kind: BlockKind::Transparent,
    physical: PhysicalSpec {
        shape: Shape::Partial,
        piston_reaction: PistonReaction::Destroy,
        ..EMPTY
    }
    .checked(),
    state: NativeState::Unsigned {
        property: "age",
        max: 15,
    },
    support: DelayedSupport {
        query: SupportQuery::Any(&[
            SupportQuery::At {
                offset: BELOW,
                fact: SupportFact::SameIdentity,
            },
            ROOT,
        ]),
        delay: 1,
        priority: 3,
    },
}
.checked()];
pub fn named(name: &str) -> Option<&'static PlantSpec> {
    BUILTINS
        .iter()
        .find(|s| s.names.contains(&short_name(name)))
}
pub fn of_block(block: &Block) -> Option<&'static PlantSpec> {
    named(block.observed_name.as_deref()?).filter(|s| s.matches(block))
}
