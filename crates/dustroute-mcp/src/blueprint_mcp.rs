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
use serde_json::{Value, json};
use std::collections::BTreeMap;

use crate::state::PlanStateStore;

const MAX_ARCHIVE_BYTES: u64 = 16 * 1024 * 1024;
const MAX_REQUEST_BYTES: usize = 4 * 1024 * 1024;
const STORE_SCHEMA: &str = "dustroute.mcp-blueprints.v2";

/// Current stores always carry the grounding map, including when it is empty.
/// An absent map is a retired representation, never invented placement evidence.
#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct StoredBlueprints {
    schema: String,
    owner: String,
    archive: Value,
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
    Write(BlueprintWrite),
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

pub(crate) fn failure(message: impl ToString) -> Value {
    json!({"ok":false,"schema_version":"dustroute.blueprint-mcp.v1","error_code":"invalid_state","error":message.to_string(),"writes_minecraft":false})
}

pub(crate) fn report_json(report: &PromotionReport, catalog: &BlueprintCatalog) -> Value {
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
    json!({"status":report.status(),"fresh":true,"scope":scope,
        "diagnostics":report.diagnostics(64),
        "placement_validation_profile":report.placement_validation_profile(),
        "initial_state_checks_passed":report.status()==CheckStatus::Passed,
        "checks":RecordedReview::from(report),
        "behavior_verified":false,
        "declared_behavior_verified":report.status()==CheckStatus::Passed && report.behavior_status()==Some(CheckStatus::Passed),
        "behavior_scope":"declared_obligations_in_selected_model_only",
        "behavior_status":report.behavior_status(),"live_world_verified":false})
}

fn operation(
    updates: &BlueprintUpdates,
    id: &BlueprintUpdateId,
    details: bool,
) -> Result<Value, String> {
    let proposal = updates
        .proposal(id)
        .ok_or("unknown blueprint update operation")?;
    let mut result = json!({"ok":true,"schema_version":"dustroute.blueprint-mcp.v1","operation_id":id,
    "operation":{"id":id,"kind":"blueprint_update","status":proposal.status(),"request":proposal.request(),
        "history":proposal.events()},"writes_minecraft":false,"stored_history_is_validation_proof":false,
    "next_step":if proposal.status()==UpdateStatus::Open {
        "show_operation for a fresh review; invoke_operation with confirm=true and an explicit blueprint_decision"
    } else {
        "retain this decision and its exact revisions; a further change requires a new proposal"
    }});
    if details {
        result["diff"] = serde_json::to_value(updates.diff(id).map_err(|e| e.to_string())?)
            .map_err(|e| e.to_string())?;
    }
    Ok(result)
}

fn assembly_lifecycle(
    updates: &BlueprintUpdates,
    groundings: &BTreeMap<AssemblyRevisionId, AssemblyGrounding>,
    id: &AssemblyRevisionId,
) -> Value {
    let adopted_by = updates
        .proposals()
        .filter(|proposal| {
            proposal.status() == UpdateStatus::Adopted
                && proposal.request().candidate_state.id == *id
        })
        .map(|proposal| &proposal.request().id)
        .collect::<Vec<_>>();
    if adopted_by.is_empty() {
        json!({
            "status":"catalog_only_unadopted",
            "adopted":false,
            "placement_eligible":false,
            "reason":"catalog import or capture is immutable data, not an adopted update"
        })
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
            && updates.proposal(adopted_by[0]).is_some_and(|proposal| {
                matches!(
                    proposal.request().behavior_context.as_ref(),
                    Some(BehaviorReviewContext::Runtime(_))
                )
            });
        json!({
            "status":"adopted",
            "adopted":true,
            "placement_eligible":grounded || construction,
            "grounded_placement_eligible":grounded,
            "new_target_construction_eligible":construction,
            "adopted_by":adopted_by,
            "reason":if construction {
                "new-target construction may be requested; fresh target review, complete empty-region observation and server settings are still required"
            } else if grounded {
                "placement may be requested; fresh model and live-world checks are still mandatory"
            } else {
                "adoption is established, but no captured literal observation grounds this Assembly ancestry"
            }
        })
    }
}

#[derive(Debug)]
pub(crate) struct GroundedSource {
    pub record: AssemblyRevision,
    pub adopted_by: BlueprintUpdateId,
    pub grounding_assembly_revision_id: AssemblyRevisionId,
    pub grounding: AssemblyGrounding,
    /// Display report only; admission already checked the typed PromotionReport.
    pub fresh_review: Value,
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
        fresh_review: report_json(&report, updates.catalog()),
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
) -> Result<Value, String> {
    let catalog = updates.catalog();
    let record = match query {
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
            json!({"kind":"catalog","blueprints":revisions.iter().skip(offset).take(limit).map(|r|json!({"id":r.id,"name":r.name,"classifications":r.classifications})).collect::<Vec<_>>(),
                "total_blueprints":revisions.len(),"next_offset":(offset.saturating_add(limit)<revisions.len()).then_some(offset.saturating_add(limit)),
                "classifications":catalog.classifications().map(|r|json!({"id":r.id,"name":r.name})).collect::<Vec<_>>(),
                "types":catalog.type_revisions().map(|r|json!({"id":r.id,"name":r.name})).collect::<Vec<_>>(),
                "assemblies":catalog.assemblies().map(|r|&r.id).collect::<Vec<_>>(),
                "operations":updates.proposals().map(|p|json!({"operation_id":p.request().id,"status":p.status(),"title":p.request().title})).collect::<Vec<_>>()})
        }
        BlueprintRead::Blueprint { id } => {
            json!({"kind":"blueprint","record":catalog.revision(&id).ok_or("unknown Blueprint Revision ID")?})
        }
        BlueprintRead::Assembly {
            id,
            validate,
            behavior_context,
        } => {
            let behavior_context = behavior_context.map(RequestedBehaviorContext::resolve);
            let record = catalog
                .assembly(&id)
                .ok_or("unknown Assembly Revision ID")?;
            let mut result = json!({"kind":"assembly","record":record,
                "lifecycle":assembly_lifecycle(updates, groundings, &id)});
            if validate.unwrap_or(false) {
                result["validation"] = report_json(
                    &review_assembly_with_context(
                        catalog,
                        &record.assembly,
                        behavior_context.as_ref(),
                        BehaviorBudget::default(),
                    )
                    .map_err(|e| e.to_string())?,
                    catalog,
                );
                result["validation_context"] = json!(behavior_context);
                result["world_execution_context"] = json!(
                    behavior_context
                        .as_ref()
                        .map(|context| context.execution_context())
                );
            }
            result
        }
        BlueprintRead::Type { id } => {
            json!({"kind":"type","record":catalog.type_revision(&id).ok_or("unknown Type Revision ID")?})
        }
        BlueprintRead::Classification { id } => {
            json!({"kind":"classification","record":catalog.classification(&id).ok_or("unknown Classification Revision ID")?})
        }
        BlueprintRead::Archive => {
            json!({"kind":"archive","archive":serde_json::from_str::<Value>(&updates.to_json().map_err(|e|e.to_string())?).map_err(|e|e.to_string())?})
        }
    };
    Ok(
        json!({"ok":true,"schema_version":"dustroute.blueprint-mcp.v1","result":record,"writes_minecraft":false,"catalog_membership_is_validation_proof":false}),
    )
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

fn generated_building_json(generated: impl Serialize) -> Value {
    json!({"ok":true,"schema_version":"dustroute.blueprint-mcp.v1",
        "result":generated,"writes_minecraft":false,"catalog_changed":false,
        "adoption_authorized":false,
        "next_step":"Import result.records; propose_update with result.request after removing its id; review and adopt; then new_placement with assembly_target. After interrupted placement, diagnose and create a fresh reconstruction plan."})
}

fn building_result_json<T: Serialize>(
    result: Result<T, dustroute_translate::building::BuildingDesignError>,
) -> Value {
    match result {
        Ok(generated) => generated_building_json(generated),
        Err(error) => json!({"ok":false,"schema_version":"dustroute.blueprint-mcp.v1",
            "errors":[error],"writes_minecraft":false,"catalog_changed":false,"adoption_authorized":false}),
    }
}

fn perform(
    updates: &mut BlueprintUpdates,
    groundings: &mut BTreeMap<AssemblyRevisionId, AssemblyGrounding>,
    command: Command,
) -> Result<(Option<Value>, bool), String> {
    match command {
        Command::Read(query) => Ok((Some(read(updates, groundings, query)?), false)),
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
            Ok((
                Some(
                    json!({"ok":true,"schema_version":"dustroute.blueprint-mcp.v1","assembly_revision_id":id,"writes_minecraft":false,"validation":"not_evaluated"}),
                ),
                true,
            ))
        }
        Command::Write(write) => {
            if serde_json::to_vec(&write).map_err(|e| e.to_string())?.len() > MAX_REQUEST_BYTES {
                return Err("Blueprint request exceeds 4 MiB".into());
            }
            match write {
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
                    let mut response = building_result_json(generated);
                    if response["ok"] == true {
                        response["next_step"] = json!(
                            "Import result.records; propose_update with result.request after removing its id; inspect diff, review and adopt. Existing placed instances retain their pinned source; a separate site-edit operation is required to modify one."
                        );
                    }
                    Ok((Some(response), false))
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
                            return Ok((
                                Some(json!({"ok":false,
                            "schema_version":"dustroute.blueprint-mcp.v1", "errors":[{
                                "code":"source_not_adopted_or_reviewable", "detail":detail,
                                "item":request.component.as_ref().map(|c| &c.name)}],
                            "writes_minecraft":false,"catalog_changed":false,"adoption_authorized":false})),
                                false,
                            ));
                        }
                    };
                    Ok((Some(building_result_json(generated)), false))
                }
                BlueprintWrite::GenerateBuilding { request } => {
                    let generated = dustroute_translate::building::generate_building(*request);
                    Ok((Some(building_result_json(generated)), false))
                }
                BlueprintWrite::GenerateBuildingWithDoor { request } => {
                    let source = construction_basis(updates, &request.door.assembly_revision_id)?;
                    let generated = dustroute_translate::building::generate_building_with_door(
                        *request,
                        &source.catalog,
                        &source.context,
                    );
                    Ok((Some(building_result_json(generated)), false))
                }
                BlueprintWrite::GenerateFlyingMachine { request } => {
                    let generated = dustroute_translate::flying_machine::generate_flying_machine(
                        *request,
                        BehaviorBudget::default(),
                    )?;
                    let passed = generated.verification.status == CheckStatus::Passed;
                    Ok((
                        Some(
                            json!({"ok":passed,"schema_version":"dustroute.blueprint-mcp.v1",
                            "result":generated,"writes_minecraft":false,"catalog_changed":false,
                            "adoption_authorized":false,
                            "next_step":if passed {
                                "Import result.records; propose_update with result.request after removing its id; review and adopt explicitly; then plan placement at the target."
                            } else {
                                "Inspect result.verification; this candidate did not establish a usable generated flight."
                            }}),
                        ),
                        false,
                    ))
                }
                BlueprintWrite::EnumerateLayouts { request, budget } => {
                    let budget = budget.unwrap_or_default();
                    if budget.max_layouts > 4096
                        || budget.max_bindings > 256
                        || budget.max_millis > 30_000
                    {
                        return Err("Blueprint enumeration limits are 4096 layouts, 256 bindings and 30000 milliseconds".into());
                    }
                    let report = dustroute_optimize::blueprint_reduction::enumerate_torch_supports(
                        updates.catalog(),
                        &request,
                        budget,
                    )?;
                    Ok((
                        Some(
                            json!({"ok":true,"schema_version":"dustroute.blueprint-mcp.v1","result":report,
                        "writes_minecraft":false,"catalog_changed":false,"adoption_authorized":false,
                        "next_step":"Review each candidate status; only passing candidates satisfy the selected type in this exact context. Use an explicit update proposal and fresh adoption checks."}),
                        ),
                        false,
                    ))
                }
                BlueprintWrite::Optimize { request, budget } => {
                    let budget = budget.unwrap_or_default();
                    if budget.max_layouts > 4096
                        || budget.max_bindings > 256
                        || budget.max_millis > 30_000
                    {
                        return Err("Blueprint optimization limits are 4096 layouts, 256 bindings and 30000 milliseconds".into());
                    }
                    let report = dustroute_optimize::blueprint_reduction::reduce_blueprint_blocks(
                        updates.catalog(),
                        &request,
                        budget,
                    )?;
                    Ok((
                        Some(
                            json!({"ok":true,"schema_version":"dustroute.blueprint-mcp.v1","result":report,
                        "writes_minecraft":false,"catalog_changed":false,"adoption_authorized":false,
                        "next_step":"Use a passing smaller candidate in an explicit parent update proposal, retain its target binding and execution context, then review and explicitly adopt. Search reports never authorize adoption."}),
                        ),
                        false,
                    ))
                }
                BlueprintWrite::Import { records } => {
                    import(updates, records)?;
                    Ok((
                        Some(
                            json!({"ok":true,"schema_version":"dustroute.blueprint-mcp.v1","writes_minecraft":false,"validation":"not_evaluated","next_step":"read the catalog or create an explicit update proposal"}),
                        ),
                        true,
                    ))
                }
                BlueprintWrite::ProposeUpdate { request } => {
                    let id = BlueprintUpdateId::new(uuid::Uuid::new_v4().to_string())
                        .map_err(str::to_owned)?;
                    updates
                        .create(request.into_request(id.clone()))
                        .map_err(|e| e.to_string())?;
                    Ok((Some(operation(updates, &id, false)?), true))
                }
                BlueprintWrite::CaptureRevision { .. } => {
                    Err("capture must resolve a saved circuit revision first".into())
                }
            }
        }
        Command::Show(uuid)
        | Command::Get(uuid)
        | Command::Decide(uuid, _, _)
        | Command::Undo(uuid) => {
            let id = BlueprintUpdateId::new(uuid.to_string()).map_err(str::to_owned)?;
            if updates.proposal(&id).is_none() {
                return Ok((None, false));
            }
            match command {
                Command::Get(_) => Ok((Some(operation(updates, &id, false)?), false)),
                Command::Show(_) => {
                    let open = updates.proposal(&id).unwrap().status() == UpdateStatus::Open;
                    let report = if open {
                        updates.validate(&id)
                    } else {
                        updates.review(&id)
                    }.map_err(|e| e.to_string())?;
                    let mut result = operation(updates, &id, true)?;
                    result["can_adopt"] = json!(open && report.status() == CheckStatus::Passed);
                    result["validation"] = report_json(&report, updates.catalog());
                    Ok((Some(result), open))
                }
                Command::Decide(_, decision, confirm) => {
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
                        Ok(()) => Ok((Some(operation(updates, &id, false)?), true)),
                        Err(BlueprintUpdateError::Validation(report)) => {
                            let mut result = operation(updates, &id, true)?;
                            result["ok"] = json!(false);
                            result["error_code"] = json!("verification_failed");
                            result["error"] = json!("adoption refused; required checks failed or are undetermined");
                            result["validation"] = report_json(&report, updates.catalog());
                            Ok((Some(result), true))
                        }
                        Err(e) => Err(e.to_string()),
                    }
                }
                Command::Undo(_) => Err("Blueprint decisions cannot be undone by changing history; retain the old revision or create a new proposal".into()),
                _ => unreachable!(),
            }
        }
    }
}

/// Lock covers load, validation and atomic save across MCP instances. No cached
/// fallback can overwrite a newer decision or resurrect an outdated status.
pub(crate) fn execute(
    store: &PlanStateStore,
    player: &str,
    command: Command,
) -> Result<Option<Value>, String> {
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
        Ok((placement_basis(updates, groundings, id)?, false))
    })
}

/// Read a freshly reviewed construction source without serializing through a tool response.
pub(crate) fn construction_source(
    store: &PlanStateStore,
    player: &str,
    id: &AssemblyRevisionId,
) -> Result<crate::source_identity::SourceIdentity, String> {
    transaction(store, player, |updates, _| {
        Ok((construction_basis(updates, id)?, false))
    })
}

/// Owns the catalog lock, load/validate/save contract for every application entry.
fn transaction<T>(
    store: &PlanStateStore,
    player: &str,
    action: impl FnOnce(
        &mut BlueprintUpdates,
        &mut BTreeMap<AssemblyRevisionId, AssemblyGrounding>,
    ) -> Result<(T, bool), String>,
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
    let path = root.join("catalog.json");
    let (mut updates, mut groundings) = match fs::File::open(&path) {
        Ok(file) => {
            let mut bytes = Vec::new();
            file.take(MAX_ARCHIVE_BYTES + 1)
                .read_to_end(&mut bytes)
                .map_err(|e| e.to_string())?;
            if bytes.len() as u64 > MAX_ARCHIVE_BYTES {
                return Err("stored Blueprint archive exceeds 16 MiB".into());
            }
            let saved: Value = serde_json::from_slice(&bytes).map_err(|e| e.to_string())?;
            if saved["schema"] != STORE_SCHEMA {
                return Err("retired or unsupported MCP Blueprint store; preserve the archive separately and recreate/review with v2".into());
            }
            let saved: StoredBlueprints = serde_json::from_value(saved)
                .map_err(|e| format!("invalid current MCP Blueprint store: {e}"))?;
            if saved.owner != player {
                return Err("Blueprint archive owner mismatch".into());
            }
            let updates = BlueprintUpdates::from_json(&saved.archive.to_string())
                .map_err(|e| e.to_string())?;
            (updates, saved.groundings)
        }
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => (
            BlueprintUpdates::new(
                dustroute_library::builtin_blueprints::builtin_blueprints().clone(),
            ),
            BTreeMap::new(),
        ),
        Err(e) => return Err(e.to_string()),
    };
    let (result, write) = action(&mut updates, &mut groundings)?;
    if write {
        let archive: Value = serde_json::from_str(&updates.to_json().map_err(|e| e.to_string())?)
            .map_err(|e| e.to_string())?;
        let bytes = serde_json::to_vec(&StoredBlueprints {
            schema: STORE_SCHEMA.into(),
            owner: player.into(),
            archive,
            groundings,
        })
        .map_err(|e| e.to_string())?;
        if bytes.len() as u64 > MAX_ARCHIVE_BYTES {
            return Err("Blueprint catalog exceeds 16 MiB; no changes saved".into());
        }
        crate::storage::replace(&path, &bytes, crate::storage::Durability::FileAndDirectory)
            .map_err(|e| e.to_string())?;
    }
    Ok(result)
}
