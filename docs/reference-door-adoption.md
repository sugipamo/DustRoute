# Reference door adoption audit

Current agreement: the user accepts the reference door's ordinary completed
open/close operation and agreement with Java; interruption tolerance is optional.
The [ordinary 3×3 door type specification](piston-door-type.md) records that
requirement. The rejection below belongs to the older, stronger unrestricted
diagnostic contract. It does not reject the agreed ordinary-door function.
The later [ordinary-type implementation and adoption](reference-door-ordinary-adoption.md)
uses explicit new v3 records; the historical audit below is unchanged.

The 2026-09-27 audit **does not adopt** the downloaded 3×3 door. Its normal
open/close replay remains successful. The subsequently authorized observer-front
ordering change now passes model construction and teardown; arbitrary-input
behavior has a freshly replayed counterexample in the current model. A subsequent
[six-case live short-pulse comparison](reference-door-short-input-comparison.md)
also reproduces interrupted-input failures in Java. Live placement remains
unverified.

| Historical unrestricted candidate gate | Result |
| --- | --- |
| Exact imported initial world, current v5 native placement gate | Passed |
| New aperture bindings on a settled close/open cycle | Passed |
| Ordinary sequential construction | Passed after observer-front ordering: exact 43-block world, followed by 43-step empty teardown; original failure retained below |
| Full repeated-settling behavior, default public budget | Undetermined: elapsed verification budget exhausted |
| Directed interrupted-input replay in the unchanged model | Failed: a reachable incorrect fixed point disproves the proposed unrestricted contract |
| Subsequent six-case live short-input comparison | Model and Java agree, including the failed OFF aperture at 1, 3 and 14 ticks; no adoption grant |
| Adoption through the shared Blueprint/MCP adoption core | Refused; proposal remains Open; candidate not published |
| Adoption after saving and starting another process | Revalidated and refused again; original catalog unchanged |

The original adoption audit used isolated catalogs and diagnostic files without
starting Minecraft or changing any world. The later short-pulse comparison used
dedicated empty regions in the private instrumented server, then removed them;
it did not modify a user's MCP catalog. The actual MCP transport and target-environment/readback
checks were untested in that audit because its earlier gates did not pass.
The subsequent ordinary-type audit tests public MCP adoption and model target
review; it still does not claim live sequential placement of this door.

## Explicit candidate

[The candidate builder](../crates/dustroute-translate/tests/support/reference_door_blueprint.rs)
imports the retained 43-block snapshot, preserving coordinates, block identities,
properties and material names. `reference-door.mechanism.v1` is an unadopted
literal source. A new `mechanism.v2` adds the current thirteen law requirements
and the behavior binding. Explicit parent v1→v2 and Assembly v1→v2 proposals
change their child references; the original records remain intact. The child
owns the behavior requirement and the parent retains the child occurrence.

The single lever is at relative `(0,11,0)`. At each of the nine aperture cells
`x=0, y=6..8, z=-1..1`, two predicates observe Air and Solid. Lever OFF requires
Air=true/Solid=false everywhere; ON requires Air=false/Solid=true everywhere.
Thus a piston head or moving piston cannot count as a closed door. The behavior
type checks solid occupancy, not exact quartz identity; exact materials remain
in the source geometry and in the separately retained normal-cycle comparison.
No explicit Air source claims are added at moving aperture positions.

The proposed type is the existing **RepeatedSettling** contract. Inputs can
change between any completed runtime roots, including during motion, and holding
an input must eventually stabilize all outputs to its row without resetting the
world. This is stronger than the four previously captured changes separated by
100 ticks. The candidate does not silently impose a minimum input interval or
weaken that contract when exploration takes too long.

The detailed review reached 1,727 states and evaluated 2,048 steps without
closing the reachable graph under the default 30-second budget. Independent
adoption attempts reached 1,827/2,189 and 1,703/2,012 states/steps. These counts
depend on execution speed. All three results were **Undetermined**, with no
counterexample. Budget exhaustion does not establish that the actual door fails
under interrupted inputs, or that any particular physics mechanism is missing.

After the ordering implementation, loading the saved candidate into a new
process and attempting adoption again reached 1,746 states / 2,077 evaluated
steps before the same elapsed-budget limit. It remained Open and unpublished;
the original catalog stayed unchanged. This new check does not reuse any of
the historical review results as authority.

The subsequent [interruption investigation](reference-door-interruptions.md)
found a concrete reachable incorrect fixed point without changing the runtime,
contract or public verifier. ON → two model advances → OFF, followed by holding
OFF, leaves `(0,8,0)` solid. Fresh replay and two complete-state recurrence checks
establish a counterexample to the proposed model contract. A separate native
runtime probe applies inputs only after world ticks: 17 of 33 tested pulse widths
leave quartz in the aperture after settling. Those finite samples are neither
live evidence nor a proof of a safe minimum interval.

The shared adoption attempt still reports its recorded **Undetermined** result;
the diagnostic is not injected into saved review authority. The audit decision
is **do not adopt this candidate under its declared unrestricted contract**.
The subsequent live comparison of six short/control pulses agrees with the
model. The user then selected completed-operation inputs as the ordinary-door
assumption. Its separate [contract and verifier](piston-door-type.md) are now
implemented; their fresh pass does not relabel this historical candidate.

## Original construction failure and implemented correction

`ElectricalConstruction::new` starts from empty known space, installs each
block with normal callbacks, waits for settling after each installation, and
compares the completed world with the fresh imported world's settled state.
Before the ordering change that final comparison failed for this fixture. The follow-up diagnostic records
two different cells: smooth quartz expected at `(0,5,0)` is instead at `(0,6,0)`.
For both cells, the last change between settled construction snapshots occurs
at **step 36**, which installs a south-facing observer at `(0,2,0)`. This locates
the placement boundary that leaves the displaced block, not its precise causal
callback. All other final cells match the declared settled world. The previous
live capture used strict initialization, so it cannot certify this sequential
procedure.

After the cause investigation and explicit user approval, the production
planner now places a declared occupied watched cell before its observer while
retaining support requirements and deterministic rank/coordinate tie-breaking.
Unresolvable dependency cycles are rejected. Every write still executes normal
callbacks, and all settled snapshots, final equality and teardown remain checked.
Separately, adoption needs a completed behavior proof under its declared input
contract. Increasing a diagnostic timeout, sampling more normal cycles, or
accepting a saved pass does not satisfy the present public adoption gate.

The subsequently authorized [cause investigation and concrete proposal](reference-door-construction-proposal.md)
traces the placement-triggered observer/repeater pulse through the incomplete
two-stage extender. The previously diagnostic-only successful order is now
generated by the production rule, without door-specific coordinates or material
substitution. The later interruption audit rejects the declared behavior
contract; live sequential construction and public placement remain unperformed.

## Evidence and reproduction

[Machine-readable results](evidence/reference-door-adoption-20260927.json)
record the context, budget, failures, fresh adoption attempts, catalog equality,
artifact hashes and validation commands. Full isolated records and archives
are retained under `.local/e2e-artifacts/reference-door-adoption-*-20260927.json`.
They are diagnostic/proposal data, not adoption authority.

[Construction diagnostic follow-up](evidence/reference-door-construction-diagnostic-20260927.json)
records the two-cell difference, last settled change and updated source hash.
The original audit retains its original code hashes. The sole production-code
change in the follow-up enriches the existing rejection message with up to 16
differing cells and their last construction step; the construction order,
callbacks, settling and acceptance conditions were unchanged by that diagnostic.

[Authorized ordering implementation](evidence/reference-door-construction-order-20260927.json)
records current construction results and regression checks separately from the
historical failed audit. Its reference-door regression covers the original
location plus four rotations at a translated target. These are fresh model
checks at those coordinates, not live environment or adoption certificates.

The [interruption evidence](evidence/reference-door-interruptions-20260927.json)
retains the directed model counterexample and the separate post-world-tick
probe. Its two additional regressions confirm the negative evidence; they do
not grant adoption.

Run the following commands sequentially from the repository root. The example
prints structured JSON even when the audit result is failed or undetermined;
inspect the reported status, not just the process exit code.

```sh
cargo build --offline --locked -j 1 -p dustroute-translate --example audit_reference_door_adoption
target/debug/examples/audit_reference_door_adoption prepare > /tmp/door-candidate.json
target/debug/examples/audit_reference_door_adoption construction > /tmp/door-construction.json
target/debug/examples/audit_reference_door_adoption review > /tmp/door-review.json
target/debug/examples/audit_reference_door_adoption adopt /tmp/door-candidate.json > /tmp/door-first.json
target/debug/examples/audit_reference_door_adoption adopt /tmp/door-first.json > /tmp/door-restart.json
cargo test --offline --locked -j 1 -p dustroute-translate --test reference_door_adoption -- --test-threads=2
```

The initial two regression tests check unchanged geometry/source references after archive
loading and the actual proposed observations through one settled close/open
cycle. That cycle test deliberately does not claim arbitrary-input certification.
The ordering implementation has nine construction/reference regression tests,
including six-direction observer chains, watched Air, dependency cycles and the
relocated reference door. It changes candidate construction order only; the
physical runtime and adoption conditions are unchanged.
The existing MCP construction test also passes, exercising fresh adoption and
baseline checks, stage readback, persistence and failure handling with its
existing mixed-piston fixture and transport stub. This tenth regression test
does not establish live Minecraft placement or adoption of the reference door.
The later interruption audit adds two tests, bringing the distinct relevant
regressions to twelve. The current reference-door target contains five tests:
source/archive retention, construction/relocation, a normal cycle, and the two
interruption cases. All five pass, including confirmation of the model's
counterexample rather than a pass of the door's unrestricted contract.
