# MCP JSON contracts

Reverse-analysis responses may include `physical_function_model` when
truth-table inference is requested. Its output functions are derived from the
shared physical network; `shared_physical_components` reports components that
depend on multiple inputs or influence multiple outputs. Local gate labels are
explanatory and do not exclusively own physical blocks. See
[`physical-function-model.md`](physical-function-model.md).

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
For simple wire-path optimization, DustRoute independently infers the original
and candidate terminal interfaces, compares their steady truth tables, and then
exhaustively simulates every ordered transition for up to four inputs. Inferred
terminal anchors may move inside the focus; comparison follows the comparable
terminal order while the explicit physical focus and endpoints remain fixed.

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
