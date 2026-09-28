# Executable torch law and burnout observations

The current mixed-world adapter also has [shared torch history and callback execution](shared-torch-runtime.md).
The isolated-law evidence and profile boundaries below remain historical contracts.

The local torch transition law is stored as immutable Blueprint Revision
`dustroute.law.torch.java-1-21-11.v1` in
[`torch-law-v1.json`](../crates/dustroute-library/blueprints/torch-law-v1.json).
It contains executable conditions and state updates, not an identifier that
dispatches to a hardcoded torch implementation. `dustroute_minecraft::law`
interprets the program; `dustroute_translate::torch_law` supplies support power.
`RedstoneTickSimulator` now uses this law for its torch state transitions.

The program keeps the lit state, recent **lit-to-unlit** events and the pending
scheduled callback separately. Its clock is game ticks. The observed rules are:

- A neighbor notification schedules a callback after two game ticks when the
  current lit state conflicts with support power. It does not reset an existing
  pending callback.
- At the callback, a lit torch with powered support turns off and records one
  off event. Turning on is not an additional off event.
- Eight off events in an inclusive 60-game-tick window cause burnout and request
  a callback 160 game ticks later. In these isolated-input observations, that
  callback remains pending and the torch stays off, including when the input is
  released or toggled during the wait. Feedback can compete for the pending
  callback; see the [clock counterexample](periodic-clock-conformance.md).
- At the recovery callback, an unpowered support permits relighting. Powered
  support keeps the torch off; a later input release schedules the normal delay.

These timings are physical model behavior, not new deadlines or input-rate
restrictions in the `RepeatedSettling` type. The existing public
`torch_burnout_candidates` diagnostic now tracks actual modeled threshold
crossings rather than counting both transition directions. It remains
conservative: the scenario API still requires live observation for such cases.

## Evidence and reproduction

The fixture `crates/dustroute-translate/tests/fixtures/torch_burnout_1_21_11.json`
contains direct server block-state samples from the pinned official Java
1.21.11 JAR (SHA-1 `64bb6d763bed0a9f1d632ec347938594144943ed`). It records one
sample after every explicitly stepped game tick, relative to the first input.
Both standing and wall torches are covered. Cases distinguish seven/eight off
events, held high/low input, input changes during recovery, release after the
recovery callback, spaced transitions, a second burnout, and the inclusive
60-tick history boundary versus an expired entry at 61 ticks.

The test world is frozen between steps. `/setblock` does not invoke a player's
lever-use callback, so the probe assigns the lever state and explicitly delivers
a neutral neighbor notification by placing/removing adjacent glass within the
same frozen tick. This stimulus is recorded; it is not presented as a recording
of player interaction. Each case uses a fresh coordinate to avoid inheriting
position-keyed history. The probe checks empty, loaded space before placement,
checks cleanup, releases its chunk tickets and stops the server afterward.

Run against the existing **stopped, private, local** test server with its EULA
already accepted:

```bash
python3 tools/observe_torch_burnout.py \
  --server-dir .local/minecraft-server-1.21.11 \
  --output .local/e2e-artifacts/torch-observation-new.json
cargo test -p dustroute-translate --test torch_laws
cargo test -p dustroute-library --test executable_laws
```

After inspecting a new capture, preserve it under a new fixture filename:

```bash
python3 tools/promote_torch_observation.py \
  .local/e2e-artifacts/torch-observation-new.json \
  crates/dustroute-translate/tests/fixtures/torch-observation-new.json \
  --reason "Describe the reviewed physical behavior"
```

The probe refuses to overwrite its output or an occupied test region and restores
the server properties it temporarily changes. Raw console commands and responses
remain in ignored local artifacts; the observation records their SHA-256. The
tracked fixture omits absolute server time, player identifiers and the world.
The companion metadata records the promotion reason and artifact hashes.

The regression executes the exact law Revision against every captured input and
compares every sampled output. It also drives the existing electrical simulator
for cases whose input changes lie on its two-game-tick compatibility boundaries.
The 61-tick case uses the local law directly; it does not claim a finer global
simulation scheduler. Runtime inspection of the locally cached, pinned Yarn
1.21.11+build.6 class informed the probe design; bytecode is not copied into the
tracked law or used as a substitute for observed samples.

## Execution and guarantee boundaries

`LawProgram` is a bounded local event language: named inputs/registers, recent
event histories, expressions, conditional assignments and deduplicated delayed
callbacks. It cannot perform I/O or arbitrary Rust execution. Compilation checks
names and structural limits; execution checks declared ranges and returns a new
state atomically. Invalid rules, exhausted history capacity or mismatched state
return an error, never a successful behavioral proof. Ages and deadlines are
relative, so absolute clock growth does not create artificial distinct states.

An explicit `advance_with_register_effects` adapter API can deliver synchronous
effects between instructions on tentative state. The original atomic API is
unchanged. The single-torch block-effects profile maps changed `lit` assignments
to electrical resolution and neighbor delivery before the local handler resumes;
the law program itself retains its original Revision. Unsupported nested block
effects are rejected. See [clock conformance](periodic-clock-conformance.md).

Law-bearing Blueprint records use catalog schema v3. Existing v1/v2 archives
remain readable; records are never rebound under an existing Revision ID.
`builtin_laws()` loads this law as a regular Blueprint catalog, which can be
imported into other catalogs. It is separate from the frozen geometric asset;
no new MCP tool is introduced. Legacy cell projection rejects executable-law
records because it cannot retain their execution semantics.

The adapter assumes a declared initial lit state with empty history and no
pending callback. Observing a block's lit property alone cannot recover hidden
burnout history. The probe establishes this initial condition by creating fresh
torches at unused coordinates, then retains their state across all input changes.

This evidence supports the isolated local torch law under the stated stimuli.
It does not establish Vanilla's internal callback order, arbitrary interacting
torch networks, history across block replacement/chunk unloads, or full-world
conformance. The interpreter orders simultaneous local timer names
deterministically; this is a model policy. This torch law has only one timer.
The existing global compatibility scheduler and other block models retain their
own evidence boundaries.

The [physical behavior adapter](physical-behavior.md) now binds these local states
and a data-defined dust-strength law to whole-circuit repeated-settling checks.
It preserves histories/timers across input changes and matches all 3,888 isolated
torch samples. The original synchronous profile diverges from the autonomous
feedback-clock capture at tick 190. The explicitly selected block-effects profile
matches that capture and the isolated observations, but fails this candidate's
periodic requirement. Finite agreement does not establish arbitrary-network conformance.
Geometry rules and the execution profile remain explicit model
assumptions. The separate [history abstraction](abstract-behavior-verification.md)
now proves repeated-settling NOT behavior in the selected model, and
[block-count search](blueprint-block-reduction.md) produces verified smaller
candidates with movable ports. Piston-law migration remains subsequent work.
Finite trace agreement does not itself certify all reachable input histories
or minimality.
