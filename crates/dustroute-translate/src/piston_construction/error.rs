//! Typed model failures; rendering belongs at the MCP/string boundary.
use dustroute_minecraft::time::runtime::RuntimeError;
use std::fmt::{Display, Formatter};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ConstructionBudget {
    ContextBlocks,
    StageChanges,
    JobChanges,
    WorkStages,
}
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ConstructionError {
    InvalidInput(String),
    Budget {
        resource: ConstructionBudget,
        actual: usize,
        maximum: usize,
    },
    Runtime(RuntimeError),
    Literal(crate::snapshot::LiteralSnapshotError),
    Model(String),
}
impl Display for ConstructionError {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::InvalidInput(s) | Self::Model(s) => f.write_str(s),
            Self::Runtime(e) => Display::fmt(e, f),
            Self::Literal(e) => Display::fmt(e, f),
            Self::Budget {
                resource,
                actual,
                maximum,
            } => write!(
                f,
                "construction {resource:?} budget exceeded: {actual}, maximum {maximum}"
            ),
        }
    }
}
impl std::error::Error for ConstructionError {}
impl From<String> for ConstructionError {
    fn from(s: String) -> Self {
        Self::Model(s)
    }
}
impl From<&str> for ConstructionError {
    fn from(s: &str) -> Self {
        Self::InvalidInput(s.into())
    }
}
impl From<RuntimeError> for ConstructionError {
    fn from(e: RuntimeError) -> Self {
        Self::Runtime(e)
    }
}
impl From<ConstructionError> for String {
    fn from(e: ConstructionError) -> Self {
        e.to_string()
    }
}

impl From<crate::snapshot::LiteralSnapshotError> for ConstructionError {
    fn from(e: crate::snapshot::LiteralSnapshotError) -> Self {
        Self::Literal(e)
    }
}
