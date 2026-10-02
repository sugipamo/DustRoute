# Inventory changes during bounded mining

Approved 2026-10-02 after the consumer's selected-hand change concern. This work
retains library ownership: Voxrig records received prerequisites and the original
operation, while the consumer chooses permissions, recovery and a new plan.

Implement receive-time, latched inventory-change evidence for pending mining:
selected hand/selection, player screen/cursor and unsupported inventory state.
Preserve the first cause and receive boundary, including an occupied-then-empty
hand. Unrelated slot updates and repeated compatible empty-hand receipts are not
interruptions. Never release the old mutation guard or infer who supplied an item.
The ordinary mining helper must return the typed inspection outcome when its
prerequisites changed, including a race before FINISH; transport failures remain
errors with retained history.

The consumer will use explicit retirement, independent target inspection and
fresh recovery. Only an exact original target or exact air can be reconciled;
other state, missing evidence or non-inventory conflicts stop. Each new attempt
requires fresh geometry and an empty received hand. No recovered-item credit,
entity physics, gathering policy, or durable job restoration is added to Voxrig.

Validation: received TCP inventory packets (including occupied then empty, other
slot changes, selection/cursor/screen changes), no FINISH/replay after invalidation,
retained history and retirement, then the existing temporary-access live comparison
through the public API. Live evidence and limitations belong to the consumer.

Implementation checkpoint: native all-target tests passed 189 cases with six live
cases ignored; four documentation tests, all-target Clippy with warnings as errors
and formatting passed. Receive tests cover four new inventory interruption cases.
`MiningRecord::inventory_change` retains the first typed snapshot and `sole_cause`
is cleared if a separate known target/world conflict is recorded. It is evidence
for caller diagnosis, not automatic recovery authority. Historical removal results
remain historical; later inventory updates do not rewrite a completed receipt.
