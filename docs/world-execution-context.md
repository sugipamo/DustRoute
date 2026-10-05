# World execution contexts

New piston work uses the shared electrical runtime for all six directions:
`new_piston_runtime` for execution and `RuntimeBehaviorContext::fresh_pistons`
for review. Public MCP requests can use `behavior_context.piston` without a
directional profile choice. See [custom construction](custom-piston-assembly-placement.md)
and its [comparison evidence](piston-electrical-live-evidence.md).
The [unified runtime](unified-piston-runtime.md) replaces the retired direct-input
and directional callback profiles. Saved records still require exact supported
profile IDs; retired IDs are rejected without defaults or checkpoint conversion.

A Blueprint declares the immutable physical laws it requires. The world selects
and executes those laws, and owns the actual block state, histories and pending
work. Interpreting the same blocks as several nested or overlapping Blueprints
never creates another device, timer or physics update.

Phase 4b brings the existing execution models under a common contract. It does
not combine their different device behavior or extend behavioral proof to every
published law.

## The shared contract

[`WorldExecutionContext`](../crates/dustroute-minecraft/src/execution_context.rs)
records a model profile, exact law Revision IDs by role, initialization and input
policies, and the settings that model implements. Resolution checks the selected
sources and their transitive dependencies. An unknown or extra role, incompatible
initialization/input policy, changed fixed pin, or invalid scheduler/motion
setting is rejected. Resolving references alone is not an execution or behavior
proof; an adapter must also compile and implement the selected program.

| Model profile | Laws and supported execution | Initialization and external inputs | Ordering |
| --- | --- | --- | --- |
| `dustroute.dust-torch-synchronous-game-tick.v1` | Selected dust/torch and four fixed spatial laws; existing fixed geometry and supported blocks | Fresh declared torch state, empty history/timers, initial notification; actual lever bindings between steps | Parallel one-game-tick torch advancement and electrical settling |
| `dustroute.dust-single-torch-block-effects.v1` | Same law families, at most one torch; supported local block effects | Same fresh construction and physical input binding checks | Synchronous feedback after visible torch changes, with the existing single callback slot |
| `dustroute.redstone-compatibility-boundary.v1` | Fixed dust, torch, spatial, repeater, comparator, observer and lamp programs | Existing device seeds, caches, observations and queues; explicit compatibility mutation APIs | Two-game-tick boundaries and the configured compatibility event ordering |
| `dustroute.bounded-redstone-events.v1` | Fixed spatial/dust, bounded repeater/lamp, and four piston programs | Supplied blocks with an initially empty event queue; explicitly scheduled inputs/world changes | Existing event queue, scheduler and piston activation/completion settings |
| `dustroute.piston-electrical-callbacks.java-1-21-11.v19` | Eighteen fixed roles including device callbacks, comparator output, torch history, electrical queries, payload and motion; all six body facings, slime/honey adhesion and declared crop destruction/support | Explicit stable fresh construction, initialization notifications and actual levers/device uses between synchronous calls; declared fixed soil and enclosed source water | One callback queue with source-ordered notifications, independent carrier ticks and deferred support ticks |

The v19 profile pins `dustroute.device-programs.java-1-21-11.v7` and physical
admission v10 in addition to its selected laws. It includes stone-button use/release in the library runtime;
the behavioral explorer continues to accept explicit lever bindings. Previous
electrical v1–v18 execution contexts/checkpoints are rejected rather than migrated.
The v19 root comparison contract v5 uses a versioned non-JSON encoding of the
complete normalized state, retaining identity-guarded deferred support ticks,
device ticks and carriers. See [comparison migration](json-boundary-migration.md)
and the
[fixed environment and cane scope](existing-machine-modification.md).
Adhesive materials are explicitly excluded from the older execution adapters,
including the standalone bounded piston planner. Their passive registration
does not grant those adapters branching movement. See [adhesion](piston-adhesion.md).
See [typed device definitions](typed-device-runtime.md) for compile-time contracts,
multiple properties, analog signals and variant selection.

The former directional and isolated-direct callback profiles are retired. Their
saved IDs are rejected, not converted to electrical behavior. See
[retired paths](stabilization-legacy-paths.md).

The compatibility and bounded profiles describe distinct existing adapters.
The bounded runner samples supplied comparator/observer outputs but does not
calculate their timing or pulses. Its piston laws retain the existing horizontal
low-layer subset. Neither native profile is accepted as a dust/torch behavioral
proof profile. Arbitrary spatial or native-device law replacement and new device
coupling need separately implemented adapters and justified execution semantics.

## Execution and state ownership

The proof model resolves its laws from the normalized contract before executing
the selected programs. Compatibility and bounded runners use shared bundles of
their fixed stateless adapters. Their constructors retain the old defaults:

- `RedstoneTickSimulator::new_in_context` validates a complete compatibility
  contract before creating the simulator. `execution_context()` reports its
  actual selection, including scheduler overrides.
- `PhysicsEngine::with_execution_context` accepts only the bounded contract and
  settings the engine already supports. Its `execution_context()` reflects
  current scheduler and piston settings, including after checkpoint restoration.
- Existing profile-specific builder methods remain available. Shared context
  construction does not erase pending events or change the old input APIs.

The context is an assumptions record, not a saved simulation. Worlds retain
actual geometry and input bindings, known regions, device caches, history,
reservations, pending events and execution budgets in their existing owners.
A bounded checkpoint retains these fields for exact in-memory continuation.
Reconstructing from visible blocks and a context cannot recover unknown live
history, timers or scheduler phase. The initializer labels describe model
construction; they do not certify an observed world's hidden state.

## Bounded dust input correction

The preflight found an unmigrated native wire-strength formula and diagnostic
propagation of values above 15. The repository audit found no existing dependency
on that out-of-domain bounded behavior. The approved correction is now applied:

- The original immutable dust Blueprint asset is shared with the low layer.
  Both adapters compile its memoryless program over the 16 × 16 legal domain.
- The bounded evaluator accepts resolved signal levels from 0 through 15.
  Levels above 15 produce an error before the affected event commits or is
  consumed. This also covers analog input consumed by its receivers and pistons.
- Raw observation records and source Revisions remain readable and unchanged.
  Unrelated raw fields are not interpreted as new signal inputs. Validation is
  local to the facts consumed by the event, not a whole-world rewriting pass.
- The old translate dust adapter retains its clamp policy. The separate
  compatibility comparator's explicitly supported `u8` behavior also remains.
  The bounded correction does not globally narrow the block representation.

The [preflight capture](blueprint-architecture.md#phase-4b-preflight-bounded-dust-strength-migration-gap)
is historical evidence. Running its diagnostic example now shows rejection for
16 and 255; do not regenerate the historical fixture to match the new policy.

## Persistence, review and MCP

`PhysicalBehaviorContext` keeps its existing serialized form. Its
`execution_context()` derives the shared contract without rewriting archived
source pins, review requests, old histories or input bindings. Old catalogs can
still omit the profile's implicit spatial records. A supplied conflicting record
under a fixed ID is rejected rather than silently replaced with the built-in.

Dependency selection and executable support remain separate checks. A Blueprint
requiring an unselected law fails its requirement check. A selected program whose
world adapter is unsupported remains undetermined. Catalog membership, a parent
pass or an old review cannot supply a missing child proof. Adoption still runs
fresh checks. Optimization may relocate ports and drivers but cannot change the
execution assumptions to make a candidate pass.

An existing MCP Assembly query with `validate: true` returns the original
`validation_context` and the additive `world_execution_context` diagnostic
(null if no behavior context was supplied). No new endpoint or native-device
behavioral proof input is added. Proposal/archive formats stay unchanged.

## Verification and remaining work

Regression checks cover legal dust inputs and rejected values, atomic event
failure, actual context construction, incompatible contracts, checkpoint
continuation, per-world state isolation, unchanged archive formats and immutable
pins. Retained device captures compare state transitions, timing, no-ops and
pending work. Existing nested/shared interpretation, review/adoption,
optimization, fixed 1×2 piston and MCP regressions remain applicable.

The 2026-09-21 workspace run passed **718 tests, 0 failures**, with one existing
manual scalability measurement ignored (77 result groups). `cargo fmt --all
--check`, `cargo clippy --workspace --all-targets -- -D warnings`, and
`git diff --check` also passed. Historical device fixtures were not regenerated.

These are model and retained-observation regressions, not new live-world evidence.
After phase 4b, the user agreed that terminals denote locations and that movement
of constituent blocks is a state transition of the complete realization. Local
processing is a computational partition; terminals do not follow moved blocks.
Optimization may still relocate terminals in a separately verified candidate.
See the [movement semantics and implementation gaps](blueprint-architecture.md#agreed-movement-semantics).

General mechanical types, new device coupling and entities remain separate
implementation work. The piston candidate/placement and native observation
migrations subsequently acquired their declared-case implementations and evidence;
see [general piston placement](piston-general-placement-roadmap.md) and
[native client rollout](voxrig-rollout.md). The historical location-state
verification sequence is described below. The semantic
agreement does not upgrade the old dust/torch profiles, reinterpret snapshot
conditions as temporal requirements, or rewrite immutable sources.

### Historical movement prerequisites and subsequent cutover

The movement goal found an execution prerequisite at motion-time input
reversal. See the [preflight and six model captures](blueprint-architecture.md#movement-verification-preflight-input-changes-during-motion).
A drained bounded queue cannot certify the existing repeated-settling requirement;
its input histories exceed the retained piston conformance scope.
The user approved widening that scope without weakening `RepeatedSettling`.
The [target-version source audit](piston-motion-source-audit.md) then identified
a runtime prerequisite: enclosing tick context, synchronous nested notifications
and per-carrier motion history. That prerequisite was subsequently approved;
the [synchronous runtime foundation](synchronous-world-runtime.md) now provides
the delivery/state primitives and mandatory adapter initialization gate. It was
originally the delivery contract of a separate horizontal piston world profile,
whose context selected 13 law roles. That directional profile was subsequently
retired in favor of the all-facing electrical v19 profile described above; see
[the current callback runtime](piston-callback-runtime.md).
`synchronous_runtime_profile()` reports the required
runtime without reinterpreting the old serialized scheduler field. This native
profile family is not accepted by the bounded/compatibility constructors or dust/torch proof
adapters. The separate `RuntimeBehaviorContext` now supplies location-only
repeated-settling exploration and whole-realization review. The common
`BehaviorReviewContext` dispatches to either model without rewriting old context
JSON. Existing MCP/proposal/adoption paths use that selection and revalidate
before publishing records; native contexts require proposal-history v5. See
[native verification and review](runtime-location-review.md). The old profiles
have not acquired new behavior or stronger evidence.
