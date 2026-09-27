//! Lossless Java states for construction commands and expected observations.
use crate::minecraft_export::{JavaExportConfig, java_block_state};
use crate::snapshot::{MinecraftSnapshot, MinecraftSnapshotBlock, assembly_from_snapshot};
use dustroute_library::blueprint::BlueprintCatalog;
use dustroute_minecraft::piston_electrical::validate_evidence;
use dustroute_minecraft::{Block, BlockKind, Region, World};
use std::collections::BTreeMap;

/// Exact Java state export for admitted electrical identities. Historical
/// generic export deliberately initializes some devices OFF; this path must
/// preserve their actual strength, lock and powered fields instead.
pub fn electrical_snapshot(world: &World, region: Region) -> Result<MinecraftSnapshot, String> {
    let mut blocks = Vec::new();
    for (pos, block) in world.iter() {
        if !region.contains(*pos) {
            return Err("block outside electrical snapshot region".into());
        }
        validate_evidence(block).map_err(|e| e.to_string())?;
        if block.kind == BlockKind::Air {
            continue;
        }
        let encoded =
            java_block_state(block, &JavaExportConfig::default()).map_err(|e| e.to_string())?;
        let (name, properties) = encoded
            .split_once('[')
            .map_or((encoded.as_str(), ""), |(n, p)| {
                (n, p.trim_end_matches(']'))
            });
        let mut name = name.to_string();
        let mut properties: BTreeMap<String, String> = properties
            .split(',')
            .filter(|s| !s.is_empty())
            .map(|s| {
                let (k, v) = s.split_once('=').expect("typed exporter state");
                (k.into(), v.into())
            })
            .collect();
        match block.kind {
            BlockKind::Solid | BlockKind::Transparent => {
                if let Some(observed) = &block.observed_name {
                    name = format!(
                        "minecraft:{}",
                        observed.strip_prefix("minecraft:").unwrap_or(observed)
                    );
                }
            }
            BlockKind::RedstoneWire => {
                properties.insert(
                    "power".into(),
                    block.power_level.expect("validated level").to_string(),
                );
            }
            BlockKind::Repeater => {
                properties.insert(
                    "powered".into(),
                    block.powered.expect("validated power").to_string(),
                );
                properties.insert(
                    "locked".into(),
                    block
                        .observed_properties
                        .get("locked")
                        .cloned()
                        .unwrap_or_else(|| "false".into()),
                );
            }
            _ => {}
        }
        if block.observed_name.is_some() && properties != block.observed_properties {
            return Err(format!(
                "lossless electrical export requires exact supported properties at {pos:?}"
            ));
        }
        blocks.push(MinecraftSnapshotBlock {
            pos: *pos,
            name,
            properties,
        });
    }
    Ok(MinecraftSnapshot {
        min: region.min,
        max: region.max,
        blocks,
    })
}

pub(super) fn literal_world(snapshot: &MinecraftSnapshot) -> Result<World, String> {
    let region = Region::new(snapshot.min, snapshot.max);
    Ok(
        assembly_from_snapshot(snapshot, "construction state", vec![region])
            .map_err(|e| e.to_string())?
            .inspect(&BlueprintCatalog::default())
            .map_err(|e| e.to_string())?
            .proposed_world(),
    )
}

pub(super) fn snapshot_state(block: &MinecraftSnapshotBlock) -> String {
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
pub(super) fn initialization_request(
    mut block: Block,
    mut requested: MinecraftSnapshotBlock,
) -> (Block, String) {
    // Explicit command initialization, visible in the placement plan:
    // Java preprocesses an unpowered observer into a queued pulse even
    // with its watched cell already present. A powered requested state
    // instead resets OFF in onBlockAdded (flags 18), without scheduling
    // that pulse. Simulate this actual request and verify the complete
    // settled world. Never alter the declared Assembly or use strict
    // writes; later changes to watched cells can still trigger pulses.
    if let Some(program) = dustroute_minecraft::device_program::program(block.kind)
        && let Some(initialization) = &program.definition().command_initialization
        && block.powered == Some(initialization.declared_powered)
    {
        let property = &program.definition().power_property;
        let powered = initialization.requested_powered;
        block.powered = Some(powered);
        block
            .observed_properties
            .insert(property.clone(), powered.to_string());
        requested
            .properties
            .insert(property.clone(), powered.to_string());
    }
    (block, snapshot_state(&requested))
}
