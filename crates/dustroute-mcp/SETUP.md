# DustRoute MCP setup

This document is for the person configuring the Minecraft server, bot and MCP client. For tool use, see the [LLM guide](README.md).

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
directory/file permissions on Unix, and use the configured TTL on disk. A running process can still fall back to its repair-plan cache; this is not a strict execution deadline. The
stored data contains physical patches and verification baselines, not API keys.

The visible bot reconnects three seconds after disconnecting. Every scan and
preview carries the selected dimension, so moving between dimensions invalidates
the operation instead of silently targeting a different world.

Region selection and reverse translation remain read-only. World mutations
require a preview operation ID, `confirm=true`, and
`DUSTROUTE_READ_ONLY=false`.
