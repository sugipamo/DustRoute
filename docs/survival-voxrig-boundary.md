# Voxrig checked survival boundary

Approved by the user on 2026-10-02: organize Voxrig first, retaining library
ownership. Native snapshot `f81c17bafdec54ee7c3de9e3fd4964c90e5654d0` is imported
unchanged with its manifest; changes are developed and committed upstream first.

| Voxrig | DustRoute |
| --- | --- |
| Version-selected capabilities and checked operation API | Deployment choice, permissions and user-facing diagnosis |
| Received state, collision, aim and bounded motion prediction | Route and temporary-access/action selection |
| Single-operation intents and observations | Blueprint adoption and material reservation |
| Explicit source retirement and once-only fresh-session validation | Decide recovery, persist jobs, inspect/replan afterward |

The library exposes `Client::survival()` and `checked_survival` data types.
Java 1.21.11 implements the bounded observed dry-cube contract. Java 1.16.1's
legacy API remains intact and refuses this checked contract explicitly.
Capabilities describe static support, never live readiness or action permission.
No command or creative fallback is exposed on the checked handle.

The retirement handle binds the source, observer and watch. Prepare first, retain
the handle, explicitly close, inspect/wait, then explicitly reconnect once.
Native UUID removal, local closure, fresh loading/state gates and cancellation
history are unchanged. No caller job is restored by reconnecting.

Native offline tests exercise façade receipt/closure/rejoin refusal and cancelled
TCP login followed by second-login refusal. A native edge comparison independently
confirmed the aiming correction. Consumer route and sequence checking imports use
the façade. Version-specific item registry setup remains in explicitly versioned
fixture drivers only.

## Declared consumer façade live comparison

Repeat the existing temporary-access fixture on the dedicated direct vanilla
1.21.11 non-OP server at localhost:25572 in a fresh flat world. Stone floor
x=-5..12, z=-5..8, y=-61; air above; builder `[0.5,-60,0.5]`, observer
`[0.5,-60,5.5]`, three dirt in main inventory. Console preparation precedes actions.
No later console mutation, automatic retry, drop recovery or authority bypass.
The public façade must place three dirt, climb/retreat, remove all three, and
complete three explicit retirement/fresh-recovery cycles with retained traces.
This verifies the API migration, not adopted roofed construction or durable jobs.

## Validation and concern stop

[Exact checks and traces](evidence/survival-voxrig-boundary-20261002.json) pin
DustRoute `bd4d439` and Voxrig `f81c17b`. Native tests: 185 passed / six ignored at
the façade commit, four doc tests; the final read-only player forwarding addition
passed focused retirement tests and all-target Clippy. Consumer survival tests:
nine passed / two ignored. Formatting and manifest checks passed. Consumer-wide
Clippy was not rerun before the live concern stop.

The façade live trial placed three dirt, climbed and retreated, removed one dirt,
and completed one exact retirement/fresh recovery. It then started mining
`[2,-60,0]` after receive sequence 66, relying on the empty hand received at 50.
The fresh connection's first captured subsequent packet (sequence 67, slot update
0x14) supplies one dirt to selected player slot 36. The next mining check refused
with `mining requires a received empty selected hand and supported player screen`.
Start was dispatched; no finish or confirmed second removal is recorded. This
must not be reported as a refusal before all I/O or as completed cleanup.

Automatic collection of the first removed dirt is the likely explanation, but
its item-entity origin was not independently audited. The directly established
fact is a received selected-hand change during the second mining attempt.
The intent and inspection reason remain in native history. Both clients were
closed and the dedicated server stopped normally (exit 0). No operation was
blindly replayed; the failed world and traces are retained.

In accordance with the user's concern-stop instruction, no further corrective
implementation or live retry is undertaken. Proposed next work: examine native
inventory-change/operation invalidation and expose the necessary bounded evidence;
use existing explicit retirement plus fresh inventory/site observation on the
caller side before choosing a new empty hand and replanning. Never release the
old mining guard, silently credit recovered materials, or assume unchanged hand
contents because recovery just succeeded. This does not require adding entity
physics or gathering policy to Voxrig. The complete three-removal comparison and
full roofed construction remain unvalidated at this checkpoint.

## Inventory recovery approved (2026-10-02 UTC)

The user approved the follow-up. Receive-time native diagnosis, caller-side
reconciliation and bounded fresh plans are implemented; the earlier stop above
is historical. Current scope and live comparison declaration are recorded in
[inventory recovery](survival-inventory-recovery.md). Library ownership is unchanged.
