# Synchronous world execution boundary

The approved prerequisite for motion-time piston work is implemented in
[`time::runtime`](../crates/dustroute-minecraft/src/time/runtime/mod.rs).
Its delivery profile is `dustroute.synchronous-world-callbacks.v2`. This is a
runtime foundation with an adapter interface, not a complete new piston physics
profile or a passing location-terminal proof by itself. The separate
[electrical piston adapter](unified-piston-runtime.md) now supplies the physical
rules and enrollment/notification mapping for its bounded world profile.

The [1.21.11 source audit](piston-motion-source-audit.md) motivated three required
distinctions: synchronous callbacks versus globally queued events, enclosing
world-tick context, and separate histories/lifetimes for moving carriers. The
existing `PhysicsEngine`, `SchedulerProfile`, law Revisions, `WorldExecutionContext`
profiles, block wire format and historical fixtures retain their meanings.

## Execution and input boundaries

`SynchronousWorldRuntime<A>` invokes the stateless `RuntimeAdapter` selected by
its Rust type and immutable revision label. Its `Payload` carries local values
and continuation stages. Adapters must not keep execution state in closures,
global variables or uncheckpointed caches.

`RuntimeOutcome` describes one operation: an optional checked `WorldDelta`,
carrier lifecycle/history effects, queued future work, synchronous calls, and
an optional continuation. These effects are staged before commit. After commit,
the runtime drains each synchronous call and its descendants in request order,
then evaluates the continuation against the updated world, before taking another
queued root event. A continuation can therefore read changes made by a callback.

This is an explicit call/return primitive. A physics adapter must use it to encode
the audited method boundaries and notification batching. In particular, Java's
neighbor updater enqueues nested notifications while an entry is running and
drains them after that entry returns. The primitive does not infer those
entry boundaries, automatically send neighbor notifications for every write, or
claim that every notification request is a direct recursive Java call. Pending
adapter-specific batches must be retained in payloads/continuations, never a side
cache. That mapping belongs to the selected physical adapter.

`microstep()` exposes each operation for diagnostics, intermediate observations
and checkpointing. `step()` finishes one complete root operation, including an
already active chain restored from a checkpoint. `input_now()` is allowed only
between those complete root operations. It runs before the next queued root and
inherits the current world-tick section; it does not move backward to an earlier
global phase. Input changes inside a synchronous chain are rejected without
changing state. This defines the new runtime's model-step boundary and does not
change either existing `RepeatedSettling` or old proof profiles.

Five ordered sections are retained: External, BlockTicks, BlockEvents,
BlockEntities, and AfterWorldTick. The last represents a server-thread input
after the world tick returns, with the same world time as its carrier ticks.
It is distinct from External before the next tick; this distinction is backed
by [transient observations](piston-transient-conformance.md).
Synchronous calls and continuations retain their caller's time and
section. `in_block_tick()` is true in BlockTicks and BlockEvents, including their
callbacks, rather than being inferred from a callback's event kind. A block
event requested after the block-event section is queued for the next tick's pass.
Requests for an earlier time/section are rejected; timestamps never saturate on
overflow. Future carrier/scheduled tick eligibility is explicitly supplied by
the physics adapter. Full Java world scheduling, chunk lifecycle and block-entity
enrollment are not inferred from these four sections.

Queued block events retain the receiving block's identity and are filtered if
that block has been replaced before delivery. Identity includes native piston
variant and observed block name, but excludes changing power/facing properties.
The adapter separately reads current power at delivery. Identical pending block
events in one pass retain the first insertion and cause; no speculative IDs are
allocated for duplicates. A consumed event can be requested again.

## Native block tick queues (v2)

Prioritized block ticks distinguish the future pending queue (`isQueued`) from
remaining callbacks collected for the current tick (`isTicking`). A callback is
removed from the ready batch before it runs. Deduplication applies to future
pending ticks; a still-ready callback does not suppress a new future tick.
Repeaters additionally check `isTicking`; observers check only `isQueued`.
Normal-priority observer/lamp ticks follow repeater priorities, with stable
insertion order within each priority. Legacy unguarded `ScheduledTick` requests
retain their existing FIFO behavior.

Both queries derive from the existing checkpointed pending map and tick section.
No unsaved scheduler cache is introduced. This bounded adapter always requests
positive delays; chunk budgets, unloaded chunks, and same-tick re-enrollment
are outside its contract. The runtime profile advances to v2 and its electrical
adapter to v4; old checkpoints are not reinterpreted.

## Moving state and validation

Runtime carrier state is separate from `PistonBlockEntityState.progress`, whose
old 0/1 marker is unchanged. `MotionHistory` records progress and lastProgress
using exact Zero/Half/Full values and retains saved world time. These are state
values, not an implementation of the progress, finish or retraction law. The
adapter derives changes from its selected physical law Revisions.

Every installed carrier receives a new lifetime token. Pending carrier ticks
retain that token, so replacing a carrier at the same coordinate does not allow
the previous carrier's tick to act on the new one. Retired ticks remain recorded
as filtered deliveries. Carrier history and block changes commit together;
history cannot be removed while leaving a moving block behind, or a payload
changed under an existing token. Separate body/payload positions can have
independent lifetimes. No Blueprint, Assembly, child reference or terminal is
part of this runtime state, and no interpretation is automatically moved.

Construction always calls the selected adapter's required `validate_initial`
gate. The gate must check its supported initial geometry and evidence. The old
`ValidatedWorld` placement profile rejects pistons; it is not repurposed as
mechanical evidence. A future mechanical adapter must explicitly validate its
new supported placement contract. The foundation additionally rejects out-of-
region blocks and initial moving state without a checkpoint. It does not guess
histories from a snapshot or admit an initial "moving but queue empty" world.

Reads and effects are bounded by an explicitly known region. The checked `block`
query treats sparse absence as Air only within that region. An adapter using the
raw world for geometry must also use the supplied known region. Runtime writes,
move endpoints, callback targets and queued targets outside it are rejected.

## Checkpoints, failures and equality

Opaque, non-deserializable checkpoints retain the full world, region, current
section/time, queues and their order, active calls/continuations, carrier history
and tokens, ID counters, budgets/counters, trace and status. Restore requires the
same profile, adapter revision and adapter Rust type. Reusing a label with a
different implementation does not authorize checkpoint reinterpretation.

`RuntimeStateKey` compares exact state, including unfinished work, history,
budgets and failure status; it never substitutes the world's state hash. Trace
contents are excluded from the key because they are output, not future work.
Absolute times and IDs are intentionally retained. The native piston adapter has
a separate [behavioral comparison](runtime-location-review.md#what-is-compared)
at complete root boundaries. Its normalization is specific to that pinned
adapter and does not change this exact key or apply to arbitrary adapters.
A comparison that distinguishes too much leads to a budget/unknown result, not
a false proof based on visible blocks alone.

A rejected operation preserves its pending work, world, carrier records, clock
and counters. Earlier accepted operations remain in the trace, which becomes
failed. Failed runs cannot continue without restoring an earlier checkpoint.
Recursion, pending work and total microsteps have finite budgets. Exhaustion
does not discard work or produce success. An empty event queue with surviving
carriers is reported as unfinished motion. Draining a supported execution is
not, by itself, a behavioral type check.

## Verification and next integration

The [21 focused regressions](../crates/dustroute-minecraft/tests/synchronous_runtime.rs)
cover the original A/callback/B ordering problem, nested writes followed by a
fresh continuation read, checkpoints in mid-call, input boundaries, failed
prefixes, atomic world/history/queue rejection, distinct histories behind an
identical block snapshot, carrier replacement and stale ticks, block replacement
versus changed power, duplicate delivery, known-region checks, budgets and the
mandatory initial gate. Their adapter is explicitly a test program, not a
Vanilla implementation or new live evidence.

Validation on 2026-09-21 passed **145 Minecraft-layer tests** (including the 21
new runtime cases) and **15 retained translation integration tests** covering
execution contexts, piston completion, the immutable piston model baseline and
recorded payload/1×2 sequences. The old four-row execution-boundary diagnostic
still matches its saved capture byte for byte. `cargo fmt --all --check`,
Minecraft all-target Clippy with `-D warnings`, and `git diff --check` passed.
The full workspace suite was not repeated for this isolated delivery module.

```bash
cargo test -p dustroute-minecraft
cargo test -p dustroute-translate --test execution_context --test piston_completion --test piston_laws --test piston_payload_push
cargo clippy -p dustroute-minecraft --all-targets -- -D warnings
```

The [horizontal piston adapter and pinned laws](piston-callback-runtime.md) now
use these boundaries, including notification batching and block-entity enrollment.
Location observations, complete-state exploration and a native whole-realization
diagnostic review are now connected; see [moving-world review](runtime-location-review.md).
The existing MCP and adoption workflow now accepts the explicit native context
and revalidates before publishing immutable records. General
entities and the other deferred mechanics remain outside this work.
