//! Admission accounting for retained model allocations, separate from previews.
use super::*;
use dustroute_translate::piston_construction::{
    ExpectedState, expected_state::estimated_block_bytes,
};
pub(super) const MAX_RETAINED_BLOCK_RECORDS: usize = 1_048_576;
pub(super) const MAX_RETAINED_MODEL_BYTES: usize = 128 * 1024 * 1024;

pub(super) fn records(proof: &ElectricalModification) -> usize {
    let shared = ExpectedState::retained_usage(
        proof
            .steps(false)
            .iter()
            .chain(proof.steps(true))
            .map(|s| &s.expected),
    );
    proof.before().blocks.len()
        + proof.after().blocks.len()
        + proof.requests().len()
        + shared.block_records
}
pub(super) fn estimated_bytes(proof: &ElectricalModification) -> usize {
    let shared = ExpectedState::retained_usage(
        proof
            .steps(false)
            .iter()
            .chain(proof.steps(true))
            .map(|s| &s.expected),
    );
    let snapshots = estimated_block_bytes(&proof.before().blocks)
        + estimated_block_bytes(&proof.after().blocks);
    let steps = proof
        .steps(false)
        .iter()
        .chain(proof.steps(true))
        .map(|s| s.state.capacity() + 256)
        .sum::<usize>();
    // Reserve an upper estimate for the small explicit request records.
    let requests = proof.requests().len() * 4096;
    snapshots + shared.estimated_bytes + steps + requests
}
