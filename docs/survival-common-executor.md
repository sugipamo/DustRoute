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
