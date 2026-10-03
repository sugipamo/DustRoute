//! Deserializable historical facts, deliberately separate from native plans/tokens.
use super::{ExecutionError, OperationOutcome, Result, SurvivalErrorCode};
use crate::survival_construction::{
    ConstructionMaterials, ConstructionScope, HypotheticalConstructionPlan,
    HypotheticalConstructionStep, PlacementPurpose, TemporaryBlock,
};
use serde::{Deserialize, Serialize};
use voxrig::checked_survival::diagnostic::{self as records, ToDiagnostic};
use voxrig::checked_survival::{SurvivalCapabilities, SurvivalMotionContract};
use voxrig::{ConnectionConfig, MinecraftVersion};

/// A serialized `false` marker cannot represent restorable authority or replay.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(try_from = "bool", into = "bool")]
pub struct DiagnosticOnly;
impl TryFrom<bool> for DiagnosticOnly {
    type Error = &'static str;
    fn try_from(value: bool) -> std::result::Result<Self, Self::Error> {
        if value {
            Err("historical data cannot grant native authority or automatic replay")
        } else {
            Ok(Self)
        }
    }
}
impl From<DiagnosticOnly> for bool {
    fn from(_: DiagnosticOnly) -> Self {
        false
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CheckpointClaim {
    pub new_job: uuid::Uuid,
    pub execution_id: uuid::Uuid,
    pub automatic_replay: DiagnosticOnly,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum ProtocolVersion {
    Java1_16_1,
    Java1_21_11,
}
impl From<MinecraftVersion> for ProtocolVersion {
    fn from(version: MinecraftVersion) -> Self {
        match version {
            MinecraftVersion::Java1_16_1 => Self::Java1_16_1,
            MinecraftVersion::Java1_21_11 => Self::Java1_21_11,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RecordedEndpoint {
    pub host: String,
    pub port: u16,
    pub username: String,
    pub version: ProtocolVersion,
}
impl From<&ConnectionConfig> for RecordedEndpoint {
    fn from(config: &ConnectionConfig) -> Self {
        Self {
            host: config.server.host.clone(),
            port: config.server.port,
            username: config.username.clone(),
            version: config.version.into(),
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum CheckpointSchema {
    #[serde(rename = "dustroute.survival-checkpoint.v2")]
    V2,
    #[serde(other)]
    Unknown,
}

pub use records::{
    RecordedHypotheticalBlockEdit as RecordedEdit,
    RecordedHypotheticalPlacement as RecordedPlacement,
};

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum RecordedStep {
    Move {
        #[serde(skip_serializing_if = "Option::is_none")]
        prediction: Option<Box<records::RecordedHypotheticalMovementPreview>>,
    },
    Place {
        purpose: PlacementPurpose,
        #[serde(skip_serializing_if = "Option::is_none")]
        placement: Option<RecordedPlacement>,
    },
    RemoveTemporary {
        edit: RecordedEdit,
        face_id: u8,
        rotation: [f32; 2],
        /// Diagnostic native reconnect requirement; no token can be restored.
        reconnect: records::RecordedHypotheticalReconnectBoundary,
    },
    #[serde(other)]
    Unknown,
}
impl RecordedStep {
    pub(super) fn expected_outcome(&self) -> Result<OperationOutcome> {
        match self {
            Self::Move { .. } => Ok(OperationOutcome::Predicted),
            Self::Place { .. } | Self::RemoveTemporary { .. } => Ok(OperationOutcome::Observed),
            Self::Unknown => Err(ExecutionError::new(
                SurvivalErrorCode::InvalidCheckpoint,
                "unknown historical step",
            )),
        }
    }
}
impl From<&HypotheticalConstructionStep> for RecordedStep {
    fn from(step: &HypotheticalConstructionStep) -> Self {
        match step {
            HypotheticalConstructionStep::Move { prediction } => Self::Move {
                prediction: Some(prediction.diagnostic()),
            },
            HypotheticalConstructionStep::Place { purpose, placement } => Self::Place {
                purpose: *purpose,
                placement: Some(placement.into()),
            },
            HypotheticalConstructionStep::RemoveTemporary {
                edit,
                face_id,
                rotation,
                reconnect,
            } => Self::RemoveTemporary {
                edit: edit.into(),
                face_id: *face_id,
                rotation: *rotation,
                reconnect: reconnect.into(),
            },
        }
    }
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RecordedMotionContract {
    IndependentlyObserved,
    Predicted,
}
impl From<SurvivalMotionContract> for RecordedMotionContract {
    fn from(contract: SurvivalMotionContract) -> Self {
        match contract {
            SurvivalMotionContract::IndependentlyObserved => Self::IndependentlyObserved,
            SurvivalMotionContract::Predicted => Self::Predicted,
        }
    }
}

/// A historical plan describes facts; it cannot be used with live checked APIs.
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RecordedConstructionPlan {
    pub scope: ConstructionScope,
    pub initial_temporary: Vec<TemporaryBlock>,
    pub steps: Vec<RecordedStep>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub motion_contract: Option<RecordedMotionContract>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub source: Option<Box<records::RecordedStandingContext>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub baseline: Option<dustroute_translate::snapshot::MinecraftSnapshot>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub expected: Option<dustroute_translate::snapshot::MinecraftSnapshot>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub materials: Option<ConstructionMaterials>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub final_position: Option<[f64; 3]>,
}
impl From<&HypotheticalConstructionPlan> for RecordedConstructionPlan {
    fn from(plan: &HypotheticalConstructionPlan) -> Self {
        Self {
            scope: plan.scope().clone(),
            initial_temporary: plan.initial_temporary().to_vec(),
            steps: plan.steps().iter().map(Into::into).collect(),
            motion_contract: Some(plan.motion_contract().into()),
            source: Some(Box::new(plan.source().into())),
            baseline: Some(plan.baseline().clone()),
            expected: Some(plan.expected().clone()),
            materials: Some(plan.materials().clone()),
            final_position: Some(plan.final_position()),
        }
    }
}

/// Saved search and plan facts. This does not deserialize a native generated plan.
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RecordedConstructionPreview {
    pub plan: RecordedConstructionPlan,
    pub search: crate::survival_construction::generation::ConstructionSearch,
}
impl From<&crate::survival_construction::generation::GeneratedConstructionPlan>
    for RecordedConstructionPreview
{
    fn from(
        generated: &crate::survival_construction::generation::GeneratedConstructionPlan,
    ) -> Self {
        Self {
            plan: (&generated.plan).into(),
            search: generated.search.clone(),
        }
    }
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RecordedExecutionPlan {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub plan: Option<RecordedConstructionPlan>,
    /// Static implementation support, never live operation readiness.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub capabilities: Option<SurvivalCapabilities>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub declared_reconnect: Option<RecordedEndpoint>,
}
impl RecordedExecutionPlan {
    pub(super) fn capture(
        plan: &HypotheticalConstructionPlan,
        capabilities: SurvivalCapabilities,
        reconnect: &ConnectionConfig,
    ) -> Self {
        Self {
            plan: Some(plan.into()),
            capabilities: Some(capabilities),
            declared_reconnect: Some(reconnect.into()),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    #[test]
    fn persisted_claim_cannot_enable_automatic_replay() {
        let mut wire = json!({"new_job":uuid::Uuid::new_v4(),"execution_id":uuid::Uuid::new_v4(),"automatic_replay":false});
        let claim: CheckpointClaim = serde_json::from_value(wire.clone()).unwrap();
        assert_eq!(json!(claim), wire);
        wire["automatic_replay"] = json!(true);
        assert!(serde_json::from_value::<CheckpointClaim>(wire).is_err());
    }
}
