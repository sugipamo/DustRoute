# DustRoute MCP — guide for LLM clients

Use DustRoute to observe Minecraft redstone, explain evidence, create hypothetical revisions, review Blueprint updates, and propose verified world changes. Ground live-world tasks in the configured player's gaze or an explicitly selected region. Offline Blueprint tasks use exact catalog records and need no bridge connection. Keep observation, hypothesis and execution separate.

This is the tool-use guide. Server installation, credentials, permissions and transport configuration belong in [SETUP.md](SETUP.md). Detailed subsystem examples are in [REFERENCE.md](REFERENCE.md); the 23 base tools, one native survival addition and 7 debug additions are in the [public feature guide](../../docs/mcp-public-features.md). Use the connected server's tool schemas for exact arguments.

The native Voxrig backend observes received packets and supported client
reconstruction. Its `client_reconstructed` readbacks retain connection, receive
sequence and client frame; they do not confirm server ticks or hidden queues.
Mineflayer keeps its separate command-confirmed readback contract. Fresh native
client observations support the shared workflows after their own validation;
they never become server-confirmed evidence. Saved evidence from either source
requires a fresh observation before a new world action.

Native gaze uses `block_outline`, including supported thin parts such as dust,
levers, repeaters and comparators. It uses received player pose and client world
state. Fluids/entities are excluded; moving or unavailable geometry and
unsupported context-dependent shapes make the query unavailable. Use a selected
region when gaze geometry is unavailable; this does not bypass complete
observation or simulator support checks. Native recordings use client frames,
so they cannot satisfy a pulse-width requirement stated in server game ticks.
Read the [backend comparison](../../docs/mcp-public-features.md#observation-backends)
before interpreting evidence or requesting command permissions.

Custom electrical Assembly construction returns a durable `instance_id`.
After MCP restart, use `manage_assembly` (`list`, `get`, `observe`, `diagnose`, `plan_removal`, `plan_reconstruction`)
to read records, freshly compare/revalidate the world and preview a new
conditional removal or reconstruction operation. Reconstruction tears down the
observed supported layout and rebuilds the declared initial state; review all
affected blocks and keep competing inputs/edits out of the region. It uses the
ordinary command path and does not prove empty server queues. Stored success
never authorizes writes by itself.
See [persistent placed Assemblies](../../docs/placed-assembly-management.md).
Use `diagnose` to locate missing parts or player edits even when repair cannot
be planned. Findings compare a declared reference; they do not establish cause,
ownership or a live functional pass. See [Assembly diagnosis](../../docs/assembly-diagnosis.md).

Circuit diagnosis and Assembly diagnosis share a [diagnostic result and repair
handoff](../../docs/diagnostic-system.md). Read `method` to distinguish connection
inference from design comparison. Placed-instance findings expose their pinned
Assembly ID and declared Blueprint occurrences; read those with
`get_circuit_revision(blueprint.kind=assembly|blueprint)`. A source link does not
prove a fault or authorize an update.

The [architecture cutover guide](../../docs/architecture-cutover.md) lists the
retired catalog/update/store/instance/repair formats and coordinated Rust/JS Bridge update.
Legacy mutation RPC names and untyped acknowledgements are no longer accepted.
Every update archive uses v5; the player-scoped Blueprint store uses v2 with an
explicit grounding map. Old stores are refused without rewriting their files.

## Choose the next tool from the task

| User intent | Tools and decision |
| --- | --- |
| Check connectivity or policy | `get_bot_status`. Inspect connection, configured player and mutation policy before live work. |
| Describe visible blocks | `get_world`. This is raw observation, not a design or complete functional interpretation. |
| Diagnose the circuit at the gaze | `test_circuit`, then reuse its `circuit_id`. |
| Inspect a selected region | `set_region` for `first` and `second`, then `show_region` for a fresh snapshot and `circuit_id`. `clear_region` clears selection, not blocks. |
| Explain circuit behavior | `convert_from_circuit(circuit_id)`. Read completeness, capabilities, diagnostics and `mechanisms` before assigning a function. |
| Inspect abstraction details | `get_circuit_ir(circuit_id)`, then use the returned `analysis_id` and a returned `node_id` for expansion. |
| Try a hypothetical edit or branch | `test_circuit_change` with exactly one `circuit_id` or parent `revision_id`. Read saved versions with `get_circuit_revision`. |
| Read or author Blueprint data | `get_circuit_revision` with `blueprint.kind`; `test_circuit_change` with `blueprint.action` equal to `import` or `capture_revision`. Imported data is unverified. |
| Consider a child update | `test_circuit_change(blueprint.action=propose_update)` with complete candidate definitions/state, then `show_operation`. Adopt/reject via explicit `blueprint_decision` on `invoke_operation`; these actions never write Minecraft. |
| Place a saved revision | `new_placement(revision_id)`. A new live comparison and placement proof are required. |
| Reflect an adopted Assembly | `new_placement(assembly_revision_id)`. Its ancestry must reach a complete captured Circuit Revision; reflection uses that original dimension and coordinates and reruns both adoption review and live validation. |
| Place a built-in circuit | `new_placement(circuit)`. Supported names: `half-adder`, `half-subtractor`, `mux2`, `decoder1to2`, `full-adder`, `piston-door-1x2`. |
| Repair observed circuitry | `new_repair(circuit_id)`. Use `get_repair_context` when evidence admits competing explanations; resolve the returned questions before choosing a repair. |
| Optimize supported existing wiring | `new_optimization`. Current candidate generation is limited to a non-branching dust path with fixed endpoints. |
| Replace a known macro | `new_macro_optimization` with a candidate `component_id` returned for the same `circuit_id`. Failed/unavailable verification blocks application. |
| Open or close the known 1×2 door | `new_piston_door_operation(circuit_id, target)` where `target` is `open` or `closed`. This plans normal lever activation, not construction. |
| Test a live input transition | `new_transition_test(circuit_id)`. Review scenario support, contracts and restoration behavior before running. |
| Review, execute or recover an operation | `show_operation`, `invoke_operation`, `undo_operation`; inspect status/results with `get_operation`. Recovery depends on operation kind. |

The default profile exposes all tools above. Debug-only discovery and asynchronous controls are optional; do not assume they can be called in the default profile. `get_operation` is a default tool. There is no public `get_piston_door_state` or separate `invoke_repair` endpoint.

## Keep IDs and evidence distinct

| ID | Meaning | Correct use |
| --- | --- | --- |
| `circuit_id` | Immutable observed-world snapshot | Reuse for analysis and source-specific plans. To see current world changes, capture again. |
| `revision_id` | Immutable hypothetical snapshot | Edit/branch it, retrieve it, or ask `new_placement` to validate a proposal. Never pass it as a live circuit ID or operation ID. |
| `parent_revision_ids` | Revision ancestry | Current records have zero or one parent. Multiple children form branches; merge is not implemented. |
| Blueprint/type/classification revision IDs | Immutable local definitions | Read via `blueprint.kind`; exact pins only. Classification labels do not prove meaning or signal compatibility. |
| Assembly Revision ID | Explicit composed state, independent of its sources | Read via `blueprint.kind=assembly`. It is not a circuit `revision_id` or live placement permission. |
| `base_observation_id` | Original observation reference | New revisions retain its snapshot separately for later live comparison, even when the original ID expires. |
| `analysis_id`, `node_id`, `component_id` | Identifiers scoped to an analysis | Use only with the observation/analysis that returned them. |
| `operation_id` | A particular proposal or execution record | Use the common operation lifecycle. Do not substitute a revision ID. |

A `circuit_id` is held in memory for 15 minutes and may be evicted at the 64-record limit. Revisions use the scoped state store, default TTL one hour, and survive restart under the same state scope. Reads do not extend expiry. Most operation plans are in memory and are lost on restart. Revision placement, fixed-door placement and door activation proposals have five-minute pre-execution lifetimes; do not assume all other operation kinds share this TTL. See the [lifetime and recovery tables](../../docs/mcp-public-features.md#ids-and-retention) for details.

Blueprint catalogs and proposal histories are separate and have no TTL. Capture
a circuit revision's modeled Assembly explicitly to retain it in that catalog.
Old source references stay pinned; adopting a proposal only appends new records.

Repair and optimization plans use the scoped disk store without a memory
fallback. Expiry or deletion blocks preview, apply and undo even in the process
that created the plan. Preview and successful apply/undo renew retention by
saving the plan; reads do not. Unreadable saved state returns an error.

## Review an offline Blueprint update

For an LLM-authored virtual design, use
`test_circuit_change(blueprint.action="generate_building_design")` with named
parts, `fill`/`shell`/`blocks` shapes, local cutouts and permanent air spaces.
An optional uniquely adopted equipment Assembly keeps its original requirements
and can expose explicit terminals. Material/space conflicts return item names
and coordinates for correction. A successful result permits the ordinary
import/review/adoption/placement planning path; it is not live-site evidence or
write permission. See [structured design input](../../docs/blueprint-building-design.md).

For a small building, `test_circuit_change(blueprint.action="generate_building")`
authors floor/wall/roof definitions, an opening and exact air-clearance contracts
from typed dimensions and existing materials. Generation returns fresh structural
and shared construction checks without publishing or writing blocks. Import the
records and propose the returned request, then use the normal review/adoption and
empty-site placement workflow. `generate_building_with_door` additionally mounts
a uniquely adopted typed 3x3 door, retaining its original requirements and
exporting control/aperture aliases. The combined world is freshly reviewed;
explicit motion space separates the mechanism from fixed building patterns.
The initial mount is one block deep in the north-wall frame.
See [building parameters, scope and recovery](../../docs/blueprint-building.md).

Start with `get_circuit_revision({"blueprint":{"kind":"catalog"}})` and read the
relevant exact records. Use the tool schema to supply an explicit candidate via
`test_circuit_change({"blueprint":{"action":"propose_update","request":{...}}})`.
The request includes the old and new parent/child pins, changed intermediate
definitions and complete candidate Assembly; nothing is auto-merged or routed.

Use `show_operation` to review the diff and every parent, descendant and shared
occurrence. `can_adopt` requires an open proposal and every required check to
pass. A passing parent cannot override failed or undetermined children. Snapshot
checks cover placement, physical signal types and connections. For `Periodic`,
`FiniteBurst` or `RepeatedSettling` obligations, supply `behavior_context` in the proposal (or in an Assembly
read with `validate:true`): select the explicit profile, fresh-construction initial
condition and exact dust/torch law IDs. The profile also pins four spatial law
Revisions; including a different law in a Blueprint does not select a new world
model. Blueprint `behavior_bindings` attach a
behavioral type to a named output; consumer `required_source_types` can also require
it. Without context these obligations remain undetermined. A model pass proves
only the declared requirements in that model. `FiniteBurst` requires at least
two ON-to-OFF transitions and eventual permanent OFF; it does not certify
restartability. These types do not fix numerical timing or certify live behavior.
For snapshot requirements on the realization itself, use
`static_type_bindings: [{type_revision, port}]` with a `Signal`, `BlockKind` or
`BlockPattern` type. These are checked even without a connected consumer;
`dustroute.lever.wall.v2` uses this to assert actual lever identity. Older pins
remain unchanged. All current declarations use catalog v13 and update history v5.
Use `required_laws: ["<immutable law revision ID>", ...]` to declare a source's
physical law dependencies. The selected world context must provide every required
Revision, including the laws' own dependencies. Missing context is undetermined;
a mismatched Revision fails. Declarations neither create physical state nor
certify live Minecraft conformance. Shared and nested sources use one physical
state and event stream. Law requirements remain in update history even when
declared only by a pending candidate.
Explicit `observed_inputs`/`observed_outputs` bindings declare named signal/location observations;
fixed-geometry proof contexts leave them undetermined. For supported location-only
`RepeatedSettling`, new requests use `behavior_context: {"piston": {
"known_region": ..., "input_levers": [...], "root_limits": {...}}}`.
This chooses the shared electrical runtime for all six body directions; no
horizontal/vertical selection is needed. The complete Assembly's known region
and actual input levers are required; root limits may be omitted. The stored
proposal pins the explicit electrical v1 profile and fresh initial conditions.
Every input
must observe a distinct actual lever's powered state; every output has an explicit
location predicate. Review includes input changes during motion and each
intermediate write. Retained child/static requirements must pass independently.
Dust, repeaters, conductor power and piston quasi-connectivity share this
runtime. Mixed signal/location bindings, moving-world routes and autonomous
contracts remain unsupported. Horizontal, vertical and direct-only callback
profiles have been removed. Saved objects naming them are rejected; start fresh
with the electrical context and explicitly review any changed law requirements. Contexts require proposal-history v5 and fresh adoption checks after
restart. Adopted electrical Assemblies can request `new_placement` with
`assembly_target`, followed by preview, apply, full readback and conditional
undo. See [construction scope and evidence](../../docs/custom-piston-assembly-placement.md).
Persisted diagnostics are not proof, and adoption reruns the checks.
Review also reports `placement_validation_profile`. Fixed-geometry review's v2 gate rejects
stored wire-rise arms that contradict the surrounding support, upper wire or
clearance; unknown required neighbors remain undetermined. Literal observations
and old Revisions stay unchanged. Historical v1 model success cannot override
current placement or adoption failure.

For `RepeatedSettling`, map every named type input/output to Blueprint ports
using `behavior_bindings: [{behavior_type, inputs, outputs}]` and specify actual
`input_drivers` in the context. Each driver names an Assembly terminal position,
port kind and actual lever position. Review checks that the lever establishes
the requested input value throughout the explored graph, then universally
verifies the relation using conservative history abstraction. Missing controls,
ambiguous mappings and incomplete proofs remain undetermined. See
[multiport bindings and adoption](../../docs/repeated-settling-adoption.md).

Use `test_circuit_change(blueprint.action="optimize")` to search the supplied
Assembly or an explicitly scoped component body for fewer actual blocks while preserving only an explicit
behavioral type. It can rebind ports and replace internal interpretations.
Component mode reports the body count separately from the complete setup; external
controls remain fixed. External routes and boundaries are retained and freshly
checked, including routes through shared body blocks. An external endpoint that
cannot be retained requires explicit parent reconnection; search does not discard
it. Shared physical positions count once.
`blueprint.action="enumerate_layouts"` enumerates and verifies the torch/support
family, with direct device outputs and independent lever definitions. See
[component patterns](../../docs/blueprint-component-patterns.md).
Results are new candidate data: the search does not publish, adopt or reconnect
parents. Review the candidate and use an explicit parent update proposal;
adoption still reruns every retained obligation. Global minimality is not proven.
See [request shape and search limits](../../docs/blueprint-block-reduction.md).

For the observed single-torch scope, select
`dustroute.dust-single-torch-block-effects.v1`. It rejects multiple torches and
unsupported nested block effects. The original synchronous profile remains
selectable for existing pins but disagrees with the recorded four-block feedback
clock; that candidate fails the periodic requirement and passes the separate
finite-burst requirement under the new profile.
See [physical comparison and limits](../../docs/periodic-clock-conformance.md).

When authorized, use `invoke_operation({"operation_id":"…","confirm":true,
"blueprint_decision":{"action":"adopt"}})` or a decision with `action:"reject"`
and a `reason`. Adoption revalidates and saves locally; `writes_minecraft` stays
false. Decisions survive restart and cannot be undone by rewriting history.
Keep the old revision or propose a new change. For complete arguments, failure
handling and persistence limits, see the [Blueprint MCP contract](../../docs/blueprint-mcp.md).

## Create and refine a hypothetical circuit

Use an empty edit list to save an unchanged starting revision:

```json
{"circuit_id":"<observed-id>","changes":[]}
```

Pass that request to `test_circuit_change`. Then edit or branch from the returned revision:

```json
{
  "revision_id":"<parent-revision-id>",
  "changes":[
    {"position":{"x":1,"y":0,"z":0},"block":"minecraft:stone"},
    {"position":{"x":1,"y":1,"z":0},"block":"minecraft:repeater",
     "properties":{"facing":"north","delay":"2","powered":"false","locked":"false"}}
  ]
}
```

The coordinates above are illustrative: use positions from the actual snapshot and stay inside its bounds. Each edit replaces the entire state; omitted `properties` means an empty map, not “keep existing attributes.” `minecraft:air` deletes a block. Duplicate coordinates are rejected. The final candidate is checked after applying the whole edit batch, so a support and its component can be added together.

Inspect `validation.before` and `validation.after`. A stored draft can be invalid or unsupported; `ok: true` means the revision was saved, not that it is deployable. Create a child revision to fix it. Read the exact saved state with `get_circuit_revision({"revision_id":"…","include_snapshot":true})`.

Limits: 4096 virtual edits per request, 4096 non-Air result blocks, 4 MiB per saved record, and 1–256 simulation ticks (default 64). Simulation checks initial-state behavior only. Missing Java property coverage, incomplete observations or unsupported mechanics are not a successful verification. Do not claim functional equivalence or all-input correctness from `structurally_valid`.

## Propose and execute a change

1. Create a plan for the grounded source. For a revision, call `new_placement({"revision_id":"…"})`. For an adopted Assembly whose ancestry reaches a captured Circuit Revision, call `new_placement({"assembly_revision_id":"…"})`. Do not combine either ID with `circuit` or optimization.
2. Read the returned diff, bounds, verification and limitations. If rejected, explain the reason and return to observation or revision editing.
3. Call `show_operation` with the returned operation ID. Explain the concrete change to the user and obtain the required confirmation before execution.
4. Call `invoke_operation` with that ID and `confirm: true` only when authorized. Default policy is read-only; do not treat a proposal as permission to override it.
5. Read the execution result and live verification. A successful tool transport or a submitted write is not sufficient evidence that the intended blocks are present.

Revision placement uses the cumulative diff from the original observation, not just the last parent edit. It rescans the original region plus one block of context, requires exact base agreement, and reruns the shared placement checks. Changed states must export without losing properties. Powered states or unspecified properties that the exporter would reset can be rejected even when a draft was saved successfully. Older revisions without a retained base snapshot cannot be placed.

Assembly placement uses the same preview, authorization, exact live-base check,
lossless export, apply verification and undo path. Adoption is necessary but is
not saved placement authority: the proposal is reviewed again, and an adopted
ancestor must retain a complete literal base snapshot. Candidate interpretation
remains separate from that observation. Arbitrary relocation and new construction
from an Assembly ID are outside this workflow.

Do not switch a failed revision proposal to raw writes, a different gaze target, or another operation family to bypass its validation. This revision-placement path is command-based and uses the configured bot's privileges. The separate native-only [`survival_construction` workflow](../../docs/survival-public-construction.md) accepts bounded grounded passive-building designs, checks adoption and inventory, and reports background progress and persisted diagnosis. Circuit placement does not inherit survival support.

## Handle failures according to operation kind

| Result or situation | Next action |
| --- | --- |
| Missing/expired observation | Capture a fresh `circuit_id`; do not silently substitute it into a plan tied to old evidence. |
| Invalid/unsupported revision | Explain the saved diagnostic, then create a corrected child. Do not promise application. |
| World differs from the base or preview | Reobserve and reconsider the plan. A conflict is not automatically resolved by replaying the same edit. |
| Ambiguous activation/write, verification failure, or `needs_inspection` | Inspect current blocks first. Do not automatically retry or roll back a consumed revision-placement or piston operation. Some failures return only `ok: false` and an error; absence of a status field proves nothing about partial writes. |
| Undo a successfully applied revision | Use `undo_operation(confirm=true)` only while the full expected target context still matches. A changed context blocks restoration. |
| Undo fixed 1×2 construction | The complete door must be in its original open state. If closed, first observe and plan an ordinary open operation. |
| Reverse a 1×2 open/close action | Capture again and create a new target-state plan. `undo_operation` is not supported for door activation. |
| Repair or transition-test recovery | Follow that operation's restoration checks and returned results. Do not infer its retry/rollback guarantees from the piston implementation. |
| MCP restart after a world change | Most operation/undo records are gone. Reobserve; a surviving revision alone is not a surviving undo plan. |

## Interpret the 1×2 piston capability correctly

Existing observation tools return `mechanisms`; only an exact stable contract match identifies `piston_door` and its `open`/`closed` state. Other piston structures remain `unidentified_piston_mechanism`. A `moving_piston` marker is evidence, not proof that motion will complete. Do not call mismatched static rows “moving” without evidence.

The operation/build contract is Java 1.21.11, fixed 1×2 layout, translation only. `new_placement({"circuit":"piston-door-1x2"})` installs the open layout in an empty guarded region, with the lower-piston origin three blocks above the gaze target. Relative guard bounds are `(-3,-2,-4)..(7,3,6)`. Optimization and substitutions are rejected. General piston revisions do not inherit this placement permission.

## Read more only as needed

- [Public feature inventory and support limits](../../docs/mcp-public-features.md)
- [Revision editing and live placement contract](../../docs/circuit-revisions.md)
- [Blueprint catalogs, update proposals and decisions](../../docs/blueprint-mcp.md)
- [Fixed piston contract and live evidence](../../docs/piston-door-mcp-v1.md)
- [Response schemas and compatibility](../../docs/mcp-api-v1.md)
- [Detailed repair, transition and optimization reference](REFERENCE.md)
- [Human setup and policy configuration](SETUP.md)
- [Live integration procedures](mineflayer/e2e/README.md)

### Work on larger circuits by region

`get_world({region:{min,max}})` captures complete coordinates in the observed
player dimension without requiring a gaze target. `get_bot_status` reports
`observation_capabilities` separately from policy: Voxrig permits 262,144 loaded
cells with client reconstruction evidence; Mineflayer permits 8,880 cells with
command predicate evidence. Raising policy does not raise an adapter's limit.

Create a literal revision using up to 4,096 virtual changes, then call
`new_placement({revision_id,work_regions:[{min,max},...]})`. Every change must be
covered by one of at most 64 disjoint input regions. Oversized regions are split
and bounded support/watch cycles combined into at most 64 stages of 64 declared
changes. The full context permits at most 4,096 non-Air blocks. Omitted edit_scope protects every cell outside work_regions; an
explicit edit_scope controls permitted transient effects too. Support/watch
precedence chooses a candidate region order. Only the current stage receives
a fresh full-context forward/inverse common-runtime proof. The boundary includes
natural updates; one temporary output initialization is allowed without changing
input settings, geometry or the immutable final target. A naturally satisfied
stage still needs a fresh preview and confirmation, with no writes. Later stages may
fail and need a different partition or explicit intermediate design.

Preview and apply the returned operation normally, then call
`manage_construction_job({job_id,action:"plan_next"})` for the next operation.
`get` reads durable intention/history, with large states summarized; explicitly
set `include_intention:true` to expand all saved data. `observe` compares a fresh sample with
the last verified prefix. `plan_undo` plans the last verified region's inverse,
with a new operation/preview, even after restart. Replanning invalidates the
previous preview. `cancel` invalidates pending forward work and retains history;
fresh inverse cleanup remains available without reenabling forward work.

An ambiguous write records needs_inspection before any commands. No automatic
retry or rollback follows. Explicit `plan_recovery` can create a new stage only
if fresh full-context samples match the original stage baseline; a partial
prefix requires diagnosis and a new explicit repair design. Stored targets
and sparse verified boundaries are never restored as executable proofs. v1 job
history cannot be resumed; explicitly recapture a v2 job. Jobs do not adopt or
upgrade Blueprint references or infer functional behavior. See
[region migration](../../docs/large-circuit-regions.md) for remaining limits.
