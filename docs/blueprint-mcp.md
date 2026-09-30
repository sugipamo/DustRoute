# Blueprint and Assembly workflows over MCP

The existing tools support a local catalog of immutable Blueprint, Assembly,
type and classification revisions. A Blueprint is a source definition; an
Assembly records explicit placed state and connections. Classification labels
describe interpretations, while connection types check physical signal
compatibility only. Catalog membership is never proof of behavior or placement.

This workflow needs a configured assist player but no running Minecraft bridge.
All responses from its handlers use `dustroute.blueprint-mcp.v1` and
`writes_minecraft: false`. It is available with the default world read-only
policy. The default profile has 22 tools, including `manage_assembly` for durable
live-placement records. Catalog operations use the existing Blueprint request
forms. Use `tools/list` for complete input schemas, including block,
interface, source requirement, inclusion and actual-state fields.
`test_circuit_change` and `show_operation` advertise additive local writes in
their tool annotations because they can retain records or review history;
neither annotation grants permission for a Minecraft mutation.

## Read exact records

Call `get_circuit_revision` with a `blueprint` query instead of `revision_id`:

```json
{"blueprint":{"kind":"catalog","classification_id":"dustroute.classification.not.v1","offset":0,"limit":100}}
```

The catalog lists source IDs/names/classifications, type and classification
IDs/names, Assembly IDs, and proposal operation IDs/statuses/titles. Omit
`classification_id` to list all sources. `offset`, `limit` (1–256, default 100),
`total_blueprints` and `next_offset` paginate the source list only. Other indexes
are returned in full. A new catalog starts with the frozen built-in sources.

```json
{"blueprint":{"kind":"blueprint","id":"dustroute.not.torch-top.v1"}}
```

Other `kind` values are `type`, `classification`, `assembly` and `archive`.
The first three take the exact corresponding `id`. An Assembly query can add
`validate: true` for a fresh initial-state review. `archive` exports all source
data, states and proposal history; it takes no ID. Responses put these records
under `result`. Do not combine a Blueprint query with `revision_id` or
`include_snapshot`.

## Add data or retain an observed draft

Call `test_circuit_change` with the following envelope. Each list contains full
records matching the tool schema; omitted lists are empty:

```json
{"blueprint":{"action":"import","records":{"types":[],"classifications":[],"revisions":[],"assemblies":[]}}}
```

Imports check structure and references, not physical or behavioral validity.
They are atomic: one conflict leaves the stored catalog unchanged. Reimporting
an identical record is harmless; rebinding an ID to different data is refused.
Dependencies must exist in the catalog or in the same batch. Batch order is
irrelevant. IDs reserved by proposals cannot be published through import.
Source definitions use modeled blocks, not arbitrary Minecraft command strings.

To retain the modeled Assembly from an existing hypothetical circuit revision:

```json
{"blueprint":{"action":"capture_revision","revision_id":"<saved-circuit-revision-uuid>"}}
```

Capture preserves its identity, parents and exact modeled state. Capture parent
records first when ancestry is present. A record without a decodable Assembly
is refused. Capturing unclassified observed blocks does not assign a Blueprint
identity to them. After capture, this Assembly survives expiry of the original
hypothetical revision. Neither import nor capture means verified promotion;
their response says `validation: "not_evaluated"`.

Blueprint actions cannot be combined with top-level `circuit_id`, `revision_id`,
`simulation_ticks`, or nonempty block `changes`.

## Propose an explicit child update

Use `test_circuit_change` with
`{"blueprint":{"action":"propose_update","request":{...}}}`.
The request has these required fields:

| Field | Meaning |
| --- | --- |
| `title`, `description` | Human-readable proposal intent |
| `base_state` | Existing Assembly Revision ID |
| `base_parent`, `candidate_parent` | Exact old and proposed parent Blueprint IDs |
| `parent_instance` | Instance path from the Assembly root, as an array of instance IDs |
| `child_before`, `child_after` | Child paths relative to that parent, allowing changed decomposition |
| `previous_child`, `next_child` | Exact old and proposed child Blueprint IDs |
| `revisions` | Full new parent and changed intermediate source definitions with new IDs and ancestry; include new dependencies too |
| `candidate_state` | Full explicit Assembly Revision, including new ID, ancestry, blocks, known regions, pinned instances, connections and boundaries |

Read existing records before authoring these values. The caller supplies the
complete proposed arrangement; this action does not route, merge, repair,
rewrite surrounding children, or infer which source is the latest. Existing
sources retain their exact pins. The server assigns an operation UUID and
reserves candidate IDs. A successful response saves an `open` proposal; it
does not publish its candidate definitions into the catalog.

## Review, then decide

Pass the returned UUID to the existing operation tools:

```json
{"operation_id":"<proposal-uuid>"}
```

`get_operation` reads the request, `open`/`adopted`/`rejected` status and history.
`show_operation` additionally returns `diff`, fresh `validation` and `can_adopt`.
For an open proposal it persists a validation event without adopting anything.
The diff includes actual block changes, changed occurrence paths and pins,
connections, boundaries, coverage, source-state differences and affected shared
occurrences. Validation reports every occurrence independently, including
descendants and interpretations outside the edited parent.

`validation.status` is `passed`, `failed` or `undetermined`. Its scope is
`initial_state_placement_types_and_connections_only` unless an explicit behavior
context selects additional autonomous behavioral checks (see below). The legacy
`behavior_verified` and `live_world_verified` fields remain false; scoped
behavioral results use the additional fields described below. A passing parent cannot
override a failed or undetermined child. Unknown cells are not known air.
`can_adopt` also requires that the proposal remains open.

After reviewing the concrete proposal and receiving authorization, call
`invoke_operation` with an explicit decision:

```json
{"operation_id":"<proposal-uuid>","confirm":true,"blueprint_decision":{"action":"adopt"}}
```

Adoption runs the checks again, then appends the new definitions and Assembly
atomically. It preserves all old revisions and other parents. Stored validation
events cannot satisfy this check. Failed or undetermined checks produce
`ok: false`, `error_code: "verification_failed"`, a fresh report and a retained
open proposal. No candidates are published. To decline:

```json
{"operation_id":"<proposal-uuid>","confirm":true,"blueprint_decision":{"action":"reject","reason":"Keep the previous shared layout"}}
```

Rejection retains the request and history. Final decisions cannot be rewritten;
`undo_operation` is unsupported for these records. Keep an old revision in use
or create another proposal. Missing decisions, missing confirmation, conflicting
transition contracts and rebinding attempts are refused. Other catalog/state
errors use `invalid_state` with a diagnostic; transport-level malformed tool
arguments may instead be rejected by MCP before the handler runs.

## Persistence and live-world boundary

Catalogs are isolated by server/state scope and player. `DUSTROUTE_STATE_DIR`
selects the base directory; each catalog uses a private `blueprints` subdirectory.
Unlike temporary plans and hypothetical circuit revisions, catalog records and
proposal histories have no TTL. The default directory is under the operating
system's temporary directory: configure durable storage for long-term use.
`archive` exports data for backups. Restarting under the same scope reloads the
catalog automatically, with all fixed references and decisions. There is no
public tool for overwriting a catalog with an archive or rewriting its history.

Requests are capped at 4 MiB of serialized Blueprint action data; archives at
16 MiB. Existing expansion and coordinate limits also apply. Size failures do
not partially save. An exclusive file lock spans load, validation and atomic
replacement, so concurrent instances cannot overwrite each other's decisions.
A busy catalog returns an error asking the caller to retry. Corrupt or
inconsistent stored data causes an error; it is not silently reset or replaced
by cached state. Saved history remains diagnostic data, never proof.

Reading an Assembly now includes a `lifecycle` object. A state published by an
adopted update reports `adopted: true` and the exact update operation IDs that
published it. Imported and captured states report `catalog_only_unadopted`.
This is immutable history, not a saved validation result. `placement_eligible`
is true only when exactly one adopted proposal publishes the state and its
immutable ancestry reaches a complete captured Circuit Revision.

Adoption itself never writes Minecraft. An eligible Assembly can be reflected
with `new_placement({"assembly_revision_id":"…"})` only in the captured
dimension and at the original coordinates. Planning freshly reviews the adopted
proposal, rescans the literal base and its context, and uses the existing preview,
authorization, apply verification and undo contracts. The captured snapshot is
literal evidence; the candidate Assembly is a separate interpretation. Arbitrary
relocation, new construction, automatic attribution, merge/repair, general
overlap completion and entities remain deferred.

`service_blueprint_tests.rs` exercises the actual MCP transport, restarts,
successful adoption, parent-pass/child-fail rejection, unknown clearance,
forged saved validation, unclassified capture, atomic imports and store locking.

## Periodic obligations

An imported Type Revision can use this contract:

```json
{"kind":"periodic","requirement":{"output":"signal"}}
```

It requires one autonomous Boolean output to repeat a nonconstant waveform after
finite startup. No numerical period, duty, phase or coordinate is fixed. Bursts
and recovery pauses are allowed. A separate finite-burst contract is also supported:

```json
{"kind":"finite_burst","requirement":{"output":"signal"}}
```

It requires at least two actual ON-to-OFF transitions, followed by permanent OFF.
An initially ON output counts when it falls; a single pulse or always-OFF output
fails. There is no stopping deadline, exact count above the minimum, fixed
coordinate or required burnout mechanism. Restartability is not included. Both
checks use complete execution-state recurrence, preserving histories and pending
callbacks. See [finite-burst semantics and evidence](finite-burst-behavior.md).
All current catalogs use v13; retired v1–v12 archives are rejected.
See the [cutover guide](architecture-cutover.md).

A Blueprint attaches either obligation to its own named output via
`behavior_bindings`:

```json
[{"behavior_type":"clock.type.v1","output_port":"pulse"}]
```

Consumer inputs may alternatively require the same type through
`required_source_types`. Both forms remain unverified on import. Every declaring
occurrence is checked independently in the complete actual Assembly; surrounding
blocks and connections are not assumed harmless.

To check a saved Assembly in the supported model, use the existing read tool:

```json
{
  "blueprint": {
    "kind": "assembly",
    "id": "clock.state.v1",
    "validate": true,
    "behavior_context": {
      "profile": "dustroute.dust-single-torch-block-effects.v1",
      "initial_condition": "fresh_construction",
      "dust_law": "dustroute.law.dust-strength.v1",
      "torch_law": "dustroute.law.torch.java-1-21-11.v1",
      "max_electrical_iterations": 128
    }
  }
}
```

When `validate` is true, the response retains `validation_context` and adds
`world_execution_context`, the normalized law/initialization/input contract used
by the existing proof model. Both are null when no context is supplied. This is
an additive diagnostic; it does not introduce a new proof profile or rewrite
proposal/archive data. See [world execution contexts](world-execution-context.md).

The referenced types, source records and laws must already exist in the player's
catalog. The fresh-construction condition means declared torch lit state and
empty histories/timers before initial notification. It cannot establish the
hidden memory of an observed running circuit. External input boundaries are
unsupported for these autonomous contracts. Unsupported devices, unknown
neighborhoods and resource exhaustion cannot pass.

The block-effects profile supports at most one torch and delivers feedback during
block changes. Multiple-torch order and nested neighbor block effects remain
unsupported. It matches the retained four-block server observations and rejects
that candidate's periodic obligation, while passing its separate finite-burst
obligation. The original
`dustroute.dust-torch-synchronous-game-tick.v1` remains selectable for existing
pins but has a known live counterexample. No stored context is automatically
upgraded; see [profile comparison](periodic-clock-conformance.md).

Supply the same `behavior_context` object inside a `propose_update` request to
retain these assumptions for subsequent review/adoption. With context, report
scope remains `placement_connections_and_declared_periodic_obligations` for
periodic-only catalogs. A catalog containing a repeated-settling or finite-burst type
uses the additional `placement_connections_and_declared_behavioral_obligations`
value. This preserves existing periodic responses; no-context scope is unchanged.
`behavior_status` summarizes executed behavioral checks (null when none ran).
`declared_behavior_verified` is true only when at least one behavioral check ran and all
required checks passed; its scope is declared obligations in the selected model.
It does not recognize arbitrary logical functions or imply a numerical period,
restartability or live-world guarantee.
`live_world_verified` remains false. Without context, these behavioral obligations remain
undetermined and block adoption.

`show_operation` and explicit adoption both run fresh verification, including
after restart. Saved validation history cannot supply a pass. No clock-specific
tool, automatic recognition or Minecraft write is introduced. Read the
[periodic implementation and regression evidence](periodic-behavior-status.md)
for the exact proof boundary.

## Repeated-settling obligations

The same tools also review and adopt `RepeatedSettling` obligations. Bind every
named type input/output to a Blueprint port using the relation form:

```json
[{"behavior_type":"not.type.v1","inputs":{"a":"input"},"outputs":{"out":"output"}}]
```

The execution context additionally declares actual input-terminal/lever pairs
in `input_drivers`. The input terminals must have the requested Boolean levels
throughout the universal proof, including synchronous block effects. A complete
conservative abstract proof may pass; ambiguous mappings, insufficient controls,
possible abstract failures or resource exhaustion remain undetermined.

Catalogs with this binding form require at least v6; direct-device ports and block-kind requirements need v7. Explicit `static_type_bindings: [{type_revision, port}]` require v8 and check the occurrence itself against a snapshot type, independently of connected consumers. Autonomous binding JSON is unchanged,
and omitted `input_drivers` remains an empty list. All ports, drivers and laws
are rechecked on review and adoption, including after restart. See
[repeated-settling adoption](repeated-settling-adoption.md) for the context JSON,
multiple-port and alias rules, diagnostics and supported model scope.

## Physical law requirements

Each source can declare `required_laws: ["<law revision ID>", ...]`. Every ID
must refer to an executable law in the catalog. This is a dependency declaration;
the world execution context selects the laws and creates physical execution state.
Nested and overlapping interpretations never create extra device states or events.

Review checks each occurrence against the selected world laws, including transitive
dependencies. A missing context is undetermined, incompatible law IDs fail, and an
unsupported world adapter cannot pass merely because the IDs match. Parent success
cannot suppress a child's failed or undetermined requirement. This is separate
from type and physical-behavior evidence, and does not certify live conformance.

All catalogs now use v13; v1–v12 are rejected as described in the
[cutover guide](architecture-cutover.md). All proposal histories use v5 and retain
law requirements even when only an unadopted candidate declares them.
Older immutable records remain unchanged. Generated optimized candidates declare
their pinned world laws; port movement cannot silently replace those laws.

Explicit observation bindings are retained in the current v5 proposal history.
The existing fixed-geometry proof contexts cannot certify them. Location-only
repeated-settling bindings can use the separate moving-world context below.

`PistonDoor` adds a completed-operation protocol without changing
`RepeatedSettling`. Its `piston_door` contract has one logical `closed_input` and
a 3×3 matrix of Air/Solid observation names, bound using `Observed`. It requires
the current catalog and moving-world context (proposal history v5). The same public
import/propose/show/adopt workflow performs fresh shape, behavior, Law and child
checks after restart. See [the contract](piston-door-type.md) and
[reference adoption evidence](reference-door-ordinary-adoption.md). An adopted
model result does not provide a live readiness sensor or bypass target grounding,
current-world observations, construction/readback or undo checks.

## Location behavior in the moving world

Use the same `get_circuit_revision` and update-proposal tools. New piston work
uses `behavior_context: {"piston": {"known_region": ..., "input_levers": [...]}}`,
which selects the shared electrical runtime for all six body directions.
`root_limits` can be supplied inside `piston`. Requests resolve to the following
explicit context before persistence; recorded contexts may also be supplied directly:

```json
{
  "profile": "dustroute.piston-electrical-root-exploration.v3",
  "initial_condition": "fresh_construction",
  "known_region": {
    "min": {"x": -4, "y": -1, "z": -4},
    "max": {"x": 12, "y": 5, "z": 4}
  },
  "input_levers": [{"x": -1, "y": 1, "z": 0}],
  "root_limits": {
    "max_microsteps": 100000,
    "max_pending": 10000,
    "max_call_depth": 512
  }
}
```

The region must be declared as known by the complete candidate Assembly; it
cannot invent Air through holes or crop surrounding blocks. This context selects
the shared electrical piston world profile and its 11 immutable law roles.
Declared inputs must be powered-state observations of distinct actual levers,
covering exactly `input_levers`. Named outputs use explicit location predicates.
Coordinates resolve from each candidate's ports, so independently verified
candidates may relocate and rotate them. See [location bindings](location-state-bindings.md).

Review explores input changes during movement, pending work and carrier history,
including intermediate writes inside a synchronous operation. A pass requires
complete reachable-state exploration; a reproducible bad cycle is sufficient to
refute a relation earlier. Parent behavior success never hides child violations,
literal `BlockPattern` mismatches, explicit-Air occupancy or unknown obligations.
Unsupported routes, mixed signal/location bindings and autonomous contracts remain
undetermined under this context and prevent adoption.

Native responses use scope
`whole_realization_declared_obligations_in_moving_world_model` and placement
profile `dustroute.piston-electrical-callbacks.java-1-21-11.v3`. The existing
`declared_behavior_verified` flag requires all required checks to pass, even when
the narrower `behavior_status` is passed. `live_world_verified` and
`writes_minecraft` remain false. Counterexample diagnostics are retained in
behavior check details. A saved review does not authorize adoption.

After explicit adoption, [custom construction](custom-piston-assembly-placement.md)
uses `assembly_target` to revalidate translated/rotated coordinates, plan a
supported installation sequence and verify each live write. Historical
directional/direct-only profiles are retired and rejected on loading; this
request shorthand does not reinterpret old records or convert checkpoints.

Proposal histories containing this context use `dustroute.blueprint-updates.v5`,
including a context present only in a validation-history record. Loading it under
v1–v4 is rejected. Supported fixed-geometry and electrical context objects keep their JSON shape
and archive-version requirements. Archives containing retired callback contexts
are rejected without rewriting their files. `show_operation` and explicit adoption
recheck the actual candidate and every retained obligation after restart; a
failed or unfinished review publishes no candidate source/state records.

This extends local review/adoption through the existing tools. It does not add
a door-specific endpoint or allow general piston placement in Minecraft. The
[moving-world verification contract](runtime-location-review.md) defines the
supported model and its evidence.

## Search for a smaller realization

Use `test_circuit_change` with `blueprint.action: "optimize"` to search the
complete supplied Assembly or an explicitly scoped component while preserving
only a selected behavioral type.
The request names the source occurrence/port binding, unused candidate IDs and
explicit execution context. Ports and internal decomposition may change.
Every actual block in that Assembly contributes to cost, including support,
wiring and input controls; shared positions count once.

The response contains fresh diagnostic evidence and `best` candidate data only
when a strictly smaller realization passed. It does not store, promote or adopt
the result, and `global_minimality_proven` stays false. Build an explicit parent
update from the candidate, preserve its target binding and context, and continue
through the normal saved review/adoption workflow. Read the
[full request, limits and regression evidence](blueprint-block-reduction.md).

`blueprint.action: "enumerate_layouts"` enumerates the torch/support family in
component scope, then verifies each candidate under the selected type and pinned
context. It returns separate body/total counts and per-candidate status; it never
publishes a definition. See [component patterns](blueprint-component-patterns.md)
for the complete request and explicit adoption workflow.

## Author, diagnose and revise structured buildings

`generate_building_design` accepts explicit geometry and named permanent-Air
requirements. `generate_building_design_update` checks the supplied previous
input against a uniquely adopted base, retains unchanged child pins, and returns
an ordinary immutable update/diff. Both freshly review the combined world and
construction; they publish no records or Minecraft writes. See
[building design input and updates](blueprint-building-design.md).

Generation failures and proposal review/refused adoption expose the shared
[structured review diagnostics](blueprint-review-diagnostics.md), including
failed versus undetermined checks and available occurrence/type/coordinate,
expected/actual block, runtime input/time and counterexample evidence.

For a captured revision or grounded Assembly at its original coordinates,
`new_placement(edit_scope=…)` declares editable/protected regions and uses the
shared physical modification path, including for inert building blocks.
[Scope checks](world-edit-scope.md) cover every committed model microstep and
forward/undo; live verification remains full-region readback at batch boundaries.
Design adoption never updates placed-instance source identities automatically.

## Follow design references from a diagnosis

Registered placed-Assembly diagnosis now returns its exact Assembly Revision ID
and declared nested/overlapping Blueprint occurrences through the [shared
diagnostic contract](diagnostic-system.md). Use the existing `assembly` and
`blueprint` read requests to inspect those records. Comparison uses the Assembly's
actual modeled state; source layouts, child references, requirements and adoption
records are never changed by diagnosis. Arbitrary live-region/design binding
remains a separate unimplemented workflow.
