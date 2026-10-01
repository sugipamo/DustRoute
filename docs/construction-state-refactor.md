# Construction state and runtime architecture cleanup

Continue `codex/large-circuit-regions` after the shared-world phase. Scope is the
five agreed tasks, in order. No new blocks, physics, live permissions or server
observation shortcuts are introduced. Stop if an undeclared prerequisite or a
large blocker needs doing first.

1. **Literal input preparation:** `LiteralSnapshotIndex` borrows one immutable
   snapshot's records, checks bounds/duplicates (including Air duplicates),
   produces canonical sparse state and compares coordinates. Modification
   construction reuses its initial index; placement requests use coordinate
   lookup instead of repeated vector searches. The index proves representation,
   not supported physics, observation coverage, ownership or live readiness.
2. **Partition phases:** separate subdivision, support/watch dependencies,
   dependency components and deterministic ordering in `work_regions/partition`.
   `ElectricalWorkPlan` remains the intention/admission/physical-proof boundary.
   Algorithms, exact constituent parts, bounded alternate initialization and
   reverse proof requirements are preserved.
3. **Budgets/errors/retention:** named model budgets are separate from live
   transport limits, policy and retention caps. The 64-node graph cap explicitly
   comes from the u64 representation. Literal validation and modification
   admission expose typed Rust errors; MCP renders these at its existing string
   boundary. Generic underlying model messages remain detailed strings within
   the model-error variant. Retention accounting now lives separately from
   preview rendering.
4. **Immutable expectations:** `ExpectedState` retains shared full bases and
   deltas including property changes and Air tombstones. Chains reset after 32
   deltas, bounding traversal and drop depth. Unchanged states share storage.
   Lookup, count and bounds do not retain materialized full snapshots. Live batch
   readback explicitly materializes the complete expected state, and checks all
   context cells as before. Equality remains exact content equality.
   Public and saved JSON retains the full snapshot shape. Thus archive file size
   and expanded API output are not reduced by this phase. Saved reconstruction
   and removal step sequences decode incrementally into shared internal states;
   deserialization never restores private batching evidence or executable proof.
   Preview expansion uses logical record counts; retained-cache admission counts
   unique internal allocations and conservative heap estimates. Neither limit
   is a whole-process RSS bound. Large previews must not expand simply because
   retained states became smaller.
5. **Metadata measurement and selected sharing:** compare synthetic read-only
   events with output registers, histories or pending queues populated. Output
   and history copies/scans dominate the initial probe, so those two immutable
   maps share storage and detach on mutation. Unchanged maps omit difference
   scans. Output assignments still validate block identity/value before a
   same-value shortcut. History effects retain all policy/position/clock checks.
   Root normalization detaches histories before pruning or changing ages;
   exact checkpoints preserve original history. Queue/carrier staging, ordering
   and carrier validation stay as before. Broader queue changes are not needed
   for this milestone.

Persistent expected states are an internal model representation, not captured
Minecraft hidden state. Materialized snapshots and decoded archives do not
replace fresh physical reproof or current live observations. Atomic Rust state
rollback does not imply atomic live world writes.

Verification and paired standalone debug measurements are recorded with source
fingerprints. Passive construction timings and synthetic metadata timings are
separate from actual Minecraft behavior and live placement time.

## Measurement

Three paired standalone debug sweeps alternated before/after execution with no
concurrent compiler/server. At 4,096 context blocks and 64 requested additions,
128 forward/inverse commands still represent 520,192 complete expected block
records. Internal retained records fell from 520,192 to 16,380 (96.9% lower).
Standalone sweep peak RSS medians fell from 69,560 KiB to 21,288 KiB (69.4%
lower). Proof time medians were 5.868 seconds before and 6.015 seconds after;
these runs do not establish a speed improvement. Ranges overlap and each pair
differs by only a few percent. This phase's measured gain is retained memory.
Archive JSON size, transient API serialization, full live readbacks and total
service RSS are separate costs.

The [construction measurement](measurements/construction-state-refactor-20261001.json)
retains all six sweeps and fingerprints. The example's v2 record separates
logical expanded states from actual retained block allocations; its v1 baseline
counter counted full retained snapshots. The 64-block row includes cold registry
initialization. The fixture is passive construction, not an arbitrary active
circuit or end-to-end live placement benchmark.

The [metadata measurement](measurements/runtime-metadata-sharing-20261001.json)
is a synthetic core-overhead probe with 4,096 static blocks and 128 read-only
events; setup is excluded. With 4,096 metadata entries, median output-map time
fell from 752.010 ms to 0.427 ms and history-map time from 1,996.027 ms to
0.434 ms. Queues, whose implementation was unchanged, measured 50.362 ms
before and 48.117 ms after. These are not performance claims for changing
history effects or real circuits. Changed maps still detach and receive complete
validation. Initial metadata setup and queued-event admission have their own
costs outside these timings.

## Offline regression checks

717 distinct cases passed: 318 Minecraft library/integration cases, 179
translate library cases, 62 integrations across 17 construction/door/flight/
building/shape/crop/scope suites, and 158 native MCP library cases. The Minecraft
full suite passed 317 cases before the additional metadata transaction test;
its synchronous suite then passed all 27 cases including that new regression.
The four ignored MCP cases are explicitly opt-in performance probes, not skipped
behavioral failures.

Checks cover property changes, deletion and restoration across delta-chain
rollovers; exact JSON shape; streamed historical decoding without private
batching evidence; typed input/budget errors; invalid late mutations preserving
world, output and history checkpoints; transient protected writes; and public
placement, diagnosis, repair, removal and restart paths.

Minecraft all-target, translate library/scaling-example, and native MCP
all-target Clippy passed with warnings denied. The three packages passed
format checks, and the working diff passed whitespace checks. Compiler jobs
and test execution were serialized; no benchmark or private server ran
concurrently with Cargo.
