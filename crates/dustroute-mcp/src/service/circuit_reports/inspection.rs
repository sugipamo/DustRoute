//! Literal world inventory and scan-boundary presentation.
use super::super::circuit_capture::{is_redstone_candidate_name, is_supported_redstone_name};
use crate::operations::mutation::Success;
use dustroute_physical::{BlockKind, Pos};
use dustroute_translate::snapshot::MinecraftSnapshotBlock;
use dustroute_translate::world_reverse::RegionBounds;
use serde::Serialize;
use std::collections::{BTreeMap, BTreeSet};

/// Literal diagnostic records only; no Deserialize or fresh-region constructor.
#[derive(Clone, Debug, PartialEq, Serialize)]
pub(crate) struct RawWorldInspection {
    ok: Success,
    mode: &'static str,
    inference_applied: bool,
    dimension: String,
    target: Option<Pos>,
    block_list_order_origin: Pos,
    target_block: Option<MinecraftSnapshotBlock>,
    pub scan: InspectionScan,
    counts: InspectionCounts,
    pub boundary: InspectionBoundary,
    redstone_blocks: Vec<MinecraftSnapshotBlock>,
    redstone_blocks_truncated: bool,
    blocks: Option<Vec<MinecraftSnapshotBlock>>,
    blocks_truncated: bool,
}
#[derive(Clone, Debug, PartialEq, Serialize)]
pub(crate) struct InspectionScan {
    requested_and_returned_bounds: RegionBounds,
    size: Pos,
    volume: i64,
    pub complete: bool,
    pub completeness_basis: &'static str,
    chunk_columns_with_non_air_blocks: usize,
}
#[derive(Clone, Debug, PartialEq, Serialize)]
struct InspectionCounts {
    air: usize,
    non_air: usize,
    redstone_candidates: usize,
    modeled_redstone: usize,
    unmodeled_redstone_candidates: usize,
    by_block_name: BTreeMap<String, usize>,
    state_properties_present: BTreeMap<String, usize>,
}
#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(untagged)]
pub(crate) enum InspectionBoundary {
    Raw {
        non_air_blocks: usize,
        redstone_candidates: usize,
        redstone_touches_boundary: bool,
        guidance: Option<&'static str>,
    },
    ComponentFrontier {
        component_frontier_remaining: bool,
        redstone_touches_boundary: bool,
        guidance: Option<&'static str>,
    },
}
#[derive(Debug, Serialize)]
pub(crate) struct CapturedWorldInspection {
    #[serde(flatten)]
    pub inspection: RawWorldInspection,
    pub circuit_id: uuid::Uuid,
    pub content_id: crate::snapshot_content::ContentId,
    pub observation_id: crate::snapshot_content::ObservationId,
    pub circuit_expires_in_seconds: u64,
    pub player_observation: crate::bridge::PlayerContext,
    pub observation_capabilities: crate::bridge::ObservationCapabilities,
    pub readback: crate::observation_evidence::ObservationEvidence,
    pub expansion: super::super::circuit_capture::ExpansionEvidence,
    pub mutation_authorized: bool,
}
#[derive(Debug, Serialize)]
pub(crate) struct GazeWorldInspection {
    #[serde(flatten)]
    pub inspection: RawWorldInspection,
    pub player_observation: crate::bridge::PlayerObservation,
    pub expansion: GazeExpansion,
}
#[derive(Debug, Serialize)]
pub(crate) struct GazeExpansion {
    pub strategy: &'static str,
    pub component_gap: u32,
    pub components_loaded: usize,
    pub component_limit: usize,
    pub limit_reached: bool,
    pub complete: bool,
    pub scanned_tiles: usize,
    pub scanned_block_positions: usize,
    pub guidance: Option<&'static str>,
}
/// Context of a failed observation, with absent fields distinct from false.
#[derive(Debug, Serialize)]
pub(crate) struct InspectionFailure {
    #[serde(flatten)]
    pub refusal: crate::operations::mutation::UnrecordedFailure,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub observation: Option<crate::bridge::PlayerObservation>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub scan_complete: Option<bool>,
}

fn position_on_boundary(position: Pos, min: Pos, max: Pos) -> bool {
    position.x == min.x
        || position.x == max.x
        || position.y == min.y
        || position.y == max.y
        || position.z == min.z
        || position.z == max.z
}

pub(in super::super) fn raw_world_inspection(
    snapshot: &dustroute_translate::snapshot::MinecraftSnapshot,
    target: impl Into<Option<Pos>>,
    dimension: &str,
    include_block_list: bool,
    max_listed_blocks: usize,
) -> RawWorldInspection {
    let target = target.into();
    let list_origin = target.unwrap_or(snapshot.min);
    let size = Pos::new(
        snapshot.max.x - snapshot.min.x + 1,
        snapshot.max.y - snapshot.min.y + 1,
        snapshot.max.z - snapshot.min.z + 1,
    );
    let volume = i64::from(size.x) * i64::from(size.y) * i64::from(size.z);
    // Native clients may return every air cell; other bridges omit them. Keep
    // the non-air inventory independent of that transport representation.
    let non_air_blocks = snapshot
        .blocks
        .iter()
        .filter(|block| {
            dustroute_translate::world::physical::classify(&block.name).0 != BlockKind::Air
        })
        .collect::<Vec<_>>();
    let mut counts = BTreeMap::<String, usize>::new();
    let mut chunks = BTreeSet::new();
    let mut redstone = Vec::new();
    let mut modeled_redstone_count = 0_usize;
    let mut boundary_non_air_count = 0_usize;
    let mut boundary_redstone_count = 0_usize;
    let mut state_property_counts = BTreeMap::<String, usize>::new();
    let mut target_block = None;
    for block in non_air_blocks.iter().copied() {
        *counts.entry(block.name.clone()).or_default() += 1;
        chunks.insert((block.pos.x.div_euclid(16), block.pos.z.div_euclid(16)));
        if position_on_boundary(block.pos, snapshot.min, snapshot.max) {
            boundary_non_air_count += 1;
        }
        for property in block.properties.keys() {
            *state_property_counts.entry(property.clone()).or_default() += 1;
        }
        if Some(block.pos) == target {
            target_block = Some(block);
        }
        if is_redstone_candidate_name(&block.name) {
            if is_supported_redstone_name(&block.name) {
                modeled_redstone_count += 1;
            }
            if position_on_boundary(block.pos, snapshot.min, snapshot.max) {
                boundary_redstone_count += 1;
            }
            redstone.push(block);
        }
    }
    redstone.sort_by_key(|block| {
        (
            block.pos.x.abs_diff(list_origin.x)
                + block.pos.y.abs_diff(list_origin.y)
                + block.pos.z.abs_diff(list_origin.z),
            block.pos,
        )
    });
    let listed_redstone = redstone.iter().take(max_listed_blocks).collect::<Vec<_>>();
    let listed_blocks = include_block_list.then(|| {
        non_air_blocks
            .iter()
            .take(max_listed_blocks)
            .collect::<Vec<_>>()
    });
    let non_air_count = non_air_blocks.len();
    let air_count = usize::try_from(volume)
        .unwrap_or(usize::MAX)
        .saturating_sub(non_air_count);
    RawWorldInspection {
        ok: Success,
        mode: "raw_world_inspection",
        inference_applied: false,
        dimension: dimension.into(),
        target,
        block_list_order_origin: list_origin,
        target_block: target_block.cloned(),
        scan: InspectionScan {
            requested_and_returned_bounds: RegionBounds { min: snapshot.min, max: snapshot.max },
            size,
            volume,
            complete: true,
            completeness_basis: "the bridge rejects the entire scan if any coordinate is unavailable",
            chunk_columns_with_non_air_blocks: chunks.len(),
        },
        counts: InspectionCounts {
            air: air_count,
            non_air: non_air_count,
            redstone_candidates: redstone.len(),
            modeled_redstone: modeled_redstone_count,
            unmodeled_redstone_candidates: redstone.len().saturating_sub(modeled_redstone_count),
            by_block_name: counts,
            state_properties_present: state_property_counts,
        },
        boundary: InspectionBoundary::Raw {
            non_air_blocks: boundary_non_air_count,
            redstone_candidates: boundary_redstone_count,
            redstone_touches_boundary: boundary_redstone_count > 0,
            guidance: (boundary_redstone_count > 0).then_some(
                "redstone reaches the raw scan boundary; increase the radius before assuming the circuit is complete"
            ),
        },
        redstone_blocks: listed_redstone.into_iter().map(|block| (**block).clone()).collect(),
        redstone_blocks_truncated: redstone.len() > max_listed_blocks,
        blocks: listed_blocks.map(|blocks| blocks.into_iter().map(|block| (**block).clone()).collect()),
        blocks_truncated: include_block_list && non_air_count > max_listed_blocks,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use dustroute_translate::snapshot::{MinecraftSnapshot, MinecraftSnapshotBlock};
    use serde_json::json;

    fn block(pos: Pos, name: &str) -> MinecraftSnapshotBlock {
        MinecraftSnapshotBlock {
            pos,
            name: name.into(),
            properties: BTreeMap::new(),
        }
    }

    #[test]
    fn raw_inspection_matches_for_sparse_and_explicit_air_snapshots() {
        let target = Pos::new(15, 0, 1);
        let mut observer = block(target, "minecraft:observer");
        observer.properties = BTreeMap::from([
            ("facing".into(), "south".into()),
            ("powered".into(), "false".into()),
        ]);
        let sparse = MinecraftSnapshot {
            min: Pos::new(15, 0, 0),
            max: Pos::new(16, 1, 1),
            blocks: vec![
                block(Pos::new(15, 0, 0), "minecraft:slime_block"),
                block(Pos::new(15, 1, 0), "minecraft:glass"),
                observer,
            ],
        };
        let mut dense = sparse.clone();
        dense.blocks = vec![
            block(Pos::new(16, 0, 0), "minecraft:air"),
            block(Pos::new(16, 0, 1), "minecraft:cave_air"),
            block(Pos::new(16, 1, 0), "minecraft:void_air"),
            block(Pos::new(16, 1, 1), "minecraft:air"),
            block(Pos::new(15, 1, 1), "minecraft:air"),
        ];
        dense.blocks.extend(sparse.blocks.clone());
        let inspect =
            |snapshot| raw_world_inspection(snapshot, target, "minecraft:overworld", true, 2);
        let result = inspect(&dense);
        assert_eq!(result, inspect(&sparse));
        let result = serde_json::to_value(result).unwrap();
        assert_eq!(result["counts"]["non_air"], 3);
        assert_eq!(result["counts"]["air"], 5);
        assert_eq!(result["scan"]["chunk_columns_with_non_air_blocks"], 1);
        assert_eq!(result["boundary"]["non_air_blocks"], 3);
        assert_eq!(result["target_block"]["properties"]["facing"], "south");
        assert_eq!(
            result["redstone_blocks"][0]["properties"]["powered"],
            "false"
        );
        assert_eq!(result["blocks"].as_array().unwrap().len(), 2);
        assert_eq!(result["blocks"][0]["name"], "minecraft:slime_block");
        assert_eq!(result["blocks_truncated"], true);
        assert_eq!(result["redstone_blocks_truncated"], false);
    }

    #[test]
    fn raw_inspection_excludes_all_native_air_variants() {
        let snapshot = MinecraftSnapshot {
            min: Pos::new(0, 0, 0),
            max: Pos::new(5, 0, 0),
            blocks: ["air", "cave_air", "void_air"]
                .into_iter()
                .flat_map(|name| [name.to_owned(), format!("minecraft:{name}")])
                .enumerate()
                .map(|(x, name)| block(Pos::new(x as i32, 0, 0), &name))
                .collect(),
        };
        let result =
            raw_world_inspection(&snapshot, Pos::new(0, 0, 0), "minecraft:overworld", true, 1);
        let result = serde_json::to_value(result).unwrap();
        assert_eq!(result["counts"]["non_air"], 0);
        assert_eq!(result["counts"]["air"], 6);
        assert_eq!(result["counts"]["by_block_name"], json!({}));
        assert_eq!(result["scan"]["chunk_columns_with_non_air_blocks"], 0);
        assert_eq!(result["boundary"]["non_air_blocks"], 0);
        assert_eq!(result["blocks"], json!([]));
        assert_eq!(result["blocks_truncated"], false);
    }

    #[test]
    fn raw_inspection_keeps_unknown_names_and_ignores_air_at_boundary() {
        let target = Pos::new(1, 1, 1);
        let snapshot = MinecraftSnapshot {
            min: Pos::new(0, 0, 0),
            max: Pos::new(2, 2, 2),
            blocks: vec![
                block(Pos::new(0, 0, 0), "minecraft:air"),
                block(Pos::new(0, 0, 1), "minecraft:cave_air"),
                block(target, "example:air"),
            ],
        };
        let result = raw_world_inspection(&snapshot, target, "minecraft:overworld", true, 1);
        let result = serde_json::to_value(result).unwrap();
        assert_eq!(result["counts"]["non_air"], 1);
        assert_eq!(result["counts"]["air"], 26);
        assert_eq!(result["boundary"]["non_air_blocks"], 0);
        assert_eq!(result["blocks"][0]["name"], "example:air");
        assert_eq!(result["blocks_truncated"], false);
    }
}
