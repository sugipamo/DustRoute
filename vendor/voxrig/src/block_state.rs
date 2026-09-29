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

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct BlockDefinition {
    name: String,
    min_state_id: i32,
    max_state_id: i32,
    states: Vec<Property>,
}

#[derive(Deserialize)]
struct Property {
    name: String,
    #[serde(rename = "type")]
    kind: String,
    num_values: u32,
    values: Option<Vec<String>>,
}

pub(crate) struct StateRegistry {
    definitions: Vec<BlockDefinition>,
}

impl StateRegistry {
    pub(crate) fn block_name(&self, block_id: i32) -> Option<&str> {
        usize::try_from(block_id)
            .ok()
            .and_then(|i| self.definitions.get(i))
            .map(|b| b.name.as_str())
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
        for property in &block.states {
            let value = state.properties.get(&property.name).ok_or_else(|| {
                Error::new(
                    ErrorKind::InvalidInput,
                    anyhow::anyhow!("missing property {}", property.name),
                )
            })?;
            let index = if let Some(values) = &property.values {
                values.iter().position(|v| v == value).map(|i| i as u32)
            } else if property.kind == "bool" {
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
            .filter(|i| *i < property.num_values)
            .ok_or_else(|| {
                Error::new(
                    ErrorKind::InvalidInput,
                    anyhow::anyhow!("invalid property {}", property.name),
                )
            })?;
            offset = offset * property.num_values + index;
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
    pub(crate) fn parse(json: &str) -> Result<Self> {
        let definitions: Vec<BlockDefinition> =
            serde_json::from_str(json).map_err(|error| Error::new(ErrorKind::Protocol, error))?;
        let mut next = 0;
        for block in &definitions {
            let mut count = Some(1u32);
            for property in &block.states {
                let valid = property.num_values > 0
                    && match property.kind.as_str() {
                        "bool" => property.num_values == 2 && property.values.is_none(),
                        "int" => property
                            .values
                            .as_ref()
                            .is_none_or(|v| v.len() == property.num_values as usize),
                        "enum" => property
                            .values
                            .as_ref()
                            .is_some_and(|v| v.len() == property.num_values as usize),
                        _ => false,
                    };
                if !valid {
                    return Err(Error::new(
                        ErrorKind::Protocol,
                        anyhow::anyhow!("invalid property in {}", block.name),
                    ));
                }
                count = count.and_then(|n| n.checked_mul(property.num_values));
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
            let index = offset % property.num_values;
            offset /= property.num_values;
            let value = if let Some(values) = &property.values {
                values[index as usize].clone()
            } else if property.kind == "bool" {
                (index == 0).to_string()
            } else {
                index.to_string()
            };
            properties.insert(property.name.clone(), value);
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
        for json in [
            r#"[{"name":"air","minStateId":1,"maxStateId":1,"states":[]}]"#,
            r#"[{"name":"bad","minStateId":0,"maxStateId":1,"states":[]}]"#,
            r#"[{"name":"bad","minStateId":0,"maxStateId":1,"states":[{"name":"shape","type":"enum","num_values":2,"values":["straight"]}]}]"#,
        ] {
            assert!(StateRegistry::parse(json).is_err());
        }
    }
}
