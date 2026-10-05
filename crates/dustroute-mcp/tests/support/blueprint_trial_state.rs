//! Native trial bookkeeping. Fixture recovery requests never restore execution authority.
use dustroute_library::world_edit::WorldEditScope;
use dustroute_physical::Pos;
#[cfg(feature = "voxrig")]
use dustroute_physical::Region;
use dustroute_translate::snapshot::MinecraftSnapshot;

#[allow(dead_code)]
pub struct WorkflowIds {
    pub scoped_operation: String,
    pub scope: WorldEditScope,
    pub instances: Vec<String>,
}

#[derive(serde::Serialize)]
#[cfg(feature = "voxrig")]
pub struct RegionJobIds {
    pub job_id: String,
    pub revision_id: String,
    pub regions: Vec<Region>,
}

/// Only decoded at the explicit test-fixture input boundary.
#[derive(serde::Deserialize)]
pub struct ExternalWrite {
    pub position: Pos,
    pub state: String,
}

pub fn non_air(snapshot: &MinecraftSnapshot) -> usize {
    snapshot
        .blocks
        .iter()
        .filter(|b| b.name != "minecraft:air")
        .count()
}

pub fn block_name(snapshot: &MinecraftSnapshot, pos: Pos) -> &str {
    snapshot
        .blocks
        .iter()
        .find(|b| b.pos == pos)
        .map(|b| b.name.as_str())
        .unwrap_or("minecraft:air")
}

pub fn verify_external_write(
    snapshot: &MinecraftSnapshot,
    expected: &ExternalWrite,
) -> anyhow::Result<()> {
    let block = snapshot
        .blocks
        .iter()
        .find(|b| b.pos == expected.position)
        .ok_or_else(|| {
            anyhow::anyhow!("owned external fixture is absent from the observed snapshot")
        })?;
    let properties = block
        .properties
        .iter()
        .map(|(k, v)| format!("{k}={v}"))
        .collect::<Vec<_>>()
        .join(",");
    let actual = if properties.is_empty() {
        block.name.clone()
    } else {
        format!("{}[{properties}]", block.name)
    };
    anyhow::ensure!(
        actual == expected.state,
        "owned external fixture changed before cleanup"
    );
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use dustroute_translate::snapshot::MinecraftSnapshotBlock;
    use std::collections::BTreeMap;
    #[test]
    fn native_observation_and_cleanup_require_the_exact_fixture_state() {
        let pos = Pos::new(1, 2, 3);
        let snapshot = MinecraftSnapshot {
            min: pos,
            max: pos,
            blocks: vec![MinecraftSnapshotBlock {
                pos,
                name: "minecraft:observer".into(),
                properties: BTreeMap::from([
                    ("powered".into(), "false".into()),
                    ("facing".into(), "east".into()),
                ]),
            }],
        };
        assert_eq!(non_air(&snapshot), 1);
        assert_eq!(block_name(&snapshot, pos), "minecraft:observer");
        verify_external_write(
            &snapshot,
            &ExternalWrite {
                position: pos,
                state: "minecraft:observer[facing=east,powered=false]".into(),
            },
        )
        .unwrap();
        assert!(
            verify_external_write(
                &snapshot,
                &ExternalWrite {
                    position: pos,
                    state: "minecraft:observer[facing=west,powered=false]".into()
                }
            )
            .is_err()
        );
        assert!(
            verify_external_write(
                &snapshot,
                &ExternalWrite {
                    position: Pos::default(),
                    state: "minecraft:air".into()
                }
            )
            .is_err()
        );
    }
}
