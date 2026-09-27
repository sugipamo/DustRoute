# Dust laws and whole-circuit behavioral checks

`dustroute_translate::physical_behavior::PhysicalBehaviorModel` connects an
actual Assembly Revision to a `RepeatedSettling`, autonomous `Periodic` or
`FiniteBurst` type
and executable dust/torch Blueprint Revisions. It runs complete-state behavior
checks on physical execution state. Its results establish properties of the selected deterministic
model; they do not certify arbitrary Vanilla worlds or authorize adoption.

The first autonomous feedback-clock comparison found a concrete failure of the
original execution profile: modeled recovery at tick 190 did not occur in the
server trace through tick 640. The explicit single-torch block-effects profile
now matches that trace, fails the candidate's periodic requirement and passes
the separate finite-burst requirement. See
[clock conformance](periodic-clock-conformance.md) for evidence and scope.

## Executable dust law

[`dust-blueprint-v1.json`](../crates/dustroute-minecraft/laws/dust-blueprint-v1.json)
stores `dustroute.law.dust-strength.v1`. Its program combines the maximum direct
source strength and the maximum connected neighbor strength:

```text
strength = max(direct, max(neighbor - 1, 0))
```

Both inputs and the output have domain 0–15. `DustLaw::from_revision` compiles
and executes the selected Blueprint program over all 256 input pairs, then
caches the resulting table. It retains the exact source record. The adapter
accepts only a memoryless `evaluate` program: each pair starts from the declared
register defaults, with no histories or delayed events. The attenuation formula
is not duplicated in the electrical solver or selected by a gate name.

The existing electrical solver, `RedstoneTickSimulator` and bounded event runner
use this shared compiler. The bounded runner rejects consumed levels above 15
before an event commits; the old translate adapter retains its clamp policy.
Raw records and the compatibility comparator's separate `u8` contract are
unchanged. See [world execution contexts](world-execution-context.md).
Wire shapes, directed rise/fall connections, support/conduction and emission
target patterns now use the shared [spatial law programs](blueprint-architecture.md#executable-spatial-laws).
Native adapters supply actual coordinates and state; source strengths and
remaining device timing retain their existing execution scope. Tests compare the entire finite domain with
the previous formula, retain the existing vertical-support observation tests,
and change a law program to verify that physical circuit execution changes.

## Selecting a circuit

The existing v1 execution profiles retain their historical initial-placement
rules. Reports identify their `placement_validation_profile` as
`dustroute.initial-placement.v1`. This preserves old model semantics; it does
not authorize current placement. New Assembly review and adoption additionally
require the current wire-rise consistency gate. A historical model pass can
therefore coexist with a failed current placement review. Source and observed
block states remain unchanged; see the
[validation boundary](world-validation-boundary.md).

Load the exact records into one `BlueprintCatalog`, then call
`PhysicalBehaviorModel::from_fresh_assembly` with a `PhysicalBehaviorSelection`:

- An Assembly Revision ID for the actual block state.
- A `RepeatedSettling`, `Periodic` or `FiniteBurst` Type Revision ID, independent of classification labels.
- Exact dust and torch law Revision IDs. There is no latest-version lookup.
- One actual lever position per named input, and a signal or powered-block
  observation per named output. Bindings follow the type's vector order and may
  use different coordinates in a different candidate.
- An electrical fixed-point iteration budget. This limits computation; it does
  not impose a settling deadline on the type.

The profile implicitly pins four spatial law Revisions. Old archives need not
contain those formerly hardcoded definitions. A conflicting definition under a
pinned ID is rejected by the world resolver; a different spatial law cannot be
selected by adding it to a realization. Current APIs do not add arbitrary spatial
model selection. New candidates declare all six effective law requirements.

The constructor checks the selected records, all named bindings and placement
validity. The initial profile supports dust, torches, levers, redstone blocks,
and supported solid/transparent structural blocks. Other devices, coarse block
classification, invalid supports and unknown neighborhoods return an error.
Every occupied block needs known coverage of a two-block halo; missing cells
outside known coverage are not silently read as air. Stored wire shapes remain
unchanged. Source interpretations and child classifications are not substituted
for actual blocks or accepted as behavior evidence.

The constructor explicitly selects **fresh construction**: initial lever levels
and torch lit states come from the declared layout, local histories are empty,
and no callback is pending before the initial power notification. Dust strengths
are computed by the model. A snapshot of a running circuit cannot establish this
hidden-state assumption. There is no API to deserialize a diagnostic state as
fresh execution evidence.

## Execution profile and state

The original profile is `dustroute.dust-torch-synchronous-game-tick.v1`:

1. Inputs change actual lever levels together, without advancing time or resetting
   internal state. Assigning the same vector again is an identity operation.
2. The electrical network reaches a fixed point using the selected dust program
   and existing topology/power primitives. All torch laws receive support power
   through `neighbor_update` once at this boundary.
3. One step advances every torch law by one game tick in parallel. The adapter
   then resolves the electrical network and delivers support notifications again.
4. Outputs read signal strength or block power as Boolean `level > 0`.

This boundary notification policy and simultaneous device advancement are model
rules, not evidence of Vanilla callback ordering. The interpreter's local event
ordering is documented in [torch laws](torch-laws.md). Unsupported timing,
dynamic geometry and the original simulator's other block models are not
silently included in this profile.

Use `from_fresh_assembly_with_profile` with
`PhysicalBehaviorProfile::DustSingleTorchBlockEffectsV1` to select
`dustroute.dust-single-torch-block-effects.v1`. It resolves dust and delivers a
support notification synchronously after each changed `lit` assignment, before
the remaining local instructions (including recovery scheduling). It does not
notify merely because a game tick or history entry elapsed. Initialization and
external input changes still deliver notifications; `notify_torch_neighbors`
provides explicit external diagnostic stimuli and is never invoked by autonomous
verification. The original constructor retains its original profile.

This new profile supports at most one torch, one `scheduled_tick` callback slot,
and no `lit` assignments within `neighbor_update`. Unsupported multi-torch order
or nested effects return an error (undetermined contextual review). The law
program remains immutable; the explicitly selected profile owns the coupling
between register writes and physical notifications. No geometry-specific clock
case or history compression is used.

State keys include the input vector and every torch's complete `LawState`:
registers, current law inputs, off-event ages and relative callback delays.
There is no growing absolute clock and no reset between truth-table rows.
Derived dust levels are recomputed deterministically. A state from another
model instance is rejected. Electrical nonconvergence, interpreter errors and
reachability budget exhaustion cannot produce a passing report.

`model.verify(BehaviorBudget::default())` explores all reachable input changes
and held-input cycles, including changes before settling. A pass requires the
entire reachable graph to close. A failed cycle has a replayable counterexample;
an incomplete search is `undetermined`, even when every sampled trace agrees.
The report includes the profile, initial condition, bindings and full selected
Assembly/type/law records. It is diagnostic data only and is recomputed on each
verification call. [Repeated-settling adoption](repeated-settling-adoption.md)
uses a fresh conservative abstract proof and verifies actual input controls;
[block-count optimization](blueprint-block-reduction.md) uses the same fresh checks.

For an autonomous type, supply no external inputs. `model.verify_periodic(budget)`
checks full-state recurrence with a nonconstant output waveform.
`model.verify_finite_burst(budget)` requires at least two actual falling edges
followed by an all-OFF complete-state cycle; restartability is not checked.
Blueprint `behavior_bindings` and consumer source requirements support both
contracts through fresh contextual review/adoption in the existing MCP tools.
See the [periodic contract](periodic-behavior-status.md) and
[finite-burst contract](finite-burst-behavior.md) for their explicit model and
initial-condition assumptions.

## Regression and scope

```bash
cargo test -p dustroute-translate --test physical_behavior
cargo test -p dustroute-translate --test torch_laws
cargo test -p dustroute-translate --test observation_fixtures
```

Regression coverage includes multi-input/multi-output wire circuits, attenuation,
actual execution after law replacement, counterexample replay, immutable pins,
preserved wire shapes, unsupported/unknown/invalid state and computation limits.
The physical adapter matches all 3,888 captured standing/wall torch samples,
including odd game ticks and input changes during recovery. Both existing NOT
layouts with dust match the compatibility simulator on its two-game-tick
boundaries while retaining burnout and recovery state.

Finite trace agreement does not prove the repeated-use property for every input
history. The manual reachability measurement is separate from the fast regression
suite and does not assert that NOT passes:

```bash
cargo test -p dustroute-translate --test physical_behavior \
  measure_full_physical_not_reachability -- --ignored --nocapture
```

On 2026-09-20, this measurement reached the default **65,536-state** limit after
94,947 evaluated steps (about 18.53 seconds in the local run), even for an
isolated standing torch, one support block and one lever. The reachable graph
did not close. The result was `undetermined`, with no counterexample reported;
this is neither a NOT pass nor a demonstrated circuit failure. The manual test's
successful exit records the measurement, not a successful behavioral proof.

The complete off-event age history creates many distinct execution states under
repeated input changes. Further implementation was paused here under the user's
stop condition: a verification strategy that avoids enumerating every history
is a prerequisite before block-count search can rely on this gate. No history
was discarded, input rate restricted, type deadline introduced, or exploration
limit raised to hide the incomplete result. That historical stop required a
sound verification strategy preserving the accepted type. The user subsequently approved the separate
[abstract verification route](abstract-behavior-verification.md). It closes the
NOT graph in the block-effects profile without changing concrete execution or
the original exact verifier.

The follow-up [state-reduction inventory](behavior-state-reduction.md) separates
execution state, exact verification equality and type-specific proof state.
It records the measured history distribution, dependency checks and concrete
counterexamples to snapshot-only or count-only equality. The inventory remains
analysis; the subsequent [history abstraction](abstract-behavior-verification.md)
now implements a separate conservative proof graph.

`model.verify_history_abstraction(budget)` selects the new proof route explicitly.
It retains full Assembly/type/law provenance and reports abstract state/transition
counts separately from exact-state verification. Unsupported scope, possible
abstract failure and budget exhaustion remain undetermined. Contextual
[repeated-settling adoption](repeated-settling-adoption.md) now connects this
proof to complete Blueprint port mappings and actual input-driver checks.

[Movable-port block-count optimization](blueprint-block-reduction.md) now finds
verified smaller candidates under fixed laws. Piston-law migration remains
subsequent work. Neither a closed model graph nor a finite observation suite
proves a candidate is globally minimal.
