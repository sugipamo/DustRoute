# Mining inventory interruption and caller recovery

Approved by the user on 2026-10-02 after the façade live comparison's hand-change
stop. Voxrig records received prerequisite changes and native operation/session
lifecycle. DustRoute owns temporary-block permission, target reconciliation,
bounded new attempts and material policy. This is not entity simulation or an
extension of the construction material budget to dropped items.

## Implemented contract

- Native `MiningRecord::inventory_change` captures the first incompatible receive
  with the selection, slot contents/sequence, screen and cursor. Occupied then
  empty cannot erase it. A separate known conflict clears `sole_cause`.
- A pending miner remains mutation-guarded; no FINISH follows latched inspection.
  The convenience helper returns the inspection record, including the narrow
  observation-to-FINISH race. Transport failures retain their error/history.
- `survival_cleanup::CleanupRecoveryPlan` is read-only caller policy. It requires
  correspondence to a declared owned temporary removal. It admits historical
  observed removal or a plain held-item change with no other known conflict;
  missing evidence, generic timeouts, screen/selection changes and foreign target
  conflicts do not enter this bounded policy.
- After explicit exact retirement, inspect the target independently. Require
  exact original block or exact final air; a block reappearing after confirmed
  removal is not automatically reclaimed. Reconnect performs native fresh checks.
- Reconcile the new captured scene with closed old history and fresh evidence.
  If already air, finish that removal; otherwise create a new permission-checked
  geometric plan and select a currently received empty hand. Never replay the
  old intent or assume a selected empty slot is reserved against future pickup.
- The isolated consumer driver caps attempts at three per owned target. Repeated
  interference stops with retained history. No recovered item receives material
  credit, and no job state is imported across sessions.

## Declared live comparisons

Use the existing direct vanilla 1.21.11 non-OP fixture at localhost:25572, with
NatMineBot and independent NatMineView, a fresh flat world for each comparison,
stone floor x=-5..12, z=-5..8, y=-61; air y=-60..-53. Builder at
`[0.5,-60,0.5]`, observer at `[0.5,-60,5.5]`, three dirt in main inventory slot 9.
Console setup precedes all ordinary player actions. Preserve every attempted run,
trace, rejection and server log. No retry of an uncertain attempt on its old
connection; new attempts only follow exact retirement and full reconciliation.

1. Natural sequence: three placements, climb, retreat and complete cleanup. Do
   not suppress drops or assume whether pickup occurs. Record received inventory
   and every outcome/recovery. This may or may not exercise an interruption.
2. Explicit external-input sequence: after the first removal START is recorded,
   the driver pauses and prints the selected hotbar index. The console supplies
   one dirt to exactly that slot, then releases the gate. No block edit or mining
   completion command is sent. Require a typed inventory interruption, original
   retained intent, exact retirement, fresh target/inventory and a new plan, then
   all three removals and exact final stone/air. This tests the received hand
   change deterministically, not the origin/timing of natural item pickup.

Both comparisons keep full structure/cleanup acceptance separate from these
three temporary blocks. The adopted elevated roof and durable survival jobs
remain unfinished.

## Verified checkpoint

[Source-pinned evidence and complete traces](evidence/survival-inventory-recovery-20261002.json)
record DustRoute `c10b27d` and Voxrig `1a8f258`. Native tests: 189 passed / six
ignored, four documentation tests. Consumer related tests: 12 passed / two
ignored. All-target Clippy with warnings as errors passed for both repositories;
formatting and the 293-file vendor manifest passed.

Both declared live comparisons passed on their first attempt. Each placed three
dirt, climbed, retreated, recorded one selected-hand interruption without FINISH,
retired the old miner, obtained a fresh baseline, selected a new empty hand and
completed three removals. Both used four validated retirement/recovery cycles,
with connection chain 1 -> 3 -> 4 -> 5 -> 6, exact final stone/air and complete
packet traces. Both dedicated server processes stopped normally (exit 0).

- Natural: the second target `[2,-60,0]` received dirt in selected slot 36 at
  sequence 78, after intent boundary 70. Reconciliation returned `needs_new_plan`
  once and `already_absent` for the three completed removals. The earlier failure
  condition therefore reproduced and recovered. Exact item-entity origin is not
  inferred from the inventory packet alone. Test duration 34.80 seconds includes
  console fixture wait, so it is not a throughput benchmark.
- External input: after the first target START, one console inventory edit
  supplied dirt to hotbar 0. Sequence 313 latched interruption after boundary 290.
  After retirement, the original dirt remained and a new empty-hand plan removed
  it. All remaining cleanup completed. Test duration 52.87 seconds includes both
  console waits. This is a deterministic prerequisite-change test, not evidence
  about pickup timing.

The prior concern is resolved for this bounded policy. The subsequent
[common executor milestone](survival-common-executor.md) moves the action loop out
of the isolated driver and adds durable diagnosis plus explicit retry improvements.
Public survival job orchestration and full adopted roof construction still need
the remaining roadmap integration.
