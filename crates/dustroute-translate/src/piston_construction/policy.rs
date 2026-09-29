//! Candidate ordering is policy data; support, known space and callbacks remain
//! runtime constraints. These priorities never admit a new block to execution.
//!
//! Duplicate block rules fail at compile time:
//! ```compile_fail,E0080
//! use dustroute_translate::piston_construction::policy::{checked, DEFAULT};
//! use dustroute_minecraft::BlockKind;
//! const BAD: [(BlockKind, dustroute_translate::piston_construction::policy::ConstructionPolicy); 2] =
//!     checked([(BlockKind::Observer, DEFAULT), (BlockKind::Observer, DEFAULT)]);
//! ```
//! A watched-cell dependency needs observer geometry:
//! ```compile_fail,E0080
//! use dustroute_translate::piston_construction::policy::{checked, ConstructionPolicy, Predecessor, DEFAULT};
//! use dustroute_minecraft::BlockKind;
//! const BAD: [(BlockKind, ConstructionPolicy); 1] = checked([
//!     (BlockKind::Button, ConstructionPolicy { predecessor: Predecessor::OccupiedWatchedCell, ..DEFAULT })
//! ]);
//! ```
//! Ordinary blocks cannot delegate removal to a piston body:
//! ```compile_fail,E0080
//! use dustroute_translate::piston_construction::policy::{checked, ConstructionPolicy, DEFAULT};
//! use dustroute_minecraft::BlockKind;
//! const BAD: [(BlockKind, ConstructionPolicy); 1] = checked([
//!     (BlockKind::Solid, ConstructionPolicy { removal: None, ..DEFAULT })
//! ]);
//! ```
use dustroute_minecraft::BlockKind;

#[derive(Clone, Copy, Debug, Eq, PartialEq, Ord, PartialOrd)]
pub enum BuildPhase {
    Structure,
    Piston,
    Wiring,
    Control,
    Power,
    Device,
}
#[derive(Clone, Copy, Debug, Eq, PartialEq, Ord, PartialOrd)]
pub enum RemovalPhase {
    Source,
    Wiring,
    Piston,
    Other,
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Predecessor {
    None,
    OccupiedWatchedCell,
}

#[derive(Clone, Copy, Debug)]
pub struct ConstructionPolicy {
    pub build: BuildPhase,
    /// None means removal is performed by the owning body's physical callback.
    pub removal: Option<RemovalPhase>,
    pub predecessor: Predecessor,
}
pub const DEFAULT: ConstructionPolicy = ConstructionPolicy {
    build: BuildPhase::Device,
    removal: Some(RemovalPhase::Other),
    predecessor: Predecessor::None,
};
pub const fn checked<const N: usize>(
    rules: [(BlockKind, ConstructionPolicy); N],
) -> [(BlockKind, ConstructionPolicy); N] {
    let mut i = 0;
    while i < N {
        let (kind, policy) = rules[i];
        assert!(
            matches!(policy.predecessor, Predecessor::None) || matches!(kind, BlockKind::Observer),
            "watched-cell rule requires an observer"
        );
        assert!(
            policy.removal.is_some() || matches!(kind, BlockKind::PistonHead),
            "only a piston head has body-owned removal"
        );
        let mut j = 0;
        while j < i {
            assert!(
                kind as usize != rules[j].0 as usize,
                "duplicate construction policy"
            );
            j += 1;
        }
        i += 1;
    }
    rules
}

pub const RULES: [(BlockKind, ConstructionPolicy); 9] = checked([
    (
        BlockKind::Solid,
        ConstructionPolicy {
            build: BuildPhase::Structure,
            ..DEFAULT
        },
    ),
    (
        BlockKind::Transparent,
        ConstructionPolicy {
            build: BuildPhase::Structure,
            ..DEFAULT
        },
    ),
    (
        BlockKind::Piston,
        ConstructionPolicy {
            build: BuildPhase::Piston,
            removal: Some(RemovalPhase::Piston),
            ..DEFAULT
        },
    ),
    (
        BlockKind::RedstoneWire,
        ConstructionPolicy {
            build: BuildPhase::Wiring,
            removal: Some(RemovalPhase::Wiring),
            ..DEFAULT
        },
    ),
    (
        BlockKind::Repeater,
        ConstructionPolicy {
            build: BuildPhase::Wiring,
            removal: Some(RemovalPhase::Wiring),
            ..DEFAULT
        },
    ),
    (
        BlockKind::Lever,
        ConstructionPolicy {
            build: BuildPhase::Control,
            removal: Some(RemovalPhase::Source),
            ..DEFAULT
        },
    ),
    (
        BlockKind::RedstoneBlock,
        ConstructionPolicy {
            build: BuildPhase::Power,
            removal: Some(RemovalPhase::Source),
            ..DEFAULT
        },
    ),
    (
        BlockKind::Observer,
        ConstructionPolicy {
            predecessor: Predecessor::OccupiedWatchedCell,
            ..DEFAULT
        },
    ),
    (
        BlockKind::PistonHead,
        ConstructionPolicy {
            removal: None,
            ..DEFAULT
        },
    ),
]);

pub fn for_kind(kind: BlockKind) -> ConstructionPolicy {
    RULES
        .iter()
        .find(|(block, _)| *block == kind)
        .map_or(DEFAULT, |(_, policy)| *policy)
}
