# Region-based circuit work

The practical target is to prototype and revise circuits containing thousands
of blocks in a completely observed context, with bounded, separately reviewed
work regions. A hierarchy summary is not a whole-circuit behavioral proof.

Current live backend: Voxrig only. Mineflayer was removed on 2026-10-03; its
retained comparisons below describe the migration audit, not an available
backend. For a task overview, read [workflows](workflows.md#try-a-change-before-applying-it)
([日本語](workflows.ja.md#反映前に変更を試す)).

Implemented migration order:

1. Expose observation limits/evidence at the backend boundary. Native Voxrig
   reads reconstructed client state. The retired Mineflayer bridge checked
   command predicates; its 8,880-cell command limit does not govern Voxrig.
2. Make explicit coordinate capture independent of gaze. Still resolve the
   assisted player's actual dimension and require every requested cell loaded.
3. Persist region work intentions, immutable before/target states and verified
   progress. Keep only the current region's executable model proof in memory.
   Plan, preview, apply and read back each region against the **whole** context.
   After restart, reacquire observations and generate a new operation.
4. Verify cross-region support, observer precedence, notifications and power,
   drift, ambiguous writes, restart and superseded previews. Measure realistic
   workloads before raising remaining model or transport limits.

The job path uses literal Circuit Revisions at their captured site: at most
4,096 non-Air blocks, 4,096 virtual changes and 64 disjoint input work regions.
Every changed coordinate must be covered. Oversized input regions are split
along the longest changed-coordinate axis at a median distinct coordinate.
Support/watch cycles are combined into a single stage if their total fits 64
declared changes. The result must have at most 64 stages of 64 declared changes;
an oversized cycle or excessive subdivision is refused with a partition hint.
Merged stages retain their exact constituent parts: their display bounding box
does not enlarge editable space. The full live context includes the original
one-cell guard and must fit both policy and adapter limits. All other observed
cells remain protected unless explicitly admitted by edit_scope.

Support/watch dependencies select a candidate stage order. Independent ready
stages use the same Rust construction policy as ordinary construction, favoring
structure before wiring/control/power. Ordering is not a physical proof. Only
the current stage receives fresh whole-context forward/inverse simulation,
including protected states at every committed microstep. The simulator receives
only that stage's explicit positions; natural changes elsewhere in editable
space belong to its settled boundary, not to additional placement commands.

If initializing with final output properties fails, there is one alternate:
initialize eligible stateless device power booleans to false and wire power to
zero, using existing device bindings. Devices with Use callbacks or history
are excluded. Block identities, geometry, input settings, edit_scope and the
immutable final target are unchanged. For example, a supporting lamp can be
installed OFF, then light naturally when the next region installs its powered
floor lever. This bounded initialization is not a search for arbitrary circuit
intermediates. The last stage must match the complete immutable target exactly.
Each inverse must restore the entire preceding verified boundary using only
the current stage's positions. Later stages remain unverified until planned.

A stage already satisfied by earlier natural updates has no block commands.
It still needs fresh full-context stationary observations, a new preview and
explicit confirmation before progress advances. Its inverse is also freshly
verified; no-op stages never grant permission to skip these checks.

Jobs are durable intentions/history, not restored validation capabilities.
The `dustroute.construction-job.v3` format stores sparse verified boundary
deltas, including natural updates. After restart it reconstructs the current
literal boundary from these deltas, then freshly proves only the next stage.
It never projects old progress from final properties or replays all preceding
physical proofs. Retired v1/v2 JSON files are preserved separately and refused by the current
reader; explicitly recapture and create a v3 job in the non-JSON store. Ordinary
electrical operation history remains readable through `get_operation`.
Failed or uncertain attempts stop with needs_inspection. There is no automatic
retry, rollback, chunk loading or world lock. Undo must proceed in reverse region
order and receives a fresh reviewed operation, including after restart. A
cancelled job retains its history and makes forward previews unusable. Its
verified prefix can still receive fresh inverse cleanup plans; cleanup does not
reenable forward work.

Fresh-target adopted Assembly construction, building generators, whole-circuit
functional budgets, entities, survival inventory and arbitrary unloaded worlds
retain their existing separate limits. This migration does not certify arbitrary
large circuits or atomic server execution.

## Adapter audit

| Contract | Result |
| --- | --- |
| 8,880-cell command confirmation | Historical Mineflayer-only limit; that backend is retired. Native Voxrig reports its 262,144-cell loaded-region limit, separately from user policy. |
| Gaze required even for explicit coordinates | Removed. Native coordinate capture uses received player/dimension context, without a raycast or guessed eye pose. Gaze/discovery options cannot be combined with explicit region. |
| 256-block generic gaze option versus native 64-block raycast | Adapter range is reported explicitly and checked before dispatch. It no longer restricts coordinate capture. |
| Dense transport arrays consuming a 4,096-block revision limit | Removed: native Air records are normalized before counting non-Air content. Revision base states and job intentions are saved sparsely with complete known bounds; raw observation receipts/content IDs remain unchanged. |
| 64 virtual changes | Replaced with a 4,096-change offline revision budget. Live work is still bounded to 64 changed coordinates per separately reviewed region. |
| Approximate generic baseline ignoring dynamic properties | Native literal revision edits use exact full-context common-runtime state comparisons, including passive geometry. The old Mineflayer compatibility route is retired. |
| Predicate stair corrections and server clock evidence | Retained only in historical Mineflayer evidence. Native records remain client reconstructions; server confirmation is not fabricated. |
| Native packet pacing and local wait timeout allowance | Already native-specific; retained. Local waits use a client clock, not a server tick guarantee. |
| Whole-context readback, 32-write idle batches, stationary samples, protected microsteps, consumed attempts | Retained as construction/observation safeguards, independent of Mineflayer. Removing them would need separate physics and recovery evidence. |
| Fresh Assembly/building size budgets and exhaustive analysis budgets | Retained as separate model/generator limits. Region jobs do not silently change adoption, child pins or functional requirements. |

For a coupled power/support/watch layout, a region boundary is a work boundary,
never a cut in the physical world or queue. Model refusal is an unsupported
partition or intermediate target, not evidence of a broken Minecraft circuit.
An interrupted partial prefix is deliberately not reconstructed from its command
count: examine the full observation and author a new explicit repair revision.

## Cost and resource limits

Let N be sparse non-Air context blocks, V observed cells, D requested positions
in one stage, R stages, and E processed physical microsteps. These quantities
are separate: a 64-position stage can naturally update more than 64 blocks.

| Work | Cost and consequence |
| --- | --- |
| Partition selection | Deterministic spatial subdivision plus word-sized dependency closure, O(R²) for R ≤ 64. There is no permutation search or eager proof of every future stage. |
| Support ordering during removal | Build the current support index once per settled command, then query it for candidates. This removes the previous repeated per-candidate whole-world scan; the index is rebuilt after physical updates. |
| Physical proof | Runtime stages queue/history/carrier state transactionally. Read-only events share the world; nonempty deltas detach it once. Initial protection admission inspects the full context, then each committed event checks its delta positions. Hashing, expected snapshots and metadata still add cost. See [runtime state sharing](runtime-state-sharing.md). A stage has at most two initialization attempts. |
| Retained proof | Per-command expected snapshots need O(DN) block records for forward/inverse checks. Arc sharing avoids copying the proof when looking up a plan. Only one executable proof per job is retained. Applying still creates a fresh proof, so this is not a total process-memory bound. |
| Saved boundary | Sparse actual-state deltas, at worst O(RN), rather than all commands' full snapshots. Capacity for the next completed boundary is checked before writes. Restart materializes saved deltas without physical replay. |
| Live readback | O(V) cells for each full-context sample. V obeys backend and policy limits independently of sparse model N. Loaded-client capture timing does not include planning, preview or application. |

The electrical plan cache permits 256 entries, at most 1,048,576 retained block
records and a conservative 128 MiB estimated retained heap. The heap estimate
charges the initial property-map allocation as well as per-entry/string space;
a small property map still allocates a complete tree node. It is not an RSS
limit and excludes fresh proofs, runtime state, transport and other
caches. A stage too large for retention is refused before writes and should use
smaller regions. Existing plans for other jobs are not silently evicted. Durable
jobs retain their existing 16 MiB and 256-attempt limits; the next boundary's
storage budget is reserved before application.

Large previews retain every command's position, state and wait, but summarize
repeated expected worlds by count. Executors keep complete states. Settled
differences and boundary summaries expose counts and explicit truncation flags.
The changed presentation explicitly identifies electrical-edit-preview.v2 and
construction-job-response.v2 rather than silently reusing an old shape.
Small previews remain expanded. For large job history, use
`manage_construction_job({job_id,action:"get",include_intention:true})` to
explicitly request all saved intentions and deltas; this does not restore a plan.

`cargo run --offline --locked -j 1 -p dustroute-translate --example
work_region_scaling` measures one passive 64-position forward/inverse proof,
with full-context snapshots. The example is a construction-cost probe, not a
functional circuit test. Single debug-profile measurements were:

| Non-Air context N | Before changes, seconds | Final measurement, seconds |
| --- | ---: | ---: |
| 64 | 1.34 | 1.26 |
| 256 | 0.63 | 0.57 |
| 1,024 | 2.83 | 2.41 |
| 2,048 | 7.24 | 4.88 |
| 4,096 | 13.59 | 9.99 |

Separate-process peak RSS over each sweep was 68.2 MiB before and 69.8 MiB in
the final measurement, excluding compilation. An intermediate development
measurement at N=4,096 took 15.93 seconds. The final sample is faster, but these
single-run measurements do **not** establish a reproducible speedup attributable
to this change. These samples precede the runtime sharing/incremental-check phase; its paired
comparison is recorded separately in [runtime state sharing](runtime-state-sharing.md).
Host scheduling and cold registry initialization also affect individual runs. The first N=64 sample
includes registry initialization, so it is not comparable with warm samples.
At N=4,096, all runs retained 520,192 per-command expected block records. The
bounded preview test keeps all 64 commands in less than 64 KiB for its 512-block
fixture while retaining the complete executor states.
The [measurement record](measurements/work-region-scaling-20261001.json) retains
all three sweeps, fixture scope and source fingerprints. Planning and application
each perform a fresh proof; these times are not end-to-end placement latency.

## Verification

Offline tests cover cross-region support/watch precedence, protected power
changes and coupled power partitions, cycles, coverage/overlap limits, large
native-Air revision input, ownership/read-only enforcement, superseded previews,
explicit unchanged-baseline recovery, partial-write refusal, process restart and
reverse undo. Large-model cases and isolated native trials are recorded separately
from the transport fixture; the fixture does not serve as a Minecraft oracle.

The large passive model test adds 2,048 stone blocks across all 32 stages of
64 changes, generating and dropping each whole-context forward/inverse proof
in sequence. All eight partition/intermediate tests passed in 115.85 seconds in the debug
profile. This is construction evidence for a passive layout, not exhaustive
functional verification of a 2,048-block active circuit.

The nine public job lifecycle tests cover a temporary OFF lamp,
later natural power changes, no-write checkpoints, permanent forward
cancellation with restart, inverse cleanup and refusal to execute v1 history.
Electrical edit, revision and bridge regressions pass. Before v2, the default
MCP library regression run passed
144 tests with two opt-in tests ignored; its two outdated tool-count assertions
were updated and passed on individual reruns. Native MCP all-target Clippy and
translate-library Clippy pass with warnings denied. The unrelated translate
example's existing dead-code warnings are outside this migration. The v2
regression also passes all nine construction integration cases covering pistons,
adhesion, stairs and a flying-machine course, plus three electrical-modification
cases and ten MCP edit/presentation tests.

The opt-in `tools/verify_blueprint_iteration_live.py --region-jobs` trial uses
a stopped, private Vanilla Java 1.21.11 server and two dummy native clients.
Its declared owned fixture checks gaze-independent 32,768-cell capture, two
40-change regions with a floor lever depending on support in the other region,
MCP process restarts, protected drift refusal, reverse undo and cancellation.
The updated trial also checks a temporary OFF support lamp that naturally lights
after installing its powered lever, and separately a naturally satisfied region
whose forward and inverse checkpoints issue no block writes.
Console predicates independently confirm complete stable checkpoints and the
final empty fixture. These sequential predicates are test evidence, not an
atomic server observation or a production fallback from native reconstruction.

The [initial 2026-10-01 native trial evidence](evidence/large-circuit-regions-20261001.json)
records a successful 22-call public workflow with three expected refusals, two
MCP restarts, 160 verified public write steps and two isolated guard-test writes.
All seven checkpoints (82,808 independently checked cells) matched. The final
39,304-cell owned context was completely Air and all three MCP processes and the
server exited normally. The 32,768-cell explicit capture took 314.2 ms in this
single loaded debug fixture; this does not predict unloaded-world performance.
The separate initial trial stopped before planning/writes because of an invalid
fixture block-name encoding; its retained evidence identifies the corrected input.

The [derived-state native trial](evidence/derived-region-states-20261001.json)
passes 42 public calls with three expected refusals, two MCP restarts, 164
verified public write steps and two owned guard-test writes. All 11 checkpoints
(86,168 independently checked cells) agree. This trial places the support lamp
OFF and later observes it ON without adding that lamp to the next stage's
commands. It separately confirms zero-write forward and inverse checkpoints,
then restores their baseline. The final 39,304-cell context is entirely Air;
all MCP processes and the server exit normally. Its loaded 32,768-cell capture
takes 323.9 ms. The evidence pins the physical implementation before the final
addition of response schema identifiers and a more conservative property-map
retention estimate. The response identifiers and public lifecycle are checked
by the final public-tool regression. This finite trial does not certify arbitrary
active circuits or hidden server queues.

`work_region_scaling` now prints named, human-readable measurements rather than JSON. Its passive proof and retained-state calculation are unchanged.
