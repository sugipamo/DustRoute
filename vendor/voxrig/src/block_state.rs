//! Lossless native block states, interpreted with an explicitly bound registry.

use crate::{Error, ErrorKind, Result};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

/// A namespaced block name and every native state property.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct NativeBlockState {
    /// Namespaced identifier, e.g. `minecraft:quartz_stairs`.
    pub name: String,
    /// Complete native properties; their order is immaterial.
    pub properties: BTreeMap<String, String>,
}

pub(crate) struct BlockDefinition {
    pub name: &'static str,
    pub min_state_id: i32,
    pub max_state_id: i32,
    pub states: &'static [Property],
}

pub(crate) struct Property {
    pub name: &'static str,
    pub values: PropertyValues,
}
pub(crate) enum PropertyValues {
    Boolean,
    Integer {
        count: u32,
        values: Option<&'static [&'static str]>,
    },
    Enum(&'static [&'static str]),
}
impl PropertyValues {
    fn count(&self) -> u32 {
        match self {
            Self::Boolean => 2,
            Self::Integer { count, .. } => *count,
            Self::Enum(values) => values.len() as u32,
        }
    }
    fn named(&self) -> Option<&'static [&'static str]> {
        match self {
            Self::Integer { values, .. } => *values,
            Self::Enum(values) => Some(values),
            Self::Boolean => None,
        }
    }
}

pub(crate) struct StateRegistry {
    definitions: &'static [BlockDefinition],
}

impl StateRegistry {
    pub(crate) fn block_name(&self, block_id: i32) -> Option<&str> {
        usize::try_from(block_id)
            .ok()
            .and_then(|i| self.definitions.get(i))
            .map(|b| b.name)
    }
    pub(crate) fn encode(&self, state: &NativeBlockState) -> Result<i32> {
        let name = state.name.strip_prefix("minecraft:").ok_or_else(|| {
            Error::new(
                ErrorKind::InvalidInput,
                anyhow::anyhow!("native block namespace required"),
            )
        })?;
        let block = self
            .definitions
            .iter()
            .find(|b| b.name == name)
            .ok_or_else(|| {
                Error::new(
                    ErrorKind::InvalidInput,
                    anyhow::anyhow!("unknown native block {}", state.name),
                )
            })?;
        if state.properties.len() != block.states.len() {
            return Err(Error::new(
                ErrorKind::InvalidInput,
                anyhow::anyhow!("complete native properties required"),
            ));
        }
        let mut offset = 0u32;
        for property in block.states {
            let value = state.properties.get(property.name).ok_or_else(|| {
                Error::new(
                    ErrorKind::InvalidInput,
                    anyhow::anyhow!("missing property {}", property.name),
                )
            })?;
            let index = if let Some(values) = property.values.named() {
                values.iter().position(|v| *v == value).map(|i| i as u32)
            } else if matches!(property.values, PropertyValues::Boolean) {
                match value.as_str() {
                    "true" => Some(0),
                    "false" => Some(1),
                    _ => None,
                }
            } else {
                value
                    .parse::<u32>()
                    .ok()
                    .filter(|i| i.to_string() == *value)
            }
            .filter(|i| *i < property.values.count())
            .ok_or_else(|| {
                Error::new(
                    ErrorKind::InvalidInput,
                    anyhow::anyhow!("invalid property {}", property.name),
                )
            })?;
            offset = offset * property.values.count() + index;
        }
        Ok(block.min_state_id + offset as i32)
    }
    pub(crate) fn validate_id(&self, id: i32) -> Result<()> {
        if id < 0 || self.definitions.last().is_none_or(|b| id > b.max_state_id) {
            return Err(Error::new(
                ErrorKind::Protocol,
                anyhow::anyhow!("unknown block state ID {id}"),
            ));
        }
        Ok(())
    }
    pub(crate) fn new(definitions: &'static [BlockDefinition]) -> Result<Self> {
        let mut next = 0;
        for block in definitions {
            let mut count = Some(1u32);
            for property in block.states {
                let valid = property.values.count() > 0
                    && property
                        .values
                        .named()
                        .is_none_or(|v| v.len() == property.values.count() as usize);
                if !valid {
                    return Err(Error::new(
                        ErrorKind::Protocol,
                        anyhow::anyhow!("invalid property in {}", block.name),
                    ));
                }
                count = count.and_then(|n| n.checked_mul(property.values.count()));
            }
            if block.min_state_id != next
                || count.map(i64::from)
                    != Some(i64::from(block.max_state_id) - i64::from(block.min_state_id) + 1)
            {
                return Err(Error::new(
                    ErrorKind::Protocol,
                    anyhow::anyhow!("invalid state range for {}", block.name),
                ));
            }
            next = block.max_state_id.checked_add(1).ok_or_else(|| {
                Error::new(ErrorKind::Protocol, anyhow::anyhow!("state range overflow"))
            })?;
        }
        Ok(Self { definitions })
    }

    pub(crate) fn decode(&self, id: i32) -> Result<NativeBlockState> {
        let index = self
            .definitions
            .partition_point(|block| block.max_state_id < id);
        let block = self
            .definitions
            .get(index)
            .filter(|b| id >= b.min_state_id)
            .ok_or_else(|| {
                Error::new(
                    ErrorKind::Protocol,
                    anyhow::anyhow!("unknown block state ID {id}"),
                )
            })?;
        let mut offset = (id - block.min_state_id) as u32;
        let mut properties = BTreeMap::new();
        for property in block.states.iter().rev() {
            let index = offset % property.values.count();
            offset /= property.values.count();
            let value = if let Some(values) = property.values.named() {
                values[index as usize].to_owned()
            } else if matches!(property.values, PropertyValues::Boolean) {
                (index == 0).to_string()
            } else {
                index.to_string()
            };
            properties.insert(property.name.to_owned(), value);
        }
        Ok(NativeBlockState {
            name: format!("minecraft:{}", block.name),
            properties,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn native_properties_preserve_stair_shape_facing_and_waterlogging() {
        let state = crate::versions::java_1_16_1::native_state(6754).unwrap();
        assert_eq!(state.name, "minecraft:quartz_stairs");
        assert_eq!(
            state.properties,
            BTreeMap::from([
                ("facing".into(), "north".into()),
                ("half".into(), "bottom".into()),
                ("shape".into(), "straight".into()),
                ("waterlogged".into(), "false".into()),
            ])
        );
        let inner = crate::versions::java_1_16_1::native_state(6756).unwrap();
        assert_eq!(inner.properties["shape"], "inner_left");
        let wire = crate::versions::java_1_16_1::native_state(3218).unwrap();
        assert_eq!(wire.properties["power"], "0");
        assert_eq!(wire.properties["east"], "none");
        assert!(crate::versions::java_1_16_1::native_state(-1).is_err());
        assert!(crate::versions::java_1_16_1::native_state(i32::MAX).is_err());
    }

    #[test]
    fn malformed_registry_ranges_and_property_counts_are_rejected() {
        for definitions in [
            &[BlockDefinition {
                name: "air",
                min_state_id: 1,
                max_state_id: 1,
                states: &[],
            }][..],
            &[BlockDefinition {
                name: "bad",
                min_state_id: 0,
                max_state_id: 1,
                states: &[],
            }][..],
            &[BlockDefinition {
                name: "bad",
                min_state_id: 0,
                max_state_id: 1,
                states: &[Property {
                    name: "shape",
                    values: PropertyValues::Integer {
                        count: 2,
                        values: Some(&["straight"]),
                    },
                }],
            }][..],
        ] {
            assert!(StateRegistry::new(definitions).is_err());
        }
    }
}
