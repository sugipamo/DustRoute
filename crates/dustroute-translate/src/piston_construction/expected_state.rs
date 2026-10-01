//! Immutable model expectations. Shared bases and bounded delta chains retain
//! states; full snapshots are materialized explicitly at serialization/readback.
use crate::snapshot::{LiteralSnapshotIndex, MinecraftSnapshot, MinecraftSnapshotBlock};
use dustroute_minecraft::{Pos, Region};
use serde::{Deserialize, Deserializer, Serialize, Serializer};
use std::collections::{BTreeMap, BTreeSet};
use std::sync::Arc;

const MAX_DELTA_DEPTH: usize = 32;
#[derive(Clone)]
pub struct ExpectedState(Arc<Node>);
struct Node {
    bounds: Region,
    count: usize,
    depth: usize,
    storage: Storage,
}
enum Storage {
    Full(MinecraftSnapshot),
    Delta {
        parent: ExpectedState,
        changes: Vec<MinecraftSnapshotBlock>,
    },
}
impl std::fmt::Debug for ExpectedState {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ExpectedState")
            .field("bounds", &self.bounds())
            .field("non_air_blocks", &self.len())
            .field("delta_depth", &self.0.depth)
            .finish()
    }
}
impl ExpectedState {
    fn full(snapshot: MinecraftSnapshot) -> Self {
        Self(Arc::new(Node {
            bounds: Region {
                min: snapshot.min,
                max: snapshot.max,
            },
            count: snapshot.blocks.len(),
            depth: 0,
            storage: Storage::Full(snapshot),
        }))
    }
    pub fn bounds(&self) -> Region {
        self.0.bounds
    }
    pub fn len(&self) -> usize {
        self.0.count
    }
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }
    pub fn get(&self, pos: Pos) -> Option<&MinecraftSnapshotBlock> {
        let mut state = self;
        loop {
            match &state.0.storage {
                Storage::Full(snapshot) => return snapshot.blocks.iter().find(|b| b.pos == pos),
                Storage::Delta { parent, changes } => {
                    if let Some(b) = changes.iter().find(|b| b.pos == pos) {
                        return (b.name != "minecraft:air").then_some(b);
                    }
                    state = parent;
                }
            }
        }
    }
    pub fn materialize(&self) -> MinecraftSnapshot {
        let mut deltas = vec![];
        let mut state = self;
        let base = loop {
            match &state.0.storage {
                Storage::Full(snapshot) => break snapshot,
                Storage::Delta { parent, changes } => {
                    deltas.push(changes);
                    state = parent;
                }
            }
        };
        if deltas.is_empty() {
            return base.clone();
        }
        let mut blocks = base
            .blocks
            .iter()
            .map(|b| (b.pos, b))
            .collect::<BTreeMap<_, _>>();
        for changes in deltas.into_iter().rev() {
            for b in changes {
                if b.name == "minecraft:air" {
                    blocks.remove(&b.pos);
                } else {
                    blocks.insert(b.pos, b);
                }
            }
        }
        MinecraftSnapshot {
            min: base.min,
            max: base.max,
            blocks: blocks.into_values().cloned().collect(),
        }
    }
    fn into_snapshot(self) -> MinecraftSnapshot {
        match Arc::try_unwrap(self.0) {
            Ok(Node {
                storage: Storage::Full(snapshot),
                ..
            }) => snapshot,
            Ok(node) => Self(Arc::new(node)).materialize(),
            Err(node) => Self(node).materialize(),
        }
    }
    /// Count shared allocations once. Pointer identity is used only for memory
    /// accounting, never state equality, live readiness or execution authority.
    pub fn retained_usage<'a>(states: impl IntoIterator<Item = &'a Self>) -> ExpectedStateUsage {
        let mut seen = BTreeSet::new();
        let mut usage = ExpectedStateUsage::default();
        for mut state in states {
            loop {
                if !seen.insert(Arc::as_ptr(&state.0) as usize) {
                    break;
                }
                usage.estimated_bytes += std::mem::size_of::<Node>() + 32;
                let blocks = match &state.0.storage {
                    Storage::Full(s) => &s.blocks,
                    Storage::Delta { changes, .. } => changes,
                };
                usage.block_records += blocks.len();
                usage.estimated_bytes += estimated_block_bytes(blocks);
                match &state.0.storage {
                    Storage::Full(_) => break,
                    Storage::Delta { parent, .. } => state = parent,
                }
            }
        }
        usage
    }
}
#[derive(Clone, Copy, Debug, Default)]
pub struct ExpectedStateUsage {
    pub block_records: usize,
    pub estimated_bytes: usize,
}
/// Conservative allocation estimate, not RSS. Small property maps allocate a whole node.
pub fn estimated_block_bytes(blocks: &Vec<MinecraftSnapshotBlock>) -> usize {
    blocks.capacity() * std::mem::size_of::<MinecraftSnapshotBlock>()
        + blocks
            .iter()
            .map(|b| {
                b.name.capacity()
                    + usize::from(!b.properties.is_empty()) * 1024
                    + b.properties
                        .iter()
                        .map(|(k, v)| 192 + k.capacity() + v.capacity())
                        .sum::<usize>()
            })
            .sum::<usize>()
}
impl PartialEq for ExpectedState {
    fn eq(&self, other: &Self) -> bool {
        Arc::ptr_eq(&self.0, &other.0)
            || (self.bounds() == other.bounds()
                && self.len() == other.len()
                && self.materialize() == other.materialize())
    }
}
impl Eq for ExpectedState {}
impl PartialEq<MinecraftSnapshot> for ExpectedState {
    fn eq(&self, other: &MinecraftSnapshot) -> bool {
        self.materialize() == *other
    }
}
impl PartialEq<ExpectedState> for MinecraftSnapshot {
    fn eq(&self, other: &ExpectedState) -> bool {
        other == self
    }
}
impl Serialize for ExpectedState {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        self.materialize().serialize(serializer)
    }
}
impl<'de> Deserialize<'de> for ExpectedState {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        Ok(Self::full(MinecraftSnapshot::deserialize(d)?))
    }
}

/// One sequence's immutable expectations; the last full snapshot is transient
/// scratch space and is dropped when planning finishes.
#[derive(Default)]
pub(super) struct ExpectedStates {
    previous: Option<(ExpectedState, MinecraftSnapshot)>,
}
impl ExpectedStates {
    pub(super) fn capture(&mut self, snapshot: MinecraftSnapshot) -> Result<ExpectedState, String> {
        let next = if let Some((parent, last)) = &self.previous {
            if snapshot.min != last.min || snapshot.max != last.max {
                return Err("expected-state context bounds changed".into());
            }
            let changes = differences(last, &snapshot);
            if changes.is_empty() {
                parent.clone()
            } else if parent.0.depth >= MAX_DELTA_DEPTH {
                ExpectedState::full(snapshot.clone())
            } else {
                ExpectedState(Arc::new(Node {
                    bounds: parent.bounds(),
                    count: snapshot.blocks.len(),
                    depth: parent.0.depth + 1,
                    storage: Storage::Delta {
                        parent: parent.clone(),
                        changes,
                    },
                }))
            }
        } else {
            ExpectedState::full(snapshot.clone())
        };
        self.previous = Some((next.clone(), snapshot));
        Ok(next)
    }
}
fn differences(
    before: &MinecraftSnapshot,
    after: &MinecraftSnapshot,
) -> Vec<MinecraftSnapshotBlock> {
    let mut old = before.blocks.iter().peekable();
    let mut new = after.blocks.iter().peekable();
    let mut changes = vec![];
    loop {
        match (old.peek(), new.peek()) {
            (Some(a), Some(b)) if a.pos == b.pos => {
                if a != b {
                    changes.push((*b).clone());
                }
                old.next();
                new.next();
            }
            (Some(a), b) if b.is_none_or(|b| a.pos < b.pos) => {
                changes.push(MinecraftSnapshotBlock {
                    pos: a.pos,
                    name: "minecraft:air".into(),
                    properties: BTreeMap::new(),
                });
                old.next();
            }
            (_, Some(b)) => {
                changes.push((*b).clone());
                new.next();
            }
            (None, None) => break,
            _ => unreachable!("coordinate merge"),
        }
    }
    changes
}
/// Saved steps retain their existing full JSON shape. Streaming decoding
/// compacts each historical state without restoring the private batching proof.
pub fn deserialize_steps<'de, D: Deserializer<'de>>(
    d: D,
) -> Result<Vec<super::ElectricalConstructionStep>, D::Error> {
    struct Steps;
    impl<'de> serde::de::Visitor<'de> for Steps {
        type Value = Vec<super::ElectricalConstructionStep>;
        fn expecting(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            f.write_str("historical construction steps")
        }
        fn visit_seq<A: serde::de::SeqAccess<'de>>(
            self,
            mut seq: A,
        ) -> Result<Self::Value, A::Error> {
            let mut states = ExpectedStates::default();
            let mut result = vec![];
            while let Some(mut step) = seq.next_element::<super::ElectricalConstructionStep>()? {
                let snapshot = step.expected.into_snapshot();
                let snapshot = LiteralSnapshotIndex::new(&snapshot)
                    .map_err(serde::de::Error::custom)?
                    .canonical_snapshot();
                step.expected = states.capture(snapshot).map_err(serde::de::Error::custom)?;
                result.push(step);
            }
            Ok(result)
        }
    }
    d.deserialize_seq(Steps)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::piston_construction::{ElectricalConstructionStep, construction_batches};
    fn baseline() -> MinecraftSnapshot {
        MinecraftSnapshot {
            min: Pos::default(),
            max: Pos::new(255, 0, 0),
            blocks: (0..256)
                .map(|x| MinecraftSnapshotBlock {
                    pos: Pos::new(x, 0, 0),
                    name: "minecraft:stone".into(),
                    properties: BTreeMap::new(),
                })
                .collect(),
        }
    }
    #[test]
    fn bounded_shared_states_preserve_properties_deletions_restoration_and_all_prefixes() {
        let mut full = baseline();
        let mut builder = ExpectedStates::default();
        let first = builder.capture(full.clone()).unwrap();
        let mut states = vec![first.clone()];
        for i in 0..70 {
            let p = Pos::new(i % 8, 0, 0);
            full.blocks.retain(|b| b.pos != p);
            if i % 3 != 0 {
                full.blocks.push(MinecraftSnapshotBlock {
                    pos: p,
                    name: "minecraft:glass".into(),
                    properties: BTreeMap::from([("test_property".into(), i.to_string())]),
                });
            }
            full.blocks.sort_by_key(|b| b.pos);
            let state = builder.capture(full.clone()).unwrap();
            assert_eq!(state.materialize(), full);
            assert_eq!(state.get(p), full.blocks.iter().find(|b| b.pos == p));
            assert_eq!(state.len(), full.blocks.len());
            assert!(state.0.depth <= MAX_DELTA_DEPTH);
            states.push(state);
        }
        assert_eq!(first.materialize(), baseline());
        let actual = ExpectedState::retained_usage(&states);
        assert!(actual.block_records < states.iter().map(ExpectedState::len).sum::<usize>() / 4);
        let restored = builder.capture(baseline()).unwrap();
        assert_eq!(restored.materialize(), baseline());
        let mut wrong = baseline();
        wrong.max.x += 1;
        assert!(builder.capture(wrong).is_err());
    }
    #[test]
    fn saved_json_stays_full_but_streamed_history_shares_states_and_revokes_batching() {
        let mut builder = ExpectedStates::default();
        let steps = (0..4)
            .map(|x| ElectricalConstructionStep {
                position: Pos::new(x, 0, 0),
                state: "minecraft:stone".into(),
                wait_ticks: 4,
                expected: builder.capture(baseline()).unwrap(),
                immediate_idle: true,
            })
            .collect::<Vec<_>>();
        assert_eq!(construction_batches(&steps).count(), 1);
        let saved = serde_json::json!({"steps":steps});
        assert_eq!(
            saved["steps"][0]["expected"],
            serde_json::to_value(baseline()).unwrap()
        );
        #[derive(Deserialize)]
        struct History {
            #[serde(deserialize_with = "deserialize_steps")]
            steps: Vec<ElectricalConstructionStep>,
        }
        let history: History = serde_json::from_value(saved).unwrap();
        assert_eq!(construction_batches(&history.steps).count(), 4);
        assert_eq!(
            ExpectedState::retained_usage(history.steps.iter().map(|s| &s.expected)).block_records,
            256
        );
        assert!(history.steps.iter().all(|s| s.expected == baseline()));
    }
}
