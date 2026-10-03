//! Immutable contents are shared; content identity never grants freshness or ownership.
use dustroute_translate::snapshot::{MinecraftSnapshot, index_literal_snapshot};
use serde::{Serialize, Serializer};
use sha2::{Digest, Sha256};
use std::collections::HashMap;
use std::ops::Deref;
use std::sync::{Arc, Mutex, Weak};

/// SHA-256 over the versioned, canonical exact snapshot, including bounds and known cells.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct ContentId([u8; 32]);
impl std::fmt::Display for ContentId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        for byte in self.0 {
            write!(f, "{byte:02x}")?;
        }
        Ok(())
    }
}
impl Serialize for ContentId {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.collect_str(self)
    }
}

#[derive(Debug)]
struct Content {
    id: ContentId,
    snapshot: Arc<MinecraftSnapshot>,
}

/// Only the interner constructs these references; callers cannot mutate the shared payload.
#[derive(Clone, Debug)]
pub struct SharedSnapshot(Arc<Content>);
impl SharedSnapshot {
    #[must_use]
    pub fn id(&self) -> ContentId {
        self.0.id
    }
    #[must_use]
    pub fn shares_storage_with(&self, other: &Self) -> bool {
        Arc::ptr_eq(&self.0, &other.0)
    }
    /// Owned archive/API boundary; the live internal paths keep the shared reference.
    #[must_use]
    pub fn to_owned_snapshot(&self) -> MinecraftSnapshot {
        self.deref().clone()
    }
    #[cfg(feature = "voxrig")]
    pub(crate) fn shared_allocation(&self) -> Arc<MinecraftSnapshot> {
        self.0.snapshot.clone()
    }
}
impl Deref for SharedSnapshot {
    type Target = MinecraftSnapshot;
    fn deref(&self) -> &Self::Target {
        &self.0.snapshot
    }
}
impl Serialize for SharedSnapshot {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        self.deref().serialize(serializer)
    }
}

#[derive(Debug, Default)]
struct Entries {
    by_id: HashMap<ContentId, Weak<Content>>,
    by_pointer: HashMap<usize, PointerEntry>,
}
#[derive(Debug)]
struct PointerEntry {
    source: Weak<MinecraftSnapshot>,
    content: Weak<Content>,
    id: ContentId,
}
/// Per bridge/session service; weak entries do not retain expired observations.
#[derive(Debug, Default)]
pub(crate) struct SnapshotContents(Mutex<Entries>);
impl SnapshotContents {
    pub(crate) fn intern(&self, snapshot: MinecraftSnapshot) -> Result<SharedSnapshot, String> {
        self.intern_arc(Arc::new(snapshot))
    }
    pub(crate) fn intern_arc(
        &self,
        mut snapshot: Arc<MinecraftSnapshot>,
    ) -> Result<SharedSnapshot, String> {
        let measurement = crate::performance::span(crate::performance::Phase::ContentIntern);
        let mut entries = self.0.lock().unwrap_or_else(|e| e.into_inner());
        let pointer = Arc::as_ptr(&snapshot) as usize;
        if let Some(entry) = entries.by_pointer.get(&pointer)
            && entry
                .source
                .upgrade()
                .is_some_and(|source| Arc::ptr_eq(&source, &snapshot))
            && let Some(content) = entry.content.upgrade()
        {
            let _measurement = measurement.acquisition(0, true);
            return Ok(SharedSnapshot(content));
        }
        // Compatibility reads may release the shared handle while the native
        // conversion cache still holds exactly this immutable allocation.
        // Its validated canonical hash remains usable without another full walk.
        if let Some(entry) = entries.by_pointer.get(&pointer)
            && entry
                .source
                .upgrade()
                .is_some_and(|source| Arc::ptr_eq(&source, &snapshot))
        {
            let id = entry.id;
            let content = if let Some(existing) = entries.by_id.get(&id).and_then(Weak::upgrade) {
                if *existing.snapshot != *snapshot {
                    return Err("snapshot content hash collision".into());
                }
                existing
            } else {
                Arc::new(Content { id, snapshot })
            };
            entries.by_pointer.get_mut(&pointer).unwrap().content = Arc::downgrade(&content);
            entries.by_id.insert(id, Arc::downgrade(&content));
            let _measurement = measurement.acquisition(0, true);
            return Ok(SharedSnapshot(content));
        }
        // Validation retains explicit air and missing coverage in the content hash.
        index_literal_snapshot(&snapshot).map_err(|e| e.to_string())?;
        if !snapshot.blocks.windows(2).all(|w| w[0].pos < w[1].pos) {
            Arc::make_mut(&mut snapshot).blocks.sort_by_key(|b| b.pos);
        }
        let id = content_id(&snapshot);
        let _measurement = measurement.acquisition(snapshot.blocks.len(), false);
        if let Some(content) = entries.by_id.get(&id).and_then(Weak::upgrade) {
            if *content.snapshot != *snapshot {
                return Err("snapshot content hash collision".into());
            }
            entries
                .by_pointer
                .retain(|_, e| e.source.strong_count() > 0);
            entries.by_pointer.insert(
                Arc::as_ptr(&snapshot) as usize,
                PointerEntry {
                    source: Arc::downgrade(&snapshot),
                    content: Arc::downgrade(&content),
                    id,
                },
            );
            return Ok(SharedSnapshot(content));
        }
        // Retain every live entry, including during concurrent acquisitions; no live
        // reference can be evicted and later acquire a duplicate payload in this store.
        entries.by_id.retain(|_, weak| weak.strong_count() > 0);
        entries
            .by_pointer
            .retain(|_, e| e.source.strong_count() > 0);
        let content = Arc::new(Content { id, snapshot });
        entries.by_pointer.insert(
            Arc::as_ptr(&content.snapshot) as usize,
            PointerEntry {
                source: Arc::downgrade(&content.snapshot),
                content: Arc::downgrade(&content),
                id,
            },
        );
        entries.by_id.insert(id, Arc::downgrade(&content));
        Ok(SharedSnapshot(content))
    }
}

struct HashWriter(Sha256);
impl std::io::Write for HashWriter {
    fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
        self.0.update(bytes);
        Ok(bytes.len())
    }
    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}
pub(crate) fn content_id(snapshot: &MinecraftSnapshot) -> ContentId {
    use std::io::Write;
    let mut writer = HashWriter(Sha256::new());
    writer.0.update(b"dustroute.exact-snapshot.v1\0");
    let mut buffer = std::io::BufWriter::new(writer);
    let position = |writer: &mut std::io::BufWriter<HashWriter>, pos: dustroute_physical::Pos| {
        for coordinate in [pos.x, pos.y, pos.z] {
            writer
                .write_all(&coordinate.to_be_bytes())
                .expect("hash writer cannot fail");
        }
    };
    let string = |writer: &mut std::io::BufWriter<HashWriter>, value: &str| {
        writer
            .write_all(&(value.len() as u64).to_be_bytes())
            .expect("hash writer cannot fail");
        writer
            .write_all(value.as_bytes())
            .expect("hash writer cannot fail");
    };
    position(&mut buffer, snapshot.min);
    position(&mut buffer, snapshot.max);
    buffer
        .write_all(&(snapshot.blocks.len() as u64).to_be_bytes())
        .expect("hash writer cannot fail");
    for block in &snapshot.blocks {
        position(&mut buffer, block.pos);
        string(&mut buffer, &block.name);
        buffer
            .write_all(&(block.properties.len() as u64).to_be_bytes())
            .expect("hash writer cannot fail");
        for (key, value) in &block.properties {
            string(&mut buffer, key);
            string(&mut buffer, value);
        }
    }
    let writer = buffer
        .into_inner()
        .unwrap_or_else(|_| unreachable!("hash writer cannot fail"));
    ContentId(writer.0.finalize().into())
}

/// Analysis identity pins both input contents and the model/conditions which interpret them.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq, Serialize)]
#[serde(transparent)]
pub struct ValidationKey(ContentId);
impl std::fmt::Display for ValidationKey {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.0.fmt(f)
    }
}
impl ValidationKey {
    pub(crate) fn new(
        inputs: &[ContentId],
        context: &dustroute_translate::world::execution_context::WorldExecutionContext,
        conditions: impl Serialize,
    ) -> Self {
        let mut writer = HashWriter(Sha256::new());
        writer.0.update(b"dustroute.validation-key.v1\0");
        let mut buffer = std::io::BufWriter::new(writer);
        serde_json::to_writer(
            &mut buffer,
            &(
                inputs,
                context,
                context.physical_admission_revision(),
                conditions,
            ),
        )
        .expect("typed analysis conditions serialize");
        let writer = buffer
            .into_inner()
            .unwrap_or_else(|_| unreachable!("hash writer cannot fail"));
        Self(ContentId(writer.0.finalize().into()))
    }
}

/// Identity of one acquisition, independent of content sharing. It cannot create a fresh capability.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq, Serialize)]
#[serde(transparent)]
pub struct ObservationId(uuid::Uuid);
impl ObservationId {
    #[cfg(any(test, feature = "voxrig"))]
    pub(crate) fn new() -> Self {
        Self(uuid::Uuid::new_v4())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use dustroute_physical::Pos;
    use dustroute_translate::snapshot::MinecraftSnapshotBlock;
    fn snapshot() -> MinecraftSnapshot {
        MinecraftSnapshot {
            min: Pos::new(0, 0, 0),
            max: Pos::new(1, 0, 0),
            blocks: vec![
                MinecraftSnapshotBlock {
                    pos: Pos::new(0, 0, 0),
                    name: "minecraft:stone".into(),
                    properties: Default::default(),
                },
                MinecraftSnapshotBlock {
                    pos: Pos::new(1, 0, 0),
                    name: "minecraft:air".into(),
                    properties: Default::default(),
                },
            ],
        }
    }
    #[test]
    fn canonical_contents_share_storage_but_bounds_coverage_and_states_do_not() {
        let store = SnapshotContents::default();
        let first = store.intern(snapshot()).unwrap();
        let mut reversed = snapshot();
        reversed.blocks.reverse();
        let second = store.intern(reversed).unwrap();
        assert!(first.shares_storage_with(&second));
        for case in 0..4 {
            let mut other = snapshot();
            match case {
                0 => {
                    other.blocks.pop();
                }
                1 => other.max.x = 2,
                2 => other.blocks[0].name = "minecraft:glass".into(),
                _ => {
                    other.blocks[0]
                        .properties
                        .insert("waterlogged".into(), "true".into());
                }
            }
            let other = store.intern(other).unwrap();
            assert_ne!(first.id(), other.id());
            assert!(!first.shares_storage_with(&other));
        }
        let mut bad = snapshot();
        bad.blocks.push(bad.blocks[0].clone());
        assert!(store.intern(bad).is_err());
        assert_eq!(
            serde_json::to_value(&first).unwrap(),
            serde_json::to_value(snapshot()).unwrap()
        );
    }
    #[test]
    fn concurrent_interning_and_pointer_reuse_preserve_one_live_payload() {
        let store = Arc::new(SnapshotContents::default());
        let input = Arc::new(snapshot());
        let threads: Vec<_> = (0..8)
            .map(|_| {
                let store = store.clone();
                let input = input.clone();
                std::thread::spawn(move || store.intern_arc(input).unwrap())
            })
            .collect();
        let results: Vec<_> = threads.into_iter().map(|t| t.join().unwrap()).collect();
        assert!(results.iter().all(|s| s.shares_storage_with(&results[0])));
        drop(results);
        let entries = store.0.lock().unwrap();
        assert_eq!(
            entries
                .by_id
                .values()
                .filter(|v| v.strong_count() > 0)
                .count(),
            0
        );
    }
    #[test]
    fn a_hash_collision_cannot_alias_a_different_payload() {
        let store = SnapshotContents::default();
        let first = store.intern(snapshot()).unwrap();
        let mut changed = snapshot();
        changed.blocks[0].name = "minecraft:glass".into();
        store
            .0
            .lock()
            .unwrap()
            .by_id
            .insert(content_id(&changed), Arc::downgrade(&first.0));
        assert!(store.intern(changed).unwrap_err().contains("collision"));
    }

    #[test]
    fn immutable_pointer_hash_survives_owned_reads_but_not_source_mutation() {
        let store = SnapshotContents::default();
        let mut source = Arc::new(snapshot());
        let first = store.intern_arc(source.clone()).unwrap();
        let id = first.id();
        drop(first);
        let resumed = store.intern_arc(source.clone()).unwrap();
        assert_eq!(resumed.id(), id);
        Arc::make_mut(&mut source).blocks[0].name = "minecraft:glass".into();
        let changed = store.intern_arc(source).unwrap();
        assert_ne!(resumed.id(), changed.id());
        assert!(!resumed.shares_storage_with(&changed));
    }

    #[test]
    fn validation_identity_changes_with_model_pins_and_conditions() {
        use dustroute_translate::world::execution_context::{
            WorldExecutionContext, WorldExecutionProfile,
        };
        let store = SnapshotContents::default();
        let input = store.intern(snapshot()).unwrap();
        let context = WorldExecutionContext::for_profile(
            WorldExecutionProfile::UnifiedPistonElectricalCallbacksJava12111V19,
        );
        let conditions = serde_json::json!({"scope":"test","complete":true,"ticks":64});
        let key = ValidationKey::new(&[input.id()], &context, &conditions);
        assert_eq!(
            key,
            ValidationKey::new(&[input.id()], &context, &conditions)
        );
        let mut changed = context.clone();
        changed.max_electrical_iterations =
            Some(context.max_electrical_iterations.unwrap_or(0) + 1);
        assert_ne!(
            key,
            ValidationKey::new(&[input.id()], &changed, &conditions)
        );
        changed = context.clone();
        changed.laws.clear();
        assert_ne!(
            key,
            ValidationKey::new(&[input.id()], &changed, &conditions)
        );
        assert_ne!(
            key,
            ValidationKey::new(
                &[input.id()],
                &context,
                serde_json::json!({"scope":"test","complete":false,"ticks":64})
            )
        );
        assert_ne!(key, ValidationKey::new(&[], &context, &conditions));
    }

    #[test]
    fn canonical_encoding_frames_strings_and_properties_unambiguously() {
        let store = SnapshotContents::default();
        let mut first = snapshot();
        first.blocks[0].properties = [("a".into(), "bc".into())].into();
        let mut second = first.clone();
        second.blocks[0].properties = [("ab".into(), "c".into())].into();
        assert_ne!(
            store.intern(first).unwrap().id(),
            store.intern(second).unwrap().id()
        );
    }
}
