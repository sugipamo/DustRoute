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
