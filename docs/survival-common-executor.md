# Common survival sequence executor

Goal created 2026-10-02. Keep the original elevated roof objective in
survival-blueprint-construction.md. This milestone moves the fixed-sequence
execution kernel out of opt-in trial code without broadening native capabilities.

## Roadmap and acceptance

1. Reuse checked hypothetical construction sequences, edit/body bounds and
   temporary ownership. Add a temporary-only site contract for the existing
   access comparison; do not relabel it as the complete roofed build.
2. Separate Rust operation-result and continuation states. Persist intention
   before any mutating native call. Reuse atomic replacement, file/directory
   fsync and exclusive file locking from existing stores. Electrical job records
   have a different proof/boundary contract and are not reinterpreted.
3. Execute inventory setup, motion, placement, mining and retirement/recovery in
   a reusable DustRoute module. Retain native admission and independent receipts.
   Retry only after retirement/fresh scene and record the prerequisite that
   improved (e.g. a different received empty slot). Unchanged conditions stop.
4. Reopen saved records for diagnosis only. A restart never reconstructs native
   tokens; an in-flight/cancelled stage requires inspection. Preserve uncertain
   intent across cancellation, disconnection and storage failure. No automatic
   command fallback or replay.
5. Use the common executor in the existing three-block placement/climb/retreat/
   cleanup trial and hand-change trial. Exercise target conflict, cancellation,
   connection loss and restart boundaries with targeted tests. Record completed
   steps, recoveries, intervention reasons, and exact remaining temporary blocks.
   Repeated/cross-condition trials test the kernel before larger planning work.

Voxrig remains responsible for received state, movement/aim physics, individual
operation guards and exact native retirement. DustRoute owns permissions, supplied
material accounting, sequencing, recovery decisions and persistence. Native
retirement is not replaced by delay/acknowledgement or cache absence.

Automatic roof/access generation, the full public survival MCP workflow and
full roof acceptance remain subsequent roadmap work. This milestone's public
Rust executor does not imply Blueprint adoption or grant permissions from JSON.
The user's stop condition remains: report genuine new correctness concerns or
necessary work outside the declared scope before proceeding with it.

## Declared isolated comparisons

Reuse the dedicated vanilla 1.21.11 survival/non-OP fixture, localhost:25572,
NatMineBot plus independent NatMineView. Each run gets a fresh level-name and a
fresh journal/output. Console only prepares the stone floor/air, player positions
and three supplied dirt before execution, except the explicitly declared inputs.
The same checked plan places three dirt, climbs/retreats and removes its three
owned temporary blocks. The full captured region is compared after each completed
edit and independently at completion. No Blueprint/full roof acceptance claimed.

- Natural run: no further console changes, retain pickup/recovery evidence.
- Injected run: after first mining START, put one dirt in the selected hotbar slot;
  require a recorded inventory interruption and improved fresh plan, then cleanup.
- Cancel boundary: drop advance while waiting after first START; a further advance
  must refuse, no FINISH may have been sent. Persist cancellation, drop executor,
  reopen diagnostic record only; all three temporary blocks remain for inspection.
- Disconnect boundary: close source after first START, require refusal without
  FINISH/retry; retain diagnostics and three remaining temporary blocks.
- Foreign target boundary: after first START replace its dirt with stone through
  fixture console, require stop without FINISH or automatic recovery/reclaim.
  Record the foreign stone and two remaining owned dirt; do not call this cleanup.

Keep all attempted outputs, including refused runs. Boundary runs intentionally
stop without cleanup; their isolated worlds are retained and servers stopped.
Any repeated runs use fresh worlds rather than replaying an uncertain old session.

## Implemented boundary

`survival_execution::SurvivalExecutor` consumes an in-process
`HypotheticalConstructionPlan` after the caller authorizes its scope. Create
requires current source identity/position, exact declared baseline and actually
received supplied inventory. It is not an adoption or permission receipt.

`advance` performs one bounded transition. Mining START returns `MiningStarted`;
the next call observes its result, retires the source and reconciles a fresh
connection. The driver now supplies geometry/fixture inputs and records progress;
production orchestration no longer lives in the test. Every next step checks the
whole last-confirmed scene and safe body scope. Temporary removal additionally
requires an owned observed placement and a matching fresh geometric edit. Final
acceptance checks the declared geometry/retreat and independently checks all
captured cells. A received inventory change admits a new attempt only after native
retirement, fresh reconciliation and a different received empty hotbar slot, with
the concrete improvement saved. An attempt bound remains a separate limit.

The private journal reuses atomic file replacement/file+directory fsync and an
exclusive file lock. A new execution cannot overwrite an existing record.
`OperationOutcome` and `Continuation` remain separate Rust enums. Each native
mutation has a durable preceding intent; storage failure or a dropped `advance`
future blocks further dispatch. `cancel` stops forward dispatch; it does not
assert native abort or retirement. Retained operations still require inspection.
The caller must exclusively own the bot, including any cloned handles.

`diagnose` returns historical evidence plus `NeedsInspection` in a fresh process,
even for a historically completed record. Completion history is preserved; it is
not a current-world claim. There is deliberately no constructor from a journal,
no deserialization of native tokens, and no automatic replay after restart.
The reconnect endpoint/name/version are diagnostic caller declarations; native
recovery still validates its own session/server/player contract.

## Verified milestone (2026-10-02 UTC)

[Source-pinned records, hashes and retained traces](evidence/survival-common-executor-20261002.json)
cover runtime source `3dcaadc`, native Voxrig `1a8f258`, and fresh-process checker
plus reconnect diagnostic metadata at `e1e3e4b`. The latter change adds diagnostic
metadata/checks without changing the execution transitions used by the live runs.

| Run | Completed steps | Validated reconnects | Result / observed remnants |
| --- | ---: | ---: | --- |
| Natural A | 8/8 | 3 | Exact 2550-cell final site, no temporary remnants |
| Hand-change input | 8/8 | 4 | Received change, no old FINISH, empty hotbar 0 → 1, complete cleanup |
| Natural B | 8/8 | 3 | Exact 2550-cell final site, no temporary remnants |
| Dropped advance future | 5/8 | 0 | Further dispatch refused; no FINISH; three dirt retained |
| Source disconnected | 5/8 | 0 | Further dispatch refused; no FINISH; three dirt retained |
| Target replaced with stone | 5/8 | 0 | Foreign target refused; no FINISH; stone plus two dirt retained |

These are three completed runs and three expected inspection stops, not six
completed constructions. Each comparison used a fresh isolated non-OP vanilla
world; all test traces reported complete within their explicitly started trace
intervals. Every server stopped normally. Remaining blocks in boundary worlds
were intentionally preserved for diagnosis, not automatically reclaimed.

The injected first target received `selected_hand_changed` at receive sequence
285, with `sole_cause=true` and no FINISH. After native retirement, the new plan
recorded `different_received_empty_hotbar_after_retirement`, slot 0 → 1, then
completed all cleanup. Neither natural run needed this recovery in this batch;
do not infer pickup timing or entity provenance from that difference.

All six retained journals were then opened by separate fresh test processes:
no native client was created, overwrite/restart was refused, and original bytes
were unchanged. This verifies restart diagnosis, not autonomous crash recovery.
Related tests: **19 passed / three opt-in tests ignored** in the ordinary run;
all-target Clippy with warnings as errors and formatting passed. Dedicated live
and fresh-process cases were executed separately as described above. A failed
relative-path fixture configuration caused one preliminary boot of the old
isolated world with no test players/actions; it was normally stopped before any
comparison. Its log is retained rather than counted as a live acceptance run.

The executor milestone is complete. Next: connect this kernel to the original
roof/access sequence selection and caller-owned durable public workflow, then
accept the full adopted roof/cleanup contract. Automatic route/access generation,
public MCP orchestration and automatic restart recovery have not been claimed.

## Explicit planned reconnect state

Every checked temporary removal carries the native hypothetical reconnect
obligation because this executor retires its mining connection before continuing.
Planning cannot silently carry the previous gravity/rest phase across that reset.
After recovery, exact site comparison and the native received-start boundary must
both pass before the removal completes. A future obligation is neither a saved
native token nor proof that retirement already happened. The fresh connection is
still captured and checked on each subsequent action. Motion checks compare
initial model state and all trajectory frames exactly; refusals retain both
predictions before any input is sent. This does not implement stage-4 durable-job
continuation: historical journals remain diagnosis-only.
