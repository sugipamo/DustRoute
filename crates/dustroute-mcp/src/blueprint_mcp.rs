//! Local immutable definitions and proposal decisions, never Minecraft writes.
use std::fs::{self, OpenOptions};
use std::io::Read;

use dustroute_library::assembly::AssemblyRevision;
use dustroute_library::behavior_context::BehaviorReviewContext;
use dustroute_library::blueprint::*;
use dustroute_translate::behavior_type::BehaviorBudget;
use dustroute_translate::blueprint_update::*;
use dustroute_translate::promotion::{CheckStatus, PromotionReport, review_assembly_with_context};
use rmcp::schemars;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

use crate::state::PlanStateStore;

mod response;
use response::*;
pub(crate) use response::{Response, ReviewResponse};

const MAX_ARCHIVE_BYTES: u64 = 16 * 1024 * 1024;
const MAX_REQUEST_BYTES: usize = 4 * 1024 * 1024;
const STORE_SCHEMA: &str = "dustroute.mcp-blueprints.v3";

/// Current stores always carry the grounding map, including when it is empty.
/// An absent map is a retired representation, never invented placement evidence.
#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct StoredBlueprints {
    schema: String,
    owner: String,
    archive: BlueprintUpdateArchive,
    groundings: BTreeMap<AssemblyRevisionId, AssemblyGrounding>,
}

/// New requests may select the standard piston context without an orientation
/// or profile ID. Resolve it before storage; saved records never gain defaults.
#[derive(Debug, Deserialize, Serialize, schemars::JsonSchema)]
#[serde(untagged)]
pub(crate) enum RequestedBehaviorContext {
    Recorded(BehaviorReviewContext),
    FreshPistons(FreshPistonRequest),
}

#[derive(Debug, Deserialize, Serialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub(crate) struct FreshPistonRequest {
    piston: FreshPistonAssumptions,
}

#[derive(Debug, Deserialize, Serialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
struct FreshPistonAssumptions {
    known_region: dustroute_translate::world::Region,
    input_levers: Vec<dustroute_translate::world::Pos>,
    #[serde(default)]
    root_limits: dustroute_library::runtime_behavior::RuntimeLimits,
}

impl RequestedBehaviorContext {
    fn resolve(self) -> BehaviorReviewContext {
        match self {
            Self::Recorded(context) => context,
            Self::FreshPistons(request) => {
                let mut context =
                    dustroute_library::runtime_behavior::RuntimeBehaviorContext::fresh_pistons(
                        request.piston.known_region,
                        request.piston.input_levers,
                    );
                context.root_limits = request.piston.root_limits;
                context.into()
            }
        }
    }
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub(crate) enum BlueprintRead {
    /// List IDs and interpretation labels. Membership is not verification.
    Catalog {
        classification_id: Option<ClassificationRevisionId>,
        offset: Option<usize>,
        limit: Option<usize>,
    },
    Blueprint {
        id: BlueprintRevisionId,
    },
    Assembly {
        id: AssemblyRevisionId,
        validate: Option<bool>,
        /// Optional bounded model assumptions and actual input drivers for declared behavior.
        behavior_context: Option<RequestedBehaviorContext>,
    },
    Classification {
        id: ClassificationRevisionId,
    },
    Type {
        id: TypeRevisionId,
    },
    /// Export all definitions, states and proposal history as data, never proof.
    Archive,
}

pub(crate) use dustroute_library::blueprint::BlueprintRecords;

#[derive(Debug, Deserialize, Serialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub(crate) struct UpdateInput {
    pub title: String,
    pub description: String,
    pub base_state: AssemblyRevisionId,
    pub base_parent: BlueprintRevisionId,
    pub candidate_parent: BlueprintRevisionId,
    pub parent_instance: InstancePath,
    /// Relative to parent_instance, not the assembly root.
    pub child_before: InstancePath,
    pub previous_child: BlueprintRevisionId,
    /// Relative to parent_instance; decomposition may change.
    pub child_after: InstancePath,
    pub next_child: BlueprintRevisionId,
    /// Complete new parent and changed intermediates, with new immutable IDs.
    pub revisions: Vec<BlueprintRevision>,
    /// Explicit candidate physical state and connections; nothing is auto-routed.
    pub candidate_state: AssemblyRevision,
    /// Fresh model checks on review and adoption; omitted means snapshot only.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub behavior_context: Option<RequestedBehaviorContext>,
}

impl UpdateInput {
    fn into_request(self, id: BlueprintUpdateId) -> BlueprintUpdateRequest {
        BlueprintUpdateRequest {
            id,
            title: self.title,
            description: self.description,
            base_state: self.base_state,
            base_parent: self.base_parent,
            candidate_parent: self.candidate_parent,
            parent_instance: self.parent_instance,
            child_before: self.child_before,
            previous_child: self.previous_child,
            child_after: self.child_after,
            next_child: self.next_child,
            revisions: self.revisions,
            candidate_state: self.candidate_state,
            behavior_context: self.behavior_context.map(RequestedBehaviorContext::resolve),
        }
    }
}

#[derive(Debug, Deserialize, Serialize, schemars::JsonSchema)]
#[serde(tag = "action", rename_all = "snake_case", deny_unknown_fields)]
pub(crate) enum BlueprintWrite {
    /// Author passive survival geometry with existing protected ground; no adoption or live writes.
    GenerateGroundedBuildingDesign {
        request: Box<dustroute_library::building::GroundedBuildingDesignRequest>,
    },
    /// Author explicit virtual geometry and permanent air spaces, optionally
    /// wrapping one uniquely adopted equipment Assembly. No world writes.
    GenerateBuildingDesign {
        request: Box<dustroute_library::building::BuildingDesignRequest>,
    },
    /// Generate a diff against a uniquely adopted immutable design. Check the
    /// previous structured input, retain unchanged pins, freshly review the new
    /// candidate. Does not replace a placed instance or publish definitions.
    GenerateBuildingDesignUpdate {
        request: Box<dustroute_library::building::BuildingDesignUpdateRequest>,
    },
    /// Generate a bounded enclosure, its structural obligations and a proposal.
    /// Does not import, adopt or write any Minecraft blocks.
    GenerateBuilding {
        request: Box<dustroute_library::building::BuildingRequest>,
    },
    /// Compose a bounded enclosure and a uniquely adopted, typed 3x3 door.
    /// Freshly checks the whole world; neither publishes nor places it.
    GenerateBuildingWithDoor {
        request: Box<dustroute_library::building::BuildingWithDoorRequest>,
    },
    /// Generate one bounded single-launch flying-machine candidate. Returns
    /// unadopted records, a proposal and fresh model checks without publishing.
    GenerateFlyingMachine {
        request: Box<dustroute_library::flying_machine::FlyingMachineRequest>,
    },
    /// Append unverified definitions/states; never promotes or adopts them.
    Import { records: BlueprintRecords },
    /// Retain the exact modeled Assembly Revision of a saved circuit revision.
    CaptureRevision { revision_id: String },
    /// Create a PR-like proposal; definitions stay unpublished until adoption.
    ProposeUpdate { request: Box<UpdateInput> },
    /// Enumerate the physical torch/support family under an explicit component
    /// boundary, then freshly check the selected behavior for every candidate.
    EnumerateLayouts {
        request: Box<dustroute_optimize::blueprint_reduction::TorchSupportEnumerationRequest>,
        budget: Option<dustroute_optimize::blueprint_reduction::BlueprintReductionBudget>,
    },
    /// Search the supplied Assembly or component for fewer actual blocks, preserving
    /// only the selected behavioral type. Returns standalone candidate data;
    /// publishing or changing a parent still needs an explicit update proposal.
    Optimize {
        request: Box<dustroute_optimize::blueprint_reduction::BlueprintReductionRequest>,
        budget: Option<dustroute_optimize::blueprint_reduction::BlueprintReductionBudget>,
    },
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
#[serde(tag = "action", rename_all = "snake_case", deny_unknown_fields)]
pub(crate) enum BlueprintDecision {
    Adopt,
    Reject { reason: String },
}

pub(crate) enum Command {
    Read(BlueprintRead),
    Write(crate::mcp_input::MeasuredBlueprintWrite),
    Capture {
        record: Box<AssemblyRevision>,
        grounding: AssemblyGrounding,
    },
    Show(uuid::Uuid),
    Get(uuid::Uuid),
    Decide(uuid::Uuid, Option<BlueprintDecision>, bool),
    Undo(uuid::Uuid),
}

/// Durable literal placement evidence, separate from Assembly interpretation
/// and from every saved review result.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct AssemblyGrounding {
    pub assembly_revision_id: AssemblyRevisionId,
    pub circuit_revision_id: uuid::Uuid,
    pub base_observation_id: uuid::Uuid,
    pub dimension: String,
    pub complete: bool,
    pub base_snapshot: dustroute_translate::snapshot::MinecraftSnapshot,
}

pub(crate) fn failure(message: impl ToString) -> Response {
    Response::failure(message)
}

pub(crate) fn review_response(
    report: &PromotionReport,
    catalog: &BlueprintCatalog,
) -> ReviewResponse {
    // Keep the existing v1 value for catalogs with only periodic obligations.
    let native = report
        .behavior_context
        .as_ref()
        .is_some_and(BehaviorReviewContext::is_runtime);
    let scope = if native {
        "whole_realization_declared_obligations_in_moving_world_model"
    } else if report.behavior_context.is_none() {
        "initial_state_placement_types_and_connections_only"
    } else if catalog.type_revisions().any(|r| {
        matches!(
            r.contract,
            TypeContract::FiniteBurst { .. }
                | TypeContract::RepeatedSettling { .. }
                | TypeContract::PistonDoor { .. }
        )
    }) {
        "placement_connections_and_declared_behavioral_obligations"
    } else {
        "placement_connections_and_declared_periodic_obligations"
    };
    ReviewResponse {
        status: report.status(),
        fresh: true,
        scope,
        diagnostics: report.diagnostics(64),
        placement_validation_profile: report.placement_validation_profile(),
        initial_state_checks_passed: report.status() == CheckStatus::Passed,
        checks: RecordedReview::from(report),
        behavior_verified: false,
        declared_behavior_verified: report.status() == CheckStatus::Passed
            && report.behavior_status() == Some(CheckStatus::Passed),
        behavior_scope: "declared_obligations_in_selected_model_only",
        behavior_status: report.behavior_status(),
        live_world_verified: false,
    }
}

fn operation(
    updates: &BlueprintUpdates,
    id: &BlueprintUpdateId,
    details: bool,
) -> Result<OperationResponse, String> {
    let proposal = updates
        .proposal(id)
        .ok_or("unknown blueprint update operation")?;
    Ok(OperationResponse {
        operation_id: id.clone(),
        operation: ProposalDetail {
            id: id.clone(),
            kind: "blueprint_update",
            status: proposal.status(),
            request: proposal.request().clone(),
            history: proposal.events().to_vec(),
        },
        stored_history_is_validation_proof: false,
        next_step: if proposal.status() == UpdateStatus::Open {
            "show_operation for a fresh review; invoke_operation with confirm=true and an explicit blueprint_decision"
        } else {
            "retain this decision and its exact revisions; a further change requires a new proposal"
        },
        diff: details
            .then(|| updates.diff(id).map_err(|e| e.to_string()))
            .transpose()?,
        can_adopt: None,
        validation: None,
        error_code: None,
        error: None,
    })
}

fn operation_response(operation: OperationResponse) -> Response {
    Response::success(Body::Operation(Box::new(operation)))
}

fn assembly_lifecycle(
    updates: &BlueprintUpdates,
    groundings: &BTreeMap<AssemblyRevisionId, AssemblyGrounding>,
    id: &AssemblyRevisionId,
) -> AssemblyLifecycle {
    let adopted_by = updates
        .proposals()
        .filter(|proposal| {
            proposal.status() == UpdateStatus::Adopted
                && proposal.request().candidate_state.id == *id
        })
        .map(|proposal| proposal.request().id.clone())
        .collect::<Vec<_>>();
    if adopted_by.is_empty() {
        AssemblyLifecycle::Unadopted {
            status: "catalog_only_unadopted",
            adopted: false,
            placement_eligible: false,
            reason: "catalog import or capture is immutable data, not an adopted update",
        }
    } else {
        let grounded = updates.catalog().assembly(id).is_some_and(|record| {
            let mut current = record;
            loop {
                if groundings.contains_key(&current.id) {
                    break true;
                }
                let [parent] = current.parents.as_slice() else {
                    break false;
                };
                let Some(next) = updates.catalog().assembly(parent) else {
                    break false;
                };
                current = next;
            }
        });
        let construction = adopted_by.len() == 1
            && updates.proposal(&adopted_by[0]).is_some_and(|proposal| {
                matches!(
                    proposal.request().behavior_context.as_ref(),
                    Some(BehaviorReviewContext::Runtime(_))
                )
            });
        AssemblyLifecycle::Adopted {
            status: "adopted",
            adopted: true,
            placement_eligible: grounded || construction,
            grounded_placement_eligible: grounded,
            new_target_construction_eligible: construction,
            adopted_by,
            reason: if construction {
                "new-target construction may be requested; fresh target review, complete empty-region observation and server settings are still required"
            } else if grounded {
                "placement may be requested; fresh model and live-world checks are still mandatory"
            } else {
                "adoption is established, but no captured literal observation grounds this Assembly ancestry"
            },
        }
    }
}

#[derive(Debug)]
pub(crate) struct GroundedSource {
    pub record: AssemblyRevision,
    pub adopted_by: BlueprintUpdateId,
    pub grounding_assembly_revision_id: AssemblyRevisionId,
    pub grounding: AssemblyGrounding,
    /// Display report only; admission already checked the typed PromotionReport.
    pub fresh_review: ReviewResponse,
}

fn placement_basis(
    updates: &BlueprintUpdates,
    groundings: &BTreeMap<AssemblyRevisionId, AssemblyGrounding>,
    id: &AssemblyRevisionId,
) -> Result<GroundedSource, String> {
    let adopted = updates
        .proposals()
        .filter(|proposal| {
            proposal.status() == UpdateStatus::Adopted
                && proposal.request().candidate_state.id == *id
        })
        .collect::<Vec<_>>();
    let [proposal] = adopted.as_slice() else {
        return Err("Assembly placement requires exactly one adopted update".into());
    };
    let report = updates
        .review(&proposal.request().id)
        .map_err(|error| error.to_string())?;
    if report.status() != CheckStatus::Passed {
        return Err("fresh Assembly review did not pass; placement is refused".into());
    }
    let record = updates
        .catalog()
        .assembly(id)
        .ok_or("adopted Assembly Revision is missing from the catalog")?;
    let mut ancestor = record;
    loop {
        if groundings.contains_key(&ancestor.id) {
            break;
        }
        let [parent] = ancestor.parents.as_slice() else {
            return Err("adopted Assembly has no unique captured Circuit Revision ancestor".into());
        };
        ancestor = updates
            .catalog()
            .assembly(parent)
            .ok_or("adopted Assembly ancestry is incomplete")?;
    }
    let grounding = groundings
        .get(&ancestor.id)
        .ok_or("captured Assembly grounding is missing")?;
    Ok(GroundedSource {
        record: record.clone(),
        adopted_by: proposal.request().id.clone(),
        grounding_assembly_revision_id: ancestor.id.clone(),
        grounding: grounding.clone(),
        fresh_review: review_response(&report, updates.catalog()),
    })
}

fn construction_basis(
    updates: &BlueprintUpdates,
    id: &AssemblyRevisionId,
) -> Result<crate::source_identity::SourceIdentity, String> {
    let adopted = updates
        .proposals()
        .filter(|p| p.status() == UpdateStatus::Adopted && p.request().candidate_state.id == *id)
        .collect::<Vec<_>>();
    let [proposal] = adopted.as_slice() else {
        return Err("Assembly construction requires exactly one adopted update".into());
    };
    let context = proposal
        .request()
        .behavior_context
        .as_ref()
        .ok_or("construction requires an explicit runtime context")?;
    if !matches!(context, BehaviorReviewContext::Runtime(_)) {
        return Err("construction requires adoption under the expanded electrical context".into());
    }
    let report = updates
        .review(&proposal.request().id)
        .map_err(|e| e.to_string())?;
    if report.status() != CheckStatus::Passed {
        return Err("fresh adopted Assembly review did not pass".into());
    }
    let BehaviorReviewContext::Runtime(context) = context else {
        unreachable!("context checked above")
    };
    Ok(crate::source_identity::SourceIdentity {
        record: updates
            .catalog()
            .assembly(id)
            .ok_or("adopted Assembly is missing")?
            .clone(),
        adopted_by: proposal.request().id.clone(),
        context: context.clone(),
        catalog: updates.catalog().clone(),
    })
}

fn read(
    updates: &BlueprintUpdates,
    groundings: &BTreeMap<AssemblyRevisionId, AssemblyGrounding>,
    query: BlueprintRead,
) -> Result<Response, String> {
    let catalog = updates.catalog();
    let result = match query {
        BlueprintRead::Catalog {
            classification_id,
            offset,
            limit,
        } => {
            let limit = limit.unwrap_or(100);
            if !(1..=256).contains(&limit) {
                return Err("limit must be 1 through 256".into());
            }
            if let Some(id) = &classification_id
                && catalog.classification(id).is_none()
            {
                return Err("unknown classification_id".into());
            }
            let revisions = catalog
                .revisions()
                .filter(|r| {
                    classification_id
                        .as_ref()
                        .is_none_or(|id| r.classifications.contains(id))
                })
                .collect::<Vec<_>>();
            let offset = offset.unwrap_or(0);
            ReadRecord::Catalog(Box::new(CatalogResponse {
                blueprints: revisions
                    .iter()
                    .skip(offset)
                    .take(limit)
                    .map(|r| BlueprintSummary {
                        id: r.id.clone(),
                        name: r.name.clone(),
                        classifications: r.classifications.clone(),
                    })
                    .collect(),
                total_blueprints: revisions.len(),
                next_offset: (offset.saturating_add(limit) < revisions.len())
                    .then_some(offset.saturating_add(limit)),
                classifications: catalog
                    .classifications()
                    .map(|r| DefinitionSummary {
                        id: r.id.clone(),
                        name: r.name.clone(),
                    })
                    .collect(),
                types: catalog
                    .type_revisions()
                    .map(|r| DefinitionSummary {
                        id: r.id.clone(),
                        name: r.name.clone(),
                    })
                    .collect(),
                assemblies: catalog.assemblies().map(|r| r.id.clone()).collect(),
                operations: updates
                    .proposals()
                    .map(|p| ProposalSummary {
                        operation_id: p.request().id.clone(),
                        status: p.status(),
                        title: p.request().title.clone(),
                    })
                    .collect(),
            }))
        }
        BlueprintRead::Blueprint { id } => ReadRecord::Blueprint {
            record: Box::new(
                catalog
                    .revision(&id)
                    .ok_or("unknown Blueprint Revision ID")?
                    .clone(),
            ),
        },
        BlueprintRead::Assembly {
            id,
            validate,
            behavior_context,
        } => {
            let behavior_context = behavior_context.map(RequestedBehaviorContext::resolve);
            let record = catalog
                .assembly(&id)
                .ok_or("unknown Assembly Revision ID")?;
            let review = if validate.unwrap_or(false) {
                let report = review_assembly_with_context(
                    catalog,
                    &record.assembly,
                    behavior_context.as_ref(),
                    BehaviorBudget::default(),
                )
                .map_err(|e| e.to_string())?;
                Some(AssemblyReview {
                    validation: review_response(&report, catalog),
                    world_execution_context: behavior_context
                        .as_ref()
                        .map(|c| c.execution_context()),
                    validation_context: behavior_context,
                })
            } else {
                None
            };
            ReadRecord::Assembly(Box::new(AssemblyResponse {
                record: record.clone(),
                lifecycle: assembly_lifecycle(updates, groundings, &id),
                review,
            }))
        }
        BlueprintRead::Type { id } => ReadRecord::Type {
            record: Box::new(
                catalog
                    .type_revision(&id)
                    .ok_or("unknown Type Revision ID")?
                    .clone(),
            ),
        },
        BlueprintRead::Classification { id } => ReadRecord::Classification {
            record: Box::new(
                catalog
                    .classification(&id)
                    .ok_or("unknown Classification Revision ID")?
                    .clone(),
            ),
        },
        BlueprintRead::Archive => ReadRecord::Archive {
            archive: Box::new(updates.archive()),
        },
    };
    Ok(Response::success(Body::Read {
        result,
        catalog_membership_is_validation_proof: false,
    }))
}

fn import(updates: &mut BlueprintUpdates, records: BlueprintRecords) -> Result<(), String> {
    for r in records.types {
        if updates.catalog().type_revision(&r.id) != Some(&r) {
            updates.append_type(r).map_err(|e| e.to_string())?;
        }
    }
    for r in records.classifications {
        if updates.catalog().classification(&r.id) != Some(&r) {
            updates
                .append_classification(r)
                .map_err(|e| e.to_string())?;
        }
    }
    let mut revisions = records.revisions;
    while !revisions.is_empty() {
        let previous = revisions.len();
        let mut pending = vec![];
        for r in revisions {
            if updates.catalog().revision(&r.id) == Some(&r) {
                continue;
            }
            match updates.append_revision(r.clone()) {
                Ok(()) => {}
                Err(BlueprintUpdateError::Blueprint(BlueprintError::UnknownRevision(_))) => {
                    pending.push(r)
                }
                Err(e) => return Err(e.to_string()),
            }
        }
        if pending.len() == previous {
            return Err(
                "unresolved revision dependencies or cycle; supply missing parents/children".into(),
            );
        }
        revisions = pending;
    }
    let mut states = records.assemblies;
    while !states.is_empty() {
        let previous = states.len();
        let mut pending = vec![];
        for r in states {
            if updates.catalog().assembly(&r.id) == Some(&r) {
                continue;
            }
            match updates.append_state(r.clone()) {
                Ok(()) => {}
                Err(BlueprintUpdateError::Blueprint(BlueprintError::UnknownAssembly(_))) => {
                    pending.push(r)
                }
                Err(e) => return Err(e.to_string()),
            }
        }
        if pending.len() == previous {
            return Err("unresolved Assembly ancestry; import the parent records first".into());
        }
        states = pending;
    }
    Ok(())
}

const BUILDING_NEXT_STEP: &str = "Import result.records; propose_update with result.request after removing its id; review and adopt; then new_placement with assembly_target. After interrupted placement, diagnose and create a fresh reconstruction plan.";
const BUILDING_UPDATE_NEXT_STEP: &str = "Import result.records; propose_update with result.request after removing its id; inspect diff, review and adopt. Existing placed instances retain their pinned source; a separate site-edit operation is required to modify one.";

#[derive(Clone, Copy, Eq, PartialEq)]
enum CatalogPersistence {
    NotRequired,
    Required,
}

/// A catalog effect is independent of the diagnostic response's success flag.
/// A refused adoption can require saving its review history.
struct CatalogActionResult<T> {
    value: T,
    persistence: CatalogPersistence,
}
impl<T> CatalogActionResult<T> {
    fn read_only(value: T) -> Self {
        Self {
            value,
            persistence: CatalogPersistence::NotRequired,
        }
    }
    fn requires_save(value: T) -> Self {
        Self {
            value,
            persistence: CatalogPersistence::Required,
        }
    }
}

fn perform(
    updates: &mut BlueprintUpdates,
    groundings: &mut BTreeMap<AssemblyRevisionId, AssemblyGrounding>,
    command: Command,
) -> Result<CatalogActionResult<Option<Response>>, String> {
    match command {
        Command::Read(query) => Ok(CatalogActionResult::read_only(Some(read(
            updates, groundings, query,
        )?))),
        Command::Capture { record, grounding } => {
            let id = record.id.clone();
            if grounding.assembly_revision_id != id || !grounding.complete {
                return Err(
                    "capture grounding must identify the same complete Assembly Revision".into(),
                );
            }
            match groundings.get(&id) {
                Some(existing) if existing != &grounding => {
                    return Err("Assembly grounding is immutable".into());
                }
                Some(_) => {}
                None => {
                    groundings.insert(id.clone(), grounding);
                }
            }
            import(
                updates,
                BlueprintRecords {
                    assemblies: vec![*record],
                    ..Default::default()
                },
            )?;
            Ok(CatalogActionResult::requires_save(Some(Response::success(
                Body::Captured {
                    assembly_revision_id: id,
                    validation: "not_evaluated",
                },
            ))))
        }
        Command::Write(write) => {
            let write = write.into_checked(MAX_REQUEST_BYTES)?;
            perform_write(updates, write)
        }
        Command::Show(uuid) => perform_proposal(updates, uuid, ProposalAction::Show),
        Command::Get(uuid) => perform_proposal(updates, uuid, ProposalAction::Get),
        Command::Decide(uuid, decision, confirm) => {
            perform_proposal(updates, uuid, ProposalAction::Decide { decision, confirm })
        }
        Command::Undo(uuid) => perform_proposal(updates, uuid, ProposalAction::Undo),
    }
}

/// Generated candidates remain unpublished; explicit catalog commands save.
fn perform_write(
    updates: &mut BlueprintUpdates,
    write: BlueprintWrite,
) -> Result<CatalogActionResult<Option<Response>>, String> {
    match write {
        BlueprintWrite::GenerateGroundedBuildingDesign { request } => {
            let generated =
                dustroute_translate::building::generate_grounded_building_design(*request);
            Ok(CatalogActionResult::read_only(Some(Response::building(
                generated.map(|r| Generated::Grounded(Box::new(r))),
                BUILDING_NEXT_STEP,
            ))))
        }
        BlueprintWrite::GenerateBuildingDesignUpdate { request } => {
            let generated = (|| {
                let base = construction_basis(updates, &request.base_assembly_revision_id)?;
                let previous = request
                    .previous
                    .component
                    .as_ref()
                    .map(|c| construction_basis(updates, &c.assembly_revision_id))
                    .transpose()?;
                let next = request
                    .design
                    .component
                    .as_ref()
                    .map(|c| construction_basis(updates, &c.assembly_revision_id))
                    .transpose()?;
                dustroute_translate::building::generate_building_design_update(
                    *request,
                    &base.catalog,
                    previous.as_ref().map(|s| &s.context),
                    next.as_ref().map(|s| &s.context),
                )
            })();
            Ok(CatalogActionResult::read_only(Some(Response::building(
                generated.map(|r| Generated::DesignUpdate(Box::new(r))),
                BUILDING_UPDATE_NEXT_STEP,
            ))))
        }
        BlueprintWrite::GenerateBuildingDesign { request } => {
            let source = request
                .component
                .as_ref()
                .map(|c| construction_basis(updates, &c.assembly_revision_id))
                .transpose();
            let generated = match source {
                Ok(source) => dustroute_translate::building::generate_building_design(
                    *request,
                    source.as_ref().map(|s| (&s.catalog, &s.context)),
                ),
                Err(detail) => {
                    return Ok(CatalogActionResult::read_only(Some(Response::building(
                        Err(dustroute_translate::building::BuildingDesignError {
                            code: "source_not_adopted_or_reviewable",
                            detail,
                            item: request.component.as_ref().map(|c| c.name.clone()),
                            position: None,
                            diagnostics: None,
                        }),
                        BUILDING_NEXT_STEP,
                    ))));
                }
            };
            Ok(CatalogActionResult::read_only(Some(Response::building(
                generated.map(|r| Generated::Design(Box::new(r))),
                BUILDING_NEXT_STEP,
            ))))
        }
        BlueprintWrite::GenerateBuilding { request } => {
            let generated = dustroute_translate::building::generate_building(*request);
            Ok(CatalogActionResult::read_only(Some(Response::building(
                generated.map(|r| Generated::Building(Box::new(r))),
                BUILDING_NEXT_STEP,
            ))))
        }
        BlueprintWrite::GenerateBuildingWithDoor { request } => {
            let source = construction_basis(updates, &request.door.assembly_revision_id)?;
            let generated = dustroute_translate::building::generate_building_with_door(
                *request,
                &source.catalog,
                &source.context,
            );
            Ok(CatalogActionResult::read_only(Some(Response::building(
                generated.map(|r| Generated::Building(Box::new(r))),
                BUILDING_NEXT_STEP,
            ))))
        }
        BlueprintWrite::GenerateFlyingMachine { request } => {
            let generated = dustroute_translate::flying_machine::generate_flying_machine(
                *request,
                BehaviorBudget::default(),
            )?;
            let passed = generated.verification.status == CheckStatus::Passed;
            Ok(CatalogActionResult::read_only(Some(Response::generated(
                Generated::FlyingMachine(Box::new(generated)),
                passed,
                if passed {
                    "Import result.records; propose_update with result.request after removing its id; review and adopt explicitly; then plan placement at the target."
                } else {
                    "Inspect result.verification; this candidate did not establish a usable generated flight."
                },
            ))))
        }
        BlueprintWrite::EnumerateLayouts { request, budget } => {
            let budget = budget.unwrap_or_default();
            if budget.max_layouts > 4096 || budget.max_bindings > 256 || budget.max_millis > 30_000
            {
                return Err("Blueprint enumeration limits are 4096 layouts, 256 bindings and 30000 milliseconds".into());
            }
            let report = dustroute_optimize::blueprint_reduction::enumerate_torch_supports(
                updates.catalog(),
                &request,
                budget,
            )?;
            Ok(CatalogActionResult::read_only(Some(Response::generated(
                Generated::Enumeration(Box::new(report)),
                true,
                "Review each candidate status; only passing candidates satisfy the selected type in this exact context. Use an explicit update proposal and fresh adoption checks.",
            ))))
        }
        BlueprintWrite::Optimize { request, budget } => {
            let budget = budget.unwrap_or_default();
            if budget.max_layouts > 4096 || budget.max_bindings > 256 || budget.max_millis > 30_000
            {
                return Err("Blueprint optimization limits are 4096 layouts, 256 bindings and 30000 milliseconds".into());
            }
            let report = dustroute_optimize::blueprint_reduction::reduce_blueprint_blocks(
                updates.catalog(),
                &request,
                budget,
            )?;
            Ok(CatalogActionResult::read_only(Some(Response::generated(
                Generated::Reduction(Box::new(report)),
                true,
                "Use a passing smaller candidate in an explicit parent update proposal, retain its target binding and execution context, then review and explicitly adopt. Search reports never authorize adoption.",
            ))))
        }
        BlueprintWrite::Import { records } => {
            import(updates, records)?;
            Ok(CatalogActionResult::requires_save(Some(Response::success(
                Body::Imported {
                    validation: "not_evaluated",
                    next_step: "read the catalog or create an explicit update proposal",
                },
            ))))
        }
        BlueprintWrite::ProposeUpdate { request } => {
            let id =
                BlueprintUpdateId::new(uuid::Uuid::new_v4().to_string()).map_err(str::to_owned)?;
            updates
                .create(request.into_request(id.clone()))
                .map_err(|e| e.to_string())?;
            Ok(CatalogActionResult::requires_save(Some(
                operation_response(operation(updates, &id, false)?),
            )))
        }
        BlueprintWrite::CaptureRevision { .. } => {
            Err("capture must resolve a saved circuit revision first".into())
        }
    }
}

/// Proposal handling receives only proposal actions, without a second copy of
/// the ID or impossible read/write/capture command variants.
enum ProposalAction {
    Show,
    Get,
    Decide {
        decision: Option<BlueprintDecision>,
        confirm: bool,
    },
    Undo,
}

/// An open review or a refused adoption can still need to save review history.
fn perform_proposal(
    updates: &mut BlueprintUpdates,
    uuid: uuid::Uuid,
    action: ProposalAction,
) -> Result<CatalogActionResult<Option<Response>>, String> {
    let id = BlueprintUpdateId::new(uuid.to_string()).map_err(str::to_owned)?;
    if updates.proposal(&id).is_none() {
        return Ok(CatalogActionResult::read_only(None));
    }
    match action {
        ProposalAction::Get => Ok(CatalogActionResult::read_only(Some(operation_response(
            operation(updates, &id, false)?,
        )))),
        ProposalAction::Show => {
            let open = updates.proposal(&id).unwrap().status() == UpdateStatus::Open;
            let report = if open {
                updates.validate(&id)
            } else {
                updates.review(&id)
            }.map_err(|e| e.to_string())?;
            let mut result = operation(updates, &id, true)?;
            result.can_adopt = Some(open && report.status() == CheckStatus::Passed);
            result.validation = Some(review_response(&report, updates.catalog()));
            Ok(CatalogActionResult {
                value: Some(operation_response(result)),
                persistence: if open {
                    CatalogPersistence::Required
                } else {
                    CatalogPersistence::NotRequired
                },
            })
        }
        ProposalAction::Decide { decision, confirm } => {
            if !confirm {
                return Err("confirm=true is required for a Blueprint decision; no Minecraft change is involved".into());
            }
            let Some(decision) = decision else {
                return Err("provide blueprint_decision: {action: adopt} or {action: reject, reason: ...}".into());
            };
            let result = match decision {
                BlueprintDecision::Adopt => updates.adopt(&id),
                BlueprintDecision::Reject { reason } => updates.reject(&id, reason),
            };
            match result {
                Ok(()) => Ok(CatalogActionResult::requires_save(Some(operation_response(
                    operation(updates, &id, false)?,
                )))),
                Err(BlueprintUpdateError::Validation(report)) => {
                    let mut result = operation(updates, &id, true)?;
                    result.error_code = Some(crate::api::McpErrorCode::VerificationFailed);
                    result.error = Some("adoption refused; required checks failed or are undetermined");
                    result.validation = Some(review_response(&report, updates.catalog()));
                    let mut response = operation_response(result);
                    response.ok = false;
                    Ok(CatalogActionResult::requires_save(Some(response)))
                }
                Err(e) => Err(e.to_string()),
            }
        }
        ProposalAction::Undo => Err("Blueprint decisions cannot be undone by changing history; retain the old revision or create a new proposal".into()),
    }
}

/// Lock covers load, validation and atomic save across MCP instances. No cached
/// fallback can overwrite a newer decision or resurrect an outdated status.
pub(crate) fn execute(
    store: &PlanStateStore,
    player: &str,
    command: Command,
) -> Result<Option<Response>, String> {
    transaction(store, player, |updates, groundings| {
        perform(updates, groundings, command)
    })
}

pub(crate) fn grounded_source(
    store: &PlanStateStore,
    player: &str,
    id: &AssemblyRevisionId,
) -> Result<GroundedSource, String> {
    transaction(store, player, |updates, groundings| {
        Ok(CatalogActionResult::read_only(placement_basis(
            updates, groundings, id,
        )?))
    })
}

/// Read a freshly reviewed construction source without serializing through a tool response.
pub(crate) fn construction_source(
    store: &PlanStateStore,
    player: &str,
    id: &AssemblyRevisionId,
) -> Result<crate::source_identity::SourceIdentity, String> {
    transaction(store, player, |updates, _| {
        Ok(CatalogActionResult::read_only(construction_basis(
            updates, id,
        )?))
    })
}

/// Owns the catalog lock, load/validate/save contract for every application entry.
fn transaction<T>(
    store: &PlanStateStore,
    player: &str,
    action: impl FnOnce(
        &mut BlueprintUpdates,
        &mut BTreeMap<AssemblyRevisionId, AssemblyGrounding>,
    ) -> Result<CatalogActionResult<T>, String>,
) -> Result<T, String> {
    let root = store.blueprint_root(player);
    fs::create_dir_all(&root).map_err(|e| e.to_string())?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(&root, fs::Permissions::from_mode(0o700)).map_err(|e| e.to_string())?;
    }
    let mut options = OpenOptions::new();
    options.read(true).write(true).create(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    let lock = options
        .open(root.join("catalog.lock"))
        .map_err(|e| e.to_string())?;
    fs2::FileExt::try_lock_exclusive(&lock)
        .map_err(|_| "Blueprint catalog is busy; retry the same request".to_owned())?;
    let path = root.join("catalog.store");
    let (mut updates, mut groundings) = match fs::File::open(&path) {
        Ok(file) => {
            let mut bytes = Vec::new();
            file.take(MAX_ARCHIVE_BYTES + 1)
                .read_to_end(&mut bytes)
                .map_err(|e| e.to_string())?;
            if bytes.len() as u64 > MAX_ARCHIVE_BYTES {
                return Err("stored Blueprint archive exceeds 16 MiB".into());
            }
            let saved: StoredBlueprints =
                dustroute_codec::storage::decode(STORE_SCHEMA, &bytes, MAX_ARCHIVE_BYTES as usize)
                    .map_err(|e| format!("invalid current MCP Blueprint store: {e}"))?;
            if saved.schema != STORE_SCHEMA {
                return Err("retired MCP Blueprint store; preserve it separately and recreate/review with v3".into());
            }
            if saved.owner != player {
                return Err("Blueprint archive owner mismatch".into());
            }
            let updates =
                BlueprintUpdates::from_archive(saved.archive).map_err(|e| e.to_string())?;
            (updates, saved.groundings)
        }
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
            if root
                .join("catalog.json")
                .try_exists()
                .map_err(|e| e.to_string())?
            {
                return Err("retired JSON Blueprint store; preserve it separately and recreate/review with v3".into());
            }
            (
                BlueprintUpdates::new(
                    dustroute_library::builtin_blueprints::builtin_blueprints().clone(),
                ),
                BTreeMap::new(),
            )
        }
        Err(e) => return Err(e.to_string()),
    };
    let CatalogActionResult { value, persistence } = action(&mut updates, &mut groundings)?;
    if persistence == CatalogPersistence::Required {
        let bytes = dustroute_codec::storage::encode(
            STORE_SCHEMA,
            &StoredBlueprints {
                schema: STORE_SCHEMA.into(),
                owner: player.into(),
                archive: updates.archive(),
                groundings,
            },
            MAX_ARCHIVE_BYTES as usize,
        )
        .map_err(|e| e.to_string())?;
        if bytes.len() as u64 > MAX_ARCHIVE_BYTES {
            return Err("Blueprint catalog exceeds 16 MiB; no changes saved".into());
        }
        crate::storage::replace(&path, &bytes, crate::storage::Durability::FileAndDirectory)
            .map_err(|e| e.to_string())?;
    }
    Ok(value)
}

// Corruption tests edit the MCP-shaped fixture, then encode a typed archive.
// This helper is never part of a production storage or adoption path.
#[cfg(test)]
pub(crate) fn read_store_fixture(bytes: &[u8]) -> serde_json::Value {
    let record: StoredBlueprints =
        dustroute_codec::storage::decode(STORE_SCHEMA, bytes, MAX_ARCHIVE_BYTES as usize).unwrap();
    serde_json::to_value(record).unwrap()
}
#[cfg(test)]
pub(crate) fn encode_store_fixture(value: serde_json::Value) -> Vec<u8> {
    if value.get("groundings").is_none() {
        #[derive(Deserialize, Serialize)]
        struct MissingGroundings {
            schema: String,
            owner: String,
            archive: BlueprintUpdateArchive,
        }
        let record: MissingGroundings = serde_json::from_value(value).unwrap();
        return dustroute_codec::storage::encode(STORE_SCHEMA, &record, MAX_ARCHIVE_BYTES as usize)
            .unwrap();
    }
    let record: StoredBlueprints = serde_json::from_value(value).unwrap();
    dustroute_codec::storage::encode(STORE_SCHEMA, &record, MAX_ARCHIVE_BYTES as usize).unwrap()
}
