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
}

impl Property {
    pub const fn maximum(self) -> u16 {
        match self {
            Self::Bool(_) => 1,
            Self::Power => 15,
        }
    }
    pub const fn name(self) -> &'static str {
        match self {
            Self::Bool(p) => p.name(),
            Self::Power => "power",
        }
    }
    pub(crate) const fn same(self, other: Self) -> bool {
        match (self, other) {
            (Self::Bool(a), Self::Bool(b)) => a as u8 == b as u8,
            (Self::Power, Self::Power) => true,
            _ => false,
        }
    }
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq)]
pub enum SignalLevel {
    Binary(BoolProperty),
    Analog,
}

impl SignalLevel {
    pub const fn property(self) -> Property {
        match self {
            Self::Binary(p) => Property::Bool(p),
            Self::Analog => Property::Power,
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
                .map(u16::from),
            Property::Power => block.power_level.map(u16::from),
        }
        .ok_or_else(|| format!("missing device property {}", property.name()))?;
        if value > property.maximum() {
            return Err("device property outside its range".into());
        }
        if block.observed_name.is_some() {
            let expected = match property {
                Property::Bool(_) => (value != 0).to_string(),
                Property::Power => value.to_string(),
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
                || *value > property.maximum()
                || values[..index].iter().any(|(p, _)| p == property)
            {
                return Err("invalid device state write".into());
            }
            match property {
                Property::Bool(p) if *p == self.primary_power => after.powered = Some(*value != 0),
                Property::Power => after.power_level = Some(*value as u8),
                _ => {}
            }
            if after.observed_name.is_some()
                || matches!(property, Property::Bool(p) if *p != self.primary_power)
            {
                after.observed_properties.insert(
                    property.name().into(),
                    match property {
                        Property::Bool(_) => (*value != 0).to_string(),
                        Property::Power => value.to_string(),
                    },
                );
            }
        }
        Ok(after)
    }

    pub fn signal_strength(&self, block: &Block) -> Result<u8, String> {
        let value = self.state(block, self.signal_level.property())? as u8;
        Ok(match self.signal_level {
            SignalLevel::Binary(_) => value * 15,
            SignalLevel::Analog => value,
        })
    }

    pub fn emission(
        &self,
        block: &Block,
        query: crate::Facing,
    ) -> Result<crate::piston_electrical_law::Emission, String> {
        use super::Signal;
        use crate::piston_electrical_law::Emission;
        let level = self.signal_strength(block)?;
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
