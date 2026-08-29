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
references such as “this circuit” are resolved by the LLM through the MCP tool
sequence: `observe_player`, `discover_looked_at_circuit` or
`mark_region_corner`, `preview_region`, and `analyze_selected_region`.

Long analyses can use `start_selected_region_analysis`, `get_operation`, and
`cancel_operation`. `preview_compiled_circuit` returns a block diff, collisions,
material counts, an operation UUID, and an exact undo plan without changing the
world. When `DUSTROUTE_READ_ONLY=false`, an explicitly confirmed plan can be
written with `apply_placement_plan`; `undo_placement_plan` restores the captured
blocks.

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
