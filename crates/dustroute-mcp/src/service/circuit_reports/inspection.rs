//! Literal world inventory and scan-boundary presentation.
use super::super::circuit_capture::{is_redstone_candidate_name, is_supported_redstone_name};
use dustroute_physical::Pos;
use serde_json::{Value, json};
use std::collections::{BTreeMap, BTreeSet};

fn position_on_boundary(position: Pos, min: Pos, max: Pos) -> bool {
    position.x == min.x
        || position.x == max.x
        || position.y == min.y
        || position.y == max.y
        || position.z == min.z
        || position.z == max.z
}

pub(in super::super) fn raw_world_inspection(
    snapshot: &dustroute_translate::MinecraftSnapshot,
    target: Pos,
    dimension: &str,
    include_block_list: bool,
    max_listed_blocks: usize,
) -> Value {
    let size = Pos::new(
        snapshot.max.x - snapshot.min.x + 1,
        snapshot.max.y - snapshot.min.y + 1,
        snapshot.max.z - snapshot.min.z + 1,
    );
    let volume = i64::from(size.x) * i64::from(size.y) * i64::from(size.z);
    let mut counts = BTreeMap::<String, usize>::new();
    let mut chunks = BTreeSet::new();
    let mut redstone = Vec::new();
    let mut modeled_redstone_count = 0_usize;
    let mut boundary_non_air_count = 0_usize;
    let mut boundary_redstone_count = 0_usize;
    let mut state_property_counts = BTreeMap::<String, usize>::new();
    let mut target_block = None;
    for block in &snapshot.blocks {
        *counts.entry(block.name.clone()).or_default() += 1;
        chunks.insert((block.pos.x.div_euclid(16), block.pos.z.div_euclid(16)));
        if position_on_boundary(block.pos, snapshot.min, snapshot.max) {
            boundary_non_air_count += 1;
        }
        for property in block.properties.keys() {
            *state_property_counts.entry(property.clone()).or_default() += 1;
        }
        if block.pos == target {
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
            block.pos.x.abs_diff(target.x)
                + block.pos.y.abs_diff(target.y)
                + block.pos.z.abs_diff(target.z),
            block.pos,
        )
    });
    let listed_redstone = redstone.iter().take(max_listed_blocks).collect::<Vec<_>>();
    let listed_blocks = include_block_list.then(|| {
        snapshot
            .blocks
            .iter()
            .take(max_listed_blocks)
            .collect::<Vec<_>>()
    });
    let non_air_count = snapshot.blocks.len();
    let air_count = usize::try_from(volume)
        .unwrap_or(usize::MAX)
        .saturating_sub(non_air_count);
    json!({
        "ok": true,
        "mode": "raw_world_inspection",
        "inference_applied": false,
        "dimension": dimension,
        "target": target,
        "target_block": target_block,
        "scan": {
            "requested_and_returned_bounds": { "min": snapshot.min, "max": snapshot.max },
            "size": size,
            "volume": volume,
            "complete": true,
            "completeness_basis": "the bridge rejects the entire scan if any coordinate is unavailable",
            "chunk_columns_with_non_air_blocks": chunks.len()
        },
        "counts": {
            "air": air_count,
            "non_air": non_air_count,
            "redstone_candidates": redstone.len(),
            "modeled_redstone": modeled_redstone_count,
            "unmodeled_redstone_candidates": redstone.len().saturating_sub(modeled_redstone_count),
            "by_block_name": counts,
            "state_properties_present": state_property_counts
        },
        "boundary": {
            "non_air_blocks": boundary_non_air_count,
            "redstone_candidates": boundary_redstone_count,
            "redstone_touches_boundary": boundary_redstone_count > 0,
            "guidance": (boundary_redstone_count > 0).then_some(
                "redstone reaches the raw scan boundary; increase the radius before assuming the circuit is complete"
            )
        },
        "redstone_blocks": listed_redstone,
        "redstone_blocks_truncated": redstone.len() > max_listed_blocks,
        "blocks": listed_blocks,
        "blocks_truncated": include_block_list && non_air_count > max_listed_blocks
    })
}
