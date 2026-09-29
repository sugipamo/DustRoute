//! Short-lived executable placements. Progress records and durable Assembly
//! instances have different lifetimes and are deliberately owned elsewhere.
use super::RevisionPlacementContext;
use crate::PlacementPlan;
use std::collections::HashMap;
use uuid::Uuid;

enum PlacementSource {
    Translated,
    Revision(Box<RevisionPlacementContext>),
}

#[derive(Clone, Copy)]
enum AppliedState {
    Unapplied,
    Applied,
}

struct PlacementEntry {
    plan: PlacementPlan,
    dimension: String,
    source: PlacementSource,
    applied: AppliedState,
}

#[derive(Default)]
pub(super) struct PlacementRegistry {
    entries: HashMap<Uuid, PlacementEntry>,
}

impl PlacementRegistry {
    #[cfg(test)]
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }
    pub fn insert(
        &mut self,
        plan: PlacementPlan,
        dimension: String,
        revision: Option<RevisionPlacementContext>,
    ) {
        let source = revision.map_or(PlacementSource::Translated, |context| {
            PlacementSource::Revision(Box::new(context))
        });
        self.entries.insert(
            plan.operation_id,
            PlacementEntry {
                plan,
                dimension,
                source,
                applied: AppliedState::Unapplied,
            },
        );
    }
    pub fn contains_key(&self, id: &Uuid) -> bool {
        self.entries.contains_key(id)
    }
    pub fn get(&self, id: &Uuid) -> Option<&PlacementPlan> {
        self.entries.get(id).map(|entry| &entry.plan)
    }
    pub fn get_mut(&mut self, id: &Uuid) -> Option<&mut PlacementPlan> {
        self.entries.get_mut(id).map(|entry| &mut entry.plan)
    }
    pub fn dimension(&self, id: &Uuid) -> Option<&String> {
        self.entries.get(id).map(|entry| &entry.dimension)
    }
    pub fn revision(&self, id: &Uuid) -> Option<&RevisionPlacementContext> {
        match &self.entries.get(id)?.source {
            PlacementSource::Translated => None,
            PlacementSource::Revision(context) => Some(context),
        }
    }
    pub fn revision_mut(&mut self, id: &Uuid) -> Option<&mut RevisionPlacementContext> {
        match &mut self.entries.get_mut(id)?.source {
            PlacementSource::Translated => None,
            PlacementSource::Revision(context) => Some(context),
        }
    }
    pub fn has_revision(&self, id: &Uuid) -> bool {
        self.revision(id).is_some()
    }
    pub fn is_applied(&self, id: &Uuid) -> bool {
        self.entries
            .get(id)
            .is_some_and(|entry| matches!(entry.applied, AppliedState::Applied))
    }
    pub fn set_applied(&mut self, id: &Uuid, applied: bool) {
        if let Some(entry) = self.entries.get_mut(id) {
            entry.applied = if applied {
                AppliedState::Applied
            } else {
                AppliedState::Unapplied
            };
        }
    }
}
