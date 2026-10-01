//! Bounded presentation and retention; executable proofs keep full states.
use super::*;

pub(super) const MAX_RETAINED_BLOCK_RECORDS: usize = 1_048_576;
pub(super) const MAX_RETAINED_MODEL_BYTES: usize = 128 * 1024 * 1024;
const MAX_EXPANDED_BLOCK_RECORDS: usize = 8192;

pub(crate) fn state_summary(snapshot: &MinecraftSnapshot) -> Value {
    json!({"bounds":{"min":snapshot.min,"max":snapshot.max},
        "non_air_blocks":snapshot.blocks.len(),
        "state_content_id":crate::snapshot_content::content_id(snapshot),
        "role":"model intention, not live-world evidence"})
}
pub(super) fn records(proof: &ElectricalModification) -> usize {
    proof.before().blocks.len()
        + proof.after().blocks.len()
        + proof.requests().len()
        + proof
            .steps(false)
            .iter()
            .chain(proof.steps(true))
            .map(|s| s.expected.blocks.len())
            .sum::<usize>()
}
pub(super) fn estimated_bytes(proof: &ElectricalModification) -> usize {
    let snapshot = |s: &MinecraftSnapshot| {
        s.blocks.capacity()
            * std::mem::size_of::<dustroute_translate::snapshot::MinecraftSnapshotBlock>()
            + s.blocks
                .iter()
                .map(|b| {
                    b.name.capacity()
                        + b.properties
                            .iter()
                            .map(|(k, v)| 192 + k.capacity() + v.capacity())
                            .sum::<usize>()
                })
                .sum::<usize>()
    };
    snapshot(proof.before())
        + snapshot(proof.after())
        + proof
            .steps(false)
            .iter()
            .chain(proof.steps(true))
            .map(|s| snapshot(&s.expected) + s.state.capacity() + 256)
            .sum::<usize>()
        + proof.requests().len() * 4096
}
pub(super) fn states(proof: &ElectricalModification) -> Value {
    let expanded = records(proof) <= MAX_EXPANDED_BLOCK_RECORDS
        && proof.before().blocks.len() + proof.after().blocks.len() <= 512;
    let steps = |undo| {
        if expanded {
            json!(proof.steps(undo))
        } else {
            json!(
                proof
                    .steps(undo)
                    .iter()
                    .map(|s| json!({"position":s.position,"state":s.state,
            "wait_ticks":s.wait_ticks,"expected_non_air_blocks":s.expected.blocks.len(),
            "complete_expected_state_retained_by_executor":true}))
                    .collect::<Vec<_>>()
            )
        }
    };
    let mut result = json!({"step_states_expanded":expanded,
        "before":if expanded {json!(proof.before())} else {state_summary(proof.before())},
        "after":if expanded {json!(proof.after())} else {state_summary(proof.after())},
        "requested_changes":proof.requests(),"steps":steps(false),"undo_steps":steps(true),
        "no_write_checkpoint":proof.steps(false).is_empty()});
    if !expanded {
        let differences =
            dustroute_translate::diagnostic::difference::differences(proof.before(), proof.after())
                .expect("validated full-context proof");
        result["settled_differences_count"] = json!(differences.len());
        result["settled_differences_truncated"] = json!(differences.len() > 64);
        result["settled_differences"] = json!(differences.into_iter().take(64).collect::<Vec<_>>());
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn large_preview_keeps_all_command_values_but_not_repeated_worlds() {
        let after = MinecraftSnapshot {
            min: Pos::new(-1, -1, -1),
            max: Pos::new(513, 1, 1),
            blocks: (0..512)
                .map(|x| dustroute_translate::snapshot::MinecraftSnapshotBlock {
                    pos: Pos::new(x, 0, 0),
                    name: "minecraft:stone".into(),
                    properties: Default::default(),
                })
                .collect(),
        };
        let before = MinecraftSnapshot {
            blocks: after.blocks[..448].to_vec(),
            ..after.clone()
        };
        let proof = ElectricalModification::new(&before, &after, Default::default()).unwrap();
        let preview = states(&proof);
        assert_eq!(preview["step_states_expanded"], false);
        assert_eq!(preview["requested_changes"].as_array().unwrap().len(), 64);
        assert_eq!(preview["steps"].as_array().unwrap().len(), 64);
        assert!(preview["steps"][0].get("expected").is_none());
        assert_eq!(preview["after"]["non_air_blocks"], 512);
        assert!(serde_json::to_vec(&preview).unwrap().len() < 65536);
        assert_eq!(proof.steps(false).last().unwrap().expected, after);
    }
}
