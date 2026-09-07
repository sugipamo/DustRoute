# Fixed 1×2 piston door MCP contract

The current public mechanism is a fixed single-input 1×2 door on Java 1.21.11.
It supports construction, read-only recognition, normal-lever open/close, and
conditional removal. General piston placement and arbitrary door layouts remain
unsupported. The [3×3 diagnostic fixtures](piston-diagnostics.md) are separate.

## Exact layout

The embedded [contract](../crates/dustroute-mcp/src/piston_door_v1.json) retains
complete live block properties for open and closed states. The lower piston is
at origin `(0,0,0)`, the upper opposing piston at `(4,1,0)`, and the lever at
`(2,0,4)`. Select the complete guard `(-3,-2,-4)..(7,3,6)` relative to origin.

Frame, floor, roof, panel blocks, piston/head metadata, lever attachment and
dust shape/power must match. Translation is allowed; rotation, substitutions,
extra non-air blocks, duplicate coordinates and incomplete coverage are not.

## Observe and interpret

Use `show_region` to capture current selected blocks and a new `circuit_id`.
`test_circuit` and `convert_from_circuit` return mechanism interpretation through
the existing observation surface. Supplying an existing ID interprets that
immutable snapshot rather than refreshing it. `get_world` is the raw interface;
there is no dedicated door-state read tool.

Only an exact stable match is identified as `piston_door`. Other piston regions
remain `unidentified_piston_mechanism`; candidate assessments explain the mismatch
without claiming an arbitrary structure is a broken door. Recognition currently
assesses the whole selected region, not independently segmented mechanisms.

| Candidate observation | Meaning |
| --- | --- |
| `open`, `closed` | Exact full stable-state match |
| `moving` | A moving-piston marker at a contract movement cell; no inferred payload, progress or eventual completion |
| `indeterminate` | Mixed/intermediate evidence without a complete stable match; mismatched rows alone do not prove motion |
| `configuration_mismatch` | Block identity, property or required structure differs |
| `observation_incomplete` | Missing coverage/properties, invalid coordinates or unavailable observation |
| `unsupported_version` | Version differs from the fixed contract |

Reports include issue positions where available and a suggested next action.
They are not mutation capabilities. Configuration errors take precedence over
movement markers; raw observation does not infer block-entity internals.

## Build through the common placement API

Call `new_placement({"circuit":"piston-door-1x2"})`, then `show_operation` and
`invoke_operation(confirm=true)` after confirmation. No additional construction
endpoint exists. `max_blocks` applies; `optimize=true` is rejected.

The lower-piston origin is three blocks above the gaze target, keeping the full
guard above the targeted ground. The proposal returns anchor, origin, absolute
bounds, material counts, writes and undo writes. The entire guarded region must
be empty; collisions are not accepted.

A private non-deserializable `ValidatedDoorPlacement` checks the fixed template,
ordinary structural/state constraints and empty baseline. It does not construct
`ValidatedWorld` or change generic piston capabilities. Execution rechecks
owner, policy, version/dimension, preview and full guard under the mutation lock,
then consumes the attempt before writes. Construction uses the existing
command-based bridge, support-first and lever-last; bot operator rights are
required. After 30 ticks, the full open-state contract must match.

## Operate an existing door

1. Select and capture the full door/guard with `show_region`.
2. Call `new_piston_door_operation(circuit_id, target)` with `open` or `closed`.
3. Preview with `show_operation`, then execute with `invoke_operation(confirm=true)`.
4. The bot approaches before a fresh exact scan, checks the state against the
   proposal and consumes the attempt before normal lever activation.
5. If already at the target, return a verified no-op. Otherwise activate once,
   wait 30 ticks and require the complete target state to match.

Mutation policy still applies; default read-only mode denies execution. Plans
are bound to the configured player and dimension, are single-use, and expire
after five minutes before execution.

## Undo and uncertain outcomes

`undo_operation` can remove a successfully installed preset only when a fresh
scan exactly matches the original open layout. If closed, first create and run
a new ordinary open operation. Removal reverses placement order and verifies
that the full guard is empty. The external ground anchor is not removed.

Door activation itself has no undo: reobserve and create a new target-state
plan. Failed write/wait/verification leaves `needs_inspection`; no automatic retry
or rollback is allowed, even if a later scan happens to see the requested state.
Unknown partial builds require inspection; there is no arbitrary repair bypass.
Undo attempts are consumed before writing as well.

All placement/activation records are in memory and lost on MCP restart. Applied
construction records may be used for undo beyond proposal expiry while still
in memory. A saved revision or a matching observation does not recreate a lost
undo record.

## Validation and limits

- [Low-layer evidence](piston-low-layer-validation.md) establishes the bounded
  mechanical and single-input fixture progression.
- [MCP operation summary](../crates/dustroute-mcp/mineflayer/e2e/fixtures/piston-door-mcp-summary.json)
  records three close/open/close trials, fresh recognition, preview and stale-world gates.
- [MCP construction summary](../crates/dustroute-mcp/mineflayer/e2e/fixtures/piston-placement-mcp-summary.json)
  records three construction/recognition/operation/removal trials and cleanup.
- Synthetic tests cover incomplete/mixed/moving-marker snapshots, single-use
  plans, uncertain responses, read-only policy, version and exact-state checks.

Stable live state matches are not proof of exact tick equivalence, all pulse
widths, distant effects, entities, survival building or endurance behavior.
Reproduction procedures are in the [E2E guide](../crates/dustroute-mcp/mineflayer/e2e/README.md).
