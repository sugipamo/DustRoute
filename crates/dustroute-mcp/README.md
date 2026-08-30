# dustroute-mcp

This crate is the primary AI-facing entry point for DustRoute. It exposes MCP
tools over stdio and talks to a visible Mineflayer player through a localhost
JSON-lines bridge.

## Start the visible bot

```bash
cd crates/dustroute-mcp/mineflayer
npm install
DUSTROUTE_SERVER_ADDRESS=127.0.0.1:25565 \
  DUSTROUTE_MC_VERSION=1.21.11 npm start
```

The default bot name is `DustRouteBot`. For offline test servers, grant it
operator permission if region previews should use particles and chat messages.
The bridge listens only on `127.0.0.1:25580`.

Automated live testing with a second Mineflayer player is documented in
[`mineflayer/e2e/README.md`](mineflayer/e2e/README.md). It controls player gaze
and exercises observation, diagnostics, component limits, repair application,
verification, and undo without requiring a human to enter the world.

## Start the MCP server

Configure an MCP client to launch:

```bash
DUSTROUTE_SERVER_ADDRESS=127.0.0.1:25565 \
  DUSTROUTE_ASSIST_PLAYER=YourMinecraftName \
  cargo run -p dustroute-mcp
```

`DUSTROUTE_SERVER_ADDRESS` and `DUSTROUTE_ASSIST_PLAYER` are required. The same
server address must be supplied to the Mineflayer process. MCP tools use the
configured player automatically, so callers normally omit their optional
`player` argument. An attempt to override it with another player is rejected.
If that player is online but outside the bot's entity-tracking range, gaze tools
and debug-only `get_visible_player` move only `DustRouteBot` to the configured player,
wait for tracking to resume, and retry once. The observation reports
`reacquired=true` when this happens. Player names are validated before a
teleport command is issued; an offline player remains an explicit error.

## Tool naming and profiles

MCP tool names follow a PowerShell-style Verb-Noun contract written as
`snake_case`. The supported verbs are deliberately small:

- `get` retrieves current state.
- `resolve` grounds a reference such as player gaze to a circuit.
- `test` judges health or consistency.
- `convert_from` reverse-translates Minecraft physics into IR views.
- `new` creates a non-mutating plan.
- `show` renders a plan or selection without applying it.
- `invoke` performs a confirmed world mutation.
- `undo` returns the immediately changed resource to its prior state.
- `restore` returns a test resource to its captured baseline.
- `start`, `stop`, and `get` manage asynchronous operations.
- `set` and `clear` manage the current region selection.

`DUSTROUTE_MCP_TOOL_PROFILE=default` exposes the 20 tools intended for normal
LLM collaboration. `debug` additionally exposes low-level gaze/discovery,
operation history, full plan retrieval, asynchronous conversion control, and
explicit component-removal planning. Debug tools remain implemented but cannot
be listed or called in the default profile. Old pre-convention tool names are
not retained as aliases.

Set `DUSTROUTE_BOT_BRIDGE` to override the local bridge address. Natural-language
references such as “what is this?” use `convert_from_looked_at_circuit`. One call
returns the focused physical component, its local signal role, recognized
AND/OR/NOT-style gates, traceable expressions, optional whole-circuit function
candidates, observation completeness, diagnostics, and non-mutating repairs.
The same response also returns safe lever-transition candidates, so the client
can continue from explanation to validation without rediscovering or rescanning
the circuit.
Set `include_truth_table=true` only when a small circuit explicitly needs a
truth table; local hierarchical inspection is the default. Explicit regions continue to
use two `set_region_corner` calls, `show_selected_region`, and
`convert_from_selected_region`. Debug clients may call
`resolve_looked_at_circuit` directly.

For observation debugging, `get_looked_at_world` starts near the block the
player is looking at and progressively follows adjacent redstone components
without applying circuit inference. There is no caller-selected scan radius.
Expansion ends when the component frontier is exhausted or `max_components`
(8192 by default) is reached; the latter is reported as an incomplete result.
`component_gap` defaults to 2 so a one-block break can be inspected as a nearby
fragment. The result includes exact block-name and block-state-property counts,
the targeted block, raw redstone states, truncation, and expansion completeness.
Use `include_block_list=true` only when the non-air listing is needed; both raw
lists are bounded by `max_listed_blocks`. `resolve_looked_at_circuit` and
`convert_from_looked_at_circuit` use the same component-limited expansion.

`convert_from_looked_at_circuit` reports a physical-first hierarchy. Observed facts
become a directed physical graph, recognized local cells, traceable logic
expressions, and finally optional functional candidates. Every stage reports
its own completeness and unresolved count while retaining physical component
origins. Circuits above 128 discovered redstone components deliberately skip a
flat whole-circuit truth table and broad repair enumeration. The MCP still
returns local cells, hierarchical summaries, and repair candidates within 12
blocks of the gaze target; distant repair enumeration remains bounded.

`signal_liveness` is evaluated independently of physical fragments. It follows
directed signal edges while preserving whether a source is a controllable
input, intrinsic source, observation boundary, or inferred primary input.
Repeater, comparator, piston, and torch-control inputs are classified as driven,
awaiting an external input, disconnected, or lacking a known source. An
inferred bare input is therefore reported separately from a genuine fault and
is not used by itself as evidence for an automatic repair. This catches
directional failures that Union-Find connectivity alone cannot detect without
shorting independent inputs together.

For conversational entry points, call `test_looked_at_circuit` first. It is
a read-only fast path: it discovers the connected components around the
player's gaze, skips truth-table inference, repair enumeration, and transition
scenario generation, and returns `dustroute.diagnostic.v1`. The response places
`diagnostic.health`, typed `diagnostic.counts`, ranked `diagnostic.findings`, and
one `diagnostic.recommended_next_action` ahead of detailed evidence. Use
`convert_from_looked_at_circuit` only when higher-level logical interpretation or
repair/transition proposals are actually needed.

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

The `temporal` result reports a lossless timed graph, a traceable steady-state
projection summary, and whether higher-level logic is `steady_state_safe`,
`timing_sensitive`, or `temporal_required`. Repeater delays use redstone ticks
(two game ticks each). A steady-state label remains useful for delayed paths,
but MCP clients must present it as provisional when unequal-delay paths
reconverge and must not treat feedback or mechanical devices as purely
combinational logic. Basic repeater locking and comparator analog behavior are
simulated; exact same-tick update order remains an explicit live-trace result.

Transient output distinguishes structural risk from measured behavior. A
hierarchical scan reports `not_simulated` until transition scenarios have run.
Trace evaluation reports pulse polarity, redstone-tick width, surrounding
steady value, and exact source-event indices. With no registered signal intent,
an observed deviation is only a `hazard_candidate`; `hazard_confirmed` is
reserved for an explicit stability or pulse-width contract violation, while a
matching pulse contract becomes `intentional_pulse`.

The intended conversational workflow is:

```text
convert_from_looked_at_circuit
  -> explain physical evidence, local role, and provisional higher roles
  -> choose a returned transition scenario or repair proposal
  -> preview the exact region or block diff
  -> obtain explicit player confirmation
  -> execute and restore/verify
  -> report the post-operation re-analysis
```

In the debug profile, long conversions can use
`start_selected_region_conversion`, `get_operation`, and `stop_operation`.
`new_circuit_placement` returns a block diff, collisions,
material counts, an operation UUID, and an exact undo plan without changing the
world. When `DUSTROUTE_READ_ONLY=false`, an explicitly confirmed plan can be
written with `invoke_circuit_placement`; `undo_circuit_placement` restores the captured
blocks.

## Transition scenarios

Live pulse observation uses the visible Mineflayer bot as an actuator and
sensor; Rust remains responsible for scenario policy, interpretation, and
restoration. The initial workflow supports one normal lever activation at a
time:

```text
new_transition_test
  -> show_transition_test
  -> explicit player confirmation
  -> invoke_transition_test(confirm=true)
  -> block-update trace, transient assessment, and Rust-simulator comparison
  -> automatic lever and region restoration verification
  -> restore_transition_test(confirm=true), if recovery is needed
```

The bridge uses Mineflayer's normal block activation rather than changing a
`powered` state with `/setblock`. The bot must be within 5.5 blocks of the
lever. Observations record packet-visible block updates with sequence numbers
and a Mineflayer physics-tick clock. Conversion rounds these observations into
the internal simulator's redstone-tick unit while retaining within-tick order
as separate evidence. Runs are bounded to 200 game ticks and 65,536 events.
TNT, fire, water, and lava reject a scenario. Pistons, observers,
containers with activation behavior, and unsupported sensors remain
preview-only. Every run captures the original snapshot and reports failure
unless both the lever state and full region are restored.
`scenario_verification` contains the normalized live trace, simulated trace,
typed differences, and an `equivalent` flag. Same-tick ordering differences are
retained rather than silently treated as electrical mismatches.

## Physical repair workflow

After selecting and previewing a region, `new_repair_plan` ranks partial physical
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
new_repair_plan
  -> show_repair_plan
  -> explicit player confirmation
  -> invoke_repair(confirm=true)
  -> automatic block-state rescan and circuit re-analysis
  -> undo_repair(confirm=true), when needed
```

Failed block-state verification triggers an automatic rollback attempt. A
successful application returns the resulting logical classification and, when
the original analysis included a truth table, an explicit before/after semantic
comparison. This comparison describes whether behavior changed relative to the
observed pre-repair circuit; it does not by itself prove the user's intended
function.
A suspected short cannot be inferred safely from geometry alone;
Debug-only `new_component_removal_plan` is available only for a component the
player explicitly identifies while looking at it.

## Safety configuration

The server defaults to read-only mode, requires previews, allows only the
overworld, limits scans to 262,144 blocks, and limits placement plans to 32,768
blocks. Optional environment variables:

```text
DUSTROUTE_ALLOWED_PLAYERS=BuilderOne,BuilderTwo
DUSTROUTE_ALLOWED_DIMENSIONS=minecraft:overworld
DUSTROUTE_ALLOWED_REGION=-100,0,-100,100,255,100
DUSTROUTE_MAX_SCAN_VOLUME=262144
DUSTROUTE_MAX_PLACEMENT_BLOCKS=32768
DUSTROUTE_READ_ONLY=true
DUSTROUTE_PREVIEW_REQUIRED=true
DUSTROUTE_STATE_DIR=/private/path/to/dustroute-state
DUSTROUTE_PLAN_TTL_SECONDS=3600
```

Repair plans are persisted across HTTP/MCP sessions. They are scoped by the
configured Minecraft server and assist player, written atomically with private
directory/file permissions on Unix, and expire after the configured TTL. The
stored data contains physical patches and verification baselines, not API keys.

The visible bot reconnects three seconds after disconnecting. Every scan and
preview carries the selected dimension, so moving between dimensions invalidates
the operation instead of silently targeting a different world.

Region selection and reverse translation remain read-only. World mutations
require a preview operation ID, `confirm=true`, and
`DUSTROUTE_READ_ONLY=false`.
