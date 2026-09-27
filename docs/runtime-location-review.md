# Moving-world behavior exploration and review

The Rust verification path connects explicit location bindings to the selected
piston runtime. It retains the complete actual Assembly and checks
input histories during motion. This is model evidence, not new live Minecraft
conformance evidence or, by itself, authorization to adopt a revision.

New work uses `RuntimeBehaviorContext::fresh_pistons` or public
`behavior_context.piston`, which selects the electrical profile for all six
directions. See [custom construction](custom-piston-assembly-placement.md).
The same explorer also retains the opt-in
`dustroute.piston-direct-root-exploration.v1` context for the
[unified horizontal/up/down runtime](unified-piston-runtime.md). That profile
has a separate isolated direct-input admission contract; the horizontal context
described below retains its historical electrical assumptions. No existing
Blueprint requirement or saved review is migrated automatically.

For historical horizontal execution, `RuntimeBehaviorContext` selects
`dustroute.horizontal-piston-root-exploration.v1`, fresh construction, a known
region, actual external input levers and computation limits per synchronous
root. Its physical dependency is the separately pinned
`dustroute.horizontal-piston-callbacks.java-1-21-11.v1` world profile. Source law
requirements are checked against that selection. Including a law in a Blueprint
does not execute it again or create another physical state.

`RuntimeBehaviorModel::from_fresh_assembly` resolves the declared binding through
the actual occurrence, then constructs the native runtime from all actual blocks.
It never constructs the world from a source's default geometry. Terminals remain
at their resolved coordinates. Its opaque states belong to one model instance;
they cannot be imported from a snapshot or transferred to another model.

## What is compared

`PistonBehaviorState` is a process-local comparison representative for the pinned
native adapter, with method `dustroute.horizontal-piston-root-comparison.v1`.
It is not a generic runtime optimization or a deserializable checkpoint.

| State component | Treatment |
| --- | --- |
| Every world block and its metadata; known region | Retained |
| Pending roots, payloads, guards, relative delays and insertion order | Retained |
| Current tick section | Retained |
| Carrier position, progress, last progress and lifetime relationships | Retained |
| Saved carrier time | Retain whether it equals the current tick; the pinned control law reads that equality |
| Current absolute time | Keep zero distinct; normalize positive epochs to one and preserve pending delays |
| Event/cause IDs | Rename root IDs consistently; omit diagnostic causes, which this adapter never reads |
| Carrier IDs | Rename active and retired referenced lifetimes consistently; new tokens remain distinct |
| Trace and processed-operation counter | Excluded from recurrence; renew the computation budget per complete root |

Fresh carrier history has saved time zero. Normalizing a positive tick to zero
would change reversal decisions and is forbidden. Retired queued carrier ticks
remain present: filtering them out of the graph would remove input opportunities.
Unsupported queued payloads are rejected if a later adapter change introduces
new captured state that this comparison does not understand.

Comparison is available only at a complete root boundary, with no pending
immediate input or active continuation. Failed runs and unfinished motion with
no pending work cannot become representatives. Restored representatives retain
the complete world and scheduling state; exact checkpoints, exact state keys and
cumulative diagnostic-runtime limits retain their original meanings.

The comparison models physical recurrence with a relative time origin. It does
not prove that an indefinitely running machine integer clock or diagnostic ID
counter will never overflow. Native exact execution still rejects overflow.
Root, state, step and elapsed limits are computation limits; exhausting one
cannot certify the circuit or become a circuit timing requirement.

## Input and observation boundaries

An exploration advance finishes one complete queued root and its synchronous
descendants, or exposes the next tick's External boundary before later work.
The world clock also advances while the queue is empty. This preserves input
changes before the next carrier tick and after an idle interval. The existing
native `step()` operation itself remains unchanged.

Inputs operate explicit actual levers through native callbacks. Assigning the
same input vector again is idempotent and does not restart physics. For multiple
inputs, each changed lever is a complete input root; exploration also covers
vectors differing by just one input, so intermediate assignments remain reachable.
The initial notification root remains pending in the fresh-construction state.

Each committed microstep is observable even though another input cannot interrupt
that synchronous root. The repeated-settling verifier includes those observations
on its edges. A recurrent internal output pulse cannot be hidden by sampling only
the value after the root returns. Consecutive identical output samples can be
coalesced because this type imposes no numerical duration requirement.

A pass requires the reachable graph to close and every held-input cycle to have
the required output values throughout. A finite prefix leading to an established
bad held-input cycle is already sufficient for a failure, even if other branches
have not been explored completely. Reports expose `graph_closed` separately from
the status. A budget limit without such a counterexample remains undetermined.
Counterexample `SetInputs`/`Advance` sequences replay from the same initial model.

## Whole-realization review

`review_assembly_in_runtime_context` checks each declared behavior independently
and observes every visited physical state and committed microstep for retained
static obligations. It records the actual Assembly, explicit context, occurrence
results and behavior diagnostics. Results cannot be deserialized into authority.

The monitor preserves fixed terminals, exact `BlockPattern` requirements, block
identity requirements, known source positions and explicit source Air. Ordinary
source geometry is still an interpretation rather than an equality constraint.
Law requirements, source connections and consumer requirements remain independent
checks. Unclassified surrounding blocks remain part of physical execution.

A parent can pass its opening/closing relation while a child requiring an actual
Piston at the body location fails during retraction, when that coordinate contains
a MovingPiston. The failed interpretation does not roll back the physical move or
replace the child. Inherited explicit Air also remains attributed to ancestors
by the existing source-claim index; that can fail both parent and child placement
checks even while the parent's behavior check passes.

Known failures remain failures alongside unknown locations, unsupported checks,
or an exhausted budget. A static pass over execution requires a closed physical
graph; an initial snapshot or a successful prefix is insufficient. Parent passes
never change child revisions, their requirements, or terminal positions.

## Supported scope and adoption

The current native proof adapter supports location-only `RepeatedSettling`
bindings whose inputs are explicit powered-state predicates on actual levers.
Both predicate polarities are supported. Each relation must cover exactly the
context's distinct externally controlled levers. Other levers remain fixed by the
declared environment. Multiple named outputs can observe the same coordinate.

The complete known space must be covered by a declared rectangular region;
fragmented regions are not silently filled and surrounding state is not cropped.
Unsupported physical blocks, imported motion without history, unknown required
observations and unsupported physical execution remain errors/undetermined.
All limitations of the [native piston adapter](piston-callback-runtime.md) apply.

Mixed/electrical observation bindings, moving-world route validation, producer
behavior requirements across routes, and autonomous Periodic/FiniteBurst checks
are not yet supported by this new review. They remain undetermined. Existing
fixed-geometry contexts keep their separate capabilities and meanings.

The native context now enters the existing review/proposal/adoption path through
`BehaviorReviewContext`, which selects either the old fixed-geometry context or
the native context without changing the JSON shape of either. The common review
retains native child results and counterexamples. Its adoption gate does not
manufacture an old `ValidatedWorld` for moving geometry. Selected law and source
dependencies stay pinned, and explicit adoption revalidates the complete candidate.

The existing MCP read, proposal, review and decision tools accept this same
context. Native histories require proposal-history v5; even a native context
present only in an old validation event prevents downgrade. Saved diagnostics,
including edited apparent passes, cannot supply adoption authority. Old context
objects and archive versions retain their meanings. No door-specific endpoint,
automatic revision replacement or live-world write was introduced. See
[MCP usage](blueprint-mcp.md#location-behavior-in-the-moving-world).

## Regression evidence

The focused tests cover canonical-state replay through interrupted movement,
retired deliveries, progress/time/section distinctions, empty-clock boundaries,
root-internal pulses, replayable failure before complete closure, immutable
sources and coordinates, parent success with child failure, inherited explicit
Air, literal rotated patterns, unknown space and computation limits.

The body-state relation closes and passes in the native model. The existing
single-input 1×2 fixture produces a replayable counterexample to an OFF-to-empty
relation under arbitrary input histories. Its earlier settled ON/OFF observations
are still retained; they are a narrower claim.

Adoption regressions additionally cover relocated candidate terminals, pinned law
dependencies, fresh verification after archive reload, forged historical passes,
unfinished execution, and rejection of archive schema downgrades. The MCP test
uses the existing tools through a server restart and checks both successful
adoption and refusal when a retained child fails during motion.

Validation on 2026-09-21 passed **795 workspace tests**, with zero failures and
one existing manual scalability test ignored, across 83 test/doc-test groups.
This includes the multiple-input/polarity case and the Rust/MCP adoption tests.
Workspace all-target Clippy passed with `-D warnings`; formatting and diff
whitespace checks passed.

```bash
cargo test -p dustroute-minecraft piston_behavior --lib
cargo test -p dustroute-translate --test location_behavior --test behavior_types
cargo test -p dustroute-translate --test runtime_adoption
cargo test -p dustroute-mcp runtime_blueprint_review_and_adoption_preserve_child_failures_after_restart
```
