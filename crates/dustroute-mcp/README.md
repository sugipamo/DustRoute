# DustRoute MCP — guide for LLM clients

Use DustRoute to observe Minecraft redstone, explain evidence, create hypothetical revisions, and propose verified world changes. Ground each task in the configured player's gaze or an explicitly selected region. Keep observation, hypothesis and execution separate.

This is the tool-use guide. Server installation, credentials, permissions and transport configuration belong in [SETUP.md](SETUP.md). Detailed subsystem examples are in [REFERENCE.md](REFERENCE.md); the complete 21-tool default inventory and 7 debug additions are in the [public feature guide](../../docs/mcp-public-features.md). Use the connected server's tool schemas for exact arguments.

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
| Place a saved revision | `new_placement(revision_id)`. A new live comparison and placement proof are required. |
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
| `base_observation_id` | Original observation reference | New revisions retain its snapshot separately for later live comparison, even when the original ID expires. |
| `analysis_id`, `node_id`, `component_id` | Identifiers scoped to an analysis | Use only with the observation/analysis that returned them. |
| `operation_id` | A particular proposal or execution record | Use the common operation lifecycle. Do not substitute a revision ID. |

A `circuit_id` is held in memory for 15 minutes and may be evicted at the 64-record limit. Revisions use the scoped state store, default TTL one hour, and survive restart under the same state scope. Reads do not extend expiry. Most operation plans are in memory and are lost on restart. Revision placement, fixed-door placement and door activation proposals have five-minute pre-execution lifetimes; do not assume all other operation kinds share this TTL. See the [lifetime and recovery tables](../../docs/mcp-public-features.md#idと保存の違い) for details.

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

Limits: 64 edits per request, 4096 block records/result blocks, 4 MiB per saved record, and 1–256 simulation ticks (default 64). Simulation checks initial-state behavior only. Missing Java property coverage, incomplete observations or unsupported mechanics are not a successful verification. Do not claim functional equivalence or all-input correctness from `structurally_valid`.

## Propose and execute a change

1. Create a plan for the grounded source. For a revision, call `new_placement({"revision_id":"…"})`; do not combine it with `circuit` or optimization.
2. Read the returned diff, bounds, verification and limitations. If rejected, explain the reason and return to observation or revision editing.
3. Call `show_operation` with the returned operation ID. Explain the concrete change to the user and obtain the required confirmation before execution.
4. Call `invoke_operation` with that ID and `confirm: true` only when authorized. Default policy is read-only; do not treat a proposal as permission to override it.
5. Read the execution result and live verification. A successful tool transport or a submitted write is not sufficient evidence that the intended blocks are present.

Revision placement uses the cumulative diff from the original observation, not just the last parent edit. It rescans the original region plus one block of context, requires exact base agreement, and reruns the shared placement checks. Changed states must export without losing properties. Powered states or unspecified properties that the exporter would reset can be rejected even when a draft was saved successfully. Older revisions without a retained base snapshot cannot be placed.

Do not switch a failed revision proposal to raw writes, a different gaze target, or another operation family to bypass its validation. Placement is currently command-based and uses the configured bot's privileges; it is not survival inventory construction.

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
- [Fixed piston contract and live evidence](../../docs/piston-door-mcp-v1.md)
- [Response schemas and compatibility](../../docs/mcp-api-v1.md)
- [Detailed repair, transition and optimization reference](REFERENCE.md)
- [Human setup and policy configuration](SETUP.md)
- [Live integration procedures](mineflayer/e2e/README.md)
