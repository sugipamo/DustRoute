# Behavioral verification state: reduction inventory

Status: analysis and proposed proof obligations, 2026-09-20. The user requested
an inventory before reduction. No execution state, equality key, law Revision,
type requirement, search budget or acceptance gate is changed by this document.

The subject is `PhysicalBehaviorModel` with the fixed
`dustroute.dust-torch-synchronous-game-tick.v1` profile and the current executable
dust/torch Revisions. Conclusions about one pinned program must not become
name-based rules for arbitrary Blueprint programs.

## Three different boundaries

1. **Execution state** must retain everything needed to execute the actual laws,
   including hidden memory, pending callbacks and diagnostics used by existing
   consumers. This remains authoritative for simulation and trace replay.
2. **Exact verification equality** may omit a field if its value cannot affect
   any future relevant output, transition or execution error under the selected
   laws and adapter. This requires a dependency argument, not just agreement of
   the current block snapshot.
3. **Type-specific proof state** may describe several physically different
   states together if a proof preserves the selected behavioral requirement for
   every represented state and allowed future input. This is a different proof
   mechanism from choosing one concrete representative and running the current
   deterministic cycle checker.

`RepeatedSettling` requires every reachable state to accept any subsequent fixed
input vector and eventually keep all named outputs at the required row values.
Input changes during settling remain allowed; there is no reset between rows.
Finite delay and transient waveform differences are not themselves failures.
Internal cycles are allowed when outputs remain correct. Unknown transitions or
execution errors still cannot establish a pass.

The current verifier already merges identical states regardless of how or when
they were reached. Absolute game tick and the witness prefix are not equality
fields. The prefix is separate predecessor data for counterexample replay.
Concrete state fields are defined in
[`physical_behavior.rs`](../crates/dustroute-translate/src/physical_behavior.rs)
and [`law.rs`](../crates/dustroute-minecraft/src/law.rs); interning and cycle
checks are in [`behavior_type.rs`](../crates/dustroute-translate/src/behavior_type.rs).

## Inventory and proposed treatment

| Information | Why it exists / who reads it | Treatment to investigate | Expected effect |
| --- | --- | --- | --- |
| Assembly, type, law Revisions, topology, bindings and execution profile | Define the model being proved | Keep fixed in the proof context; never merge across different contexts by state key alone | Already outside changing state; no reduction in this run |
| `PhysicalBehaviorState.model` | Rejects a state belonging to another model instance | Keep the ownership check; one model's search may carry identity in its context | Constant within a run; no state-count reduction |
| External input vector | Input assignment idempotency, physical lever levels and future transitions | Retain the complete named vector | Required; the input space is not to be restricted |
| Torch `inputs.powered` | The scheduled handler reads the last delivered support-power value | Consider reconstructing it only after proving an invariant for the pinned adapter and law | In the measured isolated circuit it duplicates the lever value, so removal alone does not merge additional states |
| Torch `registers.lit` | Electrical outputs, neighbor power and law branches | Retain for physical execution and an initial exact projection | Directly affects future behavior, including other torches |
| Torch `registers.burnout_seen` | Written by the law; read by diagnostics and the compatibility simulator | Candidate for omission from **behavioral equality only**, under the current pinned program | No gain in the first 65,536 states, where every value was zero; possible gain after a completed burnout cycle |
| `histories.off_edges` ages | Expiration changes future counts; `Remember` also checks history capacity | Retain in execution. Investigate symbolic sets or local proof summaries for type verification | Main source of distinct states; count-only replacement is insufficient |
| Pending event identity and remaining delay | Determine whether/when handlers run; scheduling preserves the first pending event | Retain in execution. A verification abstraction must preserve event identity, relative timing effects and eventual progress | Potentially useful, but no safe blanket deletion or rounding is established |
| Derived dust strengths and block power | Drive torch inputs and named outputs through the electrical solver | Recompute from exact retained state, topology and the selected dust law | Already omitted from the equality key; analog values must still be used in electrical evaluation |
| History sequence representation / map storage | Implements local memory and named fields | Consider a lossless packed representation separately from state merging | May reduce memory or CPU cost; encoding the same age vectors does not reduce their number |
| Predecessor links, explored edges and resource counters | Witness replay and controlling verification work | Keep outside semantic equality | Already separate; they did not generate the 65,536 distinct states |

Two details prevent overly broad conclusions from the table:

- For the current torch program, `neighbor_update` only schedules work; it does
  not change `lit`. This helps establish that stored support power agrees with
  the just-resolved network at a state boundary. Another valid law may change
  `lit` in that handler; recomputation could then differ from the value delivered
  to the handler. Reconstruction must be checked against the actual program.
- A packed history must preserve multiplicity if a law can remember several
  events at the same age. The generic interpreter permits this. A set of ages is
  not generally equivalent to its history list. The existing torch histories
  already occur in chronological order; sorting them is not an unexplored
  source of state-count savings.

## Diagnostic information is not globally disposable

Inspection of the pinned torch program gives these explicit dependencies:

| Operation | Fields |
| --- | --- |
| Read input | `powered` |
| Read register | `lit` |
| Read history count | `off_edges` |
| Write register | `lit`, `burnout_seen` |
| Append history | `off_edges` |
| Schedule callback | `scheduled_tick`, after 2 or 160 game ticks |

The program never reads `burnout_seen`. For valid states differing only in that
bit, the current law's other state updates, output and execution success are
therefore independent of it: the bit is either retained or set to the constant
1. This is a candidate exact projection for the new behavioral verifier, subject
to retaining the selected program, its schema and adapter observation boundary.

It is not permission to delete the register from execution. In
[`sim.rs`](../crates/dustroute-translate/src/sim.rs), it populates
`torch_burnout_candidates`; [`scenario.rs`](../crates/dustroute-translate/src/scenario.rs)
uses that diagnostic to require live observation. Existing reverse-analysis
state comparison also observes the diagnostic. Those behaviors remain intact.
A future law that reads this register invalidates the projection argument.

Explicit expression reads are not the whole dependency graph. The interpreter
implicitly reads history ages during expiration, history length at capacity
checks, and pending delays during event delivery. Range/schema validation and
execution errors must be preserved as well. A register name being absent from a
branch does not by itself make every operation on it removable.

## What the measured states establish

The isolated standing-torch run stopped after 65,536 registered states and
94,947 evaluated steps. A separate diagnostic traversal reproduced the insertion
order and counts. This is an explored prefix, not a complete reachable graph.

| Stored off-event count | States |
| ---: | ---: |
| 0 | 5 |
| 1 | 263 |
| 2 | 2,636 |
| 3 | 11,830 |
| 4 | 24,339 |
| 5 | 20,707 |
| 6 | 5,538 |
| 7 | 218 |
| **Total** | **65,536** |

There were 12,288 distinct history age vectors. The oldest stored event was
28 ticks old. Every `burnout_seen` value was zero: this traversal exhausted its
budget before registering a burnout state. Pending events were absent in
18,733 states, one tick away in 23,401, and two ticks away in 23,402.

For diagnosis only, grouping this prefix by the remaining fields after removing
history ages gives 10 groups; retaining only history length gives 73. These are
counts of projections of already visited states, **not** valid reduced graphs,
guaranteed exploration sizes, or proofs of NOT. The original run stopped before
the held-input cycle-check stage.

## Concrete reasons not to merge by snapshot or count alone

The same physical model can reach a lit torch with input off and no pending
callback after either zero or seven recent off events. Giving both states the
same input-on interval followed by input-off makes only the latter burn out.
Visible block/electrical state alone is insufficient for exact continuation.

Even preserving the number of off events is insufficient. Using the first seven
pulses of the captured `inclusive_sixty_tick_window` case, inspect the state at
tick 60 and at tick 61, before the final input change. Both have input off, a lit
torch, seven off events, no callback and no previous burnout. Their ages are:

```text
A: [58, 50, 42, 34, 26, 18, 10]
B: [59, 51, 43, 35, 27, 19, 11]
```

Apply input on for two ticks to each. In A, the oldest event reaches age 60 and
still counts; the new off event triggers burnout and a 160-tick callback. In B,
the oldest event expires at age 61, so the new event leaves the count at seven.
Apply input off for two more ticks: A stays off with 158 ticks pending; B lights.
This continuation was checked with `PhysicalBehaviorModel`; the inclusive and
expired window cases also have existing pinned server-observation fixtures.

These are counterexamples to **exact physical equality**, not counterexamples
to `RepeatedSettling`. Both may still eventually produce the required output.
A type-specific proof may handle them together, provided it accounts for every
represented continuation, later input changes and eventual settling.

## Progress must survive a type-specific abstraction

Ignoring a finite delay in the type does not allow a callback to wait forever.
For example, collapsing concrete `remaining=2` and `remaining=1` into `waiting`
turns a finite countdown into these abstract possibilities:

```text
Concrete: 2 -> 1 -> done
Abstract: waiting -> waiting | done
```

The abstract self-loop can describe waiting forever unless the proof carries a
reason progress must occur. Choosing a representative may instead skip input
opportunities or transitions. Either approach can change the conclusion.

The current cycle checker expects one deterministic successor for a state and
held input. It cannot directly consume a set of possible successors with hidden
progress conditions. Symbolic history sets, local settling lemmas or summaries
of finite waiting would require an explicit proof interface. This inventory
does not select or implement one.

There is a plausible boundary for further analysis: retain exact state for
execution and replay, and prove a pinned local law's settling behavior for sets
of histories when its input becomes stable. Applying such a result to a circuit
also requires proving when that device's support power becomes stable; feedback
or another torch can invalidate that assumption. Classification names and a
parent's passing result do not supply this proof.

## Recommended order and acceptance conditions

1. Define a proposed verifier projection with an explicit observation boundary.
   Start with `burnout_seen` as the bounded example. Keep runtime state and
   existing diagnostics. Record law/type/profile/binding context and show that
   equal projected states have equal outputs, projected successors and execution
   success for **every** input assignment and tick step. This is an infrastructure
   example, not the expected cure for the current history explosion.
2. For the dominant history/timer fields, compare symbolic history representation
   and local-law settling summaries on the isolated torch. Specify what is kept,
   what is summarized, how arbitrary input changes are covered and how progress
   is proved. Measure required proof work; do not infer savings from the 73-group
   diagnostic projection.
3. Only after an argument covers the isolated law, define how circuit topology
   and interactions discharge its assumptions. Preserve all named outputs and
   inspect feedback explicitly. Unknown cases stay undetermined.
4. Implement a chosen reduction only after this design is reviewable. Validate
   proposed exact projections against the current transition model on closed,
   manageable cases, including negative cases where a changed law reads a
   formerly diagnostic field. Abstract counterexamples need concrete replay or
   further refinement; an artificial abstract cycle is not a proven circuit
   failure. Recheck the existing physical observation regressions and retain
   the full state for replay and diagnostics.

This order preserves the accepted type and immutable source/state separation.
It introduces no input-rate restriction, fixed timing requirement, implicit
child certification, live deployment or new public MCP endpoint. Block-count
optimization remains after a usable behavioral proof gate.

The later [single-torch settling investigation](torch-settling-proof.md) uses
the block-effects profile, establishes the electrical independence premise for
the existing NOT layouts, and replays held-input continuations from every
retained isolated-torch sample. It distinguishes a conditional local proof from a data-driven abstract verifier.
The latter is now [implemented separately](abstract-behavior-verification.md),
without changing concrete histories or exact equality. [Contextual adoption](repeated-settling-adoption.md)
now revalidates complete port mappings and actual input drivers. The subsequent
[block-count search](blueprint-block-reduction.md) uses this gate for each candidate.
