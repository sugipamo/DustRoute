use std::fmt::{Display, Formatter};

use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Deserialize, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize)]
#[serde(try_from = "String", into = "String")]
pub struct ComponentId(String);

impl ComponentId {
    pub fn new(value: impl Into<String>) -> Result<Self, &'static str> {
        let value = value.into();
        if value.is_empty()
            || !value.bytes().all(|byte| {
                byte.is_ascii_lowercase() || byte.is_ascii_digit() || b"._-".contains(&byte)
            })
        {
            return Err(
                "component id must contain only lowercase ASCII letters, digits, '.', '_', or '-'",
            );
        }
        Ok(Self(value))
    }

    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl TryFrom<String> for ComponentId {
    type Error = &'static str;

    fn try_from(value: String) -> Result<Self, Self::Error> {
        Self::new(value)
    }
}

impl From<ComponentId> for String {
    fn from(value: ComponentId) -> Self {
        value.0
    }
}

impl Display for ComponentId {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ComponentKind {
    Not,
    And,
    Or,
    Xor,
    HalfAdder,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum PortDirection {
    Input,
    Output,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct Port {
    pub name: String,
    pub direction: PortDirection,
    pub bit_width: usize,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct LogicalSpec {
    pub ports: Vec<Port>,
    /// Rows are ordered as all input values followed by all output values.
    pub truth_table: Vec<Vec<bool>>,
    pub stateful: bool,
}

impl LogicalSpec {
    #[must_use]
    pub fn input_names(&self) -> Vec<&str> {
        self.ports
            .iter()
            .filter(|port| port.direction == PortDirection::Input)
            .map(|port| port.name.as_str())
            .collect()
    }

    #[must_use]
    pub fn output_names(&self) -> Vec<&str> {
        self.ports
            .iter()
            .filter(|port| port.direction == PortDirection::Output)
            .map(|port| port.name.as_str())
            .collect()
    }
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct PhysicalMetrics {
    pub bounding_size: [usize; 3],
    pub occupied_blocks: usize,
    pub dust_blocks: usize,
    pub repeater_count: usize,
    pub delay_redstone_ticks: Option<usize>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct Compatibility {
    pub edition: String,
    pub versions: Vec<String>,
    #[serde(default)]
    pub incompatible_versions: Vec<String>,
    pub orientation_sensitive: bool,
    pub update_order_sensitive: bool,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum EvidenceKind {
    PublishedClaim,
    LogicalExhaustive,
    Simulated,
    MinecraftE2e,
    MinecraftE2eRejected,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct Evidence {
    pub kind: EvidenceKind,
    pub reference: String,
    pub notes: Option<String>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct Provenance {
    pub author: String,
    pub source_url: Option<String>,
    pub license: Option<String>,
    pub retrieved_on: Option<String>,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum VerificationLevel {
    Claimed,
    LogicallyVerified,
    Simulated,
    MinecraftVerified,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct Component {
    pub id: ComponentId,
    pub name: String,
    pub kind: ComponentKind,
    pub logical: LogicalSpec,
    /// Stable reference resolved by the translation layer.
    pub layout_reference: Option<String>,
    pub physical: Option<PhysicalMetrics>,
    pub compatibility: Option<Compatibility>,
    pub provenance: Provenance,
    pub evidence: Vec<Evidence>,
}

impl Component {
    #[must_use]
    pub fn verification_level(&self) -> VerificationLevel {
        if self
            .evidence
            .iter()
            .any(|item| item.kind == EvidenceKind::MinecraftE2e)
        {
            VerificationLevel::MinecraftVerified
        } else if self
            .evidence
            .iter()
            .any(|item| item.kind == EvidenceKind::Simulated)
        {
            VerificationLevel::Simulated
        } else if self
            .evidence
            .iter()
            .any(|item| item.kind == EvidenceKind::LogicalExhaustive)
        {
            VerificationLevel::LogicallyVerified
        } else {
            VerificationLevel::Claimed
        }
    }

    #[must_use]
    pub fn may_automatically_replace_physical_circuit(&self) -> bool {
        self.verification_level() == VerificationLevel::MinecraftVerified
            && !self
                .evidence
                .iter()
                .any(|item| item.kind == EvidenceKind::MinecraftE2eRejected)
            && self.physical.is_some()
            && self.compatibility.is_some()
            && self.provenance.license.is_some()
    }
}
