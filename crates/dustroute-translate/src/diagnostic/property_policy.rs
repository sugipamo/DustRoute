//! Literal property roles used by comparison, not block admission or physics.
//! Unknown properties remain configuration differences, including unsupported blocks.
//!
//! Duplicate declarations fail at compile time:
//! ```compile_fail,E0080
//! use dustroute_translate::diagnostic::property_policy::{checked, PropertyRole};
//! const BAD: [(&str, PropertyRole); 2] = checked([
//!     ("facing", PropertyRole::Orientation), ("facing", PropertyRole::State),
//! ]);
//! ```
//! Malformed native property names are also rejected:
//! ```compile_fail,E0080
//! use dustroute_translate::diagnostic::property_policy::{checked, PropertyRole};
//! const BAD: [(&str, PropertyRole); 1] = checked([("", PropertyRole::State)]);
//! ```
use super::difference::DifferenceKind;

#[derive(Clone, Copy, Debug)]
pub enum PropertyRole {
    Orientation,
    State,
    Configuration,
}
impl PropertyRole {
    pub const fn difference_kind(self) -> DifferenceKind {
        match self {
            Self::Orientation => DifferenceKind::Orientation,
            Self::State => DifferenceKind::State,
            Self::Configuration => DifferenceKind::Configuration,
        }
    }
}

pub const fn checked<const N: usize>(
    rules: [(&str, PropertyRole); N],
) -> [(&str, PropertyRole); N] {
    let mut i = 0;
    while i < N {
        let name = rules[i].0.as_bytes();
        assert!(!name.is_empty(), "property name required");
        let mut c = 0;
        while c < name.len() {
            assert!(
                name[c].is_ascii_lowercase() || name[c].is_ascii_digit() || name[c] == b'_',
                "native property name required"
            );
            c += 1;
        }
        let mut j = 0;
        while j < i {
            let other = rules[j].0.as_bytes();
            let mut equal = name.len() == other.len();
            let mut k = 0;
            while equal && k < name.len() {
                equal = name[k] == other[k];
                k += 1;
            }
            assert!(!equal, "duplicate property classification");
            j += 1;
        }
        i += 1;
    }
    rules
}

use PropertyRole::{Orientation, State};
pub const RULES: [(&str, PropertyRole); 13] = checked([
    ("facing", Orientation),
    ("axis", Orientation),
    ("face", Orientation),
    ("powered", State),
    ("power", State),
    ("lit", State),
    ("extended", State),
    ("short", State),
    ("locked", State),
    ("north", State),
    ("east", State),
    ("south", State),
    ("west", State),
]);

pub fn role(property: &str) -> PropertyRole {
    RULES
        .iter()
        .find(|(name, _)| *name == property)
        .map_or(PropertyRole::Configuration, |(_, role)| *role)
}
