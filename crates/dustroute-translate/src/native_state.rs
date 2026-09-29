//! Literal Java command state. Parsing checks representation only; it does not
//! admit a block to a simulator or authorize placement.
use serde::{Deserialize, Serialize};
use std::{collections::BTreeMap, fmt};

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(try_from = "String", into = "String")]
pub struct NativeBlockState {
    name: String,
    properties: BTreeMap<String, String>,
}

impl NativeBlockState {
    pub fn from_parts(name: String, properties: BTreeMap<String, String>) -> Result<Self, String> {
        let token = |s: &str| {
            !s.is_empty()
                && s.bytes()
                    .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == b'_')
        };
        if !name.strip_prefix("minecraft:").is_some_and(token)
            || properties.iter().any(|(k, v)| !token(k) || !token(v))
        {
            return Err("invalid literal Minecraft block state".into());
        }
        Ok(Self { name, properties })
    }
    pub fn name(&self) -> &str {
        &self.name
    }
    pub fn properties(&self) -> &BTreeMap<String, String> {
        &self.properties
    }
    pub fn air() -> Self {
        Self {
            name: "minecraft:air".into(),
            properties: BTreeMap::new(),
        }
    }
}
impl std::str::FromStr for NativeBlockState {
    type Err = String;
    fn from_str(value: &str) -> Result<Self, Self::Err> {
        let (name, props) = match value.split_once('[') {
            Some((name, rest)) => (
                name,
                Some(rest.strip_suffix(']').ok_or("unterminated block state")?),
            ),
            None => (value, None),
        };
        let mut properties = BTreeMap::new();
        if let Some(props) = props {
            for property in props.split(',') {
                let (key, value) = property
                    .split_once('=')
                    .ok_or("invalid block state property")?;
                if properties.insert(key.into(), value.into()).is_some() {
                    return Err("duplicate block state property".into());
                }
            }
        }
        Self::from_parts(name.into(), properties)
    }
}
impl TryFrom<String> for NativeBlockState {
    type Error = String;
    fn try_from(value: String) -> Result<Self, String> {
        value.parse()
    }
}
impl fmt::Display for NativeBlockState {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.name)?;
        if !self.properties.is_empty() {
            f.write_str("[")?;
            for (i, (key, value)) in self.properties.iter().enumerate() {
                if i > 0 {
                    f.write_str(",")?;
                }
                write!(f, "{key}={value}")?;
            }
            f.write_str("]")?;
        }
        Ok(())
    }
}
impl From<NativeBlockState> for String {
    fn from(value: NativeBlockState) -> Self {
        value.to_string()
    }
}
