//! One owner for transient executable plans. Typed handles share an arena;
//! durable facts and progress reports retain their separate lifetimes.
use super::{
    StoredDoorPlan, StoredPistonPlacement, StoredTransitionPlan,
    assembly_placement::StoredAssemblyPlacement,
    placement_registry::{PlacementEntry, PlacementTable},
};
use std::{collections::HashMap, marker::PhantomData, sync::Arc};
use tokio::sync::{Mutex, OwnedMutexGuard};
use uuid::Uuid;

pub(super) enum StoredOperation {
    Placement(Box<PlacementEntry>),
    Transition(Box<StoredTransitionPlan>),
    Door(Box<StoredDoorPlan>),
    Piston(Box<StoredPistonPlacement>),
    Assembly(Box<StoredAssemblyPlacement>),
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum PlanKind {
    Placement,
    Transition,
    Door,
    Piston,
    Assembly,
}
pub(super) trait PlanPayload: Sized {
    fn wrap(self) -> StoredOperation;
    fn get(value: &StoredOperation) -> Option<&Self>;
    fn get_mut(value: &mut StoredOperation) -> Option<&mut Self>;
}
macro_rules! payload {
    ($type:ty, $variant:ident) => {
        impl PlanPayload for $type {
            fn wrap(self) -> StoredOperation {
                StoredOperation::$variant(Box::new(self))
            }
            fn get(value: &StoredOperation) -> Option<&Self> {
                if let StoredOperation::$variant(value) = value {
                    Some(value)
                } else {
                    None
                }
            }
            fn get_mut(value: &mut StoredOperation) -> Option<&mut Self> {
                if let StoredOperation::$variant(value) = value {
                    Some(value)
                } else {
                    None
                }
            }
        }
    };
}
payload!(PlacementEntry, Placement);
payload!(StoredTransitionPlan, Transition);
payload!(StoredDoorPlan, Door);
payload!(StoredPistonPlacement, Piston);
payload!(StoredAssemblyPlacement, Assembly);

#[derive(Clone, Default)]
pub(super) struct OperationPlans {
    entries: Arc<Mutex<HashMap<Uuid, StoredOperation>>>,
}
impl OperationPlans {
    pub fn table<T: PlanPayload>(&self) -> PlanTable<T> {
        PlanTable {
            entries: self.entries.clone(),
            kind: PhantomData,
        }
    }
    pub fn placements(&self) -> PlacementTable {
        PlacementTable(self.table())
    }
    pub async fn kind(&self, id: &Uuid) -> Option<PlanKind> {
        self.entries.lock().await.get(id).map(|entry| match entry {
            StoredOperation::Placement(_) => PlanKind::Placement,
            StoredOperation::Transition(_) => PlanKind::Transition,
            StoredOperation::Door(_) => PlanKind::Door,
            StoredOperation::Piston(_) => PlanKind::Piston,
            StoredOperation::Assembly(_) => PlanKind::Assembly,
        })
    }
}
pub(super) struct PlanTable<T> {
    entries: Arc<Mutex<HashMap<Uuid, StoredOperation>>>,
    kind: PhantomData<T>,
}
impl<T: PlanPayload> PlanTable<T> {
    pub async fn lock(&self) -> PlanGuard<T> {
        PlanGuard {
            entries: self.entries.clone().lock_owned().await,
            kind: PhantomData,
        }
    }
}
pub(super) struct PlanGuard<T> {
    entries: OwnedMutexGuard<HashMap<Uuid, StoredOperation>>,
    kind: PhantomData<T>,
}
impl<T: PlanPayload> PlanGuard<T> {
    pub fn get(&self, id: &Uuid) -> Option<&T> {
        self.entries.get(id).and_then(T::get)
    }
    pub fn get_mut(&mut self, id: &Uuid) -> Option<&mut T> {
        self.entries.get_mut(id).and_then(T::get_mut)
    }
    pub fn insert(&mut self, id: Uuid, plan: T) {
        // Executable plan identities are freshly generated and cannot be rebound.
        assert!(
            !self.entries.contains_key(&id),
            "duplicate executable operation identity"
        );
        self.entries.insert(id, plan.wrap());
    }
    pub fn retain(&mut self, mut keep: impl FnMut(&Uuid, &mut T) -> bool) {
        self.entries
            .retain(|id, value| T::get_mut(value).is_none_or(|value| keep(id, value)));
    }
    pub fn len(&self) -> usize {
        self.entries
            .values()
            .filter(|v| T::get(v).is_some())
            .count()
    }
    #[cfg(test)]
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }
}
