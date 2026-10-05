//! Offline construction cost; passive fixtures are not functional circuit proofs.
use dustroute_minecraft::Pos;
use dustroute_translate::piston_construction::ElectricalModification;
use dustroute_translate::snapshot::{MinecraftSnapshot, MinecraftSnapshotBlock};
use std::time::Instant;

fn main() {
    for total in [64, 256, 1024, 2048, 4096] {
        let blocks = (0..total)
            .map(|i| MinecraftSnapshotBlock {
                pos: Pos::new(i % 64, 0, i / 64),
                name: "minecraft:stone".into(),
                properties: Default::default(),
            })
            .collect::<Vec<_>>();
        let after = MinecraftSnapshot {
            min: Pos::new(-1, -1, -1),
            max: Pos::new(64, 1, total / 64),
            blocks,
        };
        let before = MinecraftSnapshot {
            blocks: after.blocks[..total as usize - 64].to_vec(),
            ..after.clone()
        };
        let start = Instant::now();
        let proof = ElectricalModification::new(&before, &after, Default::default()).unwrap();
        let usage = dustroute_translate::piston_construction::ExpectedState::retained_usage(
            proof
                .steps(false)
                .iter()
                .chain(proof.steps(true))
                .map(|s| &s.expected),
        );
        let elapsed_ms = start.elapsed().as_secs_f64() * 1000.0;
        let retained_records: usize = proof
            .steps(false)
            .iter()
            .chain(proof.steps(true))
            .map(|s| s.expected.len())
            .sum();
        println!(
            "work_region_scaling debug_assertions={} non_air_context_blocks={total} changed_positions=64 proof_elapsed_ms={elapsed_ms} forward_commands={} undo_commands={} logical_expected_block_records={retained_records} retained_expected_block_records={} estimated_expected_bytes={} scope=passive_full_context",
            cfg!(debug_assertions),
            proof.steps(false).len(),
            proof.steps(true).len(),
            usage.block_records,
            usage.estimated_bytes,
        );
    }
}
