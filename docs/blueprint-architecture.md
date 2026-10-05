# Blueprint architecture and migration contract

This document is the single source of agreed Blueprint requirements and migration
scope. The workflow, physics and evidence documents linked below describe
implementations and reproducible results; they do not define a competing target
architecture. A planned capability is not an implemented capability.

The current migration continues on `codex/blueprint-architecture`. Its objective
is to move the remaining supported behavior into the Blueprint system in
dependency order, preserve existing guarantees, and keep the explicitly deferred
features deferred. This is a migration of specifications and execution, not just
a change in where constants or names are stored.

## Agreed model

- A classification such as NOT or AND labels interpretations of realizations.
  Its name does not execute a gate or prove a type requirement. A classification
  can have several realizations; containing a B realization is a property of one
  A realization, not a requirement that every A contain B.
- A Blueprint Revision is immutable source data. Nested references pin exact
  Revision IDs. An Assembly Revision separately records placed occurrences,
  actual blocks, actual routes, external boundaries and observation coverage.
  A new source or state never overwrites an old revision or updates its users.
- Realizations may overlap. There is one actual state at each physical position,
  regardless of how many parents or children describe it. Source differences
  remain visible and are not automatic equality failures. Explicit obligations
  determine whether a changed realization is still acceptable.
- Raw blocks and unclassified regions are valid. Meaningful parts may be promoted
  after validation; every block need not acquire a Blueprint interpretation.
  Imported, built-in and locally authored sources share the same representation.
- Interfaces may have several inputs and outputs. Connection types check signal
  compatibility and declared physical conditions without inferring logical
  meaning. Explicit behavioral types separately state observable requirements.
- A consumer's requirements apply to its selected producer terminal, not to the
  producer's classification or unrelated ports. Mechanical state cannot be
  silently reduced to electrical signal state.
- Physical positions, block kinds and block states are representable. Missing
  state is unknown unless observation coverage or explicit records establish
  air. Unknown state is never invented to make a candidate pass.
- A terminal denotes a location in a concrete placement. Its value can be the
  presence, absence or state of a block there. Physical movement does not make
  that terminal follow the moved block. New optimization candidates may choose
  different terminal positions without changing the selected type requirement.
- Movement of some constituent blocks is a state transition of the complete
  realization. Local computation or optimization is a computational partition,
  not a different meaning for the Blueprint. Movement alone is not evidence of
  damage; the declared obligations and complete physical context determine it.
- The parent, every retained descendant, shared interpretations and surrounding
  physical context must be checked independently. A passing parent cannot hide
  a failed or undetermined child, and cannot alter that child's source.
- A child update may be considered through an explicit parent update proposal.
  Review and adoption are different decisions. Adoption creates new immutable
  values; existing references remain pinned.

## Behavioral types and executable laws: current extension

The existing contracts and their meanings remain unchanged:

| Contract | Requirement |
| --- | --- |
| `Signal` | A supported physical signal interface at a terminal |
| `BlockKind` | Physical block identity, independent of ordinary ON/OFF state |
| `BlockPattern` | Exact states at declared offsets in the terminal's local frame; unspecified positions are unconstrained |
| `RepeatedSettling` | A complete named Boolean input/output relation; whenever inputs are held, outputs must eventually remain correct, including after changes during settling |
| `PistonDoor` | One fixed 3×3 aperture, Air when open and Solid when closed; new commands only after certified completion |
| `SingleOperation` | One initially false input may rise once from any completed initial phase; declared completed observations must eventually remain correct; no reset/reuse promise |
| `Periodic` | An autonomous single Boolean output eventually follows a nonconstant recurring waveform |
| `FiniteBurst` | An autonomous single Boolean output falls at least twice and eventually remains OFF |

`required_source_types` attach requirements to a consumer's selected upstream
output. `static_type_bindings` attach snapshot requirements to the realization's
own named terminals and work without a connected consumer. `behavior_bindings`
attach complete observable obligations to a realization. All declared obligations
are checked against actual state and context, not just source defaults.

Behavioral contracts do not implicitly fix endpoint coordinates, delay bounds,
transient waveforms or internal decomposition. Budgets bound computation, not
the circuit's allowed settling time. Periodic and finite-burst contracts currently
have no external control inputs and one output. Restartability and internal-state
behavioral types remain deferred.

The current `dustroute.lever.wall.v2` realization binds lever identity to itself.
Both powered and unpowered actual levers satisfy that type. Its attachment support
belongs to its environment or a shared realization. The unchanged `v1` source
remains available and is not silently strengthened. NOT inputs do not require a
lever producer; the current verifier's lever driver is an execution limitation.

See [component patterns](blueprint-component-patterns.md),
[repeated-settling adoption](repeated-settling-adoption.md),
[periodic behavior](periodic-behavior-status.md), and
[finite-burst behavior](finite-burst-behavior.md).

## Agreed next type: ordinary 3x3 piston door

Following the reference-door comparison, the user accepted ordinary completed
open/close operation and agreement with the actual game. Tolerance of input
changes during movement is optional. The type requirements below are implemented
by `TypeContract::PistonDoor` and its completed-operation verifier.
See [the door type specification and implementation status](piston-door-type.md).

| Requirement | Agreed meaning |
| --- | --- |
| Function | Repeatedly open and close one declared fixed 3×3 aperture |
| Control | One logical open/close command; physical input position and polarity are explicit bindings |
| Open | All nine aperture locations are known Air |
| Closed | All nine aperture locations are Solid; a moving piston or piston head does not satisfy this condition |
| Allowed operation | Start from a declared completed state, hold the command until completion, and change it only after the preceding operation completes |
| Reuse | Repeated allowed operations preserve the actual physical state; no reset or reconstruction between commands |
| Optional property | Recovery under interrupted or rapid input changes is a separate stronger obligation, not part of the ordinary-door requirement |

The aperture binding must establish nine distinct locations forming the declared
3×3 plane in a local placement frame; nine unrelated Boolean observations do not
by themselves establish the opening's shape. Its absolute position and facing,
the quartz material, piston count and internal circuit belong to the realization.
The reference realization binds OFF to open and ON to closed; the general type
does not mandate that every physical circuit use this polarity. Initial state,
supported Laws, known environment and retained parent/child requirements remain
explicit. Type labels or classification names alone are not verification.

Completion must establish that the requested opening condition has been reached
and the next allowed operation can begin; a transient matching snapshot is
insufficient. In this verifier, completion is membership in a fully explored
recurrent region under the held input, with the correct aperture at every state
and every intermediate observation. Every phase of this region permits the next
command; all such commands must reach another correct recurrent region. The
state includes the entire declared physical world and pending work. A continuing
clock is allowed; world quiescence is not required. This conservative predicate
can defer readiness beyond the first correct visible snapshot. Initial settling
must preserve the declared aperture throughout, and commands start at its
certified recurrent region. Resource exhaustion is undetermined.

This is a model completion certificate, not a live readiness sensor. Live block
snapshots alone do not expose the complete event queue. Live readiness detection
and managed operation commands remain separate work. Neither the 20/100-tick
capture intervals nor a verification timeout define the operating protocol.

An input outside this operating assumption removes the ordinary type's guarantee
for that history. It does not make the physical input impossible or authorize
discarding it, freezing the simulator, resetting the world, or claiming automatic
recovery. Continue physical simulation according to the selected Laws. Ordinary
door verification and optional interruption-tolerance verification are distinct.

The new contract preserves the meaning of `RepeatedSettling`. The existing
`reference-door.aperture-repeated-settling.v1` proposal retains its unrestricted
meaning and historical rejection evidence. The reference door explicitly binds
`dustroute.type.piston-door-3x3.v1` in new mechanism/parent/Assembly v3 records,
forked from literal v1. Fresh review/adoption retains every other obligation.
The contract was introduced in catalog v11; current archives require v13.
Existing references and historical evidence are not rewritten. Retired archives
are rejected; see the [cutover guide](architecture-cutover.md).

Normal-cycle behavior and measured live agreement satisfy the user's functional
acceptance criterion for this reference door. Fresh model verification and
isolated public adoption now additionally pass under the ordinary-door type;
they do not certify live sequential construction. Public operation handling, if added,
must explicitly define what happens to a request outside the allowed protocol;
no silent input filter is implied by this agreement.

## Physical laws and execution

Physical laws are immutable, executable Blueprint data. They must affect actual
execution; a data record that only selects a hardcoded device handler is not a
completed law migration. Classification names are not dispatch rules.

Current executable laws cover spatial support/conduction, wire shape and
transfer, dust strength/source combination and the torch's inversion, off-event
history, burnout and delayed callbacks. The retained repeater models and the
compatibility comparator calculation, observer pulses, both retained lamp
models and the bounded piston planner/event path also execute pinned law programs.
`LawProgram` is a
bounded interpreter for registers, inputs, histories and events. It does not
currently read or mutate arbitrary world geometry. Native adapters supply local
world facts to executable rules and apply their effects to electrical state and
physical block movement.

`PhysicalBehaviorContext` explicitly selects one dust law and one torch law; its
execution profile also pins the four spatial laws. It retains a fresh-construction
initial condition and actual input controls. A new law is not selected merely because its source was included in a
geometric Blueprint. General law-to-placement composition remains migration work.

The following constraints apply to that migration:

- Law revisions and execution assumptions remain fixed while comparing candidate
  circuits. Optimizing a realization cannot pass by changing its physics.
- Interpretation overlap must not duplicate a physical block's execution state
  or execute the same physical event twice. Changing only interpretation nesting
  cannot by itself change actual circuit behavior.
- All future-relevant history, pending work and event order must remain explicit.
  World equality at one tick alone is not execution-state equality.
- Synchronous block effects, local event handlers and scheduling are separate
  concerns whose coupling must be explicit. A modeled ordering cannot be relabeled
  as observed Vanilla ordering.
- Observation of a running block snapshot does not establish empty histories or
  queues. Fresh construction and restored execution evidence must stay distinct.
- Unsupported effects, unknown neighborhoods, failed execution and incomplete
  exploration cannot produce a passing behavioral report.

The current block-effects verification profile supports fixed geometry and at
most one torch. Its arbitrary-input adapter uses actual external levers. The
older synchronous profile stays readable with its original semantics; it is not
silently upgraded to the newer profile. Multiple-device coupling and mechanical
geometry changes need separate migration and evidence.

See [physical behavior](physical-behavior.md), [torch laws](torch-laws.md),
[clock conformance](periodic-clock-conformance.md),
[update model](minecraft-update-model.md), and
[conservative history verification](abstract-behavior-verification.md).

## Validation boundaries

Import and structural validation establish record consistency only. They do not
certify placement, connectivity, behavior, live conformance or adoption.

The required validation order is:

1. Resolve exact source, type and law references; check structure and coverage.
2. Construct or inspect the complete proposed actual Assembly.
3. Check placement, physical terminals, routes and declared snapshot obligations.
4. Check every declared behavioral obligation in its explicit execution context.
5. Aggregate all retained occurrence and arrangement results. Preserve failures
   and undetermined results independently of a parent's result.
6. On explicit adoption, recheck dependencies and the complete candidate before
   publishing new immutable source/state records atomically.
7. For live placement or operation, enter the existing planning, preview,
   authorization, fresh-world validation and result-verification path separately.

Persisted reviews are diagnostics, not reusable execution permissions. Geometric
projection, an imported classification and a matching finite trace cannot bypass
any applicable gate. The compiler, optimizer, promotion, update and MCP paths
must converge on the same checks rather than maintain weaker alternate rules.

## Type-directed optimization

The first objective is occupied block count, including supports and wiring.
Shared physical positions count once. In component scope, body cost and complete
Assembly cost are reported separately; excluding equipment from cost does not
exclude it from physics or validation.

Only the selected target type is preserved within the rewritten body. A new
candidate may move its ports and change or discard its previous internal
interpretations. Old records remain unchanged. Any child or additional obligation
retained in the new candidate must pass independently.

Component scope retains declared environmental occurrences and fixed external
blocks. It also retains protected external routes and boundaries, including routes
through shared body blocks. A passing NOT cannot conceal damage to another route.
When a protected endpoint cannot be retained, automatic search reports the need
for explicit parent reconnection instead of erasing or inventing that route.

Current search compares supplied candidates and a bounded family of deletions,
single-conductor relocation and rebound ports. Torch/support enumeration covers
one top and four side placements. These family generators still have native
implementations. Search produces candidates only; it does not publish, adopt,
merge or write Minecraft, and it does not establish global minimality.

See [block reduction](blueprint-block-reduction.md) and
[component patterns](blueprint-component-patterns.md).

## Catalog-driven candidate selection and replacement

The existing Boolean compiler discovers concrete implementations through a
Blueprint-backed `CellLibrary`. Gate/classification bindings choose candidates,
not physical or behavioral proof. `PhysicalCell` and `GateKind` remain compatibility
adapters in the placer/router. Existing component evidence is also still used by
legacy automatic-search policy.

Explicit Blueprint replacement accepts named port mappings, multiple inputs and
outputs, nesting and shared occurrences. The resulting actual Assembly must be
rechecked. A geometry-only legacy adapter rejects obligations it cannot retain.
These adapters are temporary migration boundaries, not alternate definitions of
Blueprint identity or type semantics.

Existing authoring recipes remain available through the typed
`generate_builtin_blueprints` API. The JSON regeneration command has been retired.
Runtime catalogs load fixed Rust definitions without running recipes or a compiler.
Reproduce the independent recipe and geometry checks with
`cargo test --offline --locked -j1 -p dustroute-translate --test blueprints frozen_`.
Committed source revisions cannot be rewritten in place when an authoring recipe changes.
See [component library](component-library.md) and [development](development.md).

## Explicit child-update proposals

A proposal specifies old and new parent/child pins, changed source definitions,
complete candidate state and any required execution context. Review reports
physical changes, interpretation changes, obligations and independent occurrence
results. Failed or undetermined candidates cannot be adopted. Rejecting a proposal
preserves its history and all previous values.

Continuing to use an old child and adopting a new child are both explicit choices.
Automatic discovery of newer children and automatic proposal generation remain
deferred. No saved reference resolves to the latest version implicitly. General
merge, shared-region completion and repair are not part of adoption.

See [Blueprint MCP workflow](blueprint-mcp.md). Existing endpoints expose imports,
queries, capture, proposals, review and decisions; this migration does not imply
new mechanism-specific endpoints.

## Observation, persistence and live use

Observed or modeled actual layouts can be saved as Assembly Revisions without
claiming a Blueprint interpretation. Literal observations remain authoritative;
a modeled projection does not manufacture missing properties or hidden state.
General attribution of observations to library realizations is migration work.
Attribution must distinguish a candidate interpretation from verified obligations.

All current catalogs use v13. Retired v1–v12 archives are rejected rather than
upgraded implicitly; see the [cutover guide](architecture-cutover.md).
Every proposal history uses updates v5; v1–v4 are rejected. Static bindings, law
requirements and explicit observations remain present even when declared only in
unadopted candidates. Reload and adoption preserve source/type/law dependencies
and do not
trust saved success reports.

Adopting a Blueprint or Assembly currently does not construct it in Minecraft.
Those IDs cannot yet be passed as circuit revision IDs to `new_placement`.
Integrating this bridge is in the resumed migration scope, using the existing
live validation and authorization boundaries. The fixed 1×2 piston-door workflow
continues to use its separate narrow proof until that integration is validated.
Migration does not broaden the supported mechanical subset or infer live evidence.

See [circuit revisions](circuit-revisions.md),
[Blueprint MCP workflow](blueprint-mcp.md),
[fixed piston door](piston-door-mcp-v1.md), and
[world validation](world-validation-boundary.md).

## Migration sequence and acceptance criteria

Work proceeds in this order. A phase is complete only when the shared path is used
by its relevant callers and its regression checks pass. Keeping a compatibility
wrapper is allowed; retaining a second authoritative rule implementation is not
completion. A wrapper must delegate or explicitly retain its older versioned scope.

| Phase | Work and source locations | Acceptance boundary | Status |
| --- | --- | --- | --- |
| 1 | Consolidate requirements, current capability, pending work and exclusions in this document | No contradictory scope or historical completion claim; linked evidence remains available | Spec consolidated |
| 2 | Common binding of law Revisions, physical placements and execution state; `behavior_type.rs`, `physical_behavior.rs`, `law.rs`, dust/torch adapters | Existing law/profile pins and outcomes survive migration; nested/shared interpretation does not duplicate execution; saved declarations cannot lose obligations | Verified for the existing dust/torch profiles |
| 3a | Connection shape, support and weak/strong power; `blocks/*`, `wire.rs`, `connectivity.rs`, `electrical.rs` | Rule data actually drives execution and placement/connection checks; finite-domain and retained physical regressions agree | Verified for existing fixed profiles: shared executable spatial laws and current route/placement gates |
| 3b | Remaining supported electrical-device behavior; `sim.rs`, repeater/comparator/observer/lamp models | Existing event/state/evidence boundaries preserved through the common law path; no handler-name-only migration | Repeater and lamp migrated for both retained model scopes; comparator and observer for their compatibility models. External input mutations and source sampling retain their world-adapter role |
| 4a | Supported piston movement and mechanical interfaces; piston planner, deltas and event engine | Preserve the validated horizontal subset, atomic before-state checks, completion isolation, and declared mechanical output requirements | Bounded low-layer path migrated to executable laws, including the approved input-validation repair. The unified Java 1.21.11 electrical callback runtime covers horizontal/up/down motion, settling and interruption. The former directional callback profiles are retired. General mechanical verification remains unsupported |
| 4b | Execution-context generalization | Define and verify device coupling, actual input sources and justified initial state without assuming unknown history or ordering | Verified for the existing model boundaries: shared validated law/execution contracts, retained initialization/input/order policies, and approved bounded dust correction; see [world execution contexts](world-execution-context.md) |
| 5a | Candidate generation, placement and replacement; Blueprint reducer, `CellLibrary`, macro planner and compiler | Shared source/type/law definitions drive candidates; moving ports and shared/external obligations remain valid | Verified for the shared Blueprint path. Typed reduction, layout enumeration, catalog-backed compilation and explicit replacement retain pins and obligations. The older observed-macro workflow remains a versioned compatibility adapter and resolves frozen Blueprint geometry; it is not a publishing or adoption path |
| 5b | Observation attribution and adopted-Assembly placement through existing MCP workflows | Preserve literal observations and evidence scope; live plans retain preview, authorization, fresh checks and recovery contracts | Verified for reflection at the captured dimension and coordinates: only uniquely adopted Assembly ancestry with a complete Circuit Revision grounding is eligible; fresh Blueprint review and the shared live placement/recovery path remain mandatory |

Dependencies may require a small preparatory refactor within a phase. They do not
authorize expanding its physical capabilities or silently changing contracts.
Prove preservation with targeted tests, then run the workspace checks required by
[development](development.md). Retain observed fixtures and their provenance;
recorded model agreement is not a replacement for live conformance evidence.

### Phase 5a closure

The authoritative optimization path accepts an explicit Assembly, target
behavioral type and execution context. Reduction and torch/support enumeration
return standalone immutable candidate data. They may move ports and replace
internal interpretations, but component scope retains external equipment,
routes, boundaries, shared physical positions, children and environmental
requirements. Candidates enter the catalog only through an explicit update
proposal, fresh review and adoption.

`CellLibrary` discovers frozen Blueprint Revisions through immutable
classification bindings. The compiler records the exact selected Revision for
every placed cell, captures the composed Assembly and validates actual
connections and consumer requirements. Offline authoring generators remain
outside the runtime catalog and cannot certify a generated layout.

The older observed-macro API remains a compatibility workflow for its existing
stable-function and Java-version evidence. Candidate geometry and
materialization resolve pinned Blueprint Revisions, and the produced Assembly
is validated against the same catalog. It cannot publish or adopt a candidate,
and its `component_id` is scoped to the originating observation. This retained
scope is not a second definition of Blueprint identity, type verification or
the general reducer.

Focused acceptance checks cover catalog-backed compiler selection, frozen
authoring reproduction, revision-based macro resolution, moving input/output
ports, shared support counting, external equipment and route preservation,
rejection of changed laws and unknown contexts, explicit parent proposals and
fresh review after reload. Phase 5b does not inherit placement authority from
these checks.

### Phase 5b closure

A captured Assembly retains a separate grounding record containing the Circuit
Revision ID, base observation ID, dimension, completeness and literal base
snapshot. This evidence is not part of the Assembly interpretation and is never
inferred for imported records. Adoption remains a catalog decision and does not
write Minecraft.

`new_placement(assembly_revision_id)` accepts only a state published by exactly
one adopted proposal whose immutable ancestry reaches such a complete grounding.
Planning reruns the proposal review, converts the candidate's actual blocks
losslessly, rescans the original bounds plus context and requires exact agreement
with the retained literal base. It then uses the existing placement plan,
authorization, preview, confirmed apply, post-write verification and undo path.
The response names the literal observation and candidate interpretation
separately.

This phase supports reflection only at the captured dimension and coordinates.
It does not add arbitrary origins, relocation, construction from an ungrounded
catalog record, automatic observation attribution, merge/repair, entity state or
broader Minecraft mechanics.

The validated piston scope and unsupported categories are defined in
[piston low-layer validation](piston-low-layer-validation.md). Existing Boolean
behavior types must not be repurposed as unspecified mechanical contracts.

## Law application scope

The user confirmed that each Blueprint declares the physical laws it requires.
Placement, concrete state and interactions become actual only in the physical
world or a world simulator. A declaration is not a second physical execution.

- The world execution context selects immutable law Revisions for its physical
  model. The existing dust/torch context is the first supported selection.
- Each source declares its required law Revisions. Every retained occurrence,
  including nested/shared children, must be compatible with the world's selection.
- Shared physical subjects have one actual state, history and event stream.
  Multiple interpretations do not instantiate extra histories or callbacks.
- Incompatible law declarations are reported as conflicts. No parent, insertion
  order or successful higher-level type check selects a winning law implicitly.
- A Blueprint or a saved review cannot declare itself a running world or infer
  live-world conformance. Missing execution context remains explicit.

The first migration uses exact immutable law dependencies, consistently with the
existing fixed-law candidate comparison. Law compatibility is checked separately
from observed/behavioral correctness. A declaration does not prove that the
world implements arbitrary unsupported effects.

`BlueprintRevision.required_laws` is a set of immutable executable law Revision
IDs. Every declaration must name an existing law, without duplicates. World
selection must also satisfy the selected laws' transitive requirements. Cyclic
requirements are allowed because they do not describe recursive placement or
execution. Catalog presence alone does not mean the world implements a law.

Review records a `physical_law` check for each declaring occurrence: missing
context is undetermined, an incompatible requirement fails, and a selected law
whose adapter is unsupported remains undetermined. Both detailed review and
direct physical model construction enforce the requirements. Adoption rechecks
the complete immutable dependencies. Optimized sources retain the selected law
requirements; moving ports may change input bindings but not the world's laws.

Earlier source Revisions remain unchanged and have no additional declared law
requirements. Their other existing obligations still apply. Current default
electrical and legacy simulator callers compile dust/torch through the same
world-law resolver while preserving their original execution profiles. Spatial
rules now use the pinned programs described below. Repeater behavior uses two
distinct pinned law programs; comparator calculation uses its retained
compatibility law. Observer detection and pulse effects use their compatibility
law, with actual observations and timers retained by the world. Lamp updates
preserve their two distinct model scopes as separate law Revisions.

The phase 4b preflight found a remaining native bounded wire-strength calculation.
It now executes the shared dust-strength law. Following the dependency audit and
user approval, bounded evaluation rejects consumed signal levels above 15 before
the affected event commits; raw records remain unchanged. The compatibility
comparator retains its distinct `u8` contract. The shared
[world execution context](world-execution-context.md) now records and validates
each existing model's actual law, initialization, input and ordering assumptions.

## Executable spatial laws

The existing spatial model is represented by four immutable Blueprint law
Revisions. Their `LawProgram` bodies live in
[`dustroute-minecraft/laws`](../crates/dustroute-minecraft/laws), below the catalog
layer, so placement and the bounded event runner can execute the same data
without a crate dependency cycle. `builtin_laws()` publishes those exact programs
as ordinary serializable Blueprint records. The interpreter does not dispatch
on a Blueprint name or classification.

| Revision ID | Inputs and results |
| --- | --- |
| `dustroute.law.spatial-block-traits.v1` | Block-kind and observed-form tags → support, occupied shape, weak/strong conduction, wire-rise traits, connection orientation and direct/strong emission target patterns |
| `dustroute.law.wire-shape.v1` | Local neighbor facts → no arm, side arm or up arm |
| `dustroute.law.wire-transfer.v1` | Relative elevation, arms, support and profile clearance policy → directed dust transfer |
| `dustroute.law.wire-weak-power.v1` | Relative target position and actual arm → potential weak-power path |

`law::finite::FiniteLaw` checks the exact adapter inputs/outputs, rejects history
and scheduled events, and evaluates every input combination from the program's
initial registers. It caches the results, with an 8192-row compilation limit.
These tables are executable results, not independent hand-written rule tables.
They allocate no per-occurrence history. Changing a program changes its compiled
physical queries; the original program and Revision remain unchanged.

`BlockKind::properties`, `Block::redstone_traits`, support validation and
support-direction checks delegate to these laws. Temporal block profiles retain
only update-model/order metadata in Rust. Wire inference and transfer use the
same programs in translate and the bounded Minecraft runner. The electrical
solver uses the data-defined source target patterns, conduction flags and weak
targets; current connection checks share those weak targets. Source signal
values, scheduled callbacks and the runner's supported-device/error boundaries
retain their existing scope. Device timing and piston motion are not migrated
by these spatial records.

World adapters still supply coordinates, actual state and known/unknown evidence.
Observed-form tags distinguish ordinary blocks, unsupported live mechanisms,
bottom/top/double slabs, bottom/top stairs and glass; the block-kind tags are an
explicit adapter ABI, not logical meaning inferred from labels. These definitions
describe the existing bounded model, not complete Minecraft physics.

The existing v1 execution profiles and current placement profile pin these four
spatial Revisions. `PhysicalBehaviorContext::law_revisions()` reports them along
with the explicitly selected dust/torch laws. Old serialized contexts and source
records are not rewritten: an old catalog may omit the implicit spatial records,
and resolution uses the profile's exact built-ins. A conflicting supplied record
under a pinned ID makes execution unsupported; it is never silently ignored or
executed under that ID. New reduction candidates declare the complete effective
law set. Dependency comparison and fresh adoption checks include these pins.

Arbitrary spatial-law selection is not added to the old profiles. A new law may
be stored and its finite queries evaluated, but a source requiring a different
spatial Revision conflicts with the current world profile. A new execution
context would need to justify using it consistently for execution and validation;
arbitrary replacement remains beyond the completed consolidation of existing
phase 4b profiles. This does not prevent optimization under the fixed laws.

Compatibility checks retain the two historical wire-clearance scopes and the
event runner's conservative treatment of missing observed shapes. The migration
uses a pre-change snapshot of 144 block-kind/observed-form cases, exhaustive
96-case shape, 648-case transfer and 9-case weak-target comparisons, plus the
existing physical and adoption regressions. These are model-preservation checks,
not new live-world evidence.

The workspace regression suite passed, followed by the final Minecraft/translate
model tests, directional-interface and promotion tests, catalog round-trip test,
formatting and warning-free clippy checks. The catalog regression changes a
spatial Blueprint Revision, saves/reloads it, and verifies different support
diagnostics without altering the original Revision or world. The existing
manual full-reachability measurement remains ignored. Reproduce focused checks:

```sh
cargo test -p dustroute-minecraft --test spatial_laws --test wire_rise_validation
cargo test -p dustroute-library --test executable_laws
cargo test -p dustroute-translate --test blueprint_connections --test promotion --test repeated_settling_adoption
cargo test -p dustroute-mcp blueprint_mcp_rechecks_wire_to_block_arms_after_restart
```

## Executable repeater laws

The repeater portion of phase 3b preserves two existing, different model
contracts. Both execute ordinary `LawProgram` data through the common
interpreter. Their programs live in
[`dustroute-minecraft/laws`](../crates/dustroute-minecraft/laws) and
`builtin_laws()` publishes those exact bodies as immutable Blueprint Revisions.
The model IDs intentionally make no claim of complete Vanilla behavior:

- `dustroute.law.repeater.compatibility-boundary.v1` is pinned by
  `RedstoneTickSimulator`.
- `dustroute.law.repeater.bounded-event.v1` is pinned by the bounded
  `PhysicsEngine::run_redstone_propagation` path.

| Concern | Compatibility simulator | Bounded event runner |
| --- | --- | --- |
| Time | Samples at the existing two-game-tick compatibility boundary; prepares state, then commits at `RepeaterUpdate` | Neighbor delivery requests a callback after `2 * delay` game ticks; the existing scheduler delivers it |
| Delay | A queue of 1..=4 sampled Boolean values | Callback payload retains the input value that requested it |
| Short input | A sampled pulse survives the queue | A callback whose expected input no longer matches is a retained no-op |
| Lock | Actual powered, correctly oriented side repeaters/comparators freeze the main queue and output | Outside the supported contract |
| Initial state | Existing powered flag seeds the active queue; missing flag defaults to false and delay defaults/clamps to 1..=4 | Current output is in World; pending work is in the event queue; observed missing/invalid data still fails |
| Pending work | The law reports whether an active queue entry differs from output, including while locked | All delivered callbacks, including stale ones, remain in the event trace |

For example, with delay 2 and an input ON at game tick 0, then OFF at game tick
2, the compatibility simulator emits ON at tick 4 and OFF at tick 6. The bounded
runner retains its tick-4 callback without producing a pulse. This existing
difference is preserved, not resolved by selecting a new common physical rule.
Model comparison is not live conformance evidence.

The compatibility law defines initialization, queue movement, lock hold, output
and pending-work calculation. Its state belongs to each physical position in
the simulator; nested/shared Blueprint interpretations create no extra state.
Unused queue registers stay zero. The adapter supplies actual rear/side input
facts and retains the existing prepare/commit event boundary. The bounded law
defines request eligibility, delay conversion, stale-input rejection and the
delivered output. It is compiled over its finite input domain; World and the
scheduler remain the only owners of output and callback state. Existing
preflight, atomic deltas, error rollback and downstream update order are retained.
The compatibility adapter also retains its existing per-position queues across
observation mutations; this does not define a new device restart/lifecycle model.

No old Blueprint, observation, scheduler profile or physics result is rewritten.
Existing dust/torch behavioral proof contexts do not select either repeater
law: a source requiring one still fails that context's law-selection gate,
including adoption. Publishing an executable law does not add device coupling,
new proof profiles, arbitrary program selection in a running world, or live
placement support. Those remain separate migration work.

Verification retains [32 pre-migration model captures](../crates/dustroute-translate/tests/fixtures/repeater_models_v1.jsonl)
with [capture metadata](../crates/dustroute-translate/tests/fixtures/repeater_models_v1.meta.json).
They cover every delay, both initial output values, held inputs, pulses,
repeated reversals and compatibility locking/release, including event coordinates,
no-op status, output propagation and pending work. A separate exhaustive local
check explores every reachable Boolean queue and both input/lock values against
the old queue transition. Changed-program checks, archive round trips and failed
adoption checks verify executable data and immutable scope boundaries.

```bash
cargo run -p dustroute-translate --example audit_repeater_world_models
cargo test -p dustroute-minecraft --test repeater_laws
cargo test -p dustroute-translate --test repeater_laws --test repeated_settling_adoption
cargo test -p dustroute-library --test executable_laws
```

Comparator and observer migrations are described below.

## Executable comparator law

`dustroute.law.comparator.compatibility-boundary.v1` preserves the existing
`RedstoneTickSimulator` model. Its
[`LawProgram` body](../crates/dustroute-minecraft/src/law/builtins/comparator_law.rs)
is published unchanged by `builtin_laws()` as an immutable Blueprint Revision.
The native adapter reads actual rear and side coordinates; the program computes
the maximum of both side levels, then the compare or subtract result. No
classification name selects a hidden comparator formula.

| Concern | Retained behavior |
| --- | --- |
| Compare | Emit the rear level if it is at least both side levels; otherwise emit zero. Equality passes. |
| Subtract | Emit the rear level minus the larger side level, saturating at zero. |
| Mode | An observed `mode` exactly equal to `subtract` selects subtraction. The existing fallback for missing/other values remains comparison. |
| Sampling | Read all comparator inputs from the same electrical snapshot at the existing two-game-tick compatibility boundary. |
| Commit | Apply prepared outputs at `ComparatorUpdate` in that boundary. Changes to inputs or mode after preparation affect the next boundary, not the prepared result. |
| Series connection | The downstream comparator samples the pre-boundary upstream state; signal resolution follows device commits. No extra or removed queue stage is introduced. |
| Initialization | Preserve explicit `power_level`; otherwise use 15 for a powered comparator, or zero. This remains the compatibility model's seed, not recovered live history. |
| State ownership | The simulator retains its existing one-slot queues, per-position output state, pending boundary, event ordering and observation-mutation lifecycle. Law evaluation retains no device state. |

The law's adapter retains the legacy `u8` input/output domain. The existing raw
simulator can carry synthetic comparator levels above 15, so migration adds no
clamping or new rejection policy. The physical signal-domain regression covers
0..=15; additional raw-value cases preserve compatibility without certifying
those values as valid Minecraft observations. Actual signal sources and spatial
power remain responsibilities of the unchanged world adapter and electrical
solver.

This is the compatibility model's existing calculation, not a new full Vanilla
comparator implementation. Container/inventory analog reads, additional input
eligibility rules, asynchronous callback semantics and general device coupling
are not added. The bounded event runner still reads declared comparator source
levels without implementing comparator calculation or timing. Its scope remains
unchanged. Existing dust/torch behavioral proof contexts do not select this law;
declaring it still fails their selection gate and blocks adoption. Runtime
selection of arbitrary comparator revisions remains execution-context work.

Evidence includes [48 pre-migration captures](../crates/dustroute-translate/tests/fixtures/comparator_model_v1.jsonl)
and [capture metadata](../crates/dustroute-translate/tests/fixtures/comparator_model_v1.meta.json):
compare/subtract/missing/other modes, three initial levels and four horizontal
rotations. Each case exercises ten input samples through two comparators in
series, retaining event times, no-ops, before/after outputs, propagation and
pending-work results. Independent checks cover every one of the 8,192 physical
input combinations (`16 rear * 16 side A * 16 side B * 2 modes`), representative
raw `u8` values, between-event input/mode changes, initial-state fallback and
observation removal/reinsertion. Archive round trips and altered-program checks
verify immutable identity and executable data. Existing recorded comparator
mode fixtures remain regression evidence; no new live recording was made.

```bash
cargo run -p dustroute-translate --example audit_comparator_world_model
cargo test -p dustroute-minecraft --test comparator_laws
cargo test -p dustroute-library --test executable_laws
cargo test -p dustroute-translate --test comparator_laws --test repeated_settling_adoption --test observation_fixtures --test scheduler_fixtures
```

## Executable observer law

`dustroute.law.observer.compatibility-boundary.v1` preserves the existing
`RedstoneTickSimulator` observation and pulse model. Its
[`LawProgram` body](../crates/dustroute-minecraft/src/law/builtins/observer_law.rs)
is published unchanged as an immutable Blueprint Revision. Native adapters
supply differences between actual observations, block presence and timer facts;
the program decides notification, pulse output and deadline effects.

| Concern | Retained behavior |
| --- | --- |
| Observation | Observe the neighboring cell opposite the output direction, including all six directions. Compare the exact block record, signal level, weak/strong power and repeater/torch/comparator/observer/lamp output state. |
| Detection | At least one of these nine fields must differ from a known previous observation. A newly inserted observer has no previous observation and causes no notification by itself. |
| Pending work | Multiple changes before preparation coalesce into one pending notification per physical position. Changes after preparation remain separate pending work. |
| Start | If the observer is still present, set output on and replace its deadline with one compatibility boundary after the current boundary. Each boundary advances two game ticks. |
| End | Clear output and deadline only if the observer is still present and its current deadline is due. |
| Ordering | Retain the scheduler's end-before-start order within a boundary, including both transitions when a pulse ends and another starts together. |
| Initialization | Preserve the observed powered flag without inventing a missing expiry or previous observation. This remains an existing model seed, not reconstructed live history. |
| State ownership | Observations, pending notifications, deadlines, output state and event ordering remain in the world simulator, once per physical position. Law evaluation retains no separate device history. |

The law does not expand the model's observation domain or interpret raw metadata
as a new Minecraft feature. Removal and reinsertion retain their existing state
lifecycle; prepared callbacks recheck actual presence and deadline facts. The
bounded event runner still reads known observer output levels without simulating
observer pulses. Current dust/torch proof contexts do not select this law, so a
source requiring it cannot be adopted under those contexts. Phase 4b records the
compatibility simulator's full law selection separately; arbitrary replacement
and new device coupling remain unsupported.

Evidence includes [48 pre-migration captures](../crates/dustroute-translate/tests/fixtures/observer_model_v1.jsonl)
and [capture metadata](../crates/dustroute-translate/tests/fixtures/observer_model_v1.meta.json):
six directions, two initial output states and four change patterns over six
boundaries. They retain event times, no-ops, output transitions and pending work.
Additional checks cover all 1,024 combinations of baseline presence and field
differences, each native observation field, between-event mutation, removal,
immutable archives and altered executable rules. Existing recorded observer
conformance fixtures also remain regression evidence. These model captures are
not new live Minecraft observations.

```bash
cargo run -p dustroute-translate --example audit_observer_world_model
cargo test -p dustroute-minecraft --test observer_laws
cargo test -p dustroute-library --test executable_laws
cargo test -p dustroute-translate --test observer_laws --test repeated_settling_adoption --test transition_conformance --test scheduler_fixtures
```

## Executable lamp laws

The remaining lamp paths in phase 3b execute two immutable programs, published
unchanged by `builtin_laws()`:

- `dustroute.law.lamp.compatibility-boundary.v1`:
  [compatibility program](../crates/dustroute-minecraft/src/law/builtins/lamp_law.rs).
- `dustroute.law.lamp.bounded-event.v1`:
  [bounded-event program](../crates/dustroute-minecraft/src/law/builtins/lamp_law.rs).

| Concern | Compatibility simulator | Bounded event runner |
| --- | --- | --- |
| Construction | Derive the initial lit cache from resolved power, retaining the existing initialization policy regardless of the stored lamp flag | Keep the supplied block state until an event reevaluates the lamp |
| ON | At `LampUpdate`, set lit and clear any off deadline | Set lit during the delivered neighbor update |
| OFF | If currently lit with no deadline, set one two compatibility boundaries later; keep an existing deadline and turn off when it is due | Update immediately without a delayed off callback |
| Sampling | Read current resolved power at `LampUpdate`, including changes after boundary preparation | Read actual neighboring signals when evaluating the event |
| State | Per-position lit cache and deadlines belong to the world, including their existing observation-removal/reinsertion lifecycle | Actual block state belongs to World; delta application checks exact before states |

Each compatibility boundary spans two game ticks. Repeated unpowered updates do
not postpone an existing deadline; repowering clears it. The data preserves
saturating-clock behavior too: at the final representable tick, a new off
deadline is immediately due. The native adapter supplies this clock fact and
applies the rule's output/deadline effects without a second lamp formula.

The bounded runner retains its unknown-observation rejection and updates the raw
`lit` property together with the modeled powered field. Atomicity remains per
event delta: an unknown lamp can reject its own event after earlier input and
wire events have committed. Migration neither rolls back that earlier work nor
certifies an incomplete run. Source eligibility and actual signal sampling
remain in the existing spatial/world adapters.

[24 pre-migration captures](../crates/dustroute-translate/tests/fixtures/lamp_models_v1.jsonl)
with [metadata](../crates/dustroute-translate/tests/fixtures/lamp_models_v1.meta.json)
cover two initial source states, three lamp flags and four edge patterns over
seven boundaries. Local rule checks cover absent/present state and timers,
repowering, deadline expiry and clock saturation; integration checks preserve
between-event changes, observation lifecycle and unknown-state rejection.
Altered-program and archive tests demonstrate that executable data controls
effects without rebinding old Revisions. Current dust/torch proof contexts do
not select either lamp law and still reject such requirements at adoption.
No new live conformance evidence or device coupling is claimed.

The phase 3b inventory also checked lever/button/pressure-plate mutations and
source-level sampling. These are explicit world inputs and adapter facts in the
existing models; they do not implement autonomous button release, entity-driven
pressure changes or additional device callbacks. Those unsupported mechanisms
are not introduced by this migration.

```bash
cargo run -p dustroute-translate --example audit_lamp_world_models
cargo test -p dustroute-minecraft --test lamp_laws
cargo test -p dustroute-library --test executable_laws
cargo test -p dustroute-translate --test lamp_laws --test repeated_settling_adoption --test transition_conformance
```

## Executable piston laws

The approved low-layer part of phase 4a executes four immutable programs,
published unchanged by `builtin_laws()`:

| Revision ID | Executable decisions and effects |
| --- | --- |
| `dustroute.law.piston.state.bounded-v1` | Input-driven action requests, valid stable states, moving/stable state transitions and sticky pulling |
| `dustroute.law.piston.motion.bounded-v1` | Push admission and chain restrictions, head/payload offsets, moving-carrier effects, dirty radius and default motion timing |
| `dustroute.law.piston.connection.bounded-v1` | Eligible input kinds, wire arms, directional outputs and required observed shape/facing |
| `dustroute.law.piston.payload.bounded-v1` | Movable block/state requirements, literal immovable-name predicates and rejection precedence |

The [program bodies](../crates/dustroute-minecraft/laws) use the existing
`LawProgram` instructions. The [adapter](../crates/dustroute-minecraft/src/piston_law.rs)
validates exact input/effect interfaces; finite rules are compiled over their
bounded domains. Payload name predicates are declared in the program, including
their literal names. Native code extracts actual block facts and retains the
existing single `minecraft:` prefix normalization. These programs allocate no
per-interpretation physical state or history.

The world continues to own actual blocks, moving carriers, reservations and the
event queue. Native code scans known cells, constructs exact block metadata and
atomic deltas from law effects, and records movement and start/completion events.
It retains unknown-space rejection, before-state checks, local completion
dependencies, overlap handling, and the body-only fallback for old serialized
plans. Distant independent changes do not invalidate a pending completion;
changed local carriers or payload dependencies do. No source or child reference
is rewritten by a physical movement.

The pinned motion law supplies the existing initial-delay range of 0–1 game
ticks and movement duration of 2 game ticks. Explicit diagnostic
`PistonMotionProfile` overrides retain their existing validation and `u64` range.
`PISTON_PUSH_LIMIT` and `DEFAULT_PISTON_MOTION_PROFILE` remain historical public
compatibility constants; runtime planning and default construction read the law.
The retained subset is horizontal motion, at most 12 ordinary solid/transparent
payload blocks, or one isolated explicitly retracted normal piston satisfying
the existing metadata gates. Vertical movement, mixed piston chains and other
unsupported mechanisms are not enabled.

The input audit exposed an early-return defect, repaired with user approval
before recording the migration baseline: every horizontal side must pass
validation before the combined powered value is returned. The historical defect
and current rejection behavior are documented [below](#piston-migration-preflight-incomplete-input-validation).

[40 frozen captures](../crates/dustroute-translate/tests/fixtures/piston_model_v1.jsonl)
with [metadata](../crates/dustroute-translate/tests/fixtures/piston_model_v1.meta.json)
were recorded after that repair and before the law migration. They cover four
facings, two piston variants and five payloads through ON/OFF cycles, comparing
direct plans, exact transient changes, movement relations, event times/status,
final blocks and queue/trace completion. Rule tests also cover unsupported
payloads, observed connection facts, altered programs and immutable archive
round trips. Existing completion-isolation tests, recorded block observations,
transition-conformance diagnostics and the fixed 1×2 workflow remain regression
checks. These are model regressions and reuse of retained observations, not new
live conformance evidence.

Current dust/torch proof contexts do not select these piston laws and reject
such requirements at adoption. Phase 4b exposes the bounded world's full law
selection without extending these proof profiles. Arbitrary law replacement and
new device coupling remain unsupported. General mechanical types and tracking the terminals/types
of moving child Blueprints were deferred by that migration. The subsequent
[semantic agreement](#agreed-movement-semantics) fixes terminals to locations and
interprets movement as a whole-realization transition. The approved bounded path
now has [native exploration, review and explicit adoption](runtime-location-review.md);
general mechanical verification remains outside that scope.

```bash
cargo run -p dustroute-translate --example audit_piston_world_model
cargo test -p dustroute-minecraft --test piston_laws
cargo test -p dustroute-library --test executable_laws
cargo test -p dustroute-translate --test piston_laws --test piston_input_validation --test piston_completion --test piston_payload_push --test transition_conformance --test repeated_settling_adoption
cargo test -p dustroute-mcp piston_door
```

## Spatial-law migration: explicit-rise consistency

The phase 3a audit reproduced a disagreement in the existing kernels, before
changing either spatial implementation. A lower wire has an explicit east `Up`
arm and an upper wire sits one block east and one block higher. Add a solid block
directly above the lower wire while retaining that explicit arm. The shared
historical placement gate accepts this snapshot, and both executions terminate normally:

| Geometry | Historical solver: lower / upper level | Historical event runner: lower / upper level | Current initial-placement gate |
| --- | --- | --- | --- |
| Open above the lower wire | 15 / 14 | 15 / 14 | Pass |
| Solid obstruction above it, explicit `Up` retained | 15 / 14 | 15 / 0 | Fail |

Reproduce with:

```sh
cargo run -p dustroute-translate --example audit_dust_world_models
```

The translate `dust_transfer` rise path accepts the stored arm and support trait.
The Minecraft-layer `vertical_wire_transfer` path additionally checks the
obstruction. The data migration retains this difference as an explicit adapter
clearance-policy input to the shared transfer law.
This diagnostic compares local models; it is not live Minecraft evidence and
does not establish that the obstructed snapshot can persist in Minecraft.

The user selected rejection in the new validator. The shared initial-placement
profile is now `dustroute.initial-placement.explicit-wire-rise.v2`. An explicit
`Up` arm needs the matching adjacent support, an upper wire and unobstructed
clearance. A `Side` arm that known geometry resolves as a top-half rise also
requires clearance. Horizontal decorative endpoints are permitted; absent shapes
are not invented or normalized by this check. This is a bounded rise-consistency
rule, not complete validation of every Vanilla wire shape.

The same check is used by complete-world validation and partial Assembly review.
Known contradictions fail; missing neighbors needed to establish an explicit
rise remain undetermined. Known contradictions are not hidden by other unknown
neighbors. Every overlapping occurrence and the complete arrangement retain the
diagnostic. Validation, explicit adoption and new placement reject the candidate,
including after reload. Literal observations and immutable source/state records
can still be stored for diagnosis without certifying them.

The two existing v1 physical-behavior profiles retain their historical placement
assumptions through `HistoricalPlacementV1`. That proof has a distinct Rust type
and cannot satisfy current `ValidatedWorld` consumers. Physical behavior reports
state their historical placement profile; MCP reviews state the current profile.
An old model result cannot override current placement/adoption failure. The
spatial-law adapters preserve the old results reproduced by this audit.

Macro materialization retains explicitly supplied known regions through
`materialize_macro_replacement_in_known_regions`. Existing MCP callers pass the
actual scanned cuboid, including its air cells. The compatibility facade that
receives only a raw World cannot reconstruct that coverage beyond its occupied
bounds and may now return an unknown-neighborhood diagnostic. It must not enlarge
the region to make a replacement pass. The Assembly entry point continues to
preserve its supplied coverage and all parent/shared obligations.

This prerequisite resolved the explicit-rise policy stop and kept the earlier
world-owned execution decision. The executable spatial-law migration above
supplies the remaining phase 3a data path; the validator alone did not complete
that migration or reopen deferred work.

## Spatial-law migration: wire-to-block route discrepancy

The next phase 3a audit found a separate implementation inconsistency, not a new
behavioral-type requirement. A powered wire runs north/south, with explicit
`None` arms east/west. A solid block sits immediately east of the wire. A declared
two-position route connects the wire output to that block's `BlockPower` input,
approaching from the requested west face. A north-adjacent redstone block drives
the wire to level 15. The whole fixture has explicit, complete synthetic coverage.

Run the same diagnostic against the current production rules:

```sh
cargo run -p dustroute-translate --example audit_wire_block_routes
```

Before the correction, all four horizontal rotations produced this comparison:

| Arm toward the receiver | Assembly validation | Promotion review | Wire level | Receiver weak / strong power |
| --- | --- | --- | --- | --- |
| Explicit `None` | Pass | Passed | 15 | 0 / 0 |
| Explicit `Side` | Pass | Passed | 15 | 15 / 0 |

`connectivity::physical_step` accepted `DustToBlock` for horizontal adjacency
without checking the source arm. Both `check_port_connection` and Assembly
review use that step. In contrast, `electrical::dust_weak_power_targets` includes
a horizontal block only when `wire_has_arm` is true. The current placement gate
accepts both snapshots: this is a disconnected proposed route, not the earlier
obstructed-rise geometry. No behavioral requirement is declared in this fixture;
the defect concerns the local connection claim itself. Neither an unpowered
source nor a missing execution context explains the discrepancy.

The user approved rejecting routes without a physical transfer path. Current
`physical_step` now delegates its `DustToBlock` predicate to the solver's
`dust_weak_power_targets`. Assembly validation, promotion/adoption and derived
connectivity graphs therefore require the actual horizontal arm. The below-wire
target remains connected. Current ON/OFF state does not determine connectivity.
The first row now fails validation and review; the second row still passes.

Raw snapshots and immutable Revisions are unchanged. A derived graph is rebuilt
from actual state; its former adjacency-only edges are not retained as current
connection evidence. No historical electrical execution profile was changed.
Regression tests cover all four rotations, powered and unpowered sources,
archive reload, atomic promotion failure and MCP adoption after service restart.
The MCP regression also checks successful adoption of the connected control case
and retention of the rejected candidate and previous state.

This resolves the route-policy stop. The diagnostic and regressions are local
model evidence, not live Minecraft conformance or a complete inventory of model
disagreements. They are retained with the completed phase 3a data migration.

## Piston migration preflight: moving interpretation bindings

The original preflight left terminal anchoring unresolved while the bounded
piston laws and phase 4b were migrated. That semantic question is now resolved
by the user's clarification below. This agreement does not claim that the
existing behavioral proof models support movement.

### Agreed movement semantics

A terminal identifies a location in a concrete placement, not the identity of a
block passing through it. Block presence, known absence and actual block state
can be terminal values. For a door-like realization A, the same output location
can contain a block in one state and air in another. A remains the same complete
realization while its constituent blocks move. A's meaning does not require all
its blocks to translate together, nor does the output terminal chase its payload.
Classification names do not select the movement or its observable requirements.

Local execution, change-impact calculation and optimization may process only part
of A as a computational technique. Their results must remain valid in the complete
physical context under the declared requirements. A local result cannot replace
whole-realization evidence without a justified equivalence or dependency argument.
Cost boundaries do not remove surrounding blocks from physics. Existing search
already checks complete candidates and their retained environmental obligations;
this agreement does not authorize a weaker local-only acceptance path.

Physical movement changes actual world state, not an immutable Blueprint or
Assembly Revision. Runtime state and any newly captured Assembly values remain
separate from the pinned source. Movement does not automatically replace child
references, discard interpretations, or create an update proposal. A passing
parent still cannot hide a failed or undetermined retained child. Explicit
optimization candidates may change internal decomposition under the previously
agreed target-type and retained-environment rules; this is distinct from merely
advancing the physical world.

Terminals are fixed during the execution of a concrete candidate. Optimization
may choose different positions in a different candidate and verify its complete
bindings afresh, consistently with the existing movable-port search contract.

During movement, retain the actual intermediate block state and available piston
block-entity metadata. Evaluate supported requirements; preserve known failures
and mark unsupported evaluations undetermined. Revalidate at completion without
using the final state to certify the intermediate trajectory. Do not map missing
evidence to air or a stable block, assume a successful endpoint proves every
input history, or undo a physical move solely because an interpretation fails.
Existing low-layer validation and execution-error handling remain mandatory.

### Implementation gaps and consistency checks

| Boundary | Current behavior | Required treatment |
| --- | --- | --- |
| Block-state terminals | Explicit observations and native repeated-settling review/adoption are implemented through the existing MCP tools; legacy signal bindings still reject BlockState | Use an explicit native context and fresh checks; catalog membership or samples cannot certify it |
| Snapshot conditions | `BlockPattern` checks exact states in the terminal frame; explicit source air fails a snapshot review if occupied | Preserve these meanings. A condition checked at one state is not by itself a requirement describing presence/absence switching over execution |
| World execution | The callback profile and its separate native exploration retain complete movement state, carrier histories and intermediate observations; old dust/torch profiles retain fixed geometry | Preserve unknowns for unsupported routes/bindings; native adoption uses its own fresh gate and does not manufacture old fixed-placement proofs |
| Local processing | Affected-occurrence indexes aid invalidation; complete candidate review remains authoritative | Preserve physical dependencies, shared state/history and retained child/environment requirements when computing locally |

For example, a child explicitly requiring air at location p fails a snapshot
check when a piston puts a block at p, even if its parent achieves a desired
closed state. That failure must not be hidden by the parent, reclassified as
unknown, or fixed by silently treating the old air requirement as initial-only.
A whole-realization switching requirement needs its own explicit observation
and behavioral binding. The subsequent [location-state binding implementation](location-state-bindings.md)
adds explicit predicates and a new archive form, without changing the behavior
type or inventing conditional exceptions to retained obligations.

No contradiction between the agreed requirements was found in this review.
The unsupported paths above are implementation gaps, not permission to relax
existing checks. Stop and report a concrete case if the whole-realization
semantics would require weakening a retained obligation, changing an old type's
meaning, guessing physical behavior or introducing an undeclared scope change.
Read-only investigation and version-specific source/observation comparison may
resolve implementation questions. General mechanical verification remains
unimplemented; automatic terminal tracking is not part of the agreed target.

## Movement verification preflight: input changes during motion

The location-terminal goal was created after the semantic agreement above.
The first execution preflight paused implementation: the existing
`RepeatedSettling` contract admits input changes during settling, while the
retained bounded piston model explicitly excludes interruption/reversal during
motion from its validated scope. Agreeing to retain intermediate block state
does not establish how those input histories execute. The user subsequently
approved extending the physical model while preserving `RepeatedSettling`.
The follow-up [target-version source audit](piston-motion-source-audit.md) found
a separate execution-boundary prerequisite. The user approved that prerequisite
as well; its [runtime foundation](synchronous-world-runtime.md) is implemented.
The original scope stops are resolved. The separately versioned
[horizontal piston callback adapter](piston-callback-runtime.md) now implements
the motion-time branch and carrier rules, retaining the old electrical/payload
subset. The [native location behavior review](runtime-location-review.md) now
checks supported relations over input histories and preserves independent child
requirements. Common MCP/proposal/adoption integration is implemented with
proposal-history v5 for native contexts, fresh verification after restart and
no automatic source or terminal changes.

The diagnostic uses one sticky piston with one solid payload, through both the
direct-input and propagation runners, and the existing single-input two-row
fixture through propagation. Each output is a fixed location that is initially
air. All three layouts give the following deterministic model result:

| Input schedule | Output trajectory | Final input | Pending work | Trace status |
| --- | --- | --- | --- | --- |
| ON at tick 1, OFF at tick 3 during motion | Air → MovingPiston at tick 2 → Solid at tick 4; no later transition | OFF | 0 | Complete |
| ON at tick 1, OFF at tick 8 after completion | Air → MovingPiston → Solid → MovingPiston at tick 9 → Air at tick 11 | OFF | 0 | Complete |

The two-row fixture gives the same result at both output locations. Its ordinary
settled open/close sequence is preserved. These are local model results, not new
live observations or proof that Vanilla produces the same motion-time outcome.

The current immutable state law requests no action in Extending/Retracting states.
The event engine's `PistonComplete` applies the completion delta without queuing
another input/neighbor evaluation. Consequently, the observed OFF input has no
remaining modeled work that would restore the empty output. A complete event
trace means execution drained its queue; it does not prove the requested
input/output relation. Under this model the recorded input prefix is a
counterexample to an eventual OFF → empty requirement.

```bash
DUSTROUTE_DIAGNOSTIC_OUTPUT=/tmp/audit_piston_input_during_motion.json cargo test --offline --locked -j1 -p dustroute-translate --test audit_piston_input_during_motion -- --ignored --exact retain_fixture --test-threads=1
```

The [six diagnostic rows](../crates/dustroute-translate/tests/fixtures/piston_input_during_motion_preflight.jsonl)
and [metadata](../crates/dustroute-translate/tests/fixtures/piston_input_during_motion_preflight.meta.json)
retain input timing, the selected execution context, location changes and relevant
events. They capture the gap; they are not a new compatibility guarantee or a
fixture to make a future repair reproduce the same terminal result.

This is an execution/evidence prerequisite, not a contradiction in the agreed
location-terminal or whole-realization semantics. The approved route extends
the model to the input histories already required by `RepeatedSettling`; it does
not introduce a settled-boundary-only input contract or guarantee that the
current realization passes. Changed behavior needs explicit revision/profile
treatment and must not rewrite old law pins.

The 1.21.11 source audit confirmed motion-time dropping, event-time power
rechecks, a moving carrier at the retracting body position, and synchronous
notifications whose result depends on enclosing tick context and per-carrier
history. The [four additional diagnostics and recommended prerequisite](piston-motion-source-audit.md)
show why the current global phase queue and body-wide completion plan cannot
simply acquire a final input resample. Production implementation initially
stopped under the instruction to report newly required prerequisite work. The
subsequently approved synchronous runtime adds explicit calls/continuations,
tick context, motion histories and opaque checkpoints through a separate delivery
profile. Old handlers, laws and original captures remain unchanged. The runtime
foundation alone supplies no physical-law or behavior certificate. The separate
[native piston adapter](piston-callback-runtime.md) and
[location review/adoption path](runtime-location-review.md) now supply the approved
bounded implementation and its independent retained-obligation checks.

## Piston migration preflight: incomplete input validation

Before production piston changes, the low-layer audit reproduced an existing
disagreement between the validation contract and its implementation.
`piston_input_powered_in_region` promises to inspect all horizontal sides, and
the event runner preflights input changes against that query. The old query
instead returned immediately on the first powered input in North/East/South/West
order. Unknown sides appearing later in the order could escape validation.

The diagnostic uses an east-facing normal piston at `(0,1,0)`, one ordinary
payload at `(1,1,0)`, and an adjacent input. Its historical results were:

| Observation boundary | ON request | Following OFF request |
| --- | --- | --- |
| North input known; south side outside the observed region | Accepted, payload pushed, trace marked complete | Rejected for unknown south side; the input remains ON and the piston remains extended |
| South input known; north side outside the observed region | Rejected before mutation | Not run |
| All horizontal sides and movement cells known | Accepted | Accepted |

Run the same diagnostic against the current implementation with:

```bash
DUSTROUTE_DIAGNOSTIC_OUTPUT=/tmp/audit_piston_input_boundary.json cargo test --offline --locked -j1 -p dustroute-translate --test audit_piston_input_boundary -- --ignored --exact retain_fixture --test-threads=1
```

[Captured results](../crates/dustroute-translate/tests/fixtures/piston_input_boundary_pre_migration.jsonl)
and [metadata](../crates/dustroute-translate/tests/fixtures/piston_input_boundary_pre_migration.meta.json)
retain the original behavior. They are local diagnostic evidence, not new live
observations or a desired acceptance fixture. This is separate from the moving
Blueprint binding question; that unsupported feature is not used in the case.

The user approved repairing validation before migration. The current query
checks every applicable horizontal input fact before returning the combined
power value. Both incomplete-region cases now reject ON before mutation; the
complete-region case still accepts ON and OFF. The historical capture above is
retained unchanged, not regenerated as a passing expectation.

Regression tests exercise every missing-side/source-side pairing for ON and OFF
(24 cases), plus later unknown lever power, wire shape and repeater facing.
Rejection leaves the world unchanged and the input event queued, with a failed
trace and no applied transitions. This is per-event atomicity, not rollback of
an entire earlier run. No failure of the fully guarded 1×2 live contract was
established by this diagnostic. The corrected path is the baseline for the
completed low-layer law migration.

## Phase 4b preflight: bounded dust-strength migration gap

The approved next goal unifies the selected law Revisions and execution
assumptions of existing fixed-geometry models while preserving their distinct
behavior. Moving Blueprint terminal/type bindings remain a subsequent phase,
before candidate/placement migration. This goal does not authorize inventing
device coupling. The later explicit approval of the bounded signal-domain
correction is recorded below.

This section preserves the pre-change findings. The preflight found a prerequisite left by the earlier migration:
`minecraft::blocks::redstone::redstone_wire_delta` still computes wire strength
with native `max` and `saturating_sub(1)`. The spatial rules used by that function
are executable laws, but the strength calculation is not. Meanwhile, the
translate electrical/proof paths execute `dustroute.law.dust-strength.v1` via
`DustLaw`. Catalog presence cannot justify saying both paths select and execute
that law.

There is also an input-domain difference. The retained `DustLaw` adapter clamps
its inputs to 0–15. The bounded diagnostic path reads `u8` levels and retains
them through direct-source combination and wire attenuation. Its placement
validator rejects typed levels above 15, but `new_diagnostic` bypasses placement
validation by design. Switching that path to `DustLaw` without an explicit
compatibility treatment would change existing diagnostic execution.

The read-only audit supplies a directional comparator output followed by two
horizontal wires. It exercises source sampling only, not comparator timing:

| Supplied source | Bounded first / second wire | DustLaw direct / neighbor evaluation | Placement rejects source level |
| --- | --- | --- | --- |
| 0 | 0 / 0 | 0 / 0 | No |
| 1 | 1 / 0 | 1 / 0 | No |
| 14 | 14 / 13 | 14 / 13 | No |
| 15 | 15 / 14 | 15 / 14 | No |
| 16 | 16 / 15 | 15 / 14 | Yes |
| 255 | 255 / 254 | 15 / 14 | Yes |

All six diagnostic runs drained their queues. These are local model results,
not evidence that out-of-range values are legal Minecraft signals or that
placement validation failed. The two law columns are direct adapter evaluations,
not a second whole-world simulation.

```bash
DUSTROUTE_DIAGNOSTIC_OUTPUT=/tmp/audit_bounded_dust_strength.json cargo test --offline --locked -j1 -p dustroute-translate --test audit_bounded_dust_strength -- --ignored --exact retain_fixture --test-threads=1
```

[Captured rows](../crates/dustroute-translate/tests/fixtures/bounded_dust_strength_pre_migration.jsonl)
and [metadata](../crates/dustroute-translate/tests/fixtures/bounded_dust_strength_pre_migration.meta.json)
retain the pre-change result. The example now runs the current evaluator and
rejects 16 and 255, retaining the queued event and raw state on failure.

Implementation initially stopped under the user's prerequisite-work condition.
After the retention audit below, the user approved rejecting invalid bounded
signals rather than adding a compatibility law for the native calculation.
The correction and common execution contracts are now implemented; see
[world execution contexts](world-execution-context.md). The historical capture
has not been regenerated or presented as current successful behavior.

### Review of legacy-retention needs

The 2026-09-21 audit found no repository use or pre-existing regression that
requires the bounded wire path to propagate levels above 15. The first propagation
implementation (`b820b2d`) already passed raw `u8` values through its native
calculation, but its API documentation does not promise a 0–255 signal domain.
`new_diagnostic` permits raw worlds for diagnosis; it does not promise successful
evaluation of every invalid block state. Its existing evaluators already reject
missing observations and invalid repeater delays.

Evidence and limits:

- A scan of 130 JSON/JSONL files under `crates`, `docs` and `tools` found no numeric
  `power_level` or `power` field outside 0–15 (116 numeric fields). This checks
  repository records, not unknown external consumers or generated Rust fixtures.
  Rust assignments, model harnesses, callers and relevant tests were inspected
  separately. The new six-row audit describes the discrepancy; it is not a
  pre-existing use case that requires preserving it.
- The ordinary initial-placement validator rejects typed power above 15. Existing
  proof profiles also pass a placement gate. Their guarantees do not require
  making these invalid diagnostic signals executable.
- `ExecutionCheckpoint` is an opaque in-memory value, with no persistent wire
  format or cross-version restoration contract. Retained observation/Assembly
  data can stay readable without promising to evaluate invalid power as a signal.
- The separate compatibility comparator law **does** explicitly preserve raw
  `u8` calculation. Its immutable program and the tests
  `raw_u8_compatibility_values_are_preserved_without_clamping_or_history` and
  `initial_output_and_raw_u8_levels_keep_the_existing_compatibility_policy`
  cover values above 15. They exercise the comparator law and the compatibility
  simulator, not the bounded wire engine. This audit does not authorize changing
  that law or globally narrowing the raw block representation.

The initial preservation proposal was precautionary, not evidence of a required
bounded-wire feature. The revised recommendation is to retain raw observations
and immutable Revisions, accept only 0–15 at the bounded signal-evaluation
boundary, and report out-of-range values before applying the affected event.
Valid levels can then use the shared executable dust-strength law. Keep the
existing placement gate and recorded historical diagnostic output; do not add
an extra compatibility law solely to perpetuate an unused native calculation.
The user approved this correction after the audit. It is implemented and checked
at the bounded wire, receiver and piston input boundaries, with no raw-data
rewriting. This does not claim that all external callers have been audited.

## Deferred work

The following remain outside this goal:

- Automatic completion or repair of shared regions, general merge, and a fallback
  policy when completion is impossible. Detect the damage and preserve diagnostics.
- Automatic child-version tracking or generation of parent update proposals.
  Explicit proposals, review and adoption remain supported.
- Entities and general entity interaction.
- New restartability or internal-state behavioral types.

Unsupported Minecraft mechanisms outside existing validated subsets are not
implicitly authorized by the word migration. Unknown running-world history and
unobserved scheduler order must not be filled in with convenient defaults.

## Stop conditions

Stop implementation and report to the user if a required contract is insufficiently
defined, an agreed requirement conflicts with another, undeclared work becomes
necessary, or a discovered issue cannot be resolved without an unjustified policy
choice. Report the concrete example, affected phase, preserved work, proposed
options and recommendation. Do not silently relax a type, invent event ordering,
select between conflicting laws, discard external obligations or adopt a parent.

Investigation, comparison with existing code/fixtures and ordinary debugging are
part of the declared work. Mark an issue as an implementation defect, a missing
migration, an evidence limit or a missing requirement so that the user can decide
at the appropriate level.

The diagnostic example commands are retired. Explicitly ignored fixture tests retain their cases; `DUSTROUTE_DIAGNOSTIC_OUTPUT` must be a new absolute path. JSON is fixture IO only, and a diagnostic run never certifies a live circuit or adopts a design.
