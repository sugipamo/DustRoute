//! Result owners are explicit. The unmigrated variant is only a temporary
//! marker for existing workflows; it must disappear before the JSON goal ends.
use super::construction::{AssemblyConstructionResult, ElectricalEditResult};
use super::mutation::{PlacementAttempt, RepairAttempt};
use crate::failure::ExecutionProgress;
use serde::{Serialize, Serializer};
use serde_json::Value;

#[derive(Clone, Debug, PartialEq)]
pub enum OperationResult {
    Placement(Box<PlacementAttempt>),
    Repair(Box<RepairAttempt>),
    ElectricalEdit(Box<ElectricalEditResult>),
    AssemblyConstruction(Box<AssemblyConstructionResult>),
    /// Existing workflows awaiting typed migration. No implicit From<Value>.
    #[doc(hidden)]
    Unmigrated(UnmigratedResult),
}
/// Temporary payload; only the explicitly unmigrated in-crate owners can
/// construct it. It is not an additional public JSON input or restore API.
#[doc(hidden)]
#[derive(Clone, Debug, PartialEq)]
pub struct UnmigratedResult(Value);
impl Serialize for UnmigratedResult {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        self.0.serialize(serializer)
    }
}
impl From<PlacementAttempt> for OperationResult {
    fn from(result: PlacementAttempt) -> Self {
        Self::Placement(Box::new(result))
    }
}
impl From<RepairAttempt> for OperationResult {
    fn from(result: RepairAttempt) -> Self {
        Self::Repair(Box::new(result))
    }
}
impl From<ElectricalEditResult> for OperationResult {
    fn from(result: ElectricalEditResult) -> Self {
        Self::ElectricalEdit(Box::new(result))
    }
}
impl From<AssemblyConstructionResult> for OperationResult {
    fn from(result: AssemblyConstructionResult) -> Self {
        Self::AssemblyConstruction(Box::new(result))
    }
}
impl Serialize for OperationResult {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        match self {
            Self::Placement(result) => result.serialize(serializer),
            Self::Repair(result) => result.serialize(serializer),
            Self::ElectricalEdit(result) => result.serialize(serializer),
            Self::AssemblyConstruction(result) => result.serialize(serializer),
            Self::Unmigrated(result) => result.serialize(serializer),
        }
    }
}
impl OperationResult {
    pub(crate) fn unmigrated(result: Value) -> Self {
        Self::Unmigrated(UnmigratedResult(result))
    }
    pub fn failed(&self) -> bool {
        match self {
            Self::Placement(result) => result.failed(),
            Self::Repair(result) => result.failed(),
            Self::ElectricalEdit(result) => result.failed(),
            Self::AssemblyConstruction(result) => result.failed(),
            Self::Unmigrated(result) => result.0.get("ok") == Some(&Value::Bool(false)),
        }
    }
    pub fn progress(&self) -> Option<&ExecutionProgress> {
        match self {
            Self::Placement(result) => result.progress(),
            Self::Repair(result) => result.progress(),
            Self::ElectricalEdit(result) => Some(result.progress()),
            Self::AssemblyConstruction(result) => Some(result.progress()),
            Self::Unmigrated(_) => None,
        }
    }
    pub(crate) fn consumed(&self) -> bool {
        match self {
            Self::Unmigrated(result) => {
                let result = &result.0;
                result
                    .pointer("/failure/progress/operation_consumed")
                    .or_else(|| result.pointer("/execution_progress/operation_consumed"))
                    == Some(&Value::Bool(true))
            }
            _ => self.progress().is_some_and(|p| p.operation_consumed),
        }
    }
    pub(crate) fn progress_percent(&self) -> u8 {
        if !self.failed() {
            return 100;
        }
        let (verified, total) = match self {
            Self::Unmigrated(result) => {
                let result = &result.0;
                let progress = result.pointer("/failure/progress");
                (
                    progress
                        .and_then(|p| p.get("verified_steps"))
                        .or_else(|| result.get("verified_steps"))
                        .and_then(Value::as_u64)
                        .map(u128::from),
                    progress
                        .and_then(|p| p.get("total_changes"))
                        .or_else(|| result.get("total_steps"))
                        .and_then(Value::as_u64)
                        .map(u128::from),
                )
            }
            _ => (
                self.progress().map(|p| p.verified_steps as u128),
                self.progress()
                    .and_then(|p| p.total_changes)
                    .map(|n| n as u128),
            ),
        };
        verified
            .zip(total)
            .filter(|(_, n)| *n > 0)
            .map_or(0, |(v, n)| ((v * 100 / n).min(100)) as u8)
    }
}
