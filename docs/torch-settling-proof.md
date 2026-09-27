# Single-torch settling: premises and proof options

Status: investigation retained, 2026-09-20. The user subsequently approved a
separate verification abstraction. Its [implementation and model proofs](abstract-behavior-verification.md)
now close the arbitrary-input NOT graph. This offline audit remains finite
evidence. [Contextual adoption](repeated-settling-adoption.md) now performs fresh
abstract proofs, now used by [block-count search](blueprint-block-reduction.md).

The selected context is fresh construction under
`dustroute.dust-single-torch-block-effects.v1`, with the exact current executable
dust and Java 1.21.11 torch law records. The older
[state inventory](behavior-state-reduction.md) measures the synchronous profile;
its graph counts must not be presented as measurements of this newer profile.

## New reproducible evidence

Run the read-only diagnostic:

```bash
cargo run -p dustroute-translate --example audit_torch_settling
```

Its JSON includes the complete selected law records, profile, static electrical
rows and per-case continuation counts. `repeated_settling_verified` and
`adoption_authorized` are explicitly false. No Minecraft connection is used.

For each of four layouts, the electrical audit enumerates every combination
of the external lever and the torch's Boolean output (four rows). It does not
fabricate a reachable execution state or reconstruct hidden memory.

| Layout | Torch support power | Observed output |
| --- | --- | --- |
| Isolated wall torch | External lever, independent of torch output | Torch lit state |
| Isolated standing torch | External lever, independent of torch output | Torch lit state |
| Existing `not_torch_top` with its lever | Same | Same |
| Existing `not_torch_block_power` with its lever | Same | Same |
| Four-block feedback circuit, no lever | Torch lit state | Torch lit state |

The audit also replays all 18 retained isolated-torch observation cases. From
each of the **3,888** sampled prefix states it forks two exact continuations,
holding the input OFF or ON. Histories, registers and pending callbacks are
retained. Every tail is followed to a complete-state cycle; caches contain only
exact states within one model and held input.

All **7,776** continuations settle to inversion. Across the cases, the audit
visits **32,212** exact held-input states (summed across separate models/caches).
The maximum measured settling times are **160 game ticks for input OFF** and
**2 for input ON**. These are model results for the sampled prefixes, not timing
requirements, new server observations, or proof over every possible input
history. The 512-step diagnostic traversal cap aborts on exhaustion; reaching
the cap cannot count as a successful tail.

## Conditional local argument

The following argument concerns the exact pinned torch program, not a `NOT`
label and not arbitrary future law Revisions. At every atomic execution boundary,
let `p` be support power, `l` the lit register, `H` the off-event age list, and
`q` the pending callback delay, if present. History ages are inclusive through
60; the threshold/capacity is eight; callback requests are 2 or 160 ticks.

Assume from fresh construction that the support power depends only on external
input levels and never on this torch's lit state. Initial notification and every
external input change deliver the selected profile's support notification.
There is one torch, fixed supported geometry and a convergent electrical solve.

The law and these assumptions preserve these boundary invariants:

1. A lit torch has fewer than eight retained off events. Initialization starts
   with empty history; relighting requires a count below eight. History can only
   grow when a lit torch is turned off. Therefore `Remember` cannot overflow.
2. If `l == p`, a callback is pending. Input notification schedules one when
   absent. A due callback corrects the output, except for the history threshold
   guard discussed below.
3. If eight off events remain, the torch is OFF and a callback is pending with
   `q + youngest_age(H) == 160`. At the eighth off edge, the changed lit state
   is already correct for the unchanged support power. Its synchronous neighbor
   effect cannot occupy the callback slot, so the subsequent recovery request
   gets all 160 ticks. Later input changes cannot replace this pending callback.
   Aging decrements the delay and increases the youngest age together; if any
   event expires, the count falls below eight.
4. A pending delay is at most 160. A lit torch's pending delay is at most two,
   because the only 160-tick request follows a transition to OFF.

Invariant 3 excludes a due callback whose OFF torch is still blocked by eight
off events: those retained ages are at most 60, so its pending delay would have
to be at least 100. This discharges the exceptional branch in invariant 2.
The invariants cover input changes during settling and multiple input changes
between ticks; no input-rate restriction is required.

After fixing `p`, a wrong output has a finite pending delay. The callback
corrects it and cannot subsequently change it while `p` remains fixed. The
delay is at most two for `p = ON` and at most 160 for `p = OFF`. Remaining history
or a harmless later callback may outlive output settling. This is a conditional
argument for all reachable histories, distinct from the finite continuation
experiment above; it is not yet an executable proof accepted by the verifier.

## Why eventual stable support alone is insufficient

The independence premise must hold throughout the relevant reachable history,
not merely after the final input change. In the four-block feedback circuit,
turning the torch OFF immediately changes its support power to OFF too. The
neighbor effect then occupies the slot with a two-tick callback before the
eighth off event and recovery request are processed. At tick 30, history count
is eight but the pending delay is two: invariant 3 does not hold.

That callback runs while relighting is blocked. No new callback is created by
history expiry alone. Support power eventually remains OFF, but the torch also
remains OFF. This is already covered by the retained
[clock observations and model comparison](periodic-clock-conformance.md).
It prevents applying an isolated-torch summary to every circuit that eventually
has a stable local input. Source names and a passing parent do not establish
the required premise.

## Considered proof approaches

The original repeated-settling verifier requires a closed graph of exact
execution states. The investigation considered two approaches preserving the
user's circuit requirements:

- A conditional local-law proof, with an exact program check and exhaustive
  electrical premise checks for each actual placement. This is a small route
  for the present single-torch NOT layouts, but changed programs need their own
  proof or must fall back to `undetermined`. A matching law ID alone is unsafe;
  the program and adapter assumptions must be checked.
- A data-driven nondeterministic abstract verifier. Keep all registers, inputs
  and callback delays, while representing a history by its count and youngest
  age. This needs a separate transition interface and a soundness argument; it
  cannot be supplied as an exact state key to the deterministic checker.

For the latter, a nonempty history whose youngest event survives the next tick
may conservatively retain any count from one through the old count. Once even
the youngest event expires, the history is empty. `Remember` increments the
exact count and resets youngest age to zero, preserving capacity-error checks.
This overapproximates expiration without selecting a representative history.
Exact delays and youngest-age progress prevent a finite wait from becoming an
unbounded abstract self-loop. Other instructions, synchronous effects and
electrical coupling still need their own transition-coverage checks.

A pass would require the complete abstract reachable graph and every held-input
cycle to satisfy the original output relation. An abstract bad cycle would be
undetermined until a concrete witness is replayed. Real execution and replay
would continue to retain full history lists. This design has since been implemented as a separate verification route; its
reports do not authorize adoption by themselves.

The user approved establishing a pass using a sound proof over sets of complete
states while retaining complete physical execution/replay. The data-driven
abstract verifier implements that decision. Its soundness boundary, regression
evidence and remaining integration are documented in
[abstract behavioral verification](abstract-behavior-verification.md).
