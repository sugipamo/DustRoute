//! Typed application results. JSON encoding belongs to the public tool facade.
use super::*;
use crate::api::McpErrorCode;
use dustroute_translate::building::{
    BuildingDesignError, GeneratedBuilding, GeneratedBuildingDesign, GeneratedBuildingDesignUpdate,
    GeneratedGroundedBuildingDesign,
};
use dustroute_translate::review_diagnostics::ReviewDiagnostics;

#[derive(Debug, Serialize)]
pub(crate) struct Response {
    pub ok: bool,
    schema_version: &'static str,
    writes_minecraft: bool,
    #[serde(flatten)]
    pub body: Body,
}
impl Response {
    pub fn success(body: Body) -> Self {
        Self {
            ok: true,
            schema_version: "dustroute.blueprint-mcp.v1",
            writes_minecraft: false,
            body,
        }
    }
    pub fn failure(message: impl ToString) -> Self {
        let mut response = Self::success(Body::Failure {
            error_code: McpErrorCode::InvalidState,
            error: message.to_string(),
        });
        response.ok = false;
        response
    }
    pub fn generated(result: Generated, ok: bool, next_step: &'static str) -> Self {
        Self {
            ok,
            ..Self::success(Body::Generated {
                result,
                catalog_changed: false,
                adoption_authorized: false,
                next_step,
            })
        }
    }
    pub fn building(
        result: Result<Generated, BuildingDesignError>,
        next_step: &'static str,
    ) -> Self {
        match result {
            Ok(result) => Self::generated(result, true, next_step),
            Err(error) => Self {
                ok: false,
                ..Self::success(Body::BuildingErrors {
                    errors: vec![error],
                    catalog_changed: false,
                    adoption_authorized: false,
                })
            },
        }
    }
}

#[derive(Debug, Serialize)]
#[serde(untagged)]
pub(crate) enum Body {
    Read {
        result: ReadRecord,
        catalog_membership_is_validation_proof: bool,
    },
    Operation(Box<OperationResponse>),
    Captured {
        assembly_revision_id: AssemblyRevisionId,
        validation: &'static str,
    },
    Imported {
        validation: &'static str,
        next_step: &'static str,
    },
    Generated {
        result: Generated,
        catalog_changed: bool,
        adoption_authorized: bool,
        next_step: &'static str,
    },
    BuildingErrors {
        errors: Vec<BuildingDesignError>,
        catalog_changed: bool,
        adoption_authorized: bool,
    },
    Failure {
        error_code: McpErrorCode,
        error: String,
    },
}

#[derive(Debug, Serialize)]
#[serde(untagged)]
pub(crate) enum Generated {
    Building(Box<GeneratedBuilding>),
    Design(Box<GeneratedBuildingDesign>),
    DesignUpdate(Box<GeneratedBuildingDesignUpdate>),
    Grounded(Box<GeneratedGroundedBuildingDesign>),
    FlyingMachine(Box<dustroute_translate::flying_machine::GeneratedFlyingMachine>),
    Enumeration(Box<dustroute_optimize::blueprint_reduction::TorchSupportEnumerationReport>),
    Reduction(Box<dustroute_optimize::blueprint_reduction::BlueprintReductionReport>),
}

#[derive(Debug, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub(crate) enum ReadRecord {
    Catalog(Box<CatalogResponse>),
    Blueprint {
        record: Box<BlueprintRevision>,
    },
    Assembly(Box<AssemblyResponse>),
    Type {
        record: Box<TypeRevision>,
    },
    Classification {
        record: Box<ClassificationRevision>,
    },
    Archive {
        archive: Box<BlueprintUpdateArchive>,
    },
}
#[derive(Debug, Serialize)]
pub(crate) struct CatalogResponse {
    pub blueprints: Vec<BlueprintSummary>,
    pub total_blueprints: usize,
    pub next_offset: Option<usize>,
    pub classifications: Vec<DefinitionSummary<ClassificationRevisionId>>,
    pub types: Vec<DefinitionSummary<TypeRevisionId>>,
    pub assemblies: Vec<AssemblyRevisionId>,
    pub operations: Vec<ProposalSummary>,
}
#[derive(Debug, Serialize)]
pub(crate) struct BlueprintSummary {
    pub id: BlueprintRevisionId,
    pub name: String,
    pub classifications: Vec<ClassificationRevisionId>,
}
#[derive(Debug, Serialize)]
pub(crate) struct DefinitionSummary<I> {
    pub id: I,
    pub name: String,
}
#[derive(Debug, Serialize)]
pub(crate) struct ProposalSummary {
    pub operation_id: BlueprintUpdateId,
    pub status: UpdateStatus,
    pub title: String,
}
#[derive(Debug, Serialize)]
pub(crate) struct AssemblyResponse {
    pub record: AssemblyRevision,
    pub lifecycle: AssemblyLifecycle,
    #[serde(flatten, skip_serializing_if = "Option::is_none")]
    pub review: Option<AssemblyReview>,
}
#[derive(Debug, Serialize)]
pub(crate) struct AssemblyReview {
    pub validation: ReviewResponse,
    pub validation_context: Option<BehaviorReviewContext>,
    pub world_execution_context:
        Option<dustroute_translate::world::execution_context::WorldExecutionContext>,
}
#[derive(Debug, Serialize)]
#[serde(untagged)]
pub(crate) enum AssemblyLifecycle {
    Unadopted {
        status: &'static str,
        adopted: bool,
        placement_eligible: bool,
        reason: &'static str,
    },
    Adopted {
        status: &'static str,
        adopted: bool,
        placement_eligible: bool,
        grounded_placement_eligible: bool,
        new_target_construction_eligible: bool,
        adopted_by: Vec<BlueprintUpdateId>,
        reason: &'static str,
    },
}
#[derive(Clone, Debug, Serialize)]
pub(crate) struct ReviewResponse {
    pub status: CheckStatus,
    pub fresh: bool,
    pub scope: &'static str,
    pub diagnostics: ReviewDiagnostics,
    pub placement_validation_profile: &'static str,
    pub initial_state_checks_passed: bool,
    pub checks: RecordedReview,
    pub behavior_verified: bool,
    pub declared_behavior_verified: bool,
    pub behavior_scope: &'static str,
    pub behavior_status: Option<CheckStatus>,
    pub live_world_verified: bool,
}
#[derive(Debug, Serialize)]
pub(crate) struct OperationResponse {
    pub operation_id: BlueprintUpdateId,
    pub operation: ProposalDetail,
    pub stored_history_is_validation_proof: bool,
    pub next_step: &'static str,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub diff: Option<BlueprintUpdateDiff>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub can_adopt: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub validation: Option<ReviewResponse>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error_code: Option<McpErrorCode>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<&'static str>,
}
#[derive(Debug, Serialize)]
pub(crate) struct ProposalDetail {
    pub id: BlueprintUpdateId,
    pub kind: &'static str,
    pub status: UpdateStatus,
    pub request: BlueprintUpdateRequest,
    pub history: Vec<UpdateEvent>,
}
