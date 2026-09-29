//! Current passive identities and state-selected physical facts. No suffix
//! wildcard grants execution to an unknown slab, stair, glass pane or material.
//! Invalid declarations are rejected at compilation:
//! ```compile_fail,E0080
//! use dustroute_minecraft::{BlockKind, physical::{self, passive::*}};
//! const BAD: [PassiveSpec; 1] = registry([PassiveSpec {
//!     names: &["not_a_passive_device"], kind: BlockKind::Observer,
//!     states: PassiveStates::Fixed(physical::of_kind(BlockKind::Transparent)),
//! }]);
//! ```
//! A passive row cannot introduce device notifications or a signal axis:
//! ```compile_fail,E0080
//! use dustroute_minecraft::{BlockKind, physical::{self, passive::*}};
//! const BAD: [PassiveSpec; 1] = registry([PassiveSpec {
//!     names: &["stone"], kind: BlockKind::Solid,
//!     states: PassiveStates::Fixed(physical::of_kind(BlockKind::Observer)),
//! }]);
//! ```
//! Stair families cannot reuse a cube descriptor:
//! ```compile_fail,E0080
//! use dustroute_minecraft::{BlockKind, physical::{self, passive::*}};
//! const BAD: [PassiveSpec; 1] = registry([PassiveSpec {
//!     names: &["stone_stairs"], kind: BlockKind::Transparent,
//!     states: PassiveStates::DryStairs(physical::of_kind(BlockKind::Transparent)),
//! }]);
//! ```
use super::*;

#[derive(Clone, Copy, Debug)]
pub enum PassiveStates {
    Fixed(CheckedPhysical),
    DryStairs(CheckedPhysical),
    DrySlab {
        bottom: CheckedPhysical,
        top: CheckedPhysical,
        double: CheckedPhysical,
    },
}

#[derive(Clone, Copy, Debug)]
pub struct PassiveSpec {
    pub names: &'static [&'static str],
    pub kind: BlockKind,
    pub states: PassiveStates,
}

impl PassiveSpec {
    pub const fn piston_destroys(&self) -> bool {
        match self.states {
            PassiveStates::Fixed(p) | PassiveStates::DryStairs(p) => {
                matches!(p.spec().piston_reaction, PistonReaction::Destroy)
            }
            PassiveStates::DrySlab {
                bottom,
                top,
                double,
            } => {
                matches!(bottom.spec().piston_reaction, PistonReaction::Destroy)
                    || matches!(top.spec().piston_reaction, PistonReaction::Destroy)
                    || matches!(double.spec().piston_reaction, PistonReaction::Destroy)
            }
        }
    }

    pub const fn adhesion(&self) -> Adhesion {
        match self.states {
            PassiveStates::Fixed(p) => p.spec().adhesion,
            PassiveStates::DryStairs(_) | PassiveStates::DrySlab { .. } => Adhesion::None,
        }
    }
}

const fn same_name(a: &str, b: &str) -> bool {
    let a = a.as_bytes();
    let b = b.as_bytes();
    if a.len() != b.len() {
        return false;
    }
    let mut i = 0;
    while i < a.len() {
        if a[i] != b[i] {
            return false;
        }
        i += 1;
    }
    true
}

const fn passive(p: CheckedPhysical) {
    assert!(
        matches!(p.support(), Support::None)
            && matches!(p.spec().orientation, Orientation::None)
            && matches!(
                p.spec().piston_reaction,
                PistonReaction::Normal | PistonReaction::Destroy
            )
            && matches!(p.spec().wire_connection, WireConnectionRule::None),
        "passive blocks cannot declare attachment, signal axes or dust terminals"
    );
}

pub const fn registry<const N: usize>(specs: [PassiveSpec; N]) -> [PassiveSpec; N] {
    let mut i = 0;
    while i < N {
        let s = specs[i];
        assert!(
            matches!(s.kind, BlockKind::Solid | BlockKind::Transparent),
            "passive kind required"
        );
        assert!(!s.names.is_empty(), "passive names required");
        match s.states {
            PassiveStates::DryStairs(p) => {
                passive(p);
                assert!(
                    matches!(s.kind, BlockKind::Transparent),
                    "stair observation kind must be transparent"
                );
                assert!(
                    matches!(p.spec().shape, Shape::Stairs)
                        && matches!(p.spec().supporting, Faces::Stairs)
                        && matches!(p.spec().conducting, Faces::None),
                    "nonconducting stair geometry required"
                );
            }
            PassiveStates::Fixed(p) => {
                passive(p);
                assert!(
                    matches!(p.spec().shape, Shape::FullCube | Shape::Honey),
                    "fixed passive shape must be a cube or honey"
                );
                assert!(
                    matches!(p.support(), Support::None),
                    "passive cube cannot be attached"
                );
            }
            PassiveStates::DrySlab {
                bottom,
                top,
                double,
            } => {
                passive(bottom);
                passive(top);
                passive(double);
                assert!(
                    matches!(s.kind, BlockKind::Transparent),
                    "slab observation kind must be stable across states"
                );
                assert!(
                    matches!(bottom.spec().shape, Shape::BottomHalf)
                        && matches!(bottom.spec().supporting, Faces::Down),
                    "bottom slab geometry required"
                );
                assert!(
                    matches!(top.spec().shape, Shape::TopHalf)
                        && matches!(top.spec().supporting, Faces::Up),
                    "top slab geometry required"
                );
                assert!(
                    matches!(double.spec().shape, Shape::FullCube)
                        && matches!(double.spec().supporting, Faces::All),
                    "double slab geometry required"
                );
                assert!(
                    matches!(bottom.support(), Support::None)
                        && matches!(top.support(), Support::None)
                        && matches!(double.support(), Support::None),
                    "slabs cannot require attachment support"
                );
            }
        }
        let mut n = 0;
        while n < s.names.len() {
            let bytes = s.names[n].as_bytes();
            assert!(!bytes.is_empty(), "empty passive identity");
            let mut c = 0;
            while c < bytes.len() {
                assert!(
                    bytes[c].is_ascii_lowercase() || bytes[c].is_ascii_digit() || bytes[c] == b'_',
                    "passive names must be native paths"
                );
                c += 1;
            }
            let mut j = 0;
            while j <= i {
                let mut m = 0;
                let end = if j == i { n } else { specs[j].names.len() };
                while m < end {
                    assert!(
                        !same_name(s.names[n], specs[j].names[m]),
                        "duplicate passive identity"
                    );
                    m += 1;
                }
                j += 1;
            }
            n += 1;
        }
        i += 1;
    }
    specs
}

const BOTTOM: CheckedPhysical = PhysicalSpec {
    shape: Shape::BottomHalf,
    supporting: Faces::Down,
    ..EMPTY
}
.checked();
const TOP: CheckedPhysical = PhysicalSpec {
    shape: Shape::TopHalf,
    supporting: Faces::Up,
    ..EMPTY
}
.checked();

pub const BUILTINS: [PassiveSpec; 7] = registry([
    PassiveSpec {
        names: &["pumpkin", "melon"],
        kind: BlockKind::Solid,
        states: PassiveStates::Fixed(
            PhysicalSpec {
                piston_reaction: PistonReaction::Destroy,
                ..SOLID.spec()
            }
            .checked(),
        ),
    },
    PassiveSpec {
        names: &["slime_block"],
        kind: BlockKind::Transparent,
        states: PassiveStates::Fixed(
            PhysicalSpec {
                adhesion: Adhesion::Slime,
                ..SOLID.spec()
            }
            .checked(),
        ),
    },
    PassiveSpec {
        names: &["honey_block"],
        kind: BlockKind::Transparent,
        states: PassiveStates::Fixed(
            PhysicalSpec {
                shape: Shape::Honey,
                supporting: Faces::Honey,
                adhesion: Adhesion::Honey,
                ..EMPTY
            }
            .checked(),
        ),
    },
    PassiveSpec {
        names: &[
            "stone_stairs",
            "cobblestone_stairs",
            "quartz_stairs",
            "smooth_quartz_stairs",
        ],
        kind: BlockKind::Transparent,
        states: PassiveStates::DryStairs(
            PhysicalSpec {
                shape: Shape::Stairs,
                supporting: Faces::Stairs,
                ..EMPTY
            }
            .checked(),
        ),
    },
    PassiveSpec {
        names: &[
            "stone",
            "cobblestone",
            "smooth_stone",
            "obsidian",
            "smooth_quartz",
            "cyan_wool",
        ],
        kind: BlockKind::Solid,
        states: PassiveStates::Fixed(SOLID),
    },
    PassiveSpec {
        names: &[
            "glass",
            "tinted_glass",
            "white_stained_glass",
            "orange_stained_glass",
            "magenta_stained_glass",
            "light_blue_stained_glass",
            "yellow_stained_glass",
            "lime_stained_glass",
            "pink_stained_glass",
            "gray_stained_glass",
            "light_gray_stained_glass",
            "cyan_stained_glass",
            "purple_stained_glass",
            "blue_stained_glass",
            "brown_stained_glass",
            "green_stained_glass",
            "red_stained_glass",
            "black_stained_glass",
        ],
        kind: BlockKind::Transparent,
        states: PassiveStates::Fixed(GLASS),
    },
    PassiveSpec {
        names: &["stone_slab", "smooth_stone_slab"],
        kind: BlockKind::Transparent,
        states: PassiveStates::DrySlab {
            bottom: BOTTOM,
            top: TOP,
            double: SOLID,
        },
    },
]);

pub fn named(name: &str) -> Option<&'static PassiveSpec> {
    let name = name.strip_prefix("minecraft:").unwrap_or(name);
    BUILTINS.iter().find(|s| s.names.contains(&name))
}

pub(super) fn resolve(block: &Block) -> Option<CheckedPhysical> {
    let Some(name) = block.observed_name.as_deref() else {
        return (block.observed_properties.is_empty()
            && matches!(block.kind, BlockKind::Solid | BlockKind::Transparent))
        .then(|| of_kind(block.kind));
    };
    let spec = named(name)?;
    if block.kind != spec.kind {
        return None;
    }
    match spec.states {
        PassiveStates::DryStairs(p) => stairs::StairState::parse(block).map(|_| p),
        PassiveStates::Fixed(physical) => block.observed_properties.is_empty().then_some(physical),
        PassiveStates::DrySlab {
            bottom,
            top,
            double,
        } => {
            if block.observed_properties.len() != 2
                || block
                    .observed_properties
                    .get("waterlogged")
                    .map(String::as_str)
                    != Some("false")
            {
                return None;
            }
            match block.observed_properties.get("type").map(String::as_str) {
                Some("bottom") => Some(bottom),
                Some("top") => Some(top),
                Some("double") => Some(double),
                _ => None,
            }
        }
    }
}
