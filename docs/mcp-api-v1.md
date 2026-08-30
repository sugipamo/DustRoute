# MCP JSON contracts

DustRoute MCP tool results are JSON text. Versioned high-level response
families use these identifiers:

| Family | `schema_version` |
| --- | --- |
| focused diagnostic | `dustroute.diagnostic.v1` |
| placement mutation | `dustroute.placement.v1` |
| repair plan and mutation | `dustroute.repair.v1` |
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

## Mutation lifecycle

`new_*` creates a plan, `show_*` records preview, `invoke_*` requires explicit
confirmation, and `undo_*` or `restore_*` verifies recovery. An operation ID is
opaque. Clients must not invoke a plan belonging to another player or assume
that an expired ID can be recreated without observing the world again.
