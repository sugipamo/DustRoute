# MCP JSON contracts

For the public tool inventory and end-to-end usage, see the [public feature guide](mcp-public-features.md). This document records detailed response contracts.

## Observation sources and clocks

Fresh live scans retain the selected backend's source. The shared record
`ObservationRecord` contains the literal snapshot and a `readback` with one of
these contracts:

| Source | Schema and kind | Evidence fields |
| --- | --- | --- |
| Mineflayer command confirmation | `dustroute.server-readback.v1`, `server_confirmed` | Fresh request ID, dimension, bounds, checked cells, `start_game_tick`, `end_game_tick`, nonce, snapshot digest and predicate/attempt details |
| Voxrig client reconstruction | `dustroute.client-readback.v1`, `client_reconstructed` | `connection_id`, dimension, bounds, checked cells, `receive_sequence`, `client_tick`, `received_revision`, `client_revision`, `captured_at_millis`, `moving` |

Native `client_tick` and capture time describe the client's local clock, not
server game time. Native connection IDs are process-local counters; archived
evidence must remain scoped to its owning process/capture. A reused counter
after restart does not make an old observation fresh.

The native bridge's lower-level `ClientRegion` additionally retains the original
received cache, reconstructed cells and their origins under
`dustroute.client-observation.v1`. Neither client schema can satisfy the explicit
server-confirmed scan API. Shared workflows accept fresh native observations
through their source-aware boundary and still enforce complete coverage,
stationary-state checks where required, placement/model validation and the
normal operation policy. Deserializing any saved record does not create a fresh
observation capability. A command-submission receipt is not a block readback.

Native player observations report `targeting_geometry: "block_outline"`,
`connection_id` and `receive_sequence`. Thin static circuit parts are included;
fluids/entities are excluded. Missing pose/chunks, moving or unsupported geometry
and incomplete reconstruction reject the query. These optional targeting fields
are absent from the Mineflayer response; do not interpret their absence as
native outline evidence. Gaze describes received player pose and client world
state, not a graphical camera-frame or server receipt.

Backend selection, permissions, limits and current targeting evidence are in
the [backend guide](mcp-public-features.md#observation-backends),
[Mineflayer readback](server-readback.md) and [native rollout](voxrig-rollout.md).

## Physical interface evidence

Reverse-analysis responses expose `interface_evidence` with the observed
external input positions, observable output positions, and the mapped or
unmapped subsets. `unsupported_observed_blocks` preserves namespaced Minecraft
blocks whose event or state semantics require live observation. These fields
are evidence, not inferred intent: truth-table, transition, and optimization
verification must report `unavailable` when a boundary is missing or
ambiguous. Empty inputs, outputs, or transition cases cannot pass by
vacuous truth.

Scenario input actions are typed. Use lever state, button press/release,
pressure-plate level, or external-power actions according to the observed
driver. The compatibility `SetPowered` action is strict and rejects a wire or
other arbitrary block.

Reverse-analysis responses may include `physical_function_model` when
truth-table inference is requested. Its output functions are derived from the
shared physical network; `shared_physical_components` reports components that
depend on multiple inputs or influence multiple outputs. Local gate labels are
explanatory and do not exclusively own physical blocks. See
[`physical-function-model.md`](physical-function-model.md).

Focused responses from `test_circuit` and flat `convert_from_circuit` include a
bounded `focused_explanation`. Its `role` identifies the local signal role,
`incoming` and `outgoing` preserve adjacent directed physical edges, and
`input_candidates`/`output_candidates` expose the mapped interface evidence.
`paths_from_inputs` and `paths_to_outputs` are capped path explanations rather
than a complete graph dump. `timing`, `temporal_devices`,
`observation_complete`, and `caveats` must be shown to an LLM or user before a
logical label is treated as certain. The hierarchical large-circuit response
uses the same shape but explicitly reports that flat terminal/path inference
was skipped; empty candidate arrays are not proof of an input-free circuit.

`convert_from_circuit` accepts `include_truth_table=true` for an explicit
bounded exhaustive request, including circuits that use the hierarchical path
for their normal summary. Optional `truth_table_max_inputs`,
`truth_table_settle_ticks`, `truth_table_max_rows`, `truth_table_max_work_units`,
`truth_table_max_solver_iterations`, and `truth_table_max_elapsed_millis`
fields tighten or raise the request within the server's hard protocol bounds.
The latter two are dynamic guards on cumulative fixed-point solver iterations
and wall-clock time; they complement, rather than replace, the static work
estimate. The response reports `truth_table_status` as
`computed`, `budget_exceeded`, `unavailable`, or `not_requested`; a large
default response uses `skipped_large_circuit` and includes a structured
`truth_table_skip` reason. Component-limited observations report
`incomplete_observation` in `truth_table_error_details` and never claim a
functional result. Static, solver-iteration, and elapsed-time budget failures
all include a distinct `truth_table_error_details.code` and the number of rows
completed before the limit. Partial rows are discarded, so a budget failure is
not a successful verification and never contains a partial table in the
`truth_table` field.
The simulator stops early only after two consecutive unchanged electrical
snapshots with no queued device event. If the requested settle window ends
before that condition, `truth_table_error_details.code` is `non_settling` and
the row is not included in a returned table.
When a table is returned, `truth_table_semantics` is also present as
`combinational`, `timing_sensitive`, `stateful`, or `unknown`. This is a
classification of the physical/temporal evidence before enumeration; it does
not turn a stateful circuit into a combinational proof. A `computed` table is
therefore the result of the requested settle procedure under that semantic
caution, not an assertion that all temporal behavior was exhaustively proven.
The debug-only `start_selected_region_conversion` exposes the same truth-table
options and performs the bounded analysis in a cancellable background
operation.

When that model is available, `convert_from_circuit` also returns
`macro_replacement_candidates`. These are function-matched, version-compatible
catalog suggestions ranked by physical size. They are explicitly
`proposal_only`; the response does not authorize placement or bypass the normal
preview and contextual transition-verification workflow.

When the inferred terminals are complete, the same object also includes
`placement_plans`. A plan fixes those observed terminal positions, searches the
four horizontal rotations and reports a connector skeleton. Plans are
read-only and `automatic_apply_allowed` remains false while structural,
steady-state, or transition verification is pending.
Each plan has a `structural_report` listing immutable candidate collisions,
route collisions, cross-net contacts, invalid cell supports, supports that may
be added by materialization, and positions where such supports are blocked.
Structurally valid plans additionally expose a read-only `materialization`
preview with the exact reversible patch, added supports, and signal-strength
repeaters. This still does not authorize application; behavioral and transition
verification remain mandatory.
The `steady_state_report` re-analyzes that virtual world, maps newly inferred
terminals back to the fixed boundary components when inference permits, and
compares every truth-table row by driving the original boundary source
positions directly. The explicit boundary contract remains authoritative when
the replacement topology changes terminal inference.
After a steady-state pass, `transition_report` exhaustively compares ordered
input changes for cells with at most four inputs. It includes one-bit changes
and multi-bit swaps, reporting the first differing tick and both output traces.
Initial settled simulator states are cached per input assignment.
Boundary routes also expose `boundary_facing` and `driver_position`. Aligned
ports require compatible outward faces, and repeaters on input routes face from
the observed boundary toward the replacement cell.

Pass a returned candidate `component_id` and the same immutable `circuit_id` to
`new_macro_optimization` to recompute the placement and issue a normal
operation ID. The tool reruns structural, steady-state, exhaustive transition,
boundary-strength, and preservation-contract checks. Successful contracts use
the standard `show_operation` / `invoke_operation` / `undo_operation`
lifecycle. A candidate with a failed or unavailable category remains
previewable but cannot be invoked.

DustRoute MCP tool results are JSON text. Versioned high-level response
families use these identifiers:

| Family | `schema_version` |
| --- | --- |
| focused diagnostic | `dustroute.diagnostic.v1` |
| placement mutation | `dustroute.placement.v1` |
| observed physical optimization | `dustroute.optimization.v1` |
| repair plan and mutation | `dustroute.repair.v1` |
| repair evidence context | `dustroute.repair-context.v1` |
| transition plan, run, and restore | `dustroute.transition.v1` |
| Blueprint catalog, update review and decisions | `dustroute.blueprint-mcp.v1` |
| legacy common error | `dustroute.error.v1` |
| instrumented execution failure | `dustroute.error.v2` |

Adding an optional field is compatible within v1. Removing a field, changing
its meaning or type, or renaming an enum value requires a new schema version.
Coordinates are always objects with signed integer `x`, `y`, and `z` fields.

Electrical revision previews now explicitly identify
`dustroute.electrical-edit-preview.v2`; job history summaries identify
`dustroute.construction-job-response.v2`. Their earlier unversioned shapes
always expanded model states. In v2, inspect `step_states_expanded`: when false,
`before`/`after` contain bounds, non-Air counts and a model state content ID, and
every step retains position/state/wait plus its expected block count. Complete
expected worlds remain inside the executor. `requested_changes` records explicit
positions separately from natural settled differences; those differences have
count/truncation fields. `no_write_checkpoint` permits an empty command list,
but still requires fresh observation, preview and confirmation.

Job records use `dustroute.construction-job.v3` and retain sparse verified
`boundaries`, including natural updates. `regions[].parts` defines exact stage
membership; `region` is only its display bounding box. Large job summaries
expose `intention_expanded:false`; `manage_construction_job(action=get,
include_intention=true)` explicitly expands the complete historical record.
Retired v1/v2 JSON job files remain untouched and are refused; recapture a
new v3 job using the versioned non-JSON store. These schemas do not change the existing placement-mutation or
readback families. See [region work](large-circuit-regions.md).

The [Blueprint MCP contract](blueprint-mcp.md) describes the `blueprint`
branches of `get_circuit_revision` and `test_circuit_change`, and the explicit
`blueprint_decision` on `invoke_operation`. Their responses set
`writes_minecraft: false`. Initial-state validation and persisted proposal
history are not behavioral or live-world evidence. Optional `behavior_context`
selects fresh checks of declared `Periodic`, `FiniteBurst` and `RepeatedSettling`
obligations. Repeated-settling requires complete port mappings and explicit actual
input drivers; see [contextual adoption](repeated-settling-adoption.md). All current
catalogs use v13; catalog v1–v12 are rejected as described in the
[cutover guide](architecture-cutover.md). Existing periodic response scope values
remain supported. Catalogs with these obligations use the additional
contextual scope value `placement_connections_and_declared_behavioral_obligations`. Scoped results
use `declared_behavior_verified`, `behavior_status` and `behavior_scope`; legacy
`behavior_verified` and `live_world_verified` remain false. The complete record input
schemas are included in `tools/list`; Blueprint IDs and Assembly IDs are
separate from observed circuit and hypothetical circuit revision UUIDs.

For new piston requests, the optional `behavior_context` field accepts
`{"piston":{"known_region":...,"input_levers":[...],"root_limits":{...}}}`.
Root limits are optional. It resolves to fresh construction under
`dustroute.piston-electrical-root-exploration.v3` before persistence; no direction
selection is required. Explicit electrical profile objects are accepted;
retired horizontal, vertical and direct-only profile IDs are rejected without
conversion. These objects cannot be mixed with old
dust/torch fields. Native responses use scope
`whole_realization_declared_obligations_in_moving_world_model` and the selected
piston placement profile. Their histories require
`dustroute.blueprint-updates.v5`; earlier context objects retain their original
JSON and schema requirements. Fresh review/adoption checks the complete actual
world and retained children, including intermediate movement. See
[native context contract](blueprint-mcp.md#location-behavior-in-the-moving-world).

`test_circuit_change` additionally accepts `blueprint.action: "optimize"`.
Its request selects the Assembly, explicit cost scope, target behavior binding, new candidate
IDs and pinned execution context. The result contains fresh diagnostics and an
optional strictly smaller candidate. `catalog_changed`, `writes_minecraft` and
`adoption_authorized` remain false; a search result cannot replace an explicit
parent proposal and fresh adoption. See [block reduction](blueprint-block-reduction.md)
for candidate families, movable terminals, counting scope and bounded-search limits.

`blueprint.action: "enumerate_layouts"` uses the same endpoint to enumerate
physical torch/support patterns and freshly verify their declared behavior.
Component scope keeps external equipment fixed and reports body and complete
Assembly counts separately. See [component patterns](blueprint-component-patterns.md).

## Errors

The legacy human-readable `error` string remains available. Callers should use
the machine-readable fields for control flow:

```json
{
  "ok": false,
  "schema_version": "dustroute.error.v1",
  "error": "transition scenario not found",
  "error_code": "not_found",
  "retryable": false
}
```

Stable error codes are `invalid_argument`, `invalid_state`, `not_found`,
`permission_denied`, `observation_unavailable`, `bridge_unavailable`,
`serialization_failed`, `verification_failed`, `unsupported`, `resource_limit`,
`persistence_failed`, and `internal`. `retryable`
means the identical request may reasonably succeed after transient external
state changes; it never grants permission to repeat a mutation automatically.

Typed observation/admission failures and instrumented mutation failures use `dustroute.error.v2` and additionally return
`failure.primary`, `failure.secondary`, `failure.progress` and `recovery`.
The progress distinguishes locally submitted changes, independently verified
steps and durably checkpointed verified steps. A missing count is JSON `null`,
not zero. A failed final save can coexist with a verified world result.

Legacy string-only failures retain v1 and expose `failure.progress: null`, with
unknown phase/cause where no explicit category survived. Never infer execution
facts from the human-readable message. See
[structured failure and recovery contracts](structured-failure-recovery.md).

`get_operation.ok` means the query succeeded; inspect the nested operation
status and result to determine execution success. A recorded `ok: false` result
is `failed`, even if the API call returned normally. Its progress percentage
uses verified steps only, and does not count transport submissions as success.

`get_operation` may additionally return `activity` (`dustroute.operation_activity.v1`)
while invoke/undo or asynchronous region analysis runs. It reports measured phase
and elapsed time, with nullable execution facts. Saved operation/history fields
remain historical; `activity.active` describes the current tracked request.
Submission, verification and durable checkpoints are separate. Activity is not
persisted, grants no replay authority and adds no mutation cancellation guarantee.
See [live progress and diagnostic scope](operation-diagnostics-progress.md).

## Coordinate-keyed state

JSON object keys cannot safely represent structured coordinates. Transition
strength and power maps are therefore arrays:

```json
{
  "final_strengths": [
    { "position": { "x": 1, "y": 64, "z": -2 }, "strength": 15 }
  ],
  "final_powered": [
    { "position": { "x": 1, "y": 64, "z": -2 }, "powered": true }
  ]
}
```

This representation is required even for empty state. It prevents non-string
map keys from reaching `serde_json` and keeps live and simulated traces in the
same shape.

In addition to the compatibility `events` array, transition-test responses
include a `transitions` array. Each entry has an opaque `id`, the observed
position, the before/after signal values when a before value is known, and
`elapsed_from_previous`. `same_tick` entries retain `order_delta`; an
`exact_ticks` entry is measured in the trace's declared redstone-tick unit.
The response also exposes `status` (`in_progress`, `complete`, or `failed`),
the declared `time_unit`, and exact live duration when available. Entries from
live recordings carry `game_tick` and `phase` when known, together with
`logical_elapsed_from_previous`; these fields preserve game-tick timing even
when the compatibility `redstone_tick` bucket is rounded. The first entry has
no elapsed value. Consumers should use this array when comparing state-changing
edges and use `events` when they need the original scenario sequence or
provenance fields.

Transition verification reports `steady_state_equivalent` separately from
`trace_equivalent`. Server/physics observation can place an otherwise immediate
dust update on either side of a redstone-tick sampling boundary; this remains a
visible trace difference without incorrectly claiming a final-state mismatch.

Trace events carry optional provenance fields in addition to tick and state:
`event_kind`, `cause`, `source`, and `cause_sequence`. A Mineflayer bridge
records `event_kind=state_transition`, `cause=packet_observation`, and
`source=live_mineflayer`; packet order is evidence, not the internal vanilla
scheduler cause. Provenance differences are intentionally excluded from
behavioral equivalence, while state, redstone tick, and within-tick order are
still compared. Exact game-tick or known-phase differences are reported
separately; an unknown phase does not become a false mismatch.

The native backend records `source=live_voxrig` and preserves a `native_packet`
boundary (connection ID, receive sequence, cell order). Its recording clock is
`client_frame20_hz`; the compatibility fields named `*_game_tick` carry that
explicitly declared local clock and must not be interpreted as server ticks.
Projected traces use `time_unit=client_tick` or `client_redstone_tick` and omit
server `game_tick` values. Cross-clock trace comparisons report
`time_unit_mismatch`; pulse-width contracts with a different clock remain
unconfirmed. Received state recordings exclude reconstructed piston frames and
hidden scheduler events. See [native evidence](voxrig-rollout.md).

Temporal IR responses additionally expose a game-tick `transition_delay` for
stateful edges and devices. It can be `same_game_tick`, an exact game-tick
value, a bounded game-tick range, or `unavailable`; clients must not infer an
immediate transition from an unavailable legacy redstone-tick scalar. Piston
motion remains preview-only until the target Minecraft version has a verified
start/completion trace. The lower-level Minecraft physics engine retains
`phase` and `sub_tick_order` for same-game-tick evidence, bounds zero-delay
chains with a per-tick microstep budget, and reports a structured failure when
a child would move back to an earlier phase. A failed event is requeued rather
than silently consumed; this is an implementation safety contract and does not
promote MCP piston operations beyond their current `PreviewOnly` status.
Truncated live recordings are returned with `status=failed`; their accepted
prefix is evidence only and must not satisfy an exact transition contract.
When a physics engine is driven from a bounded live snapshot, callers must
provide the same complete region through `PistonPlanningContext` (or
`PhysicsEngine::with_piston_planning_region`); an absent coordinate outside that
region is `unknown_space`, not an empty block.

## Mutation lifecycle

`new_*` creates a plan, `show_*` records preview, `invoke_*` requires explicit
confirmation, and `undo_*` or `restore_*` verifies recovery. An operation ID is
opaque. Clients must not invoke a plan belonging to another player or assume
that an expired ID can be recreated without observing the world again.

## Observed physical optimization

`new_optimization` accepts `wire_length` and `density_then_wire_length`.
It also accepts an explicit `contract`. Omitted categories use conservative
defaults, and the fully resolved contract is echoed in the response before any
world mutation. The contract separates these concerns:

- `logical`: exact steady-state truth-table preservation.
- `timing`: `exact_trace`, `exact_transitions`, `bounded_delay`, `settled_value_only`, or
  `preserve_order`; `exact_transitions` compares only state-changing edges and
  their observed transition times, while `exact_trace` compares every sampled
  output value. The default is bounded delay with at most five added
  redstone ticks and a 20-redstone-tick settling deadline.
- `pulse`: whether pulses may be introduced or removed and their maximum width
  change; the default permits neither and requires an exact width.
- `analog`: optional signal-strength preservation.
- `boundary`: preservation of physical boundary blocks, facing, and external
  driver positions.
- `mutation`: fixed focus, temporary expansion permission, maximum changed
  blocks, and automatic-apply permission. Automatic apply defaults to false.

Every response includes `contract_assessment` with a `passed`, `failed`, or
`unavailable` result for each category. `unavailable` is deliberately not a
pass. An operation whose contract is not completely satisfied may be inspected
as a proposal, but `invoke_operation` rejects it. This keeps an unmeasured
transition or pulse characteristic from being silently treated as preserved.
Each non-passing category also exposes stable `reason_codes`. Current codes
include `too_many_inputs`, `ambiguous_terminal_mapping`,
`unsupported_physics`, `logical_truth_table_mismatch`,
`interface_evidence_insufficient`, `transition_evidence_insufficient`,
`pulse_evidence_insufficient`,
`timing_contract_violated`, `new_pulse_introduced`,
`existing_pulse_removed`, `pulse_width_changed`, `analog_strength_changed`,
`boundary_structure_invalid`, and `mutation_limit_exceeded`. Human-readable
`reasons` remain explanatory and must not be used for control flow.
For simple wire-path optimization, DustRoute independently infers the original
and candidate terminal interfaces, compares their steady truth tables, and then
exhaustively simulates every ordered transition for up to four inputs. Inferred
terminal anchors may move inside the focus; comparison follows the comparable
terminal order while the explicit physical focus and endpoints remain fixed.
At application time, the MCP server rescans the preserved boundary records and
compares block identity plus static properties such as facing, delay, and mode.
Dynamic power, lit, locked, and dust-arm states are excluded from this physical
identity comparison. A mismatch causes rejection and rollback.

The optional `search` object bounds physical path exploration with
`max_expansions`, `max_candidates`, and `max_millis`. Results echo both the
resolved budget and measured `expansions`, `candidates`, `truncated`, and
`stop_reason`. Stable stop reasons are `max_expansions`, `max_candidates`, and
`time_budget`.

The latter reports a three-stage `phase_trace`: `local_density`,
`connector_recovery`, and `global_compaction`. Search may internally accept a
denser local candidate whose connectors are temporarily longer. Intermediate
candidates are never written to Minecraft. Only a final candidate whose
lexicographic `(bounding_volume, occupied_blocks, connector_length)` score is
better than the observed baseline can become a previewable operation.

The current observed-world candidate generator handles one non-branching
redstone-dust path with fixed endpoints inside an explicit focus. The phased
score selector is more general, but arbitrary component relocation is not yet
part of this API.

## Mechanism interpretation through existing observation tools

`show_region`, `test_circuit`, and `convert_from_circuit` include `mechanisms`.
A known stable piston layout has `kind: piston_door`, an exact contract match,
and an observed `state`. Other piston regions have
`kind: unidentified_piston_mechanism`, null state/contract, and candidate
assessments explaining why recognition was not established. This is observation,
not mutation authorization. The current recognizer checks the entire observed
region; it does not yet split arbitrary scenes into separate mechanisms.

`show_region` captures fresh blocks. Conversion with a `circuit_id` preserves
that snapshot; recapture explicitly for current state. No dedicated door-state
read endpoint is exposed. See [the candidate observation schema](piston-door-mcp-v1.md#observe-and-interpret).

`unsupported_observed_blocks` is an array of `{position, block}` records,
including an empty array when no unsupported blocks exist. Coordinate-keyed
objects could not serialize nonempty observations and have been replaced.

## Hypothetical revision contract

`test_circuit_change` accepts exactly one `circuit_id` or `revision_id`, plus
`changes` with full replacement `properties`. It returns a saved
`dustroute.circuit-revision.v1` record (`analysis_mode: virtual_circuit_revision`)
with `revision_id`, `parent_revision_ids`, `base_observation_id`, exact changes
and `validation.before` / `validation.after`. This replaces the old transient
`before`, `after`, and `steady_state_simulation` response fields. Read the same
record with `get_circuit_revision`; optional `include_snapshot` returns blocks.
Both endpoints are hypothetical-only. See [the revision contract](circuit-revisions.md).

Revision records may now include retained `base_snapshot` evidence.
`new_placement` accepts `revision_id` instead of a built-in `circuit` name and
returns a separately validated common placement operation. It requires current
world agreement with that evidence and shared placement validation; revision
IDs are still not operation IDs or live circuit IDs. Legacy revisions without
base evidence cannot be reflected.

`new_placement` also accepts `assembly_revision_id` as a separate alternative.
With `assembly_target: {source_anchor, target_anchor, rotation}`, a uniquely
adopted electrical Assembly can request fresh construction at new coordinates.
The complete target region must be observed empty, target-coordinate behavior
review must pass, and installation/teardown must settle in the physical model.
After preview, every applied step receives whole-region readback; undo requires
the exact constructed state. See [construction contract](custom-piston-assembly-placement.md).

Without `assembly_target`, the historical grounded route retains its contract:
The Assembly must be published by exactly one adopted update, and its immutable
ancestry must reach a complete Assembly captured from a Circuit Revision with a
retained literal base snapshot. Planning reruns the Blueprint review and the same
live-base, placement, preview, apply verification and undo checks. It preserves
the captured dimension and coordinates; it does not authorize relocation or
fresh construction.

## Shared diagnostic core

Circuit and Assembly diagnostics share `dustroute.diagnosis.v1`: method-tagged
findings, report-local finding IDs, repair assessment and optional pinned design
references. `new_repair` and `get_repair_context` use that same core to connect
candidate evidence with findings. See [diagnostic methods and Blueprint links](diagnostic-system.md).
Assembly assessment is now `repair`; method-specific step/reset details are in
`repair_details`. Executable reconstruction journals keep their existing name.

## Durable placed Assembly management

`manage_assembly` accepts `action: list | get | observe | diagnose | plan_removal | plan_reconstruction` and an
`instance_id` UUID for all actions except `list`. Construction returns this
instance UUID separately from temporary executable plans. `get` returns the
saved source pins, transform, expected snapshot, concrete execution context and
lifecycle/progress record; it explicitly reports `fresh_observation: false`.

Attempt records retain `readbacks` for the baseline and each verified step's
before/after observations. Fresh observations retain both `readbacks`,
`sample_interval_ticks: 20`, `sample_interval_clock: "client"`, and the measured
interval for that source. Mineflayer supplies `observed_server_tick_interval`;
Voxrig supplies `observed_client_tick_interval` and sets
`observed_server_tick_interval` to `null`. Each receipt retains its schema and
kind as described above. Receipts do not authorize reusing an old observation
or reconstruct hidden events. A changed source or native connection, reversed
observation boundary, motion or differing samples cannot establish a stable baseline.
See [observation backends](mcp-public-features.md#observation-backends) for
permissions and limits. Older otherwise-compatible saved attempts default
missing `readbacks` to an empty list; fresh scans must always supply their
selected source's evidence.

`observe` returns `observation.status`, independent `revalidation.status`, and
`removal_eligible`. Observation statuses are `matches`, `changed`,
`observation_incomplete`, `history_unavailable` and `target_mismatch`.
`ok: true` on an observation means the diagnostic request completed, not that
physical state matched or removal is permitted. A refusal by default `plan_removal`
returns `ok: false` with diagnostics. A successful plan returns a new
`operation_id` for `show_operation` and `invoke_operation(confirm=true)`.

`diagnose` returns `diagnosis` with the shared schema `dustroute.diagnosis.v1`,
reference mode, coordinate/property findings and a separate reconstruction
assessment. Unsupported damage can still be reported. It performs no world
writes and creates no operation; saving its observation invalidates older plans.
Normal current input levels inform the reference, with explicit historical or
initial-state fallbacks. See [diagnosis semantics and limits](assembly-diagnosis.md).

`plan_reconstruction` freshly models teardown of a supported observed layout and
rebuild to the declared initial state. It returns complete baseline/steps,
differences and operator conditions for explicit preview/confirmation. Matching
client samples do not prove empty server queues; no companion MOD is required.
Failed attempts are preserved; this is a new operation, never a blind retry.
The response retains a `diagnosis` even when reconstruction planning fails.

Registry records use schema `dustroute.placed-assembly.v5`, no TTL, and lifecycle
`needs_inspection | applied | removed`. A failure after an attempted write keeps
`needs_inspection`, the verified prefix and an error. Persisted records and old
observations never deserialize into execution permission. Retired v1–v4 records
are rejected without automatic upgrade; see the [cutover guide](architecture-cutover.md).
See the
[complete persistence and conditional-removal contract](placed-assembly-management.md).

### Removal after a completed operation

`manage_assembly(action="plan_removal", instance_id=...,
removal_reference="observed_inputs")` explicitly selects the same settled
reference as diagnosis. It requires exact whole-region agreement with a fresh
replay from the design and observed declared inputs. `constructed` is the
default; `observed_inputs` is rejected on other actions. Preview and execution retain
and revalidate the selected baseline and steps. See
[placed Assembly management](placed-assembly-management.md).

### Flying-machine generation

`test_circuit_change` accepts `blueprint.action="generate_flying_machine"` with a typed `request`, including `engine="slime_relay"` (default) or `engine="honey_direct"`. Engine definitions select geometry and endpoint requirements, never a different runtime. It returns `result.records`, `result.request`, declared geometry and fresh model checks without changing the catalog or Minecraft. Only a passed candidate has `ok: true`; failed/undetermined checks remain explicit. Import and explicit fresh adoption remain required. See [parameters](flying-machine-generation.md).
