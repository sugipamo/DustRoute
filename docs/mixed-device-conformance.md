# Mixed-device live conformance

Target: Java 1.21.11, redstone experiments disabled, on the existing private
instrumented server. Work starts from `b2f7ba5` on
`codex/data-driven-block-runtime`. Existing source-derived tests are not new
live evidence. No shared-world construction or adoption is implied by a capture.

## Ordered work

1. Extend the existing offline replay and capture comparison to ordinary stone
   button use. Establish button → waxed bulb → comparator → piston observations,
   including both bulb states, compare/subtract, and nonbinary wire output.
2. Capture multiple torches with dust feedback, explicit fresh histories and
   bounded input sequences. Compare transitions, burnout and delayed recovery.
3. Capture repeater locking with observers and pistons: short pulses, competing
   same-tick input orders and reversal during movement. Include rotated and
   translated/negative-coordinate variants without assuming rotational or
   positional invariance in Vanilla.
4. Reduce discrepancies to small reproductions, fix shared rules/delivery, and
   verify intermediate restoration and retained reference-door regressions.
5. Retain independent evidence, exact inputs, coverage and limitations. The
   actual server write establishes input time; client requested delays do not.

Each run starts in a checked empty region and declares its complete initial
block state. Compare contiguous server tick ends, ordered palette commits and
available scheduled-tick/internal boundaries. Missing observations, unsupported
mechanisms and model differences are separate outcomes. Raw artifacts are never
rewritten to match a model. Cleanup must verify an empty region, remove owned
force loads and stop the isolated server normally.

The existing instrumentation repository has earlier uncommitted measurement
work; record/reuse its current sources without reverting or treating that work
as changes made by this goal. Observation does not recover arbitrary live
comparator registers, torch histories or queues.

## Stop conditions and resource bounds

Stop before an outside-scope prerequisite or an unagreed substantial redesign
and report its evidence and impact. General support destruction, new shapes or
materials, new movable components, slime/honey, inventory/entity/fluid models,
oxidation, chunk lifetime, live hidden-state reconstruction, new public MCP
operations and a mandatory operational companion MOD remain outside this goal.
The listed measurement/replay extensions and corrections to existing shared
device rules are in scope.

Use offline locked Cargo builds with one job, one Rust test thread and no
overlapping Cargo processes. Use the existing isolated guest environment; no
host operations or fault injection are part of this work.

## Measured result

Ten completed captures on 2026-09-28 match the declared projection. Each has
verified empty-region cleanup, removal of the trial's force loads and normal
server shutdown. Execution remains v13: this work changed measurement, replay
and comparison, not production physical rules or public MCP operations.

All origins have Y=180, Z=1000; X and clockwise rotation are listed below.
Counts refer to the retained input/drain interval, not setup or cleanup.
“Callbacks” includes active-device ordinary neighbor callbacks, piston block-event boundaries, and moving-carrier tick/force-finish boundaries.

| Run | Circuit | Rotation / X | Tick ends | Writes | Device ticks | Callbacks |
| --- | --- | --- | ---: | ---: | ---: | ---: |
| a | Button → bulb → comparator compare → piston | 0 / 70000 | 120 | 46 | 8 | 208 |
| b | Button → bulb → comparator subtract → piston | 90 / -71000 | 120 | 46 | 8 | 216 |
| c | Two connected torch/dust feedback loops | 0 / 72000 | 510 | 1410 | 123 | 13216 |
| d | Repeater data first, lock, observer, piston | 0 / 73000 | 71 | 62 | 17 | 178 |
| e | Repeater lock first, data, observer, piston | 0 / 74000 | 71 | 50 | 13 | 139 |
| f2 | Connected torch feedback, rotated | 90 / -75020 | 512 | 1442 | 124 | 13520 |
| g | Bulb/comparator compare, rotated | 180 / 76000 | 119 | 46 | 8 | 208 |
| h | Bulb/comparator subtract, rotated | 270 / -77000 | 119 | 46 | 8 | 216 |
| i | Data-first locking, rotated | 90 / -78000 | 71 | 62 | 17 | 178 |
| j | Two separate fast torch feedback loops | 0 / 79000 | 512 | 144 | 68 | 1104 |

The bulb captures include four ordinary stone-button uses, their automatic
20-game-tick release, both bulb states, wire levels 0/15 in compare and 0/2 in
subtract, and vertical sticky-piston motion. The locking captures include both
orders of two verified same-game-tick inputs, actual one-game-tick intervals,
locked repeaters and observer pulses. Force-finish observations occur before a
moving carrier's final cleanup tick (progress 1, previous progress 0.5). They do
not claim coverage of every possible reversal phase.

In j, both torches switch off for the eighth time at relative tick 33 and stop.
After a quiet interval, input release at relative tick 261 leads to relighting
at 263; the second eighth off occurs at 293. These visible transitions and all
delivered ticks match. This feedback can reserve the short callback before a
160-tick recovery request; the capture does not assert an unconditional
160-tick automatic relight. Connected c/f2 alternate more slowly and are not
substitutes for that sustained burnout test. Private history contents are not
observed directly; see [shared torch semantics](shared-torch-runtime.md).

Each replay uses its own measured application times and absolute coordinates.
For example, a/b applied at offsets 0/26/53/79 despite requested gaps 0/26/26/26.
In d, the requested first one-tick wait applied in the original tick. d/e also
have different later applied timing, so their different write counts alone do
not isolate an input-order effect. No rotational/positional invariance or
client-clock accuracy is assumed.

## Discrepancies and corrections

Expanded notification comparison initially reported eight missing model-side
projection events in each bulb capture. The cached Java 1.21.11
`World.updateComparators` calls `updateNeighbor` directly; the shared model
already delivered these as Device Neighbor callbacks under `NotifyAnalogReaders`.
The comparison had only counted deliveries through the ordinary Notify iterator.
Counting the actual direct deliveries gives 208/208 and 216/216 matching events.
Original mismatching comparison files remain in the local artifacts; the
manifest identifies the corrected comparison separately. No circuit-specific
physics exception was added.

The first rotated connected-torch attempt (f) failed when the actor's fixed
viewpoint could not reach its second input. It completed block cleanup and
normal server shutdown and is classified as incomplete observation, not a model
failure or a pass. The actor now checks the shared viewpoint, and fixture
rotation also rotates that viewpoint. f2 used a new coordinate and artifact ID.

Full continuation serialization of a 71,221-microstep torch replay exceeded an
early diagnostic timeout. The optional device projection retains every record,
world delta, delivery cause/target/time, necessary notification metadata and
carrier changes while omitting unrelated continuation payloads. The default
full trace remains available. A regression compares full and projected traces
against the same independent bulb observations. This reduces diagnostic output
without changing execution. Timeouts never count as agreement.

## Evidence and replay

The [manifest](evidence/mixed-device-conformance-20260928.json) records original
artifact hashes, requested/applied inputs, origins, counts, source/probe hashes,
corrections and validation. Full local artifacts use the prefix
`.local/e2e-artifacts/device-circuit-20260928-<run>`.

Portable, lossless JSON gzip containers are retained in
`../crates/dustroute-translate/tests/fixtures/device-circuits/`. Each contains:

- The exact contiguous server input/drain interval with original header/end.
  This is explicitly a subset of the original capture, not the entire artifact.
- The circuit declaration and complete initial/final client region readbacks.
- Independently derived input times, tick-end worlds, palette writes, delivered
  block ticks and callback-world references. Shared callback worlds are deduplicated.
- SHA-256 hashes of the original raw/client/fixture files.

Retention never reads a model result. Tests re-derive the expectations from the
retained raw interval before running the simulator. Input packets must pair
with a palette commit at the same actual server tick and a verified
post-world-tick boundary. Missing commits, heartbeat gaps, unexpected features,
failed cleanup and unsupported overdue ticks are rejected. Observation missing,
unsupported observation, model rejection, timeout, comparison failure and an
actual mismatch remain distinct nonpassing outcomes.

Replay without starting Minecraft:

```sh
cargo build --offline --locked -j 1 -p dustroute-translate --example compare_electrical_pistons
python3 -m unittest discover -s tools -p 'test_*.py'
```

To generate new declarations, use `tools/make_device_circuit_fixtures.py` with a
new `--output-dir` and `--family bulb`, `torch`, `torch-separated` or `locking`.
Live runs require the existing stopped private instrumented server. Use
`tools/observe_device_circuit.py --fixture <file> --run-id <new-id> --x <new-x>`,
then `tools/retain_device_circuit.py --prefix <artifact-prefix> --fixture <file>
--output <new-json.gz>`. Output paths must be new; failed captures are retained.

## Validation and limits

The replay checks exact checkpoint continuation from a suspended movement/device
callback and behavior-state continuation from a root after movement/device-state
changes. This verifies the model's own outputs, histories, queues and final
state; it does not reconstruct hidden state from a running Minecraft world.

All 24 Python tests passed (213.734 seconds). The Python suite includes
all retained mixed-device captures, evidence-rejection tests, actual coverage
checks and the earlier 3×3-door tick-end/write/callback/restoration regressions.
Workspace/all-target Clippy, example build, formatting, actor syntax, Python
syntax and diff checks also passed. Logs and hashes are in the manifest.
A full Rust workspace test rerun is
not implied; production physics was unchanged.

Captures use explicit fresh stable initial conditions, strict placement and
Java 1.21.11 with only `minecraft:vanilla` enabled. The probe's earlier
uncommitted sources were reused and hashed, not modified by this goal. The
probe is test instrumentation, not a new required MOD for public operations.
These results establish the listed bounded circuits, not arbitrary Minecraft
world conformance, normal construction order, hidden register/history equality,
all shape/inert-target callbacks, or all rotations and input phases. The scope
exclusions and stop conditions above remain in force.
