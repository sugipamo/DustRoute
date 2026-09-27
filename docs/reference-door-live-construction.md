# Reference door: live construction counterexample and next work

Status: **v6 command-placement repair and isolated public workflow verified**.
Three initial isolated Java 1.21.11 trials reproduced the same placement failure.
The retained counterexample below describes those v5 trials. The current v6
runtime models command preprocessing and post-write ordering; observer
initialization is explicit in the generated construction plan. Physical Laws
and the source Assembly geometry are unchanged.

## Repaired public workflow result

A new isolated R0 trial at `(61000,180,1000)` on Java 1.21.11 passed:

| Check | Result |
| --- | --- |
| Fresh public proposal/review/adoption, then MCP restart | Passed under v6 |
| Ordinary replace command construction | 43 / 43 full-region readbacks |
| Normal operation | Close/open twice; all four exact region and nine-cell aperture probes matched |
| Application after placement restart | Registry and fresh observation/review matched |
| Missing client chunks / externally changed world | Removal planning refused |
| Missing preview / changed world after preview | Writes refused |
| Public removal after restoring the declared open state | 43 / 43 full-region readbacks; region empty |
| Removal after restart | Persisted removed state matched; no repeated removal eligibility |
| Test cleanup | Empty region verified, force load removed, server stopped normally |

Unlike the initial failed trials, public removal completed before test cleanup.
At the former failing observer, the capture shows six pre-write shape callbacks
with `powered=true`, no scheduled insertion pulse, then flags-258 ON and
flags-18 OFF commits in the same game tick (77344, sequences 12048–12055).
Actual lever state commits occurred at game ticks 77900, 78132, 78233 and 78472.
These are measured application times, not inferred from the client's 100-tick
wait. Each resulting snapshot was checked across the complete known region.

The continuous capture retains 50,087 consecutive records with zero suppressed
or evicted records and zero write errors. Its legacy input-trigger field remains
false because this fixture's lever is outside the recorder's default trigger;
continuous mode nevertheless retains all four actual input packets and lever
commits. No bounded-trigger or complete-readiness claim is inferred from that
field. See the [repair manifest](evidence/reference-door-command-v6-20260927.json)
for source/artifact hashes, registry attempts, input evidence and validation.

The subsequent [relocation matrix](reference-door-relocation.md) verified this
same public workflow at two new anchors and all four horizontal rotations,
with fresh target simulation and static placement-dependency audits. It adds
eight finite live trials; the completion-observation work below remains separate.

## Current command semantics

The v6 adapter models `/setblock <exact state> replace` with flags 258:

1. Requested-state shape callbacks run in west, east, north, south, down, up
   order while the position still contains Air. An observer's pending tick uses
   the requested observer identity. Each boundary can be checkpointed.
2. The explicit state is written and its on-added callback drains. The World
   shape callbacks run only if that requested state remains.
3. The command performs ordinary neighbor notifications. Removal writes Air,
   drains shape callbacks, runs the old block's removal callback, then performs
   the command's ordinary notifications.

Construction exports all supported properties explicitly. Java copies those
properties back after preprocessing: wire shape and repeater lock transforms
have no retained state or scheduling side effects at this entry. Unsupported
support is rejected before preprocessing. Admitted passive materials, pistons,
lamps, levers and redstone blocks do not enqueue shape work. Observer scheduling
is retained, including the old failing unpowered command. This scope does not
model partially specified commands, block-entity data, strict placement or
unsupported support destruction.

The planner retains support and occupied-watched-cell precedence, then explicitly
requests each declared OFF observer as `powered=true`. Java on-added resets it
OFF with flags 18; it does not schedule an insertion pulse or run the replaced
outer state's shape callbacks. The actual command is visible in each preview
step, and the expected full-region readback contains the settled OFF state.
The original 43 blocks, declared initial state, materials, Laws and source
references are not rewritten. Every insertion and removal is simulated, and
failure to reach the exact freshly reviewed world still rejects the plan.

Runtime/exploration v5 records are rejected, not converted or reinterpreted.
New requests select v6 and require fresh adoption. The catalog/type and proposal
history container versions do not change: only their explicit execution pins
change. No stored user catalog is migrated.

The retained Java command prefix and failure snapshots now form a Rust regression:
the original unpowered commands reproduce the displaced quartz. A separate
six-direction test checks scheduling while the world cell is Air and resumes
from every suspended callback boundary. The powered initialization test checks
that no delayed pulse is created.

## Initial failed trials (v5)

The public MCP client imported the ordinary-door type and explicit v3 candidate,
reviewed and adopted it, restarted MCP, planned the transformed target at
`(61000,180,1000)` with R0, checked preview/baseline guards, then applied the
ordinary `/setblock ... replace` sequence. It verified **37 of 43 stages**.
After the 38th installation, whole-region readback differed from the model and
MCP consumed the attempt as `needs_inspection`, with retry and automatic rollback
disabled. Stages 39–43, normal input cycles and public removal were not executed.

The test harness subsequently cleared only its initially verified empty,
owned region, verified it empty, removed the force load and stopped the private
server normally. This cleanup is **not** a successful MCP undo/removal. The
isolated registry preserves the failed attempt; cleanup does not promote it to
applied or removed. User catalogs and worlds were not used.

| Observation | Result |
| --- | --- |
| Installation 38 | Observer at relative `(0,3,1)`, serialized facing down |
| Model's expected quartz | Relative `(0,5,0)` |
| Actual quartz after failure | Relative `(0,6,0)` |
| Additional 100 client ticks | Same two differing cells, in trials B and C |
| Capture C | Continuous, no evicted/suppressed records or write errors |
| Captures A/B | Bounded capture without an input; insufficient construction history |

Trial B exposed a diagnostic configuration bug: a nonnegative
`max_ticks_after_input` forces the recorder into bounded mode even when its
mode argument says continuous. The harness now omits that limit for continuous
recording and checks the actual artifact header before starting the client.
Trial C supplies the construction history; no complete-history claim is made
for A or B.

## Missing behavior in the v5 model

The recorded Java sequence for installation 38 is:

| Game tick / order | Observed event |
| --- | --- |
| 76039, before the palette write | Shape callbacks see the requested observer state while the actual world cell is still Air |
| Sequence 12197 | Observer tick becomes queued while the cell is still Air |
| Sequence 12199 | Requested observer is written, flags 258 |
| 76041 | Observer turns ON; the upper piston extends |
| 76043 | Observer turns OFF; piston event type 2 retracts the body and leaves quartz above |
| 76045 | Stable retracted body is restored |

The locally cached, mapped Java 1.21.11 bytecode confirms
`BlockStateArgument.setBlockState` calls `Block.postProcessState` before
`ServerWorld.setBlockState`. The bridge invokes this command path. In contrast,
the v5 `electrical::install` wrote the block first, then runs `ElectricalAdded` and
the outer notifications. It did not model the command's pre-write shape
callbacks with a requested state different from the current world state.

The earlier observer-front placement ordering fixed a failure in the direct
installation model. It cannot suppress a pulse already scheduled by command
preprocessing. Existing normal-operation conformance from an initialized world
and the ordinary-door model type proof remain distinct evidence.

The [retained fixture](../crates/dustroute-translate/tests/fixtures/reference-door-command-placement-v1.json)
contains the expected verified prefix, failed step, two full client readbacks,
and a contiguous 291-record raw window. The
[audit manifest](evidence/reference-door-live-construction-20260927.json)
links all three attempts, original artifacts, source fingerprints and checks.
Inspect the evidence without contacting Minecraft:

```sh
python3 tools/analyze_door_command_placement.py crates/dustroute-translate/tests/fixtures/reference-door-command-placement-v1.json
python3 -m unittest discover -s tools -p 'test_door_command_placement.py'
```

## Initial stabilization before the approved repair

- Exact full-region and 3×3 aperture probes are available for two normal cycles;
  those probes were prepared but not reached in these failed placement trials.
- Failed worlds are sampled before cleanup and after an additional bounded wait.
- Continuous construction capture is region-bounded and its actual mode checked.
- Capture provenance pins the loaded actor, runner, bridge and MCP executable.
- Gradle workers are limited to one for the private test server.
- `ValidatedAssemblyPlacement::review_target` temporarily refused observer-containing
  worlds. The same gate handles new plans and fresh revalidation of saved
  placement/removal data. Model-only behavior review and explicit adoption remain
  available. This is a restriction of the public command path, not a physics fix.

## Approved scope: steps 1–3 completed; step 4 remains

1. **Model the actual command placement entry.** Represent requested-state
   postprocessing, its six shape callbacks and queued effects before the world
   write, followed by the command's write flags and notifications. Preserve
   callback order, pending block identities and restoration across the new
   boundaries. Audit every admitted block kind, not just this observer cell.
2. **Recompute construction and removal.** Re-evaluate the existing order under
   that entry. Find an order/explicit initialization that reaches the exact
   declared world, or reject it before writes. Do not silently use strict writes,
   rewrite materials, add input filters or relabel an old pass. Decide any profile
   or saved-format revision from the actual execution-state change, and reverify
   explicit new candidate data where required.
3. **Repeat the isolated public workflow.** Require all 43 placement readbacks,
   two ordinary open/close cycles, MCP restart, and conditional public removal
   with all 43 readbacks. Include changed-world and missing-observation refusals.
   Only then lift the observer placement guard for the proven scope.
4. **Design live completion evidence.** The present client sees block snapshots;
   its waits count client ticks. Two matching snapshots or a visible opening do
   not reveal queued observer/repeater ticks, piston events or carrier history.
   The existing recorder observes executed ticks and some callback-local queue
   queries, but does not expose a complete atomic readiness observation to MCP.
   Start with an opt-in read-only measurement at a server tick boundary for this
   bounded door: input identity, full region, relevant pending events and carriers.
   Missing history, unloaded chunks or a capture gap must remain unknown. Specify
   how this evidence reaches the bridge and expires before authorizing a later
   action. Do not impose world quiescence on the general type: unrelated periodic
   devices require their own supported evidence, or an unknown result.

The user approved steps 1–3 after review of this plan. Step 4 remains separate:
it adds a server-observation capability and protocol design. A successful finite
live construction trial cannot certify that readiness interface.

The user subsequently selected [recovery after damage](live-operation-readiness-and-recovery.md)
as the operational priority. A companion MOD is not a prerequisite for the
observed-state reconstruction path. Server-side atomic check-and-action and
complete readiness remain a deferred stronger option.

## Validation of the repair

All selected Rust regressions passed: **96 tests**, covering command installation,
all-direction callback/checkpoint resumption, retained Java failure replay,
construction/teardown under all Y rotations, execution pins, ordinary-door
review and public MCP persistence/readback guards. The two offline retained-
evidence checks also passed. Clippy for all targets of the four affected crates
with warnings denied, formatting, Node syntax and diff checks passed. Builds
and tests ran serially with offline locked dependencies. The isolated server
was stopped before the final Rust test run.
