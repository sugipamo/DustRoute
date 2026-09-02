# dustroute-mcp

This crate is the primary AI-facing entry point for DustRoute. It exposes MCP
tools over stdio or loopback-only Streamable HTTP and talks to a visible
Mineflayer player through a localhost JSON-lines bridge.

## Start the visible bot

Live integration requires Node.js 22/npm, Java 21, and the official Minecraft
Java Edition 1.21.11 server JAR. Keep the server and its generated world under
an ignored directory such as `.local/minecraft-server-1.21.11`; do not copy the
JAR, world, logs, operator lists, or authentication data into the repository.

Run the server once to generate its files, read the EULA, and set
`eula=true` only after accepting it. The bundled Mineflayer workflow defaults
to offline authentication, so a private test server must include at least:

```properties
server-port=25565
online-mode=false
white-list=true
gamemode=creative
force-gamemode=true
enable-command-block=true
spawn-protection=0
generate-structures=false
spawn-animals=false
spawn-monsters=false
spawn-npcs=false
level-name=dustroute-test
level-type=minecraft:flat
generator-settings={"biome":"minecraft:plains","features":false,"lakes":false,"layers":[{"block":"minecraft:bedrock","height":1},{"block":"minecraft:dirt","height":2},{"block":"minecraft:grass_block","height":1}],"structure_overrides":[]}
```

For Java 1.21.11, `level-type=minecraft:flat` by itself is not a sufficient
test-world definition: a server may generate a void flat world. Keep the
explicit `generator-settings` line above. Its `layers` are ordered bottom to
top and produce one bedrock layer, two dirt layers, and one grass-block layer.
Generator settings are used only when the world is created. If
`dustroute-test/` already exists, changing these properties does not rebuild
it; stop the server and move that disposable test world aside before starting
again. Never do this to a world containing user data.

Start it with Java 21:

```bash
cd .local/minecraft-server-1.21.11
java -Xms1G -Xmx2G -jar server.jar nogui
```

Never expose an offline-mode server to an untrusted network. Letting each
offline player join once before running `whitelist add` is the
safest way to obtain the correct UUID. If a whitelist file must be generated
manually, Minecraft derives the UUID from the UTF-8 bytes of
`OfflinePlayer:<exact player name>` using the version-3/name-based UUID
algorithm. Name spelling and case are part of that input; a UUID generated for
a differently cased name will not match.

For a new isolated server, a reliable registration sequence is:

1. Temporarily set `white-list=false` (or run `whitelist off`) while the server
   is reachable only by trusted local test clients.
2. Connect `DustRouteBot`, `dustroutetest`, and the assisted player once using
   their final spelling and case.
3. Stop the server and compare every entry in `usercache.json` with the name
   and UUID that will be written to `whitelist.json` and `ops.json`.
4. Restart, run the commands below with the exact same names, and enable the
   whitelist again.

```text
whitelist add DustRouteBot
op DustRouteBot
whitelist add dustroutetest
op dustroutetest
whitelist add YourMinecraftName
whitelist on
whitelist list
```

Do not grant operator permission to the normal assisted player unless a test
explicitly requires it. If a listed player is rejected, compare the UUID in
the server login message, `usercache.json`, and `whitelist.json`; also compare
the name case byte-for-byte. Remove only the incorrect disposable entry and
register it again. Do not substitute an online-mode UUID for an offline-mode
UUID.

After the world first loads, disable natural spawning at the world level too:

```text
gamerule doMobSpawning false
```

The `generate-structures=false` property, empty `structure_overrides`, and
`features=false` generator setting keep generated structures and decoration
out of a newly created test world. The `spawn-*` properties prevent the
server's normal animal, monster, and NPC spawning, while the game rule covers
natural spawning controlled by the world. Existing generated structures are
not removed retroactively.

```bash
cd crates/dustroute-mcp/mineflayer
npm ci
DUSTROUTE_SERVER_ADDRESS=127.0.0.1:25565 \
  DUSTROUTE_MC_VERSION=1.21.11 \
  DUSTROUTE_MC_AUTH=offline \
  DUSTROUTE_BOT_NAME=DustRouteBot \
  npm start
```

`DUSTROUTE_MC_AUTH` defaults to `offline`, and `DUSTROUTE_BOT_NAME` defaults to
`DustRouteBot`. Grant the bot operator permission on the dedicated test server
if region previews, teleport-based safe approach, and chat messages are needed.
The bridge listens only on `127.0.0.1:25580`.

`get_bot_status` also returns cumulative bridge metrics under `bot.metrics`.
They count serialized JSON payload bytes (excluding the line delimiter), total
and maximum request duration in microseconds, errors, per-method request
counts, and scan volume/non-air block counts. These counters are intentionally
bounded to a fixed method set and reset when the Mineflayer bridge process is
restarted. Use them to distinguish Rust-side analysis cost from repeated
Mineflayer scans before considering a transport or client replacement.

Before starting MCP, verify the dedicated stack from a local shell:

```bash
# Minecraft should be listening on 25565; the bridge should be loopback-only.
ss -ltn | grep -E '(:25565|127\.0\.0\.1:25580)'
```

The server console should show both `DustRouteBot` and the intended actor
joining successfully. A human can then connect as `YourMinecraftName` to port
25565. Keep the server JAR, `server.properties`, player lists, logs, and the
entire generated world below `.local/`; all are runtime state rather than
repository fixtures.

Automated live testing with a second Mineflayer player is documented in
[`mineflayer/e2e/README.md`](mineflayer/e2e/README.md). It controls player gaze
and exercises observation, diagnostics, component limits, repair application,
verification, and undo without requiring a human to enter the world.

The exported semantic Data Pack reports assertion results to player chat, not
the dedicated-server console. Join as a player or capture chat through a test
client when validating its 20 scenarios and 23 assertions.

## Start the MCP server

Configure an MCP client to launch:

```bash
DUSTROUTE_SERVER_ADDRESS=127.0.0.1:25565 \
  DUSTROUTE_ASSIST_PLAYER=YourMinecraftName \
  cargo run -p dustroute-mcp
```

`DUSTROUTE_SERVER_ADDRESS` and `DUSTROUTE_ASSIST_PLAYER` are required. The same
server address must be supplied to the Mineflayer process. MCP tools use the
configured player automatically; the default public tool schemas do not expose
a `player` argument. Internal/debug calls that attempt to override the
configured player with another name are rejected.
If that player is online but outside the bot's entity-tracking range, gaze tools
and debug-only `get_visible_player` move only `DustRouteBot` to the configured player,
wait for tracking to resume, and retry once. The observation reports
`reacquired=true` when this happens. Player names are validated before a
teleport command is issued; an offline player remains an explicit error.

stdio is the default transport. A local HTTP client can instead use `/mcp`:

```bash
DUSTROUTE_SERVER_ADDRESS=127.0.0.1:25565 \
  DUSTROUTE_ASSIST_PLAYER=YourMinecraftName \
  DUSTROUTE_MCP_TRANSPORT=http \
  DUSTROUTE_MCP_HTTP_BIND=127.0.0.1:3000 \
  cargo run -p dustroute-mcp
```

The HTTP endpoint intentionally rejects wildcard, LAN, and public bind
addresses. It has no authentication layer yet, so exposing it beyond the local
machine is not supported. Multiple HTTP sessions share the same operation and
plan state inside one server process.

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

`DUSTROUTE_MCP_TOOL_PROFILE=default` exposes the 19 tools intended for normal
LLM collaboration. `debug` additionally exposes low-level gaze/discovery,
operation history, full plan retrieval, asynchronous conversion control, and
explicit component-removal planning. Debug tools remain implemented but cannot
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
Use `test_circuit_change` for one or more non-mutating block substitutions.

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
(`target`, `observer`, daylight detectors, containers, sensors, fluids, and
rails) are retained in `unsupported_observed_blocks` and never treated as a
simulated solid. The response's `interface_evidence` lists physical external
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
lever. Observations record packet-visible block updates with sequence numbers
and a Mineflayer physics-tick clock. Conversion rounds these observations into
the internal simulator's redstone-tick unit while retaining within-tick order
as separate evidence. Runs are bounded to 200 game ticks and 65,536 events.
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
typed differences, and an `equivalent` flag. Same-tick ordering differences are
retained rather than silently treated as electrical mismatches.

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
