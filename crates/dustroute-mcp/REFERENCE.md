# MCP subsystem reference

Detailed behavior and examples for the currently exposed subsystems. Start with the [LLM guide](README.md) for tool selection and the [setup guide](SETUP.md) for configuration.

## Tool naming and profiles

MCP tool names follow a PowerShell-style Verb-Noun contract written as
`snake_case`. The supported verbs are deliberately small:

- `get` retrieves observations, saved data, or operation results; it does not always rescan the world.
- `resolve` grounds a reference such as player gaze to a circuit.
- `test` diagnoses a circuit or saves and evaluates a hypothetical revision.
- `convert_from` reverse-translates Minecraft physics into IR views.
- `new` creates a non-mutating plan.
- `show` renders a plan or selection without applying it.
- `invoke` performs a confirmed world mutation.
- `undo` restores supported operations after checking their current state; it also routes transition-test restoration. There is no public `restore_*` tool.
- `start`, `stop`, and `get` manage asynchronous operations.
- `set` and `clear` manage the current region selection.

`DUSTROUTE_MCP_TOOL_PROFILE=default` exposes the 21 tools intended for normal
LLM collaboration. `debug` additionally exposes low-level gaze/discovery,
full placement-plan retrieval, asynchronous conversion control, and
explicit component-removal planning. `get_operation` is available in the default
profile. Debug tools remain implemented but cannot
be listed or called in the default profile. Old pre-convention tool names are
not retained as aliases.

High-level JSON response versions, compatible-change rules, stable error codes,
and coordinate-state representations are documented in
[`../../docs/mcp-api-v1.md`](../../docs/mcp-api-v1.md).

Set `DUSTROUTE_BOT_BRIDGE` to override the local bridge address. Natural-language
references such as “what is this?” use `convert_from_circuit`. One call
returns the focused physical component, mixed-IR summary, optional
whole-circuit function candidates, observation completeness, and diagnostics.
Repair and transition plans are separate: use `new_repair` or
`new_transition_test` only after interpretation requires them. Circuit reads
return a TTL-bound immutable `circuit_id`. Reuse that ID for later analysis,
virtual changes, repair planning, and transition planning so moving the
player's gaze cannot silently change the target.
Set `include_truth_table=true` when an exhaustive functional result is needed;
local hierarchical inspection remains the default. The request is bounded by
`truth_table_max_inputs`, `truth_table_settle_ticks`,
`truth_table_max_rows`, `truth_table_max_work_units`,
`truth_table_max_solver_iterations`, and `truth_table_max_elapsed_millis`.
The latter two cap cumulative fixed-point solver iterations and elapsed time
in addition to the static work estimate. The same bounded
request is honored for large circuits instead of being discarded solely because
the circuit crosses the hierarchical-display threshold. Responses expose
`truth_table_status` as `computed`, `budget_exceeded`, `unavailable`, or
`not_requested`; computed responses also include `truth_table_semantics`
(`combinational`, `timing_sensitive`, `stateful`, or `unknown`). Explicit
regions continue to use two `set_region` calls and
`show_region`, then reuse the returned `circuit_id`. Debug clients may call
`resolve_looked_at_circuit` directly.

For observation debugging, `get_world` starts near the block the
player is looking at and progressively follows adjacent redstone components
without applying circuit inference. There is no caller-selected scan radius.
Expansion ends when the component frontier is exhausted or `max_components`
(8192 by default) is reached; the latter is reported as an incomplete result.
`component_gap` defaults to 2 so a one-block break can be inspected as a nearby
fragment. The result includes exact block-name and block-state-property counts,
the targeted block, raw redstone states, truncation, and expansion completeness.
Use `include_block_list=true` only when the non-air listing is needed; both raw
lists are bounded by `max_listed_blocks`. `resolve_looked_at_circuit` and
`convert_from_circuit` use the same component-limited expansion.

`convert_from_circuit` reports a physical-first hierarchy. Observed facts
become a directed physical graph, recognized local cells, traceable logic
expressions, and finally optional functional candidates. Every stage reports
its own completeness and unresolved count while retaining physical component
origins. Without `include_truth_table`, large circuits return the bounded
hierarchical summary and report why exhaustive inference was skipped. With the
flag enabled, the same circuit is sent through bounded functional inference;
an over-budget request returns `truth_table_status=budget_exceeded` rather than
running without a limit. Static, runtime-iteration, and elapsed-time limits
all return structured budget details; any rows accumulated before the limit
are discarded rather than exposed as a complete table. A component-limited snapshot returns
`truth_table_status=unavailable` with an `incomplete_observation` error until
the circuit is expanded. Call `get_circuit_ir` with `circuit_id` to obtain its
`analysis_id`, then pass all three of `circuit_id`, `analysis_id`, and `node_id`
to expand only one region or logic cell.
Rows also require two consecutive unchanged electrical snapshots with no
queued device event. A window that ends earlier returns
`truth_table_error_details.code=non_settling` instead of claiming a settled
functional result.

`signal_liveness` is evaluated independently of physical fragments. It follows
directed signal edges while preserving whether a source is a controllable
input, intrinsic source, observation boundary, or inferred primary input.
Repeater, comparator, piston, and torch-control inputs are classified as driven,
awaiting an external input, disconnected, or lacking a known source. An
inferred bare input is therefore reported separately from a genuine fault and
is not used by itself as evidence for an automatic repair. This catches
directional failures that Union-Find connectivity alone cannot detect without
shorting independent inputs together.

For conversational entry points, call `test_circuit` first. It is
a read-only fast path: it discovers the connected components around the
player's gaze, skips truth-table inference, repair enumeration, and transition
scenario generation, and returns `dustroute.diagnostic.v1`. The response places
an immutable `circuit_id`,
`diagnostic.health`, typed `diagnostic.counts`, ranked `diagnostic.findings`, and
one `diagnostic.recommended_next_action` ahead of detailed evidence. Use
`convert_from_circuit` only when higher-level logical interpretation is needed.
Use `test_circuit_change` to save an immutable hypothetical revision, starting
from `circuit_id` or branching from `revision_id`. It supports additions,
deletions (`minecraft:air`), and full block-state replacements. Read saved
versions with `get_circuit_revision`; revision IDs are never live circuit IDs.
See [circuit revisions](../../docs/circuit-revisions.md) for limits, validation,
retention and examples.

Both `test_circuit` and the flat `convert_from_circuit` response also expose
`focused_explanation`. It contains the physical block identity, local role,
directed incoming/outgoing edges, mapped input/output candidates, bounded paths
from inputs to the focus and from the focus to outputs, temporal requirements,
nearby temporal devices, and explicit caveats. The hierarchical large-circuit
path keeps the same shape but leaves terminal/path arrays empty and states that
flat inference was skipped; this is an intentional bounded result, not a
claim that no terminals exist.

Example response shape:

```json
{
  "schema_version": "dustroute.diagnostic.v1",
  "analysis_mode": "focused_fast",
  "mutation_performed": false,
  "diagnostic": {
    "health": "degraded",
    "observation_complete": true,
    "counts": {
      "healthy": 32,
      "awaiting_external_input": 3,
      "probable_faults": 1,
      "unsupported": 0,
      "incomplete_observation": 0
    },
    "recommended_next_action": {
      "kind": "inspect_fault",
      "position": { "x": 45, "y": 104, "z": 8 },
      "requires_confirmation": false
    }
  }
}
```

Analysis responses preserve the focused block's original namespaced identifier
and complete block-state map. `block_capabilities` groups every observed circuit
component by block identity and reports where physical classification,
connectivity, steady-state semantics, temporal semantics, repair, or placement
is only partial or unsupported. This lets an MCP client distinguish a complete
scan from a complete interpretation and present unsupported behavior as an
explicit limitation instead of guessing.

Reverse analysis is fail-closed at the interface boundary. Live-only blocks
(`target`, daylight detectors, containers, sensors, fluids, and rails) are
retained in `unsupported_observed_blocks` and never treated as a simulated
solid. Observers are represented as directional state-transition pulse sources
in the block-only simulator; exact live update ordering remains a preview
concern. The response's `interface_evidence` lists physical external
inputs and observable sinks together with their mapped and unmapped positions.
Truth-table and optimization contracts are unavailable when that evidence is
incomplete or ambiguous, when no input/output terminal exists, or when no
transition case was actually exercised. A `Passed` state with zero cases is
therefore not proof of equivalence.

Input mutations are typed: levers use `set_lever_state`, buttons use
press/release actions, pressure plates use an explicit level from 0 through 15,
and open boundaries use an external-power action. The legacy `SetPowered`
scenario action remains only for fixture compatibility and rejects wires or
arbitrary blocks instead of silently replacing them.

The `temporal` result reports a lossless timed graph, a traceable steady-state
projection summary, and whether higher-level logic is `steady_state_safe`,
`timing_sensitive`, or `temporal_required`. Repeater delays use redstone ticks
(two game ticks each). A steady-state label remains useful for delayed paths,
but MCP clients must present it as provisional when unequal-delay paths
reconverge and must not treat feedback or mechanical devices as purely
combinational logic. The temporal projection also exposes a
game-tick-based `transition_delay`: `same_game_tick` is an ordered zero-delay
transition, while `game_tick_range` and `unavailable` preserve variable or
unmeasured timing without rounding it into a redstone tick. Piston motion is
currently `unavailable` at this IR boundary and remains preview-only until a
1.21.11 start/completion trace is verified. Basic repeater locking, comparator
analog behavior, and Observer pulses are simulated; exact same-tick update
order remains an explicit live-trace result. The Minecraft physics layer
retains a coarse event `phase` and deterministic `sub_tick_order`, rejects a
zero-delay phase reversal, and bounds same-tick chains with a microstep budget.
Rejected physics events stay queued; this is fail-closed bookkeeping and not a
claim that MCP can currently execute piston, QC/BUD, or formal 0-tick actions.
Low-level callers that derive a physics world from a bounded live scan must
pass that complete boundary to `PistonPlanningContext` or the engine's
`with_piston_planning_region` builder; outside coordinates remain Unknown.

Transient output distinguishes structural risk from measured behavior. A
hierarchical scan reports `not_simulated` until transition scenarios have run.
Trace evaluation reports pulse polarity, redstone-tick width, surrounding
steady value, and exact source-event indices. With no registered signal intent,
an observed deviation is only a `hazard_candidate`; `hazard_confirmed` is
reserved for an explicit stability or pulse-width contract violation, while a
matching pulse contract becomes `intentional_pulse`.

The intended conversational workflow is:

```text
test_circuit -> capture circuit_id
  -> reuse circuit_id with convert_from_circuit or get_circuit_ir
  -> explain physical evidence, local role, and provisional higher roles
  -> call new_repair(circuit_id), new_transition_test(circuit_id), or new_placement when needed
  -> show_operation to preview the exact region or block diff
  -> obtain explicit player confirmation
  -> invoke_operation and restore/verify
  -> undo_operation when recovery is needed
  -> report the post-operation re-analysis
```

In the debug profile, long conversions can use
`start_selected_region_conversion`, `get_operation`, and `stop_operation`.
The asynchronous conversion accepts the same bounded truth-table options as
`convert_from_circuit`, so an explicit large-circuit request can run without
blocking the MCP request while still stopping at its row/work budget.
`new_placement` returns a block diff, collisions,
material counts, an operation UUID, and an exact undo plan without changing the
world. Set its optional `optimize=true` argument to run X-axis directional
compression followed by global compaction and rerouting. The response includes
per-phase score changes and a safety classification. Current built-in circuits
contain scheduled-tick components, so optimized results are normally
`preview_only` and require explicit confirmation. Rejected candidates do not
produce a placement plan. When `DUSTROUTE_READ_ONLY=false`, an explicitly
confirmed plan can be written with `invoke_operation`; the server first
checks that the preview baseline is still current and then verifies the live
world after writing. `undo_operation` performs the same checks while
restoring the captured blocks.

## Transition scenarios

Live pulse observation uses the visible Mineflayer bot as an actuator and
sensor; Rust remains responsible for scenario policy, interpretation, and
restoration. The initial workflow supports one normal lever activation at a
time:

```text
test_circuit -> capture circuit_id
  -> new_transition_test(circuit_id)
  -> show_operation
  -> explicit player confirmation
  -> invoke_operation(confirm=true)
  -> block-update trace, transient assessment, and Rust-simulator comparison
  -> automatic lever and region restoration verification
  -> undo_operation(confirm=true), if recovery is needed
```

The bridge uses Mineflayer's normal block activation rather than changing a
`powered` state with `/setblock`. The bot must be within 5.5 blocks of the
lever. Observations record packet-visible block updates with sequence numbers,
a Mineflayer physics-tick clock, and `sub_tick_order` within that clock tick.
Conversion rounds these observations into the internal simulator's
redstone-tick unit while retaining within-tick order as separate evidence.
That order is a causal clue, not an exact vanilla scheduler trace. Runs are
bounded to 200 game ticks and 65,536 events.
TNT, fire, water, and lava reject a scenario. Pistons, observers,
containers with activation behavior, and unsupported sensors remain
preview-only. Every run captures the original snapshot and reports failure
unless both the lever state and full region are restored.
Stateful devices such as locked repeaters may retain the post-test state even
after the lever is returned. In that case the run reports restoration failure.
An explicit `undo_operation(confirm=true)` first retries the natural
reverse operation, then reapplies the bounded pre-test block states only when
the region still differs, and verifies the complete region again.
`scenario_verification` contains the normalized live trace, simulated trace,
typed differences, and an `equivalent` flag. The response also includes a
transition-first `transitions` array: each state-changing event has an opaque
ID, before/after signal values when available, and elapsed ordering. A
`same_tick` elapsed value retains its within-tick order delta; a later
`exact_ticks` value is measured in the trace's redstone-tick unit. Same-tick
ordering differences are retained rather than silently treated as electrical
mismatches.
Each trace event additionally carries `event_kind`, `cause`, `source`, and an
optional `cause_sequence`. Live Mineflayer events use
`cause=packet_observation` and `source=live_mineflayer`; they do not claim to
know the vanilla scheduler cause. Event provenance is explanatory metadata and
does not by itself make an otherwise matching live/simulated trace unequal.

## Physical repair workflow

After capturing a gaze circuit with `test_circuit`, or selecting and previewing
a region with `show_region`, pass its `circuit_id` to `new_repair`. It ranks partial physical
patches for missing wire, missing support, and directional component problems.
Each proposal includes coordinates, evidence, confidence, a virtual before/after
impact, and an operation UUID. Virtual impact includes traversal-group and
support compatibility metrics, directed liveness changes, electrical solver
convergence, energized-position counts, and whether temporal validation is
required. A liveness bridge may therefore find a directional break even when
Union-Find reports one physical traversal group. The safe mutation sequence is:

Repair application uses Mineflayer's normal player `placeBlock`/dig behavior
rather than `/setblock`. The visible bot enters creative mode, moves above each
target, places against the recorded support face so Minecraft computes neighbor
updates and block shape, then retreats 16 blocks above the repaired area and
hovers. Post-write block-state and circuit verification still run normally.

```text
new_repair(circuit_id)
  -> get_repair_context(circuit_id, operation_id)
  -> compare supporting evidence, contradictions, and questions with the player
  -> show_operation
  -> explicit player confirmation
  -> invoke_operation(confirm=true)
  -> automatic block-state rescan and circuit re-analysis
  -> undo_operation(confirm=true), when needed
```

`get_repair_context` is read-only and progressively expands one repair
hypothesis. It returns bounded physical facts, competing interpretations,
counterfactual impact, nearby directed components, and questions that can
distinguish an intentional external input from a broken path. It does not
preview or authorize the operation.

Failed block-state verification triggers an automatic rollback attempt. A
successful application returns the resulting logical classification and, when
the original analysis included a truth table, an explicit before/after semantic
comparison. This comparison describes whether behavior changed relative to the
observed pre-repair circuit; it does not by itself prove the user's intended
function.
A suspected short cannot be inferred safely from geometry alone;
Debug-only `new_component_removal_plan` is available only for a component the
player explicitly identifies while looking at it.

## Observed physical optimization

`new_optimization` creates a reversible optimization plan from an immutable
observed `circuit_id`. The first supported objective is `wire_length`, limited
to one supported, non-branching dust path inside an explicit focus. Both path
endpoints and every block outside the focus remain fixed. Candidate generation
rejects branches, missing support, occupied targets, new redstone adjacency,
and paths that are not shorter.

Before returning a plan, DustRoute re-analyzes the virtual result, requires the
diagnostic and temporal classifications not to worsen, and rejects a differing
inferred truth table when both sides can be enumerated. An unavailable truth
table is reported explicitly rather than treated as proof. Application uses the
same `show_operation` / explicit confirmation / `invoke_operation` /
`undo_operation` lifecycle as repairs.

### Exact 1x2 piston door operation

`new_piston_door_operation(circuit_id, target)` supports an already built
`piston_door_v1` only, on Java 1.21.11. `target` is `open` or `closed`.
Capture the complete selected region using `set_region` twice and
`show_region`, then call `new_piston_door_operation`, `show_operation`, and
`invoke_operation(confirm=true)`. The bot uses normal lever activation and
leaves the door in the requested state; this is not a transition test that
restores the lever afterward.

The exact frame, two pistons, two stone panel blocks, single lever, supported
dust wiring (including connection properties), and an empty guard must match
the versioned contract. Relative to the lower piston at `(0,0,0)`, select
`(-3,-2,-4)..(7,3,6)`. Translation is supported; rotation, changed wiring,
and general piston placement are not. The fixed open-state preset can be built
with `new_placement({"circuit":"piston-door-1x2"})`, then the same
`show_operation` / `invoke_operation` flow. It requires an empty guard, places
the origin three blocks above the gaze target, and rejects optimization.
`undo_operation` removes it only after exact open-state verification. See
[the operation contract](../../docs/piston-door-mcp-v1.md).

Existing observation tools return `mechanisms`: `show_region` captures current
selected blocks, `test_circuit` provides compact interpretation, and
`convert_from_circuit` interprets the immutable snapshot. Exact known layouts
are recognized; other piston structures remain unidentified. See
[the observation contract](../../docs/piston-door-mcp-v1.md#observe-and-interpret).


`new_placement({"revision_id":"..."})` plans a saved revision's cumulative
changes at their original coordinates. It requires a retained complete base
observation, exact current-world agreement, supported placement, and lossless
state export. Use the same `show_operation`, `invoke_operation`, and
`undo_operation` tools. It never treats a revision as current-world evidence.
See [revision placement](../../docs/circuit-revisions.md#reflecting-a-revision-through-existing-placement-tools).
