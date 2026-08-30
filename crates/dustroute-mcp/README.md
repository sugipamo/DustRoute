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

Set `DUSTROUTE_BOT_BRIDGE` to override the local bridge address. Natural-language
references such as “what is this?” use `analyze_looked_at_circuit`. One call
returns the focused physical component, its local signal role, recognized
AND/OR/NOT-style gates, traceable expressions, optional whole-circuit function
candidates, observation completeness, diagnostics, and non-mutating repairs.
Set `include_truth_table=true` only when a small circuit explicitly needs a
truth table; local hierarchical inspection is the default. Explicit regions continue to
use `discover_looked_at_circuit` or `mark_region_corner`, `preview_region`, and
`analyze_selected_region`.

For observation debugging, `inspect_looked_at_world` starts near the block the
player is looking at and progressively follows adjacent redstone components
without applying circuit inference. There is no caller-selected scan radius.
Expansion ends when the component frontier is exhausted or `max_components`
(8192 by default) is reached; the latter is reported as an incomplete result.
`component_gap` defaults to 2 so a one-block break can be inspected as a nearby
fragment. The result includes exact block-name and block-state-property counts,
the targeted block, raw redstone states, truncation, and expansion completeness.
Use `include_block_list=true` only when the non-air listing is needed; both raw
lists are bounded by `max_listed_blocks`. `discover_looked_at_circuit` and
`analyze_looked_at_circuit` use the same component-limited expansion.

`analyze_looked_at_circuit` reports a physical-first hierarchy. Observed facts
become a directed physical graph, recognized local cells, traceable logic
expressions, and finally optional functional candidates. Every stage reports
its own completeness and unresolved count while retaining physical component
origins. Circuits above 128 discovered redstone components deliberately skip a
flat whole-circuit truth table and broad repair enumeration; the MCP returns
local cells and hierarchical summaries, then asks the caller to focus a smaller
functional area for detailed simulation or repair.

Long analyses can use `start_selected_region_analysis`, `get_operation`, and
`cancel_operation`. `preview_compiled_circuit` returns a block diff, collisions,
material counts, an operation UUID, and an exact undo plan without changing the
world. When `DUSTROUTE_READ_ONLY=false`, an explicitly confirmed plan can be
written with `apply_placement_plan`; `undo_placement_plan` restores the captured
blocks.

## Physical repair workflow

After selecting and previewing a region, `propose_repairs` ranks partial physical
patches for missing wire, missing support, and directional component problems.
Each proposal includes coordinates, evidence, confidence, a virtual before/after
impact, and an operation UUID. The safe mutation sequence is:

```text
propose_repairs
  -> preview_repair
  -> explicit player confirmation
  -> apply_repair(confirm=true)
  -> automatic block-state rescan and circuit re-analysis
  -> undo_repair(confirm=true), when needed
```

Failed block-state verification triggers an automatic rollback attempt. A
suspected short cannot be inferred safely from geometry alone;
`propose_targeted_component_removal` is available only for a component the
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
```

The visible bot reconnects three seconds after disconnecting. Every scan and
preview carries the selected dimension, so moving between dimensions invalidates
the operation instead of silently targeting a different world.

Region selection and reverse translation remain read-only. World mutations
require a preview operation ID, `confirm=true`, and
`DUSTROUTE_READ_ONLY=false`.
