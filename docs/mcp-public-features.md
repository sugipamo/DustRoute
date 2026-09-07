# Public MCP feature guide

Start here for supported workflows, public tools, ID lifetimes and recovery.
[Setup](../crates/dustroute-mcp/SETUP.md) is for operators;
[the LLM guide](../crates/dustroute-mcp/README.md) describes tool selection;
[JSON contracts](mcp-api-v1.md) describe responses.

The current workflow is observation → hypothetical revision → validation →
placement proposal → preview → confirmed live change and verification. The
fixed 1×2 piston preset also supports construction, recognition, open/close and
removal. This is not unrestricted autonomous circuit design or fault recovery.

## Choose a workflow

| Intent | Entry and continuation |
| --- | --- |
| Inspect the gaze target | `test_circuit`, then `convert_from_circuit` or `get_circuit_ir` using the returned ID |
| Inspect a selected area | `set_region` twice, then `show_region` to capture current blocks |
| Create or branch a hypothetical circuit | `test_circuit_change(circuit_id)` or `test_circuit_change(revision_id)`; read with `get_circuit_revision` |
| Reflect a revision | `new_placement(revision_id)`; requires retained base evidence and fresh live validation |
| Install a built-in circuit | `new_placement(circuit)` |
| Repair | `new_repair`; use `get_repair_context` to resolve competing explanations |
| Optimize | `new_optimization`, or `new_macro_optimization` for a returned compatible candidate |
| Operate an existing fixed door | Observe, then `new_piston_door_operation(circuit_id, target)` |
| Test a supported live transition | `new_transition_test`; review its restoration behavior |

Plans use `show_operation` → review/confirmation → `invoke_operation(confirm=true)`.
Use `get_operation` for results and `undo_operation` only for supported recovery.
Creating a plan does not write blocks. Default policy is read-only; observations
may still move the bot or render region previews.

## Default tools (21)

| API | Purpose |
| --- | --- |
| `get_bot_status` | Connection, configured player and policy |
| `get_world` | Raw bounded physical observation |
| `set_region` | Select first/second corners using gaze |
| `show_region` | Preview selection and capture fresh `circuit_id` plus mechanisms |
| `clear_region` | Clear selection, not world blocks |
| `test_circuit` | Compact diagnosis and local interpretation |
| `convert_from_circuit` | Physical/logical interpretation, capabilities and mechanisms |
| `get_circuit_ir` | IR summary and analysis-scoped node expansion |
| `test_circuit_change` | Save a hypothetical revision with edits and validation |
| `get_circuit_revision` | Read stored lineage, diff, validation and optional full snapshot |
| `new_placement` | Plan either built-in construction or a cumulative revision diff |
| `new_repair` | Rank repair proposals |
| `get_repair_context` | Evidence and questions for ambiguous repair intent |
| `new_optimization` | Plan supported physical wire-path optimization |
| `new_macro_optimization` | Plan a compatible, verified macro candidate replacement |
| `new_piston_door_operation` | Plan `open`/`closed` for the exact existing 1×2 contract |
| `new_transition_test` | Plan a supported live transition scenario |
| `show_operation` | Review the concrete operation |
| `invoke_operation` | Execute with confirmation and live checks |
| `undo_operation` | Restore where the operation kind supports it |
| `get_operation` | Retrieve operation status/results; not a permanent history archive |

## Additional debug tools (7)

`DUSTROUTE_MCP_TOOL_PROFILE=debug` exposes 28 tools in total.

| API | Purpose |
| --- | --- |
| `get_visible_player` | Inspect players tracked by the bot |
| `get_player_gaze` | Low-level gaze observation |
| `resolve_looked_at_circuit` | Connected-region discovery and selection candidate |
| `get_circuit_placement` | Detailed placement/undo plan |
| `new_component_removal_plan` | Explicit component-removal proposal |
| `start_selected_region_conversion` | Start asynchronous selected-region conversion |
| `stop_operation` | Stop supported asynchronous work; does not undo world changes |

Internal methods such as `invoke_repair` are not separate public endpoints.
`get_piston_door_state` is not exposed: mechanism interpretation belongs to the
existing observation tools. `get_operation` is available in the default profile.

## IDs and retention

| ID or record | Meaning and lifetime |
| --- | --- |
| `circuit_id` | Immutable observed snapshot; in-memory, 15 minutes, maximum 64 records with earlier eviction possible |
| `revision_id` | Immutable hypothetical snapshot; scoped state store, default one-hour TTL, survives restart with the same scope |
| `parent_revision_ids` | Zero/one parent today; sibling children represent branches, not a merge |
| `base_observation_id` | Original observation reference; new revisions retain its snapshot independently |
| `analysis_id`, `node_id`, `component_id` | Scoped to the originating observation/analysis; do not mix IDs from other results |
| `operation_id` | A particular plan/execution record; lifetime depends on operation kind |

Saved observations are not automatically refreshed. A revision is not live
world evidence or direct execution permission. `new_placement(revision_id)`
creates a separately checked operation.

`DUSTROUTE_STATE_DIR` selects saved-state storage; `DUSTROUTE_PLAN_TTL_SECONDS`
controls its default one-hour retention. Revision reads do not extend expiry.
A child keeps its own snapshot even if a parent expires. This is working
storage, not a permanent revision archive.

## Execution and recovery

| Operation kind | Retention and recovery |
| --- | --- |
| General built-in placement | In-memory, no dedicated five-minute expiry; undo checks and restores captured blocks |
| Revision placement | In-memory, five-minute pre-apply expiry; exact region/context checks before apply and undo; write attempts are consumed |
| Fixed 1×2 construction | In-memory, five-minute pre-apply expiry; removal requires the exact original open layout |
| Fixed 1×2 activation | In-memory, five minutes, single-use; no undo; reobserve and create a new target-state plan |
| Repair/optimization | Disk state plus process cache; disk TTL is not a strict execution deadline because the cache can be used as fallback |
| Transition test | In-memory; inspect its restoration checks and actual result |

Most operation/undo records are lost on MCP restart, independently of persisted
revisions. After uncertain writes or failed verification, reobserve first.
Some errors return only `ok: false` and a message; absence of `needs_inspection`
does not prove nothing was written. Revision placement and fixed piston actions
do not automatically retry or roll back consumed attempts. Do not assume other
operation kinds have identical recovery guarantees.

## Supported scope

- Built-ins: half-adder, half-subtractor, MUX, decoder, full-adder and fixed
  `piston-door-1x2`. The door requires an empty guarded site; its origin is three
  blocks above the gaze target. Only translation is supported.
- Mechanism recognition identifies the exact known 1×2 layout. Other piston
  structures remain unidentified. Arbitrary multi-mechanism segmentation is absent.
- Revisions support addition, deletion and full property replacement: 64 edits,
  4096 block records/result blocks, 4 MiB per saved record, 1–256 simulation ticks,
  and no expansion beyond original observation bounds.
- Invalid drafts remain editable. Structural checks and initial-state simulation
  do not prove all-input behavior or exhaustive Java property validity.
- Revision placement checks the base and one-block context, shared placement
  legality and lossless export. It does not allow general piston placement or
  certify distant circuit effects.
- Existing wire optimization is limited to a non-branching dust path with fixed
  endpoints; macro replacement requires a verified compatible candidate.
- Placement uses command writes, not survival inventory gathering/construction.
- Merge, entity handling, long-running endurance optimization and arbitrary
  fully autonomous design are outside the current scope.

## Detailed contracts and evidence

See the [documentation index](README.md), particularly
[revisions](circuit-revisions.md), [fixed piston operations](piston-door-mcp-v1.md),
[placement validation](world-validation-boundary.md), and
[diagnostic piston fixtures](piston-diagnostics.md).

The latest integrated check passed 470 Rust tests, four Node tests, formatting
and Clippy. Fixed 1×2 construction/operation/removal and cumulative revision
apply/undo each have three-trial Java 1.21.11 evidence. Evidence files describe
the tested binary and scope; they are not a claim about every possible circuit.
