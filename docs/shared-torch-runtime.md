# Torch history in the shared world runtime

The current Java 1.21.11 callback profile admits standing torches and all four
wall orientations. It uses one checked Rust device definition, the common
scheduler, and the same electrical world as dust, gates, bulbs and pistons.
The historical isolated-torch Law and its fixed-world proof profiles keep their
previous meanings; they are not used as hidden per-torch executors here.

## Rule and state ownership

A `HistorySpec` declares a namespace, inclusive time window and threshold.
Compilation rejects absent/invalid policies, out-of-domain query bindings, and
conflicting policies for one namespace even across disjoint device identities.
The runtime owns recent events by namespace and position. Removal does not erase
history. A torch reads the number of off events in the last 60 game ticks,
saturated at eight. Keeping the latest eight timestamps is sufficient: discarded
entries expire no later than every retained entry, so they cannot change a
future threshold result. Exact checkpoints retain the model's saturated record;
this is not a dump of Vanilla's private list.

Neighbor updates request two-game-tick callbacks when lit state equals support
power, unless a callback is already in the collected batch. The tick prunes
expired history, turns a powered lit torch off, drains the resulting callbacks,
records the off event, then requests 160 ticks when the threshold is reached.
An unpowered unlit torch relights only below the threshold. Turning on does not
record an off event. The existing scheduler keeps the first queued callback for
each concrete block identity and coordinate.

The cached target `RedstoneTorchBlock`, `WallRedstoneTorchBlock` and `WorldChunk`
classes were inspected. A state write invokes `onBlockAdded` even when only LIT
changes. Torch writes therefore invoke the definition's Added callback before
ordinary and shape notifications. That callback notifies around each of the six
adjacent blocks in source order. Feedback can reserve a two-tick callback during
these nested notifications, before the later 160-tick request. No clock-specific
exception is used. The same audit corrected comparator POWERED writes: inherited
Added output notifications precede shapes, and the explicit update notification
also follows them.

Support-power sampling, support-excluding weak emission, upward strong emission,
standing/wall native identity and notification routing are shared primitives.
Synthetic and observed native names use the same scheduler identity. Standing
and wall torches remain different identities; rotations of a wall torch do not
change its identity. Position history is independent of both identities.

## Checkpoint, exploration and observation

History effects participate in the runtime's transactional state update and are
recorded in traces. Checkpoints preserve exact clocks, histories, continuations,
queues and outputs. Root comparison preserves each live event's age and every
pending delay. Its positive comparison epoch leaves room for the oldest live
age; moving all states to tick 1 would lose that information. Expired entries
can be omitted from a root representative because no later history query can
count them. Checkpoints and root representatives remain opaque, process-local
objects; no new disk checkpoint importer was added.

Fresh construction explicitly starts with empty device history. A snapshot's
LIT property does not establish that history, and an empty coordinate can still
have recent server-side burnout history. Teardown/rebuild does not itself clear
that position history. Current previews expose applicable device initial-state
assumptions inside `fresh_target_review.device_initial_conditions`, including
whether an output starts at zero and whether unobserved position history is
assumed empty. This keeps the accepted [observed-state recovery workflow](live-operation-readiness-and-recovery.md);
it does not introduce a companion MOD requirement or an atomic readiness claim.
Full-region observation at each [model-reviewed construction batch](construction-batching.md)
and mismatch handling remain necessary, and a
matching block-state readback is not functional certification of hidden state.

The shared exporter preserves standing/wall names, facing and LIT, including
fresh unlit command placement that subsequently relights. Moving a torch as a
piston payload, support-destruction/drop behavior, particles/sounds, entities,
chunk unloading and reconstruction of live hidden state are outside this scope.
No new public MCP operation or live Minecraft trial was introduced.

## Verification and revisions

The shared runtime matches torch LIT in all 3,888 retained isolated-torch
samples, including the inclusive 60-tick boundary, and all 641 retained
autonomous feedback-clock samples. These compare recorded support inputs/output ticks; they do not invent
an unrecorded internal live callback trace. Source inspection and ordering tests
separately check why the short feedback callback wins.

Tests also cover all five mounts, replacement across concrete identities,
independent histories for multiple torches in one scheduler, exact restoration
through burnout effects, root restoration with another tick in the ready batch,
expiry under clock normalization, and lossless command construction/teardown.
Finite agreement and model tests do not establish arbitrary-world conformance.

Execution/root exploration is v13, device programs v7, physical admission v4,
the synchronous runtime record v5 and root comparison v3. Previous saved
contexts require fresh review; no checkpoint conversion is provided. See the
[integration roadmap](device-integration-roadmap.md) for final verification.
