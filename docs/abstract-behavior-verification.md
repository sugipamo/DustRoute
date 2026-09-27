# Conservative repeated-settling verification

The user approved a separate verification abstraction while retaining complete
physical execution state. The implementation is available through
`PhysicalBehaviorModel::verify_history_abstraction` for the explicit
`dustroute.dust-single-torch-block-effects.v1` profile. It proves the original
`RepeatedSettling` relation over all represented input histories; it does not
restrict input frequency, impose a settling deadline or assign behavior from a
classification label.

The existing `verify` method still explores exact states. `Periodic` and
`FiniteBurst` still use their exact autonomous traversal. Full history lists,
registers and pending callbacks remain in concrete execution and replay.

## Representation and transition coverage

[`AbstractLawState`](../crates/dustroute-minecraft/src/law/abstract_history.rs)
is a different, non-deserializable type from `LawState`. For each history it
stores its exact count and the age of its youngest event. Empty history has
count zero and no youngest age. All inputs, registers (including diagnostic
registers), callback names and relative delays remain exact.

For a history `H`, define its projection as `alpha(H) = (length(H), min(H))`,
with a separate empty case. The abstraction may contain extra possibilities,
but must include the projection of every concrete successor and every possible
execution error:

1. **Expiration:** if the youngest event already has the inclusive window age,
   every event expires on the next tick. Otherwise the youngest survives, ages
   by one and the remaining count can conservatively be any value from one to
   the old count. All histories branch independently; their complete Cartesian
   product is explored. Multiplicity is retained in the count.
2. **Instruction execution:** the law language can read a history's count and
   check capacity, but cannot read individual ages. An internal encoding with
   the same count and youngest age therefore gives identical expression values,
   assignments, branches, capacity failures and scheduling effects. `Remember`
   increments the count and makes the youngest age zero. Exhaustive matches over
   the instruction language require future language extensions to revisit this
   boundary.
3. **Callbacks:** exact and abstract execution share the same countdown,
   due-handler ordering, first-request scheduling, instruction interpreter and
   synchronous register-effect delivery. The internal encoding is never used
   to select one concrete history-expiration trajectory. Any possible error
   stops verification with an undetermined result.
4. **Physical coupling:** the adapter uses the same world, electrical solver,
   selected dust program, bindings and single-torch effect semantics as concrete
   execution. Electrical outputs depend on the external inputs and lit register,
   which remain exact. Feedback is recomputed during a changed lit assignment,
   before the remaining local instructions. Passing time or expiring history
   does not invent a neighbor notification.

Initialization projects the exact fresh-construction state, including its
initial notification. Input assignments preserve memory and are idempotent.
The abstraction also explores successive input assignments without a tick in
between. These local coverage conditions compose along every concrete trace.

This is an overapproximation, not an assertion that histories with equal
summaries have identical future behavior. A history-count-only representative
would lose possible expiration behavior; using every successor is essential.
Retaining youngest-age and callback countdown progress also avoids treating
finite waiting as arbitrary waiting forever.

## Graph acceptance

[`verify_abstract_repeated_settling`](../crates/dustroute-translate/src/abstract_behavior.rs)
first closes the reachable abstract graph under all input assignments and all
successor branches. It then examines strongly connected components separately
for each held input vector. Every cyclic component must have only the required
output vector. Nonterminal cycles count too: an optional exit cannot justify
assuming that an execution eventually takes it. Internal cycling with correct
outputs is allowed, as are finite transient errors before settling.

On a finite closed graph, an infinite execution with infinitely many incorrect
outputs would revisit an incorrect state in a cyclic component. Excluding every
such component establishes eventual permanent correctness on every abstract
execution, and therefore on every covered concrete execution.

Results are deliberately asymmetric:

| Result | Meaning |
| --- | --- |
| `passed` | The entire overapproximate graph closed, no possible execution error occurred, and all held-input cycles satisfy the original relation. |
| `undetermined` with a possible cycle | The abstraction permits nonsettling behavior; this is not a replayed concrete counterexample. |
| `undetermined` with a resource/unsupported/error diagnostic | Coverage or execution could not be established; no pass is issued. |

The abstract route never emits `failed` without concrete replay. It does not
silently discard branches to stay within a budget. The existing exact verifier
retains its concrete-failure reports.

Reports include the verification method, abstraction version, state/transition
counts, profile, initial condition and complete selected Assembly/type/law
records. They are diagnostics, not deserializable proof tokens. Electrical
caching is confined to one immutable model and keys every changing electrical
dependency. State ownership checks prevent mixing different physical models.

## Verified scope and results

With the existing pinned laws and default 65,536-state, 1,000,000-transition and
30-second computation budgets, all four physical layouts close at **9,520
abstract states and 74,900 transitions**:

- Isolated wall torch with one lever and its support.
- Isolated standing torch with one lever and its support.
- The existing `not_torch_top` placement with its actual lever binding.
- The existing `not_torch_block_power` placement with its actual lever binding.

These are model-level arbitrary-input repeated-settling passes. The prior exact
65,536-state measurement exhausted its budget in the older synchronous profile;
it did not establish a total exact graph size. No compression percentage or
full Vanilla guarantee follows from comparing those counts.

Regression evidence includes exhaustive small-history transition coverage,
unordered/duplicate ages, zero/maximum windows, multiple histories, effect order,
capacity errors, alternative bad cycles with exits, hidden-state cycles,
resource boundaries and multiple input assignments between ticks. For every one
of the 3,888 retained isolated-torch samples, both possible next input assignments
and their concrete steps are covered by the physical abstraction. The original
sample observations still match concrete execution under both profiles.

A changed history window is executed from its selected program and can still
pass. A law that overflows history, a law that reads the diagnostic register to
break output behavior, and the feedback layout do not pass. Different law
Revisions are never silently replaced. Multiple inputs/outputs and memoryless
wire circuits retain the original relation's vector order.

```bash
cargo test -p dustroute-minecraft law::abstract_history
cargo test -p dustroute-translate --test abstract_behavior
cargo test -p dustroute-translate --test physical_behavior
cargo test -p dustroute-translate --test physical_periodic --test periodic_clock_observation
```

## Remaining integration

The state-explosion prerequisite is resolved for these NOT realizations in the
selected model. [Contextual Blueprint/MCP adoption](repeated-settling-adoption.md)
now uses fresh abstract proofs with complete port mappings and validated actual
input controls. [Movable-port block-count search](blueprint-block-reduction.md)
now uses these fresh checks; no saved abstract report authorizes adoption.
Restartability, multi-torch scheduling, piston laws and live-world placement
remain outside this implementation step.
