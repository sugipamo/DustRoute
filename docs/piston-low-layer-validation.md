# Horizontal piston low-layer comparison

Current result: completion-time local revalidation is implemented, and the
single-input 1x2 door matches Java 1.21.11 in three independent live trials.
Earlier failures and pauses below are retained as historical evidence.

The initial 2026-09-05 Java 1.21.11 capture compares shared initial snapshots and
lever ON/OFF edges with the existing diagnostic `PhysicsEngine`. Each trial
rebuilds the same fixture, verifies all initial cells, and compares the full
54-cell bounded region after each edge. Block identity and applicable
`facing`, `extended`, `type`, and `powered` properties are compared.

| Step | Live / simulator result |
| --- | --- |
| Normal piston, stone payload | Three independent trials matched both phases; the piston retracts and the stone stays at the pushed destination |
| Sticky piston, stone payload | Three independent trials matched both phases; the stone returns to its original position |
| Sticky piston, retracted normal-piston payload facing north | Initial snapshots matched. On the first ON edge, Java moved the payload east from `(1,0,0)` to `(2,0,0)`, preserving facing and retracted state. The simulator rejected `BlockKind::Piston` as an unsupported moving block |
| Retraction of the piston payload and compound operation | Not run: stopped at the first difference as requested |

This establishes stable-state agreement for the first two cases only. Trials
settle for 1,500 ms after each real lever activation; packet-visible updates
are recorded but do not prove exact server tick or within-tick equivalence.
Piston placement remains unsupported by the common live validation boundary;
the harness is explicitly a local diagnostic and does not change capabilities
or MCP policy. No physics implementation was changed.

## Reproduce

Start the existing private Java 1.21.11 test server on `127.0.0.1:25565`, with
the whitelisted operator `dustroutetest`, following the
[E2E setup](../crates/dustroute-mcp/mineflayer/e2e/README.md). Then:

```bash
cargo build -p dustroute-translate --example piston_low_layer_replay
node crates/dustroute-mcp/mineflayer/e2e/piston-low-layer-live.js
```

The actor refuses to overwrite a nonempty region and uses world coordinates
`1097..1104, 178..183, 998..1003`. Cleanup verified that entire region empty
after the stopped run. The server started for this capture was stopped.

Shared cases are in
[`tests/fixtures/piston-low-layer`](../crates/dustroute-translate/tests/fixtures/piston-low-layer).
The Rust example imports those Minecraft snapshots using the existing importer,
injects the same lever state edges, and retains event/transition traces.
The JavaScript harness checks an independent mechanical expectation as well as
comparing the live result with the simulator result.

The [tracked summary](../crates/dustroute-mcp/mineflayer/e2e/fixtures/piston-low-layer-summary.json)
contains the case digests, trial outcomes, and all three differing cells.
Full states and traces remain in ignored
`.local/e2e-artifacts/piston-low-layer-latest.json` and its timestamped copy.
Zero process exit means the diagnostic completed, not that all cases matched;
the recorded status is `stopped_first_difference`.

The next bounded issue is retracted piston payload movement, beginning with
this exact horizontal push case. Pulling, moving extended pistons, vertical
movement, and compound sequences must not be inferred from this observation.

## Follow-up: isolated payload push

The first follow-up added support for extension that pushes one isolated, explicitly
retracted, unpowered horizontal normal-piston payload. The recorded Java
ON-state has been retained in
[`piston-low-layer-push-observation.json`](../crates/dustroute-translate/tests/fixtures/piston-low-layer-push-observation.json).
The integration regression replays the shared initial snapshot and lever ON
edge through the diagnostic engine and compares all 54 cells against that
recording. It also requires the destination payload to equal the original
block, retaining its facing, state, variant, and observed metadata.

At that stage, only extension used this exception. Sticky pulling of a piston, extended or
moving payloads, sticky-piston payloads, vertical payload facing, incomplete
state, and mixed/multiple-block piston chains remained rejected. Ordinary-block
push chains retain their existing behavior. Global piston placement/MCP
capabilities are unchanged.

```bash
cargo test -p dustroute-minecraft piston
cargo test -p dustroute-translate --test piston_payload_push
```

This follow-up compares against the existing live capture; it does not add a
new server timing measurement or verify the OFF/pull phase. The original
summary above remains the historical pre-fix result.

## Follow-up: isolated payload pull and live round trip

The planner now also permits sticky retraction to pull the same single,
unpowered, explicitly retracted horizontal normal-piston payload. The payload
retains its complete block metadata. Extended/moving, sticky, vertically
facing, incompletely specified, or powered piston payloads remain rejected;
mixed/multiple-block piston push chains remain outside this subset.
Global placement/MCP capabilities are unchanged.

Java 1.21.11 successfully pulled the north-facing payload back in the shared
case 03 layout. The pre-fix OFF observation is retained in
[`piston-low-layer-pull-observation.json`](../crates/dustroute-translate/tests/fixtures/piston-low-layer-pull-observation.json).
The integration regression now compares both ON and OFF against recorded
54-cell states and checks exact payload preservation after each movement.
Negative unit cases exercise rejection for both push and pull.

After the fix, three independently rebuilt live trials matched the simulator
for both ON and OFF, with zero initial or phase differences and successful
mechanical checks. See the
[round-trip summary](../crates/dustroute-mcp/mineflayer/e2e/fixtures/piston-low-layer-roundtrip-summary.json).
The original summary remains the historical failure. These are stable-state
checks with 1,500 ms settling, not proof of exact tick equivalence.
Cleanup was verified and the test server was stopped.

```bash
cargo build -p dustroute-translate --example piston_low_layer_replay
node crates/dustroute-mcp/mineflayer/e2e/piston-low-layer-live.js 03-sticky-piston
```

Validation: 32 piston unit tests and 22 integration regressions passed;
workspace/all-target Clippy passed with warnings denied. The next step is
two-piston compound operation, which has not yet been tested.

## Follow-up: actuating a moved piston

Case `04-moved-piston-actuation` uses two independently operated levers.
A sticky piston A pushes the north-facing normal piston B east, then B pushes
one stone north. B retracts before A pulls B back. The stone stays at its
pushed position because B is a normal piston. This tests actuation after
movement and safe sequential return; it is not yet a door opening/closing case.

All four phases matched Java 1.21.11 in three independently rebuilt trials:
zero differences across the full 108-cell region, with separate mechanical
expectations also passing. The fixture extends the comparison bounds to
`(-2,-1,-3)..(3,1,2)`; the empty-region guard and cleanup now cover world
`1097..1104,178..183,996..1003`. Cleanup was verified and the server stopped.
No physics implementation or MCP capability changes were needed.

The [compound summary](../crates/dustroute-mcp/mineflayer/e2e/fixtures/piston-low-layer-compound-summary.json)
retains all trial outcomes. A [recorded full sequence](../crates/dustroute-translate/tests/fixtures/piston-low-layer-compound-observation.json)
backs the regression `moved_piston_actuation_matches_recorded_java_sequence`.
The harness and replay now accept an input position per action; existing
single-input Boolean action fixtures remain compatible.

```bash
cargo build -p dustroute-translate --example piston_low_layer_replay
node crates/dustroute-mcp/mineflayer/e2e/piston-low-layer-live.js 04-moved-piston-actuation
cargo test -p dustroute-translate --test piston_payload_push
```

Each action still settles for 1,500 ms. Simultaneous inputs, short pulses,
exact tick timing, extended-piston movement, and a traversable piston door
remain unverified. The next useful step is a minimal mechanism that actually
clears and closes a passage, using the verified sequential movement subset.

## Follow-up: a traversable passage

Case `05-single-piston-passage` uses one fixed east-facing sticky piston at
`(0,0,0)` and a stone payload at `(1,0,0)`. Extension places the stone at
`(2,0,0)`, blocking the walking path along Z; retraction pulls it sideways
out of that path. The corridor is one block wide and two blocks tall, with
a floor, ceiling, and side walls. The ceiling prevents jumping over the
single lower blocking stone. This is a minimal passage barrier, not a full
2-block-high solid panel or the eventual 3x3 door.

Three independent Java 1.21.11 trials each ran close, open, close. Every
phase matched the simulator across all 144 cells in
`(-2,-1,-3)..(3,2,2)`. Independent mechanical expectations also passed.
The live actor then tested traversal in adventure mode, using forward
movement without jumping or flight. From `(2.5,0,-1.5)`, it stopped at
`z=-0.3` when closed, reached approximately `z=2.81` when open, and stopped
at `z=-0.3` after closing again, in all three trials. Positions are the
client's observed positions after settling for corrections; these are not
server-side entity telemetry or exact-tick measurements.

The [passage summary](../crates/dustroute-mcp/mineflayer/e2e/fixtures/piston-low-layer-passage-summary.json)
retains all comparisons and walking outcomes. The
[full recorded states](../crates/dustroute-translate/tests/fixtures/piston-low-layer-passage-observation.json)
back `passage_close_open_close_matches_recorded_java_states`, which also
checks the floor and clearance along the entire walking path. All three
recorded-observation regression tests and scoped Clippy passed. No physics
implementation changes were needed. Cleanup was verified and the private
test server stopped.

```bash
cargo build -p dustroute-translate --example piston_low_layer_replay
node crates/dustroute-mcp/mineflayer/e2e/piston-low-layer-live.js 05-single-piston-passage
cargo test -p dustroute-translate --test piston_payload_push
```

This establishes repeatable opening and closing of a traversable minimal
passage. General piston placement/MCP capabilities remain unchanged. Larger
panels, shared wiring, simultaneous inputs, and short pulses still need
separate validation before extending the supported scope.

## Follow-up: a complete two-row panel

Case `06-two-row-passage` extends the passage to a solid 1x2 stone panel.
The lower sticky piston pushes east from `(0,0,0)`; an independently controlled
upper sticky piston pushes west from `(4,1,0)`. This opposing arrangement
keeps the two input paths separate. Both stones occupy `x=2,z=0` when closed
and retract sideways out of the walking path when open.

The six edges close the lower row, close the upper row, open the lower row,
open the upper row, close the lower row, and close the upper row again.
Every intermediate state is compared across all 216 cells in
`(-2,-1,-3)..(6,2,2)`, including the partially closed states. Walking is
expected to succeed only after the fourth edge, when both rows are clear.
The harness approaches distant input levers before normal activation.
The empty-region guard and cleanup now cover
`1097..1107,178..184,996..1003` in the private test world.

```bash
cargo build -p dustroute-translate --example piston_low_layer_replay
node crates/dustroute-mcp/mineflayer/e2e/piston-low-layer-live.js 06-two-row-passage
cargo test -p dustroute-translate --test piston_payload_push
```

All six phases matched Java 1.21.11 in three independently rebuilt trials,
with zero initial or phase differences and all mechanical and walking checks
passing. See the [two-row summary](../crates/dustroute-mcp/mineflayer/e2e/fixtures/piston-low-layer-two-row-summary.json).
The [recorded sequence](../crates/dustroute-translate/tests/fixtures/piston-low-layer-two-row-observation.json)
backs `two_row_passage_matches_recorded_java_sequence`, including checks for
both panel cells at full closure and clearance only at full opening.
All four recorded-observation tests and scoped Clippy passed. Cleanup was
verified and the private server stopped. No physics implementation changes
were needed.

Evidence remains stable-state comparison after 1,500 ms settling and client
walking positions after correction settling, not exact server tick or
server-side entity telemetry. This verifies a sequentially controlled 1x2
door. Shared single-input wiring, larger panels (including 3x3), and general
MCP placement/execution remain outside the verified scope. The next bounded
step is to validate one input controlling this same two-row mechanism.

## Single-input follow-up: stopped before live validation

Case `07-single-input-two-row` replaces both independent levers with one
floor lever at `(2,0,4)`, branching through nine supported dust blocks to the
lower and upper pistons. The comparison region is 324 cells in
`(-2,-1,-3)..(6,2,5)`. Dust power is now included in replay observations;
wire connection shape and exact tick timing are not asserted. The harness
empty-region guard was extended through world Z=1006 for this fixture.

The first simulated ON edge failed before a live run was started:

```text
world delta failed: world shape changed
(expected ShapeId(3549725866577530643), found ShapeId(5164404354780996130))
```

Both dust branches reached power 11 at their piston inputs. Both pistons
started extending at game tick 2, in separate ordered block events. The first
start produced shape `3549725866577530643`; the second start changed it to
`5164404354780996130`. The first completion retained its previously generated
whole-world parent shape and was rejected, leaving two pending events.
The [blocker record](../crates/dustroute-mcp/mineflayer/e2e/fixtures/piston-single-input-blocker.json)
retains those transition identities and the failure; the full ignored replay
is `.local/piston-single-input-replay.json`.

In `time/engine.rs`, extension/retraction constructs the completion plan
immediately after staging its own start. `PistonComplete` later submits that
stored delta directly, and `WorldDelta` checks its whole-world parent shape.
This explains why the previously validated sequential movements succeeded
while overlapping independent movements are rejected. The stable-observation
formatter is not a moving-block-state oracle; the event/transition trace is
the evidence for motion starting before the error.

```bash
cargo build -p dustroute-translate --example piston_low_layer_replay
target/debug/examples/piston_low_layer_replay \
  crates/dustroute-translate/tests/fixtures/piston-low-layer/07-single-input-two-row.json
```

The replay process returns JSON even on a physics failure; inspect phase
`error` and `trace_status`, not only process exit status. Work stopped at this
new issue as requested. No live server was started, no physical comparison or
walking trial was run, and the physics implementation has not been changed.
The next bounded task is completion-time validation that permits unrelated
motion while still rejecting changed payloads, destinations, and conflicting
movement; the global validity check must not simply be removed.

## Completion failure isolation (offline)

The reproducible diagnostic `piston_completion_isolation` removes all dust
and lever activation. Two sticky pistons face east at `(0,1,0)` and
`(0,1,10)`, with independent stone payloads and disjoint movement cells.
Direct piston block events isolate scheduling/completion from wiring. Ten
schedule cases and six snapshot-mutation cases were run; no production
physics code was changed and no live server was used.

| Case | Result with current engine |
| --- | --- |
| Two extensions starting 0, 1, or 2 game ticks apart | Parent shape mismatch, two completion events remain pending |
| Two retractions starting 0, 1, or 2 game ticks apart | Same failure |
| Extension or retraction starting 3 game ticks apart | Both complete, empty queue |
| Reverse which piston starts first, same tick | Same failure for both actions |
| One start followed by no mutation | Completion succeeds |
| One start followed by a distant lever signal change | Completion succeeds; shape ID stays unchanged |
| One start followed by a distant stone addition | Completion rejected; shape ID changes |
| One start followed by the other distant piston's start | Completion rejected; shape ID changes |
| Replace destination moving carrier or remove source moving carrier | Completion rejected |

The gap-2 case still overlaps at the scheduler phase level: the second
`BlockEvent` runs before the first `BlockEntity` completion at that game
tick. Gap 3 is a control for this default motion profile, not a general
Minecraft timing recommendation or a proposed delay workaround.

For isolation only, a copied completion delta was also validated after
replacing its parent shape with the current ID. Distant geometry and the
other independent piston then passed the remaining validation; modified
source/destination carriers still failed with `BeforeMismatch` at their
specific coordinates. This demonstrates that local before-state checks
already exist beneath the global check. It does not establish that merely
rebasing the ID is sufficient for every dependency or conflict. That copy
was never applied to the engine or production implementation.

All rejected delta applications left their input world state unchanged.
This is atomicity of the rejected completion only: both already-accepted
start transitions remain in the world, and the complete operation has not
been rolled back. Failed runs must therefore not be treated as stable output.

The [isolation summary](../crates/dustroute-mcp/mineflayer/e2e/fixtures/piston-completion-isolation-summary.json)
retains all outcomes and ordered shape transitions. Full output is retained
in ignored `.local/piston-completion-isolation.json`.

```bash
cargo run -q -p dustroute-translate --example piston_completion_isolation
```

Conclusion: wiring, dust strength, piston proximity, and start ordering are
not required to reproduce this failure. The bounded fix belongs to deferred
piston completion validation. Before implementation, its validation contract
must cover the moving piston, source/destination carriers and any additional
planning dependencies; overlapping/conflicting movement must remain rejected.

## Completion-time revalidation implemented; stopped at a new limit

`PistonComplete` now calls `completion_plan` against the current world before
submitting its freshly generated delta. The common `WorldDelta` parent-shape,
before-state, and move validation remains unchanged. Local checks preserve
moving carriers and the piston body geometry (including entity/head metadata).
Legacy body-only starts additionally check unchanged payload/destination cells.
An empty source inspected by a no-payload sticky retraction is also retained
as a negative dependency: a new block there rejects completion.

The current planner's read dependency audit covers the piston body, head,
push chain and terminating empty destination, and sticky pull source. Modern
moving deltas reserve the written cells; the empty pull source is the extra
read-only cell. The current ordinary-block subset has no support-block reads
in its movement planner. Input signals select the action at start and are not
reinterpreted as a different action at completion. The engine's static known
region is unchanged during this operation.

Validation passed: 32 piston unit tests; five new completion tests covering
16 independent schedules across extension/retraction and both orders, distant
geometry/signal changes, altered local carriers, legacy plans, empty pull
sources, and conflicting destinations; and four recorded-live regression
tests. The prior ten-case diagnostic schedule matrix also completes without
shape mismatches after the fix. Its direct stored-delta mutation probe still
illustrates the deliberately unchanged global `WorldDelta` check.

The single-input case then completed ON and OFF with empty queues, but the
third ON stopped at `Minecraft physics event limit 512 exceeded`, with three
pending events. See the [follow-up record](../crates/dustroute-mcp/mineflayer/e2e/fixtures/piston-completion-followup.json).
The full ignored replay is `.local/piston-single-input-fixed.json`.
This is a new stopping condition; work was paused as requested. No limit was
increased, no live server started, and no live comparison was performed.
Whether the diagnostic budget is simply too small or event generation needs
correction remains to be isolated. The goal is not yet complete.

## Resumed verification: budget diagnosis and successful single-input door

The stop condition was clarified by the user: pause for work outside the
declared scope; diagnosis and fixes required by the declared verification
are included. Investigation and live validation resumed accordingly.

The 512-event failure was a cumulative diagnostic budget issue. The engine
counts events across its lifetime, including successive input edges. Case 07
needs 67 events for ON, 383 for OFF, and 67 for the next ON: 517 total.
A 20-cycle offline probe completed all 40 edges with exactly 67/383 events
alternating, zero pending events after every edge, and 9,000 events total.
See the [budget record](../crates/dustroute-mcp/mineflayer/e2e/fixtures/piston-event-budget-summary.json).
This shows bounded, repeatable settling for this fixture; it does not claim
the OFF propagation is optimally efficient. The replay now supplies a finite
budget of 512 times the declared action count and reports per-edge and
cumulative counts. The engine's cumulative limit remains unchanged.

The first live initial comparison exposed only a representation mismatch:
Mineflayer returned dust power as a numeric string and Rust as a number.
The live observation adapter now normalizes that property to a number.
After normalization, all 324 cells matched in all three independently rebuilt
Java 1.21.11 trials, for closed/open/reclosed states. Mechanical expectations
and forward-only adventure-mode walking checks all passed. The
[single-input success summary](../crates/dustroute-mcp/mineflayer/e2e/fixtures/piston-single-input-success.json)
retains every result; the [full stable states](../crates/dustroute-translate/tests/fixtures/piston-low-layer-single-input-observation.json)
back a regression that compares piston states, payloads, lever state, and
dust power. The region was cleared and the private test server stopped.

Completion coverage now includes six regression tests (including conflicting
extension destinations and a shared retraction payload) and five recorded-live
sequence regressions. The single-input door is validated for stable states
with 1,500 ms settling and client walking observations. Exact tick equivalence,
short-pulse behavior, larger doors, and general MCP piston placement capability
remain outside this goal and have not been promoted.

The historical nine-piston regression previously expected a global shape
mismatch. It now asserts complete independent movement, all nine payload
positions, both piston groups' states, and an empty queue after 18 transitions.
Its recorded serial/electrical fixture classification remains historical;
this mechanical test does not grant live 3x3 or electrical capability.

Final validation: `cargo test --workspace` passed (460 tests including doctests);
`cargo clippy --workspace --all-targets -- -D warnings`, formatting, JavaScript
syntax, and whitespace checks passed. The completion-revalidation and
single-input 1x2 live-comparison goal is complete.


## Restricted MCP operation (2026-09-07)

The exact single-input 1x2 configuration now has a dedicated existing-door
operation, `new_piston_door_operation`, followed by `show_operation` and
`invoke_operation`. Complete settled states and an empty guard are checked
before and after normal lever activation; this does not promote general
piston placement or arbitrary transition-test eligibility. Three real MCP
close/open/close trials and negative/no-op gates passed. See the
[restricted operation contract](piston-door-mcp-v1.md) for the exact supported
layout, selection bounds, policies, expiry, and failure behavior.
