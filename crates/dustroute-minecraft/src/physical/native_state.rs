//! Small, compiled schemas for native passive state. No implicit defaults.
use crate::Block;

#[derive(Clone, Copy, Debug)]
pub enum NativeState {
    Empty,
    Exact {
        property: &'static str,
        value: &'static str,
    },
    Unsigned {
        property: &'static str,
        max: u8,
    },
}

impl NativeState {
    pub const fn checked(self) -> Self {
        match self {
            Self::Empty => {}
            Self::Exact { property, value } => {
                assert!(
                    !property.is_empty() && !value.is_empty(),
                    "native state requires a property and value"
                );
            }
            Self::Unsigned { property, max } => {
                assert!(
                    !property.is_empty() && max > 0,
                    "bounded native state required"
                );
            }
        }
        self
    }

    pub fn matches(self, block: &Block) -> bool {
        match self {
            Self::Empty => block.observed_properties.is_empty(),
            Self::Exact { property, value } => {
                block.observed_properties.len() == 1
                    && block
                        .observed_properties
                        .get(property)
                        .is_some_and(|v| v == value)
            }
            Self::Unsigned { property, max } => {
                block.observed_properties.len() == 1
                    && block.observed_properties.get(property).is_some_and(|v| {
                        v.parse::<u8>()
                            .is_ok_and(|n| n <= max && n.to_string() == *v)
                    })
            }
        }
    }
}
