# Piston motion-time execution audit

Status on 2026-09-21: extending the physical model to motion-time input reversal
was approved. Source investigation found an execution-boundary prerequisite and
implementation stopped for review. The user subsequently approved that work;
the [synchronous runtime foundation](synchronous-world-runtime.md) is now
implemented, with a separate [horizontal piston adapter](piston-callback-runtime.md).
The location-terminal behavioral integration remains incomplete.
The earlier choice between widening the model and restricting the input contract
has been resolved in favor of widening the model. `RepeatedSettling` is unchanged.

## Evidence and reproduction

The audit inspected the local Minecraft Java **1.21.11** artifacts, remapped by
Fabric Loom **1.17.20** with Yarn **1.21.11+build.6**. The cached official client
and server JAR SHA-1 values match their Mojang version-manifest entries. Named
class bytecode was inspected with `javap -p -c`; CFR **0.152** supplied a readable
view. The decompiled files are local investigation artifacts, not repository
source. This is static implementation evidence, not a live conformance test.

The [audit metadata](../crates/dustroute-minecraft/tests/fixtures/piston_execution_boundaries_preflight.meta.json)
records official download URLs/hashes, named JAR and class hashes, decompiler
identity, and diagnostic scope. Inspect these named classes in the matching
Loom artifact to reproduce the source audit:

- `net.minecraft.block.PistonBlock`: `tryMove`, `onSyncedBlockEvent`.
- `net.minecraft.block.entity.PistonBlockEntity`: `getProgress`, `tick`, `finish`.
- `net.minecraft.server.world.ServerWorld`: `tick`, `processSyncedBlockEvents`.
- `net.minecraft.world.block.ChainRestrictedNeighborUpdater`: `enqueue`,
  `runQueuedUpdates`.

Run the independent engine diagnostics with:

```bash
DUSTROUTE_DIAGNOSTIC_OUTPUT=/tmp/audit_piston_execution_boundaries.json cargo test --offline --locked -j1 -p dustroute-minecraft --test audit_piston_execution_boundaries -- --ignored --exact retain_fixture --test-threads=1
```

The [four captured rows](../crates/dustroute-minecraft/tests/fixtures/piston_execution_boundaries_preflight.jsonl)
describe existing v1 behavior. They must not become expected outputs for a
corrected model. The first two use synthetic callbacks to isolate scheduling;
the last two exercise the actual direct-input piston runner. They do not
assert a complete Java execution for the same absolute input schedule.

## Source findings

### Reversal is decided twice

`PistonBlock.tryMove` distinguishes ordinary retract event 1 from drop retract
event 2. With an extended, unpowered body, it examines a matching extending
moving block two positions ahead. Event 2 is selected if any of these hold:

- `getProgress(0)` is below one half; this reads **lastProgress**, not progress.
- The moving block entity's saved world time equals the current world time.
- The world is in its block-tick section.

The block-tick flag is not equivalent to "this event is a scheduled block tick."
In the audited `ServerWorld.tick`, it is set before scheduled ticks and remains
set through the block-event pass, then clears before block-entity ticking.
Synchronous notifications inherit their caller's execution context.

`onSyncedBlockEvent` checks power again when the queued event is delivered. An
extension is rejected if power has gone away. A retraction is rejected if power
has returned; the body is kept extended. Request-time power alone is insufficient.
Server delivery also checks that the current block matches the queued block.

For sticky retraction, a matching still-extending moving payload is finished
at its current carrier position and is not pulled in that branch. Otherwise,
only ordinary event 1 attempts to pull an eligible stable payload. Event 2
skips that pull. Thus a returned piston can legitimately leave its payload
behind. Merely making an OFF input eventually retract the body does not prove
an OFF-to-empty requirement at the output location.

This does **not** establish that the public 1×2 realization satisfies arbitrary
input histories. Its earlier settled operation evidence remains narrower.

### The body and payload have independent moving state

During retraction, Java puts `moving_piston` at the **body coordinate**, carrying
the retracted piston with `source = true`. A pulled payload has its own carrier.
The retained v1 plan instead leaves a `Piston` with `Retracting` state at the body
and installs other carriers according to the old bounded motion law. Both may
reach the same settled geometry; their location observations during movement
differ. The old law and captures must retain their historical meaning.

The block entity advances progress by one half on each tick and first copies
progress into lastProgress. Completion is checked using that copied value before
the increment. Starting from zero, two ticking calls reach progress 1; a further
call takes the normal completion branch. This describes calls, not a universal
delay from an external lever edge: initial block-entity eligibility also matters.

Forced `finish` differs from ordinary completion. On the server it acts while
lastProgress is below 1; a source carrier becomes Air, while an ordinary payload
is materialized through block-state postprocessing. Normal ticking materializes
the carried state through its own completion path. Both can issue synchronous
neighbor notifications. A single body-wide completion plan is not a substitute
for these separately owned lifetimes.

The current `PistonBlockEntityState.progress` is explicitly a **0/1 boundary
marker**. It does not encode half-step progress, lastProgress or savedWorldTime.
Those distinctions affect block behavior even with general entities and collision
interpolation out of scope. They belong in a new runtime representation, not a
silent reinterpretation of old serialized metadata.

### A callback is not another global phase

The audited neighbor updater drains work synchronously from an outer invocation,
with newly requested nested work processed through its local pending/stack
mechanism. A notification caused by completion A is handled before execution
returns to the outer block-entity loop and starts unrelated completion B.

The existing engine instead delivers `EventOutcome.queued` through the global
tick/phase/insertion queue. The diagnostic exposes both available encodings:

| Encoding in the existing engine | Captured result |
| --- | --- |
| Completion A queues same-tick `NeighborUpdate` phase | Rejected as a phase regression; the event is rolled back and both original events remain pending |
| Completion A queues the callback using `BlockEntity` phase | A → already queued B → callback A |
| Required for a synchronous callback in this isolated example | A → callback A → B |

Reusing the caller's phase avoids the first error but does not provide synchronous
delivery. Delaying the callback to the next tick also changes its context and
may change retract selection. This is not a reason to remove the old causal-order
check: it correctly enforces the old scheduler's contract.

The existing handler also commits one `EventOutcome` atomically. The source
retraction path includes world writes and notifications between operations;
those nested effects need an explicit execution boundary, not just more queued
events after the final delta.

## Other captured mismatches

| Diagnostic | Current bounded v1 result | Source implication |
| --- | --- | --- |
| ON at tick 1; OFF in the external phase at tick 2 before queued extension | Extends at tick 2; eventually leaves an extended body and solid output with input OFF | The event delivery guard must recheck power. This is a conditional branch comparison, not a claim about the exact Java schedule of these external edges |
| ON at tick 1; settled OFF at tick 8 | During retraction at tick 9, body is `Piston`; both head and old payload coordinates are `MovingPiston` | New location-state verification needs the correct body/payload carrier layout, independently of settled success |

The earlier [six motion-time input captures](blueprint-architecture.md#movement-verification-preflight-input-changes-during-motion)
remain intact. These additional diagnostics explain why a completion-time
power resample alone would not repair that preflight.

## Recommended prerequisite and resumption boundary

There is no newly identified contradiction in location terminals, immutable
Revisions or whole-realization semantics. The missing foundation is the runtime
execution boundary needed to give the newly approved physical rules a meaning.
Before integrating those rules, define and implement a separately versioned,
bounded world execution path that can retain:

1. The enclosing world-tick section and synchronous/nested notifications,
   separately from queued block events and later block-entity ticks.
2. Per-carrier movement progress/history and lifecycle, event-time power and
   block-identity checks, and cancellation/forced completion.
3. Full checkpoint and state-comparison information for this work, including
   the exact external-input boundary. A snapshot or visible-state hash cannot
   stand in for that execution state.

The source audit establishes required distinctions; it does not select an
unreviewed new scheduler ABI, migrate old profiles or declare a new proof profile
complete. A narrow implementation can keep entities, collision interpolation,
vertical mechanics, slime/honey and other deferred features out of scope.
Whether this is implemented by a new executor or additions behind a new profile
is an implementation choice for the prerequisite work; a wholesale scheduler
replacement has not been shown necessary.

After that prerequisite, add the source-backed piston laws and differential
cases, then reconnect fixed-location observations and whole-realization behavior
review. Keep failures and incomplete/unsupported checks distinct. Do not weaken
the behavior type, force a dropped payload to return, or auto-update Blueprint
references/terminals to make a realization pass.

At the original stop, only this report, its diagnostic example/captures and links in
existing documentation were added. No new runtime profile, production handler,
physical law or live-world change was made.

Audit validation: all four diagnostic outcomes and the capture hash were checked;
`cargo fmt --all --check`, example-scoped Clippy with `-D warnings`, and
`git diff --check` passed. The full workspace suite was not repeated for this
diagnostic/documentation-only addition.

The diagnostic example commands are retired. Explicitly ignored fixture tests retain their cases; `DUSTROUTE_DIAGNOSTIC_OUTPUT` must be a new absolute path. JSON is fixture IO only, and a diagnostic run never certifies a live circuit or adopts a design.
