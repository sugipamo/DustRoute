//! Immutable hypothetical snapshots, never live-world evidence or write plans.
use dustroute_translate::{MinecraftSnapshot, MinecraftSnapshotBlock, Pos};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::{BTreeMap, BTreeSet};
use uuid::Uuid;

pub const MAX_BLOCKS: usize = 4096;
pub const MAX_BYTES: usize = 4 * 1024 * 1024;
#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct CircuitRevision {
    pub schema_version: String,
    pub revision_id: Uuid,
    pub parent_revision_ids: Vec<Uuid>,
    pub base_observation_id: Uuid,
    pub player: String,
    pub dimension: String,
    pub target: Option<Pos>,
    pub complete: bool,
    pub snapshot: MinecraftSnapshot,
    #[serde(default)]
    pub base_snapshot: Option<MinecraftSnapshot>,
    pub changes: Vec<RevisionChange>,
    pub validation: Value,
}
#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct RevisionChange {
    pub position: Pos,
    /// None is air, not an unknown/unobserved block.
    pub before: Option<MinecraftSnapshotBlock>,
    pub after: Option<MinecraftSnapshotBlock>,
}
pub fn apply(
    snapshot: &MinecraftSnapshot,
    edits: Vec<MinecraftSnapshotBlock>,
) -> Result<(MinecraftSnapshot, Vec<RevisionChange>), String> {
    if edits.len() > 64 || snapshot.blocks.len() > MAX_BLOCKS {
        return Err("revision limits: 64 edits and 4096 observed block records".into());
    }
    if snapshot.min.x > snapshot.max.x
        || snapshot.min.y > snapshot.max.y
        || snapshot.min.z > snapshot.max.z
    {
        return Err("invalid snapshot bounds".into());
    }
    let inside = |p: Pos| {
        p.x >= snapshot.min.x
            && p.x <= snapshot.max.x
            && p.y >= snapshot.min.y
            && p.y <= snapshot.max.y
            && p.z >= snapshot.min.z
            && p.z <= snapshot.max.z
    };
    let mut blocks = BTreeMap::new();
    let mut seen = BTreeSet::new();
    for b in &snapshot.blocks {
        if !inside(b.pos) || !seen.insert(b.pos) {
            return Err("snapshot has out-of-bounds or duplicate coordinates".into());
        }
        if b.name != "minecraft:air" {
            blocks.insert(b.pos, b.clone());
        }
    }
    let mut edited = BTreeSet::new();
    let mut changes = Vec::new();
    for edit in edits {
        if !inside(edit.pos) || !edited.insert(edit.pos) {
            return Err("edits must have unique positions within observed bounds".into());
        }
        let name = edit
            .name
            .strip_prefix("minecraft:")
            .ok_or("expected a minecraft block name")?;
        if name.is_empty()
            || name.len() > 128
            || !name
                .bytes()
                .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == b'_')
        {
            return Err("invalid block name".into());
        }
        if edit.properties.len() > 32
            || edit.properties.iter().any(|(k, v)| {
                k.is_empty()
                    || v.is_empty()
                    || k.len() > 64
                    || v.len() > 64
                    || !k.bytes().chain(v.bytes()).all(|c| {
                        c.is_ascii_lowercase() || c.is_ascii_digit() || c == b'_' || c == b'-'
                    })
            })
        {
            return Err("invalid block property syntax or size".into());
        }
        if edit.name == "minecraft:air" && !edit.properties.is_empty() {
            return Err("air deletion cannot carry properties".into());
        }
        let before = blocks.remove(&edit.pos);
        let after = if edit.name == "minecraft:air" {
            None
        } else {
            Some(edit.clone())
        };
        if let Some(b) = &after {
            blocks.insert(b.pos, b.clone());
        }
        if before != after {
            changes.push(RevisionChange {
                position: edit.pos,
                before,
                after,
            });
        }
    }
    if blocks.len() > MAX_BLOCKS {
        return Err("revision exceeds 4096 blocks".into());
    }
    let result = MinecraftSnapshot {
        min: snapshot.min,
        max: snapshot.max,
        blocks: blocks.into_values().collect(),
    };
    if serde_json::to_vec(&result)
        .map_err(|e| e.to_string())?
        .len()
        > MAX_BYTES
    {
        return Err("revision snapshot exceeds 4 MiB".into());
    }
    Ok((result, changes))
}

pub fn blocks(
    snapshot: &MinecraftSnapshot,
) -> Result<BTreeMap<Pos, MinecraftSnapshotBlock>, String> {
    let (normalized, _) = apply(snapshot, vec![])?;
    Ok(normalized.blocks.into_iter().map(|b| (b.pos, b)).collect())
}
pub fn state(block: &MinecraftSnapshotBlock) -> String {
    if block.properties.is_empty() {
        block.name.clone()
    } else {
        format!(
            "{}[{}]",
            block.name,
            block
                .properties
                .iter()
                .map(|(k, v)| format!("{k}={v}"))
                .collect::<Vec<_>>()
                .join(",")
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn block(pos: Pos, name: &str) -> MinecraftSnapshotBlock {
        MinecraftSnapshotBlock {
            pos,
            name: name.into(),
            properties: BTreeMap::new(),
        }
    }
    #[test]
    fn edits_are_atomic_and_preserve_source() {
        let p = Pos::new(0, 0, 0);
        let source = MinecraftSnapshot {
            min: p,
            max: Pos::new(2, 2, 2),
            blocks: vec![block(p, "minecraft:stone")],
        };
        let (added, diff) =
            apply(&source, vec![block(Pos::new(1, 0, 0), "minecraft:stone")]).unwrap();
        assert_eq!(source.blocks.len(), 1);
        assert_eq!(added.blocks.len(), 2);
        assert!(diff[0].before.is_none());
        assert!(
            apply(
                &source,
                vec![block(p, "minecraft:air"), block(p, "minecraft:stone")]
            )
            .is_err()
        );
        assert!(apply(&source, vec![block(Pos::new(3, 0, 0), "minecraft:stone")]).is_err());
        assert!(apply(&source, vec![block(p, "minecraft:")]).is_err());
        assert!(apply(&source, vec![block(p, "minecraft:stone[facing=up]")]).is_err());
        let mut duplicate = source.clone();
        duplicate.blocks.push(source.blocks[0].clone());
        assert!(apply(&duplicate, vec![]).is_err());
        let (deleted, diff) = apply(&source, vec![block(p, "minecraft:air")]).unwrap();
        assert!(deleted.blocks.is_empty());
        assert!(diff[0].after.is_none());
        assert_eq!(source.blocks[0].name, "minecraft:stone");
        assert!(apply(&source, vec![block(p, "minecraft:stone"); 65]).is_err());
    }
}
