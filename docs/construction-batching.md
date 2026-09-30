# Model-reviewed construction batches

The shared constructor and differential editor now group consecutive command
steps when each freshly simulated command root finishes without pending work.
The original command order, support/watch dependencies and per-command expected
snapshots are retained. This is an execution schedule change for the current
Java 1.21.11 model, not a new block capability or survival-building path.

## Admission and execution

The model completes one submitted command and its synchronous callbacks before
checking `pending_count`, the input boundary and that no modeled ticks elapsed.
Checking only elapsed ticks after complete settling would miss queued block
events at the same tick. A command with any remaining work keeps its own live
readback and settling boundary. Consecutive immediately idle commands share one
boundary, with a maximum of 32 writes. Removal and installation groups remain separate.

The private per-command result is omitted from serialized steps and defaults
to false on deserialization. Historical or caller-supplied JSON cannot enable
batching. All normal placement paths freshly rebuild the model and compare it
with the preview before executing. Saved operation records remain history and
never restore executable placement capabilities.

Plans and previews expose `execution_batches` and, where applicable,
`undo_execution_batches`, with their original step ranges, write counts and
wait ticks. Each group verifies the entire expected predecessor region,
persists its readback, submits commands in order, waits for the modeled settling
time plus the existing margin, and checks the complete resulting region. The
20-tick stationary intervals at differential planning, preview and apply are
unchanged. A quiet group uses the existing four-tick margin once; the native
transport also retains its two-tick quiet wait per submission.

A batch is not an atomic server transaction. If submission or verification
fails, no step within that batch is reported verified. Errors include its
original step range. Durable progress retains the last completely verified
prefix, and a failure stays `needs_inspection`, without automatic retry or
rollback. A later reconstruction starts from a new observation. Intermediate
states within a group are model predictions rather than separate live
readbacks. Existing assumptions about empty initial queues, completed prior
motion, external input exclusion and hidden histories still apply.

## Verification and measurement

Model checks cover preserved prefixes, the 32-write limit, loss of batching
authority through serialization, powered-piston settling boundaries and
observer initialization/order. Public MCP checks cover preview/application,
undo, drift and incomplete evidence, partial batch submission, restart history
and reconstruction after an uncertain attempt. Offline transport fixtures are
workflow tests, not Minecraft physics evidence.

The opt-in `profile_native_construction_batches` probe measures the actual
shared executor with a dedicated native test actor on an initially observed
empty region. It compares the same freshly modeled command sequence using
per-command readbacks and model-reviewed batches, for two and sixteen stone
blocks. Every group checks its complete region; verified teardown restores the
fixture to air between samples. Ordinary tests never run this probe.

The native probe measures submission, waits and region verification. It does
not include MCP dispatch, planning, preview, stationary prechecks or durable
checkpoint callback writes. Those costs must be reported separately when
estimating a complete user workflow. No timing upper bound or functional
conformance for arbitrary active circuits follows from the passive fixture.

The live results and raw readback receipts are retained in
[`measurements/construction-batching-live-20260930.json`](measurements/construction-batching-live-20260930.json).
Three samples per schedule gave executor medians of **619.458 → 309.782 ms**
for two blocks and **4,944.087 → 310.124 ms** for sixteen blocks. Both compare
the same current executor and sequence, changing only the per-command versus
batched observation schedule. This is not a comparison with an older server
binary. The 896-cell fixture returned to completely observed air after testing.
The temporary test player disconnected, and the existing server and MCP stayed
running. The new code will be used by the main MCP after its next normal rebuild
and start.

The corresponding offline public-handler measurement preserves real durable
checkpoint writes and verifies the two-block edit uses one submission, four
region scans, three checkpoints, and five file/directory syncs. It does not
sleep for Minecraft ticks. See [performance observation](performance-observation.md#model-reviewed-construction-batches)
for that evidence and the separate accounting of complete-workflow waits.

## Validation record (2026-09-30)

- Model unit tests: four passed, covering grouping bounds, preserved prefixes,
  removal/installation separation, deserialization and active event boundaries.
- Construction integrations: thirteen passed across electrical construction,
  differential modification, adhesion, flying machines, stairs and the retained
  observer command regression.
- Public differential-edit tests: seven passed, including conditional undo,
  uncertain full/partial submission, no retry, restart history and policy/drift
  rejection.
- Existing Assembly lifecycle checks: four passed on the initial run; the
  interruption/reconstruction test initially retained a per-command verified
  prefix expectation. It passed after updating that expectation to completed
  batches and verifying that uncertain prior attempts survive restart.
- Both opt-in measurement tests passed: twelve live apply samples and the final
  verified air scope; three offline public-handler samples with real durable
  writes and asserted phase counts.
- Clippy with warnings denied passed for the affected Translate library and six
  integration targets, and MCP all targets with and without the native feature.
  Formatting and whitespace checks passed.

The broader combined all-targets Clippy command encounters existing unused
`observed_cane_fixture` / `observed_cane_initial` functions imported by the
unmodified Translate `flying_machine_assembly_fixture` example. That pre-existing
warning is outside this construction change; the example was not modified.
Ordinary regression tests do not connect to Minecraft. Live timings are bounded
passive-fixture evidence, not active-circuit conformance.
