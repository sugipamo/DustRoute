//! Revisioned, owned state snapshots.

use std::{
    ops::{Deref, DerefMut},
    time::{Duration, Instant},
};

/// An owned state value paired with its domain revision and local timestamps.
///
/// Revisions are monotonic per state domain and start at zero. They are not
/// comparable across different domains or different `Bot` connections.
#[derive(Clone, Debug, PartialEq)]
pub struct Snapshot<T> {
    /// The `revision` value.
    pub revision: u64,
    /// Time since this Bot connected when the state domain last changed.
    pub updated_at: Duration,
    /// Time since this Bot connected when this owned snapshot was created.
    pub captured_at: Duration,
    /// The `value` value.
    pub value: T,
}

impl<T> Snapshot<T> {
    /// Performs the `into_inner` operation.
    pub fn into_inner(self) -> T {
        self.value
    }
}

impl<T> AsRef<T> for Snapshot<T> {
    fn as_ref(&self) -> &T {
        &self.value
    }
}

pub(crate) struct Versioned<T> {
    value: T,
    revision: u64,
    started: Instant,
    updated_at: Duration,
}

impl<T> Versioned<T> {
    pub(crate) fn new(value: T, started: Instant) -> Self {
        Self {
            value,
            revision: 0,
            started,
            updated_at: Duration::ZERO,
        }
    }

    pub(crate) fn snapshot(&self) -> Snapshot<T>
    where
        T: Clone,
    {
        Snapshot {
            revision: self.revision,
            updated_at: self.updated_at,
            captured_at: self.started.elapsed(),
            value: self.value.clone(),
        }
    }

    pub(crate) fn map_snapshot<U>(&self, map: impl FnOnce(&T) -> U) -> Snapshot<U> {
        Snapshot {
            revision: self.revision,
            updated_at: self.updated_at,
            captured_at: self.started.elapsed(),
            value: map(&self.value),
        }
    }
}

impl<T> Deref for Versioned<T> {
    type Target = T;

    fn deref(&self) -> &Self::Target {
        &self.value
    }
}

impl<T> DerefMut for Versioned<T> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        self.revision = self.revision.wrapping_add(1);
        self.updated_at = self.started.elapsed();
        &mut self.value
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mutation_advances_domain_revision() {
        let mut value = Versioned::new(vec![1], Instant::now());
        let before = value.snapshot();
        value.push(2);
        let after = value.snapshot();
        assert_eq!(before.revision, 0);
        assert_eq!(after.revision, 1);
        assert_eq!(after.value, vec![1, 2]);
        assert!(after.captured_at >= after.updated_at);
    }
}
