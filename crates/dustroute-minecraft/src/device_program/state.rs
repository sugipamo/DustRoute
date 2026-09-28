//! Declared physical state. Secondary Boolean properties also exist on synthetic
//! blocks; observed blocks additionally require agreement with canonical fields.
use super::DeviceDefinition;
use crate::Block;
use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub enum BoolProperty {
    Powered,
    Lit,
    Open,
    Locked,
}

impl BoolProperty {
    pub const fn name(self) -> &'static str {
        match self {
            Self::Powered => "powered",
            Self::Lit => "lit",
            Self::Open => "open",
            Self::Locked => "locked",
        }
    }
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub enum Property {
    Bool(BoolProperty),
    Power,
    /// Construction setting (1..=4), sampled but not changed by callbacks.
    Delay,
    /// Construction-only comparator mode: compare=0, subtract=1.
    ComparatorMode,
}

impl Property {
    pub const fn writable(self) -> bool {
        !matches!(self, Self::Delay | Self::ComparatorMode)
    }
    pub const fn maximum(self) -> u16 {
        match self {
            Self::Bool(_) | Self::ComparatorMode => 1,
            Self::Power => 15,
            Self::Delay => 4,
        }
    }
    pub const fn name(self) -> &'static str {
        match self {
            Self::Bool(p) => p.name(),
            Self::Power => "power",
            Self::Delay => "delay",
            Self::ComparatorMode => "mode",
        }
    }
    pub(crate) const fn same(self, other: Self) -> bool {
        match (self, other) {
            (Self::Bool(a), Self::Bool(b)) => a as u8 == b as u8,
            (Self::Power, Self::Power) => true,
            (Self::Delay, Self::Delay) | (Self::ComparatorMode, Self::ComparatorMode) => true,
            _ => false,
        }
    }
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq)]
pub enum SignalLevel {
    Binary(BoolProperty),
    Analog,
    StoredOutput,
}

impl SignalLevel {
    pub const fn property(self) -> Option<Property> {
        match self {
            Self::Binary(p) => Some(Property::Bool(p)),
            Self::Analog => Some(Property::Power),
            Self::StoredOutput => None,
        }
    }
}

impl DeviceDefinition {
    pub fn state(&self, block: &Block, property: Property) -> Result<u16, String> {
        if !self.properties.contains(&property) {
            return Err("undeclared device property".into());
        }
        let value = match property {
            Property::Bool(p) if p == self.primary_power => block.powered.map(u16::from),
            Property::Bool(p) => block
                .observed_properties
                .get(p.name())
                .and_then(|s| s.parse::<bool>().ok())
                .map(u16::from)
                .or_else(|| {
                    (p == super::BoolProperty::Locked
                        && block.observed_name.is_none()
                        && !block.observed_properties.contains_key(p.name()))
                    .then_some(0)
                }),
            Property::Power => block.power_level.map(u16::from),
            Property::Delay => block.delay.map(u16::from),
            Property::ComparatorMode => {
                match block.observed_properties.get("mode").map(String::as_str) {
                    Some("compare") => Some(0),
                    Some("subtract") => Some(1),
                    _ => None,
                }
            }
        }
        .ok_or_else(|| format!("missing device property {}", property.name()))?;
        if value > property.maximum() || property == Property::Delay && value == 0 {
            return Err("device property outside its range".into());
        }
        if block.observed_name.is_some() {
            let expected = match property {
                Property::Bool(_) => (value != 0).to_string(),
                Property::Power | Property::Delay => value.to_string(),
                Property::ComparatorMode => if value == 0 { "compare" } else { "subtract" }.into(),
            };
            if block.observed_properties.get(property.name()) != Some(&expected) {
                return Err(format!(
                    "inconsistent observed device property {}",
                    property.name()
                ));
            }
        }
        Ok(value)
    }

    pub fn validate_state(&self, block: &Block) -> Result<(), String> {
        if self.signal_level == SignalLevel::StoredOutput && block.power_level.is_some() {
            return Err("internal device output is not a block-state power property".into());
        }
        for property in &self.properties {
            self.state(block, *property)?;
        }
        Ok(())
    }

    /// Build the entire replacement first. Callers publish one delta and only
    /// then notify, so no callback can observe partially updated properties.
    pub(crate) fn write_state(
        &self,
        before: &Block,
        values: &[(Property, u16)],
    ) -> Result<Block, String> {
        self.validate_state(before)?;
        let mut after = before.clone();
        for (index, (property, value)) in values.iter().enumerate() {
            if !self.properties.contains(property)
                || !property.writable()
                || *value > property.maximum()
                || *property == Property::Delay && *value == 0
                || values[..index].iter().any(|(p, _)| p == property)
            {
                return Err("invalid device state write".into());
            }
            match property {
                Property::Bool(p) if *p == self.primary_power => after.powered = Some(*value != 0),
                Property::Power => after.power_level = Some(*value as u8),
                Property::Delay => after.delay = Some(*value as u8),
                _ => {}
            }
            if after.observed_name.is_some()
                || matches!(property, Property::Bool(p) if *p != self.primary_power)
            {
                after.observed_properties.insert(
                    property.name().into(),
                    match property {
                        Property::Bool(_) => (*value != 0).to_string(),
                        Property::Power | Property::Delay => value.to_string(),
                        Property::ComparatorMode => {
                            if *value == 0 { "compare" } else { "subtract" }.into()
                        }
                    },
                );
            }
        }
        Ok(after)
    }

    pub fn signal_strength(&self, block: &Block) -> Result<u8, String> {
        self.read_level(block, self.signal_level)
    }

    /// Comparator-readable state is independent of normal weak/strong power.
    pub fn comparator_readout(&self, block: &Block) -> Result<Option<u8>, String> {
        self.comparator_output
            .map(|level| self.read_level(block, level))
            .transpose()
    }

    fn read_level(&self, block: &Block, level: SignalLevel) -> Result<u8, String> {
        let value = self.state(
            block,
            level
                .property()
                .ok_or("device output requires runtime state")?,
        )? as u8;
        Ok(match level {
            SignalLevel::Binary(_) => value * 15,
            SignalLevel::Analog => value,
            SignalLevel::StoredOutput => unreachable!("no block-state property"),
        })
    }

    pub fn emission(
        &self,
        block: &Block,
        query: crate::Facing,
    ) -> Result<crate::piston_electrical_law::Emission, String> {
        let level = self.signal_strength(block)?;
        self.emission_with_level(block, query, level)
    }

    pub(crate) fn emission_with_level(
        &self,
        block: &Block,
        query: crate::Facing,
        level: u8,
    ) -> Result<crate::piston_electrical_law::Emission, String> {
        use super::Signal;
        use crate::piston_electrical_law::Emission;
        Ok(match self.signal {
            Signal::None => Emission { weak: 0, strong: 0 },
            Signal::Output => {
                let facing = block.facing.ok_or("device output required")?;
                let signal = if facing == query.opposite() { level } else { 0 };
                Emission {
                    weak: signal,
                    strong: signal,
                }
            }
            Signal::Attached => {
                let support = crate::piston_electrical::SIDES
                    .into_iter()
                    .find(|d| Some(d.offset()) == block.support_offset)
                    .ok_or("adjacent device support required")?;
                Emission {
                    weak: level,
                    strong: if query == support.opposite() {
                        level
                    } else {
                        0
                    },
                }
            }
        })
    }
}
