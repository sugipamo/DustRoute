//! Explicit child-update proposals, reviews and decisions over immutable data.
//! Archives retain history, never a reusable validation capability. No branch
//! tracking, automatic placement, merging or live-world writes occur here.

use std::collections::{BTreeMap, BTreeSet};

use crate::behavior_type::BehaviorBudget;
use dustroute_library::assembly::{
    AssemblyBoundary, AssemblyConnection, AssemblyRevision, SourceStateDifference,
};
use dustroute_library::behavior_context::BehaviorReviewContext;
use dustroute_library::blueprint::{
    AssemblyRevisionId, BlueprintCatalog, BlueprintError, BlueprintOccurrence, BlueprintRevision,
    BlueprintRevisionId, BlueprintUpdateId, InstancePath,
};
use serde::{Deserialize, Serialize};

use crate::promotion::{
    CheckResult, CheckStatus, OccurrenceReview, PromotionReport, review_assembly_with_context,
};
use crate::{world::Block, world::Pos, world::Region};

/// All proposed edits are explicit. The paths to children are relative to the
/// selected parent occurrence; before/after paths may differ after decomposition.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct BlueprintUpdateRequest {
    pub id: BlueprintUpdateId,
    pub title: String,
    pub description: String,
    pub base_state: AssemblyRevisionId,
    pub base_parent: BlueprintRevisionId,
    pub candidate_parent: BlueprintRevisionId,
    pub parent_instance: InstancePath,
    pub child_before: InstancePath,
    pub previous_child: BlueprintRevisionId,
    pub child_after: InstancePath,
    pub next_child: BlueprintRevisionId,
    /// New parent and any changed intermediate definitions, in any order.
    pub revisions: Vec<BlueprintRevision>,
    pub candidate_state: AssemblyRevision,
    /// Explicit fresh-construction model assumptions, never a saved pass.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub behavior_context: Option<BehaviorReviewContext>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum UpdateStatus {
    Open,
    Adopted,
    Rejected,
}

/// Historical diagnostic data only. It cannot be converted to adoption proof.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct RecordedReview {
    pub occurrences: Vec<(InstancePath, OccurrenceReview)>,
    pub arrangement: Vec<CheckResult>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub behavior_context: Option<BehaviorReviewContext>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub behavior: Vec<CheckResult>,
}

impl From<&PromotionReport> for RecordedReview {
    fn from(report: &PromotionReport) -> Self {
        Self {
            occurrences: report
                .occurrences
                .iter()
                .map(|(path, review)| (path.clone(), review.clone()))
                .collect(),
            arrangement: report.arrangement.clone(),
            behavior_context: report.behavior_context.clone(),
            behavior: report.behavior.clone(),
        }
    }
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum UpdateEvent {
    Validated {
        report: RecordedReview,
    },
    Rejected {
        reason: String,
    },
    Adopted {
        parent: BlueprintRevisionId,
        state: AssemblyRevisionId,
    },
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct BlueprintUpdate {
    request: BlueprintUpdateRequest,
    events: Vec<UpdateEvent>,
}

impl BlueprintUpdate {
    #[must_use]
    pub fn request(&self) -> &BlueprintUpdateRequest {
        &self.request
    }
    #[must_use]
    pub fn events(&self) -> &[UpdateEvent] {
        &self.events
    }
    #[must_use]
    pub fn status(&self) -> UpdateStatus {
        match self.events.last() {
            Some(UpdateEvent::Adopted { .. }) => UpdateStatus::Adopted,
            Some(UpdateEvent::Rejected { .. }) => UpdateStatus::Rejected,
            _ => UpdateStatus::Open,
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct ValueChange<T> {
    pub before: T,
    pub after: T,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct BlockDifference {
    pub position: Pos,
    /// None is unknown; an explicit Air block is known empty space.
    pub state: ValueChange<Option<Block>>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct OccurrenceDifference {
    pub instance: InstancePath,
    pub occurrence: ValueChange<Option<BlueprintOccurrence>>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct BlueprintUpdateDiff {
    pub blocks: Vec<BlockDifference>,
    pub occurrences: Vec<OccurrenceDifference>,
    pub connections: Option<ValueChange<Vec<AssemblyConnection>>>,
    pub boundaries: Option<ValueChange<Vec<AssemblyBoundary>>>,
    pub known_regions: Option<ValueChange<Vec<Region>>>,
    pub name: Option<ValueChange<String>>,
    pub candidate_sources: Vec<BlueprintRevision>,
    pub candidate_source_differences: Vec<SourceStateDifference>,
    pub affected_occurrences: BTreeSet<InstancePath>,
}

#[derive(Clone, Debug)]
pub enum BlueprintUpdateError {
    Blueprint(BlueprintError),
    UnknownProposal(BlueprintUpdateId),
    DuplicateProposal(BlueprintUpdateId),
    ClosedProposal(BlueprintUpdateId),
    Invalid(String),
    Validation(Box<PromotionReport>),
}
impl std::fmt::Display for BlueprintUpdateError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(formatter, "{self:?}")
    }
}
impl std::error::Error for BlueprintUpdateError {}
impl From<BlueprintError> for BlueprintUpdateError {
    fn from(error: BlueprintError) -> Self {
        Self::Blueprint(error)
    }
}
fn invalid<T>(message: &str) -> Result<T, BlueprintUpdateError> {
    Err(BlueprintUpdateError::Invalid(message.into()))
}

/// Owns catalog and proposal history so adoption can commit both atomically.
#[derive(Clone, Debug)]
pub struct BlueprintUpdates {
    catalog: BlueprintCatalog,
    proposals: BTreeMap<BlueprintUpdateId, BlueprintUpdate>,
}

const UPDATE_ARCHIVE_SCHEMA: &str = "dustroute.blueprint-updates.v5";

#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct UpdateArchive {
    schema: String,
    catalog: serde_json::Value,
    proposals: Vec<BlueprintUpdate>,
}

impl BlueprintUpdates {
    #[must_use]
    pub fn new(catalog: BlueprintCatalog) -> Self {
        Self {
            catalog,
            proposals: BTreeMap::new(),
        }
    }
    #[must_use]
    pub fn catalog(&self) -> &BlueprintCatalog {
        &self.catalog
    }
    /// Imports remain unverified. IDs reserved by proposals must go through
    /// adoption, so imports cannot publish or overwrite a pending candidate.
    pub fn append_revision(
        &mut self,
        revision: BlueprintRevision,
    ) -> Result<(), BlueprintUpdateError> {
        if self.proposals.values().any(|proposal| {
            proposal
                .request
                .revisions
                .iter()
                .any(|record| record.id == revision.id)
        }) {
            return invalid("revision ID is reserved by an update proposal");
        }
        Ok(self.catalog.insert_revision(revision)?)
    }
    pub fn append_state(&mut self, state: AssemblyRevision) -> Result<(), BlueprintUpdateError> {
        if self
            .proposals
            .values()
            .any(|proposal| proposal.request.candidate_state.id == state.id)
        {
            return invalid("state ID is reserved by an update proposal");
        }
        Ok(self.catalog.insert_assembly(state)?)
    }
    pub fn append_type(
        &mut self,
        definition: dustroute_library::blueprint::TypeRevision,
    ) -> Result<(), BlueprintUpdateError> {
        Ok(self.catalog.insert_type(definition)?)
    }
    pub fn append_classification(
        &mut self,
        definition: dustroute_library::blueprint::ClassificationRevision,
    ) -> Result<(), BlueprintUpdateError> {
        Ok(self.catalog.insert_classification(definition)?)
    }
    #[must_use]
    pub fn proposal(&self, id: &BlueprintUpdateId) -> Option<&BlueprintUpdate> {
        self.proposals.get(id)
    }
    pub fn proposals(&self) -> impl Iterator<Item = &BlueprintUpdate> {
        self.proposals.values()
    }

    /// Records a structurally checked proposal; publishes no candidate source
    /// or Assembly Revision, and grants no physical validation status.
    pub fn create(&mut self, request: BlueprintUpdateRequest) -> Result<(), BlueprintUpdateError> {
        if self.proposals.contains_key(&request.id) {
            return Err(BlueprintUpdateError::DuplicateProposal(request.id));
        }
        if self.catalog.revision(&request.candidate_parent).is_some()
            || self.catalog.assembly(&request.candidate_state.id).is_some()
        {
            return invalid("a proposal must name a new parent and a new Assembly Revision");
        }
        stage(&self.catalog, &request)?;
        self.check_reservations(&request)?;
        self.proposals.insert(
            request.id.clone(),
            BlueprintUpdate {
                request,
                events: vec![],
            },
        );
        Ok(())
    }

    fn get(&self, id: &BlueprintUpdateId) -> Result<&BlueprintUpdate, BlueprintUpdateError> {
        self.proposals
            .get(id)
            .ok_or_else(|| BlueprintUpdateError::UnknownProposal(id.clone()))
    }
    fn open(&self, id: &BlueprintUpdateId) -> Result<&BlueprintUpdate, BlueprintUpdateError> {
        let proposal = self.get(id)?;
        if proposal.status() != UpdateStatus::Open {
            return Err(BlueprintUpdateError::ClosedProposal(id.clone()));
        }
        Ok(proposal)
    }

    fn check_reservations(
        &self,
        request: &BlueprintUpdateRequest,
    ) -> Result<(), BlueprintUpdateError> {
        for existing in self.proposals.values() {
            let other = &existing.request;
            if request.candidate_parent == other.candidate_parent
                || request.candidate_state.id == other.candidate_state.id
            {
                return invalid(
                    "candidate parent/state IDs are already reserved by another proposal",
                );
            }
            for revision in &request.revisions {
                if other
                    .revisions
                    .iter()
                    .any(|saved| saved.id == revision.id && saved != revision)
                {
                    return invalid(
                        "proposals cannot assign different definitions to the same revision ID",
                    );
                }
            }
        }
        Ok(())
    }

    pub fn diff(
        &self,
        id: &BlueprintUpdateId,
    ) -> Result<BlueprintUpdateDiff, BlueprintUpdateError> {
        let request = &self.get(id)?.request;
        let staged = stage(&self.catalog, request)?;
        let base = &self
            .catalog
            .assembly(&request.base_state)
            .expect("checked base")
            .assembly;
        let candidate = &request.candidate_state.assembly;
        let before = base.inspect(&self.catalog)?;
        let after = candidate.inspect(&staged)?;
        let impact = after.changes_from(&before);
        let blocks = impact
            .changed_positions
            .iter()
            .map(|position| BlockDifference {
                position: *position,
                state: ValueChange {
                    before: before.block_at(*position),
                    after: after.block_at(*position),
                },
            })
            .collect();
        let paths: BTreeSet<_> = before
            .occurrences
            .keys()
            .chain(after.occurrences.keys())
            .cloned()
            .collect();
        let occurrences = paths
            .into_iter()
            .filter_map(|instance| {
                change(
                    before.occurrences.get(&instance).cloned(),
                    after.occurrences.get(&instance).cloned(),
                )
                .map(|occurrence| OccurrenceDifference {
                    instance,
                    occurrence,
                })
            })
            .collect();
        Ok(BlueprintUpdateDiff {
            blocks,
            occurrences,
            connections: change(base.connections.clone(), candidate.connections.clone()),
            boundaries: change(base.boundaries.clone(), candidate.boundaries.clone()),
            known_regions: change(base.known_regions.clone(), candidate.known_regions.clone()),
            name: change(base.name.clone(), candidate.name.clone()),
            candidate_sources: request.revisions.clone(),
            candidate_source_differences: after.source_differences(),
            affected_occurrences: impact.affected_occurrences,
        })
    }

    /// Fresh read-only verification, also available for historical decisions.
    pub fn review(&self, id: &BlueprintUpdateId) -> Result<PromotionReport, BlueprintUpdateError> {
        let request = &self.get(id)?.request;
        let staged = stage(&self.catalog, request)?;
        Ok(review_assembly_with_context(
            &staged,
            &request.candidate_state.assembly,
            request.behavior_context.as_ref(),
            BehaviorBudget::default(),
        )?)
    }

    /// Records diagnostics while leaving the proposal open and references fixed.
    pub fn validate(
        &mut self,
        id: &BlueprintUpdateId,
    ) -> Result<PromotionReport, BlueprintUpdateError> {
        self.open(id)?;
        let report = self.review(id)?;
        self.proposals
            .get_mut(id)
            .expect("open proposal")
            .events
            .push(UpdateEvent::Validated {
                report: (&report).into(),
            });
        Ok(report)
    }

    pub fn reject(
        &mut self,
        id: &BlueprintUpdateId,
        reason: impl Into<String>,
    ) -> Result<(), BlueprintUpdateError> {
        self.open(id)?;
        let reason = reason.into();
        if reason.trim().is_empty() {
            return invalid("rejection requires a reason");
        }
        self.proposals
            .get_mut(id)
            .expect("open proposal")
            .events
            .push(UpdateEvent::Rejected { reason });
        Ok(())
    }

    /// Always revalidates, even immediately after validate or archive loading.
    /// A failed attempt records its findings but publishes no candidate records.
    pub fn adopt(&mut self, id: &BlueprintUpdateId) -> Result<(), BlueprintUpdateError> {
        let request = self.open(id)?.request.clone();
        if self.catalog.revision(&request.candidate_parent).is_some()
            || self.catalog.assembly(&request.candidate_state.id).is_some()
        {
            return invalid(
                "candidate parent or state already exists; adoption cannot overwrite it",
            );
        }
        let report = self.validate(id)?;
        if report.status() != CheckStatus::Passed {
            return Err(BlueprintUpdateError::Validation(Box::new(report)));
        }
        let staged = stage(&self.catalog, &request)?;
        crate::assembly::validate_assembly_for_adoption(
            &staged,
            &request.candidate_state.assembly,
            request.behavior_context.as_ref(),
            BehaviorBudget::default(),
        )
        .map_err(|error| BlueprintUpdateError::Invalid(error.to_string()))?;
        self.catalog = staged;
        self.proposals
            .get_mut(id)
            .expect("open proposal")
            .events
            .push(UpdateEvent::Adopted {
                parent: request.candidate_parent,
                state: request.candidate_state.id,
            });
        Ok(())
    }

    /// Self-contained persistence of sources, states, candidates and decisions.
    /// Validation events are historical diagnostics and never adoption authority.
    pub fn to_json(&self) -> Result<String, BlueprintUpdateError> {
        let catalog = serde_json::from_str(&self.catalog.to_json()?)
            .map_err(|error| BlueprintUpdateError::Invalid(error.to_string()))?;
        serde_json::to_string_pretty(&UpdateArchive {
            schema: UPDATE_ARCHIVE_SCHEMA.into(),
            catalog,
            proposals: self.proposals.values().cloned().collect(),
        })
        .map_err(|error| BlueprintUpdateError::Invalid(error.to_string()))
    }

    pub fn from_json(input: &str) -> Result<Self, BlueprintUpdateError> {
        let archive: UpdateArchive = serde_json::from_str(input)
            .map_err(|error| BlueprintUpdateError::Invalid(error.to_string()))?;
        if archive.schema != UPDATE_ARCHIVE_SCHEMA {
            return invalid(
                "retired or unsupported blueprint updates schema; recreate and freshly review proposals with the current version (v5)",
            );
        }
        let mut restored = Self::new(BlueprintCatalog::from_json(&archive.catalog.to_string())?);
        for proposal in archive.proposals {
            let request = &proposal.request;
            if restored.proposals.contains_key(&request.id) {
                return Err(BlueprintUpdateError::DuplicateProposal(request.id.clone()));
            }
            stage(&restored.catalog, request)?;
            restored.check_reservations(request)?;
            let mut decided = false;
            for event in &proposal.events {
                if decided {
                    return invalid("proposal contains events after its terminal decision");
                }
                match event {
                    UpdateEvent::Validated { .. } => {}
                    UpdateEvent::Rejected { reason } => {
                        if reason.trim().is_empty() {
                            return invalid("empty rejection reason");
                        }
                        decided = true;
                    }
                    UpdateEvent::Adopted { parent, state } => {
                        if parent != &request.candidate_parent
                            || state != &request.candidate_state.id
                            || restored.catalog.assembly(state) != Some(&request.candidate_state)
                            || request.revisions.iter().any(|revision| {
                                restored.catalog.revision(&revision.id) != Some(revision)
                            })
                        {
                            return invalid(
                                "adoption history does not match committed source/state records",
                            );
                        }
                        decided = true;
                    }
                }
            }
            restored.proposals.insert(request.id.clone(), proposal);
        }
        Ok(restored)
    }
}

fn change<T: Eq>(before: T, after: T) -> Option<ValueChange<T>> {
    (before != after).then_some(ValueChange { before, after })
}

/// Referential checks only, allowing physically failing candidates to be saved.
fn stage(
    catalog: &BlueprintCatalog,
    request: &BlueprintUpdateRequest,
) -> Result<BlueprintCatalog, BlueprintUpdateError> {
    if request.title.trim().is_empty()
        || request.parent_instance.is_empty()
        || request.child_before.is_empty()
        || request.child_after.is_empty()
    {
        return invalid("a proposal needs a title, parent occurrence and child paths");
    }
    if request.base_parent == request.candidate_parent
        || request.previous_child == request.next_child
        || request.base_state == request.candidate_state.id
    {
        return invalid("updates require distinct old and new revision IDs");
    }
    if request.candidate_state.parents != [request.base_state.clone()] {
        return invalid("candidate state must descend directly from the selected base state");
    }
    let base = catalog
        .assembly(&request.base_state)
        .ok_or_else(|| BlueprintError::UnknownAssembly(request.base_state.clone()))?;
    let before = base.assembly.inspect(catalog)?;
    let child = |relative: &InstancePath| {
        request
            .parent_instance
            .iter()
            .chain(relative)
            .cloned()
            .collect::<InstancePath>()
    };
    if before
        .occurrences
        .get(&request.parent_instance)
        .map(|item| &item.revision)
        != Some(&request.base_parent)
        || before
            .occurrences
            .get(&child(&request.child_before))
            .map(|item| &item.revision)
            != Some(&request.previous_child)
    {
        return invalid("base parent/child selection does not match the pinned base state");
    }
    let mut staged = catalog.clone();
    let mut added = Vec::new();
    let mut ids = BTreeSet::new();
    for revision in &request.revisions {
        if !ids.insert(revision.id.clone()) {
            return Err(BlueprintError::DuplicateRevision(revision.id.clone()).into());
        }
        match catalog.revision(&revision.id) {
            None => added.push(revision.clone()),
            Some(existing) if existing == revision => {}
            Some(_) => return Err(BlueprintError::DuplicateRevision(revision.id.clone()).into()),
        }
    }
    if !ids.contains(&request.candidate_parent) {
        return invalid("candidate parent definition is missing from the proposal");
    }
    staged.insert_revisions(added)?;
    let parent = staged
        .revision(&request.candidate_parent)
        .expect("supplied parent");
    if parent.parents != [request.base_parent.clone()] {
        return invalid("candidate parent must descend directly from the selected base parent");
    }
    match staged.assembly(&request.candidate_state.id) {
        None => staged.insert_assembly(request.candidate_state.clone())?,
        Some(existing) if existing == &request.candidate_state => {}
        Some(_) => {
            return Err(
                BlueprintError::DuplicateAssembly(request.candidate_state.id.clone()).into(),
            );
        }
    }
    let after = request.candidate_state.assembly.inspect(&staged)?;
    if after
        .occurrences
        .get(&request.parent_instance)
        .map(|item| &item.revision)
        != Some(&request.candidate_parent)
        || after
            .occurrences
            .get(&child(&request.child_after))
            .map(|item| &item.revision)
            != Some(&request.next_child)
    {
        return invalid("candidate parent/child selection does not match the proposed state");
    }
    Ok(staged)
}
