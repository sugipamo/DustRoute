//! Explain a candidate that fails to reach its freshly reviewed world.
use super::{ElectricalConstructionStep, snapshot::snapshot_state};
use crate::snapshot::{MinecraftSnapshot, MinecraftSnapshotBlock};
use std::collections::{BTreeMap, BTreeSet};

pub(super) fn verify_constructed(
    constructed: &MinecraftSnapshot,
    settled: &MinecraftSnapshot,
    build: &[ElectricalConstructionStep],
) -> Result<(), String> {
    if constructed != settled {
        let expected: BTreeMap<_, _> = settled.blocks.iter().map(|b| (b.pos, b)).collect();
        let actual: BTreeMap<_, _> = constructed.blocks.iter().map(|b| (b.pos, b)).collect();
        let positions: BTreeSet<_> = expected.keys().chain(actual.keys()).copied().collect();
        let differences: Vec<_> = positions
            .into_iter()
            .filter(|pos| expected.get(pos) != actual.get(pos))
            .collect();
        let state = |block: Option<&&MinecraftSnapshotBlock>| {
            block.map_or_else(|| "minecraft:air".into(), |block| snapshot_state(block))
        };
        let detail = differences
            .iter()
            .take(16)
            .map(|pos| {
                // Completed placement snapshots already exist for readback.
                // Attribute the last settled change, not a causal callback.
                let last_change = build.iter().enumerate().rev().find(|(index, step)| {
                    let at = |step: &ElectricalConstructionStep| step.expected.get(*pos).cloned();
                    let previous = index.checked_sub(1).and_then(|i| at(&build[i]));
                    at(step) != previous
                });
                let boundary = last_change.map_or_else(String::new, |(index, step)| {
                    format!(
                        "; last settled change at build step {} installing {} at {:?}",
                        index + 1,
                        step.state,
                        step.position,
                    )
                });
                format!(
                    "{pos:?}: expected {}, actual {}{boundary}",
                    state(expected.get(pos)),
                    state(actual.get(pos)),
                )
            })
            .collect::<Vec<_>>()
            .join("; ");
        return Err(format!(
            "sequential construction does not reach the declared fresh-world result; {} differing cells (showing at most 16): {detail}",
            differences.len(),
        ));
    }
    Ok(())
}
