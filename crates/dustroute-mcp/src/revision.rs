//! Immutable hypothetical snapshots, never live-world evidence or write plans.
use dustroute_library::assembly::AssemblyRevision;
use dustroute_library::blueprint::AssemblyRevisionId;
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
    /// A modeled view of the literal snapshot. Older records and undecodable
    /// block states have no assembly; the literal snapshot remains authoritative.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub assembly: Option<AssemblyRevision>,
}

pub fn assembly_id(id: Uuid) -> AssemblyRevisionId {
    AssemblyRevisionId::new(format!("dustroute.assembly.{id}")).expect("UUID forms a valid ID")
}

impl CircuitRevision {
    /// Retains pinned interpretations from a modeled parent without altering any
    /// source definition. Changed states must be validated again by the caller.
    pub fn capture_assembly(
        &self,
        parent: Option<&AssemblyRevision>,
    ) -> Result<AssemblyRevision, String> {
        // scan_region fails if any chunk is unavailable. Its whole returned
        // cuboid is known, even when discovery did not capture the whole circuit.
        let mut assembly = dustroute_translate::snapshot::assembly_from_snapshot(
            &self.snapshot,
            "Hypothetical circuit state",
            vec![dustroute_translate::Region::new(
                self.snapshot.min,
                self.snapshot.max,
            )],
        )
        .map_err(|error| error.to_string())?;
        if let Some(parent) = parent {
            assembly.instances = parent.assembly.instances.clone();
            assembly.connections = parent.assembly.connections.clone();
            assembly.boundaries = parent.assembly.boundaries.clone();
        }
        assembly
            .inspect(dustroute_library::builtin_blueprints::builtin_blueprints())
            .map_err(|error| error.to_string())?;
        Ok(AssemblyRevision {
            id: assembly_id(self.revision_id),
            parents: parent.into_iter().map(|parent| parent.id.clone()).collect(),
            assembly,
        })
    }

    /// Loading a sidecar never restores proof. Reject disagreement with the
    /// literal snapshot or record identity before exposing its modeled view.
    pub fn check_assembly_record(&self) -> Result<(), String> {
        let Some(record) = &self.assembly else {
            return Ok(());
        };
        if record.id != assembly_id(self.revision_id)
            || record.parents.len() > 1
            || record.parents.iter().any(|parent| {
                !self
                    .parent_revision_ids
                    .iter()
                    .any(|id| &assembly_id(*id) == parent)
            })
        {
            return Err("assembly identity or ancestry mismatch".into());
        }
        let expected = self.capture_assembly(Some(record))?;
        if record.assembly != expected.assembly {
            return Err("assembly actual state disagrees with the literal snapshot".into());
        }
        Ok(())
    }
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
    let mut blocks = dustroute_translate::snapshot::index_literal_snapshot(snapshot)?;
    let inside = |p: Pos| {
        p.x >= snapshot.min.x
            && p.x <= snapshot.max.x
            && p.y >= snapshot.min.y
            && p.y <= snapshot.max.y
            && p.z >= snapshot.min.z
            && p.z <= snapshot.max.z
    };
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
    fn modeled_state_retains_pins_but_never_restores_source_blocks_or_rebinds_ids() {
        use dustroute_library::blueprint::{BlueprintInclusion, BlueprintRevisionId, InstanceId};
        use dustroute_library::builtin_blueprints::{NOT_TOP_REVISION, builtin_blueprints};
        let catalog_before = builtin_blueprints().to_json().unwrap();
        let mut original = CircuitRevision {
            schema_version: "dustroute.circuit-revision.v1".into(),
            revision_id: Uuid::new_v4(),
            parent_revision_ids: vec![],
            base_observation_id: Uuid::new_v4(),
            player: "builder".into(),
            dimension: "minecraft:overworld".into(),
            target: None,
            complete: false,
            snapshot: MinecraftSnapshot {
                min: Pos::default(),
                max: Pos::new(4, 2, 2),
                blocks: vec![block(Pos::default(), "minecraft:stone")],
            },
            base_snapshot: None,
            changes: vec![],
            validation: serde_json::json!({}),
            assembly: None,
        };
        let mut state = original.capture_assembly(None).unwrap();
        state.assembly.instances.push(BlueprintInclusion {
            instance: InstanceId::new("claimed-not").unwrap(),
            revision: BlueprintRevisionId::new(NOT_TOP_REVISION).unwrap(),
            origin: Pos::default(),
            rotation: Default::default(),
        });
        original.assembly = Some(state.clone());
        original.check_assembly_record().unwrap();
        let mut child = original.clone();
        child.revision_id = Uuid::new_v4();
        child.parent_revision_ids = vec![original.revision_id];
        (child.snapshot, child.changes) = apply(
            &original.snapshot,
            vec![block(Pos::default(), "minecraft:air")],
        )
        .unwrap();
        child.assembly = Some(child.capture_assembly(original.assembly.as_ref()).unwrap());
        child.check_assembly_record().unwrap();
        let current = child.assembly.as_ref().unwrap().clone();
        assert_ne!(state.id, current.id);
        assert_eq!(current.parents, vec![state.id.clone()]);
        assert_eq!(current.assembly.instances, state.assembly.instances);
        assert!(current.assembly.blocks.is_empty());
        let view = current.assembly.inspect(builtin_blueprints()).unwrap();
        assert_eq!(
            view.block_at(Pos::default()).unwrap().kind,
            dustroute_translate::BlockKind::Air
        );
        assert!(!view.source_differences().is_empty());
        assert_eq!(original.assembly.as_ref(), Some(&state));
        assert_eq!(builtin_blueprints().to_json().unwrap(), catalog_before);
        let restored: CircuitRevision =
            serde_json::from_slice(&serde_json::to_vec(&child).unwrap()).unwrap();
        restored.check_assembly_record().unwrap();
        child.assembly.as_mut().unwrap().assembly.blocks = state.assembly.blocks.clone();
        assert!(child.check_assembly_record().is_err());
        child.assembly = Some(current.clone());
        child.assembly.as_mut().unwrap().id = state.id;
        assert!(child.check_assembly_record().is_err());
        // Existing records can be read without inventing an assembly history.
        let mut legacy = serde_json::to_value(&original).unwrap();
        legacy.as_object_mut().unwrap().remove("assembly");
        let legacy: CircuitRevision = serde_json::from_value(legacy).unwrap();
        assert!(legacy.assembly.is_none());
        legacy.check_assembly_record().unwrap();
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
