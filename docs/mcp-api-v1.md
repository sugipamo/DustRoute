# MCP JSON contracts

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
| common error | `dustroute.error.v1` |

Adding an optional field is compatible within v1. Removing a field, changing
its meaning or type, or renaming an enum value requires a new schema version.
Coordinates are always objects with signed integer `x`, `y`, and `z` fields.

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
`serialization_failed`, `verification_failed`, and `internal`. `retryable`
means the identical request may reasonably succeed after transient external
state changes; it never grants permission to repeat a mutation automatically.

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

Transition verification reports `steady_state_equivalent` separately from
`trace_equivalent`. Server/physics observation can place an otherwise immediate
dust update on either side of a redstone-tick sampling boundary; this remains a
visible trace difference without incorrectly claiming a final-state mismatch.

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
- `timing`: `exact_trace`, `bounded_delay`, `settled_value_only`, or
  `preserve_order`; the default is bounded delay with at most five added
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
