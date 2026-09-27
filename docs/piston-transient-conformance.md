# Piston transient conformance

Work started 2026-09-26. Target: the existing isolated Java 1.21.11 server,
Yarn 1.21.11+build.6, redstone experiments disabled. The reference is actual
server behavior, not historical model output. This work extends measurement
and corrects demonstrated mismatches within the existing electrical subset.

## Declared comparison

- Upward and horizontal sticky bodies: ON, OFF after a short pulse, re-ON during
  retraction, and a later settled OFF.
- Downward ordinary body: short-pulse OFF and subsequent operation.
- Upward/downward sticky bodies: OFF requested at the two/three-tick boundary,
  re-ON one tick later; a separate settled-pull reference.
- Horizontal/downward sticky bodies sharing a destination: both same-tick input
  orders, followed by interruption. Actual applied intervals and carrier phases,
  rather than requested client delays, determine which boundary was exercised.

No slime/honey, entities, destructive components, general block entities,
vertical-piston payloads or multiple-piston payload chains are added. A required
prerequisite outside that scope stops work for a user report. No host repair is
part of this work.

## Observation boundary

The companion instrumentation now optionally records a bounded spatial trace:

- Actual `ChunkSection.setBlockState` palette commits before onBlockAdded and
  neighbor callbacks, with before/after identity and write flags. The delegate
  executes exactly once and its original result is returned.
- Piston block-event entry/return, event type and facing data.
- Carrier tick/forced-finish entry/return, progress, lastProgress, saved world
  time, payload, source/extending flags and removed state.
- Ordinary neighborUpdate execution throughout the declared region. Existing
  kind-filtered observation streams remain available outside that region.

The offline comparison reconstructs the live world from commits and the model
world from atomic deltas. It compares the complete known world at delivered
piston-event boundaries, carrier boundaries and ordinary callbacks whose target
is a piston, head, moving piston, wire or repeater. Atomic writes between those
boundaries are not asserted to have identical setter-by-setter ordering.
Shape callbacks, inert-target callback identity and entity collisions are not
claimed by this projection. The direct tryMove check performed onBlockAdded is
not mislabeled as an AbstractBlockState.neighborUpdate callback.

Input packets must each pair with one actual lever state write and fall after
the recorded world-tick return. `AfterWorldTick` retains that actual world time
and queues requested block events for the next tick. It is distinct from the
existing before-tick `External` section. The same positive time origin is used
for inputs, internal events and savedWorldTime; constructor zero stays zero.
Game ticks are never derived from Mineflayer physics ticks.
The declared input/drain window must have contiguous raw sequences, full
heartbeat evidence, matching spatial bounds and no write errors. The overall
bounded artifact can still be partial outside that window.

Each trial uses a new artifact prefix and a previously checked empty isolated
region. The actor captures full initial/final regions, verifies cleanup and
removes its force load. No shared-world placement authority is inferred.

## Current findings

- Trial a did not apply its first three requested inputs. It is input-unverified,
  not evidence of a simulation mismatch. Cleanup passed. A declared pre-input
  warmup was added to subsequent fixtures.
- Trial b applied all four inputs. The final region matched. The initial trace
  comparator falsely counted the runtime's onBlockAdded self check as a neighbor
  callback; correcting that projection gave 67 matching observation boundaries.
  No physical model change was justified by that diagnostic.
- Trial i, at negative coordinates with the downward input first, exposed an
  input-boundary error: the recorded two-tick OFF requests RetractDrop, but
  replaying it before the next world tick requested Retract. The final world
  was identical. The new explicit post-world input queue fixes the replay
  without changing the piston law or redefining the before-tick helper.

## Accepted observations

Offsets below are actual applied server ticks relative to the first input.
The projection contains 659 matching observation boundaries across 11 captures.

| Trial | Mechanism | Applied input offsets | Matching boundaries |
| --- | --- | --- | ---: |
| b | Upward sticky, reversal during retraction | 0, 1, 2, 12 | 67 |
| c | Horizontal sticky, same-tick cancellation then operation | 0, 0, 1, 11 | 53 |
| d | Downward ordinary, same-tick cancellation then operation | 0, 0, 7, 17 | 47 |
| e | Upward sticky, actual one-tick interruption | 0, 1, 2, 13 | 67 |
| f | Downward sticky, OFF after normal completion then re-ON | 0, 3, 4, 14 | 99 |
| g | Upward sticky, settled pull reference | 0, 6 | 47 |
| h | Shared destination, horizontal then downward | 0, 0, 1, 2 | 36 |
| i | Shared destination, downward then horizontal, negative X | 0, 0, 2, 3 | 41 |
| j | Horizontal sticky, measured one-tick interruption/reversal | 0, 1, 2, 12 | 67 |
| k | Downward ordinary, measured one-tick interruption | 0, 1, 8, 18 | 64 |
| l | Upward sticky, progress Full/lastProgress Half before completion | 0, 2, 3, 13 | 71 |

Client tick alignment was added for j–l to improve scheduling repeatability;
only the observed server writes establish the offsets above. At the two-tick
OFF boundary the previous carrier tick reached progress 1 with lastProgress
0.5, but normal materialization has not yet run. This is distinguished from f's
post-completion OFF by the recorded carrier entry/exit stream.

The retained `piston-transient-observed-*-v2.json` files contain initial/final
observations, applied inputs, the contiguous raw interval, server-derived
expected boundary worlds and artifact hashes. These expectations are recomputed
from the raw server interval before comparing the current model. Each replay
also resumes a suspended moving checkpoint and a moving behavior representative;
both must reach the same final state as uninterrupted replay. Historical full raw artifacts and their
original model outputs are unchanged, including trial i's diagnostic mismatch.

Replay and check the retained evidence without starting a server:

```bash
cargo build --offline --locked -p dustroute-translate --example compare_electrical_pistons -j 1
python3 tools/test_piston_transients.py
```

For a new authorized isolated trial, `tools/observe_mixed_pistons.py` accepts
`--transient-trace`; `tools/compare_piston_transients.py` compares its scoped
trace, and `tools/retain_piston_transient.py` retains the observed expectation
under a new output name. Insufficient input/coverage remains a rejection.

## Verification

- Minecraft package tests: 208 passed, including the regression demonstrating
  different retract event types at before/after-world input boundaries despite
  an identical final world.
- Selected library/translate integration tests: 32 passed, covering saved
  execution contexts, the ten existing electrical observations, location and
  mixed behavior exploration, and fresh runtime adoption.
- Offline transient suite: seven tests passed, including replay of all eleven
  retained captures and moving checkpoint/behavior-state restoration. Negative
  cases reject missing records, incomplete scope, unverified inputs, incorrect
  input sections, missing world-tick heartbeats and divergent progress/world/order.
- Companion normalization/compression tests: 16 passed; instrumentation Java
  compilation passed. The new mixins were exercised by the actual captures.
- Workspace all-target Clippy with warnings denied, Rust formatting, Python
  syntax and actor JavaScript syntax checks passed.

The full workspace test suite was not rerun for this change. Commands, source
and fixture hashes are recorded in the
[verification manifest](evidence/piston-transient-final-checks-20260926.json).
Working artifacts are under `.local/e2e-artifacts/piston-transient-20260926-*`.
The comparisons establish the declared projection and trials, not all Minecraft
updates, all coordinate/order combinations or new component support.
