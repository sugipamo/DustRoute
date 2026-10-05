# Reference door interrupted-input audit

Current requirement: the user has accepted normal completed open/close operation
and made interruption tolerance optional. See [the ordinary-door type](piston-door-type.md).
The stronger-contract failure documented below remains valid historical evidence;
it is not a failure of the ordinary-door function. The latter now has a separate
[contract, verifier and fresh adoption evidence](reference-door-ordinary-adoption.md).

Follow-up: the authorized [six-case Java comparison](reference-door-short-input-comparison.md)
now matches the native runtime for measured pulse widths 1, 3, 12, 14, 20 and
100 ticks, including the incorrect OFF aperture. The model-only investigation
below is retained as its history; the separate root-boundary witness and all
33 model pulse widths have not individually been compared live.

The proposed `reference-door.aperture-repeated-settling.v1` contract is
**disproved in the current model**. Do not adopt the candidate under that
contract. This result does not establish that the simulator matches Java on
these short inputs: the previously retained live comparison covered normal
cycles separated by 100 game ticks.

The approved construction correction is complete. The production planner
constructs the exact 43-block door and removes it completely, including after
translation and four rotations. The remaining issue is distinct from that
placement pulse: even a freshly initialized correct world can settle to the
wrong aperture after an interrupted command.

## Replayed counterexample

`probe_reference_door_interruptions` uses the unchanged public
`RuntimeBehaviorModel`, the exact candidate and its declared contract:

1. Start from the declared fresh world; set the lever ON.
2. Advance the model twice; set the lever OFF.
3. Hold OFF and advance 12 times to an incorrect fixed point.

At that fixed point, aperture `(0,8,0)` is Solid instead of Air. The other eight
aperture cells are Air. The diagnostic reconstructs the prefix from the initial
state, checks full physical-state equality, checks repeated OFF idempotence,
and checks exact recurrence and intermediate output samples twice. An incorrect
reachable fixed point suffices to disprove repeated settling; closure of all
other input branches is not required for this negative result.

These advances are complete runtime roots or clock advances, **not game ticks**.
No model state, runtime semantics or input relation was changed to produce the
counterexample. The diagnostic cannot publish candidate records or authorize a
world write.

## Inputs after world ticks

A separate probe uses `schedule_electrical_input_after_tick` in the native
runtime. Every case starts from the same fresh world, turns ON after tick 0,
turns OFF after the indicated tick, and runs to idle. All cases deliver both
inputs and finish with no pending roots. This removes the need to interpret a
model advance as an actual server-thread input instant, but remains simulation
evidence until a live capture confirms the schedule and observations.

| OFF after tick | Settled aperture under held OFF |
| --- | --- |
| 1–2 | All nine cells contain smooth quartz |
| 3–11, 14–19 | Smooth quartz remains at `(0,6,0)` and `(0,7,0)` |
| 12–13, 20–32, 100 | Aperture is Air and the entire initial world is restored |

There are 33 cases: 17 fail the proposed OFF row and 16 restore the original
world. This is a finite probe, **not** a proof that 20 ticks is a sufficient
minimum interval, nor a proof for alternating arbitrary pulses. Full trace and
final snapshot of the first failing tick-scheduled case are retained locally.

## Adoption result and stopping point

The shared Blueprint/MCP adoption core previously returned **Undetermined**
after its default 30-second breadth-first exploration budget, both before and
after loading the saved proposal in another process. The proposal remains Open,
the candidate is unpublished and the original catalog is unchanged. Those
recorded results are not rewritten to pretend the public verifier discovered
this directed witness itself. No larger timeout or sampled success is used as
adoption authority.

The historical audit decision is **do not adopt the candidate under its
unrestricted contract**. Target-world,
live sequential construction, MCP readback and undo checks for this door were
not attempted because it does not pass the earlier behavior gate. The existing
MCP mixed-piston regression uses a transport stub and is not evidence for live
reference-door placement.

The approved follow-up captured short ON→OFF inputs in isolated Java 1.21.11,
recording actual application times and the settling trajectory. Cases 1, 3, 12,
14 and 20 ticks plus a fresh 100-tick control all match the model; see the linked
live comparison. A monotone timing threshold is still not established.

The user selected an ordinary-door contract assuming the preceding operation
has completed before the next command. Preserve the original diagnostic
candidate and express that requirement through a new explicit type and candidate
revision. Interruption tolerance or circuit redesign is not required for this
ordinary-door function.

A restricted-input adoption route would need an explicit protocol (allowed
inputs, completion and invalid-input handling), exploration of that protocol,
and consistent review, persistence, restart and MCP operation rules. Changing
those responsibilities is a new design task; it has **not** been implemented.
The user's instruction to stop before substantial new work applies here. The
existing unrestricted contract and physical Laws remain unchanged.

## Evidence and checks

[Machine-readable evidence](evidence/reference-door-interruptions-20260927.json)
includes the replayed witness, pulse cases, source/artifact hashes and validation
commands. The diagnostic and two new regression tests complement the earlier
construction checks. This initial model-only investigation did not start
Minecraft; the linked follow-up records the subsequently authorized live runs.

```sh
cargo test --offline --locked -j1 -p dustroute-translate --test probe_reference_door_interruptions --no-run
DUSTROUTE_DIAGNOSTIC_OUTPUT=/tmp/probe_reference_door_interruptions.json cargo test --offline --locked -j1 -p dustroute-translate --test probe_reference_door_interruptions -- --ignored --exact retain_fixture --test-threads=1
cargo test --offline --locked -j 1 -p dustroute-translate --test reference_door_adoption -- --test-threads=2
```

The diagnostic exits successfully when it has produced a valid report, including
a counterexample. Inspect its status and witness; exit code zero is not an
adoption pass. Likewise, regression tests pass by confirming the documented
rejection evidence rather than by certifying the door's unrestricted contract.

The diagnostic example commands are retired. Explicitly ignored fixture tests retain their cases; `DUSTROUTE_DIAGNOSTIC_OUTPUT` must be a new absolute path. JSON is fixture IO only, and a diagnostic run never certifies a live circuit or adopts a design.
