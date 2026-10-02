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
