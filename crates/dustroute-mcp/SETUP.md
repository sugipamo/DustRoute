# DustRoute MCP setup

For an introduction, read [getting started](../../docs/getting-started.md)
([日本語](../../docs/getting-started.ja.md)). This is the detailed English
operator reference; the example private server below is for creative command
trials, not a prerequisite to run non-OP survival construction.

This document is for the person configuring the Minecraft server, bot and MCP client. For tool use, see the [LLM guide](README.md).

## Runtime

Voxrig is the only live backend and is enabled in the default build. No Node.js
bridge, bridge port or `DUSTROUTE_BOT_BRIDGE` configuration is used. An explicit
`DUSTROUTE_BOT_BACKEND` must be `voxrig`; retired settings fail startup. Builds
with `--no-default-features` support offline library work but cannot start a live
MCP client without the `voxrig` feature. The native adapter accepts Java 1.21.11
and offline authentication.

## Native Rust client (Java 1.21.11)

The tested Voxrig source is pinned in `vendor/voxrig` with commit and file
checksums in `vendor/voxrig-source.json`. No separate checkout or Node.js process
is needed. From the repository root, verify, build and start the native client:

Read `revision` in `vendor/voxrig-source.json` for the current source pin and run
the source verification command below to check its files. The manifest is the
source of truth; historical trial documents retain their tested pins.

This standalone checkout route was validated with Rust/Cargo 1.98.0 on Linux
x86_64. Exact build and runtime evidence is linked from
[native client usability](../../docs/native-client-usability.md).

```bash
python3 scripts/vendor_voxrig.py --check
cargo build --locked -j1 -p dustroute-mcp --features voxrig

DUSTROUTE_SERVER_ADDRESS=127.0.0.1:25565 \
  DUSTROUTE_ASSIST_PLAYER=YourMinecraftName \
  DUSTROUTE_BOT_BACKEND=voxrig \
  DUSTROUTE_MC_AUTH=offline \
  DUSTROUTE_MC_VERSION=1.21.11 \
  target/debug/dustroute-mcp
```

The default build enables Voxrig. There is no Node.js
bridge process for this selection. Start the vanilla server and register the bot
as described below; the same mutation policy and player/region restrictions
apply. World writes remain disabled by default. Previews, teleport approach and
Assembly construction still need operator command permission. Observation uses
received packets and supported client reconstruction without per-cell commands;
it explicitly reports client evidence, never server-confirmed evidence. No server
MOD is required. The backend currently requires offline authentication and
Java 1.21.11; unsupported explicit settings fail instead of changing adapters.

Current live evidence and limits are in [the native rollout](../../docs/voxrig-rollout.md).
Native gaze uses audited static outline shapes, including dust, switches and
gates. Fluids/entities are excluded; unsupported context-dependent shapes and
moving geometry report unavailable.
An explicit region remains available when gaze geometry cannot be determined.
This is client observation, not a graphical camera-frame receipt.

For offline builds, prefetch the locked Cargo dependencies on the build machine,
then add `--offline`; the pinned Voxrig source itself needs no network access.
Maintainer update instructions are in [vendor/README.md](../../vendor/README.md).

For bounded survival building, supply the builder's inventory and enable the
ordinary mutation/region/player policy explicitly. The same client observes and
builds. A distinct `DUSTROUTE_SURVIVAL_OBSERVER_USERNAME` is optional for independent
validation; it is read-only and is not automatically teleported. This path uses
non-OP survival actions. Command-based tools retain their own permission
requirements. See [public survival construction](../../docs/survival-public-construction.md).

## Prepare the vanilla server

The live backend uses a vanilla Minecraft Java Edition 1.21.11 server. Hosting
it requires Java 21 and the official server JAR. Keep the server and its
generated world under an ignored directory such as
`.local/minecraft-server-1.21.11`; do not copy the JAR, world, logs, operator lists,
or authentication data into the repository.

Run the server once to generate its files, read the EULA, and set
`eula=true` only after accepting it. The native backend requires offline
authentication, so the documented private test server must include at least:

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

## Start the MCP server

The Rust process connects directly; transport and policy settings below apply.

Configure an MCP client to launch:

```bash
DUSTROUTE_SERVER_ADDRESS=127.0.0.1:25565 \
  DUSTROUTE_ASSIST_PLAYER=YourMinecraftName \
  DUSTROUTE_BOT_BACKEND=voxrig \
  cargo run -p dustroute-mcp
```

`DUSTROUTE_SERVER_ADDRESS` and `DUSTROUTE_ASSIST_PLAYER` are required. MCP tools use the configured player automatically; the default public tool schemas do not expose
a `player` argument. Internal/debug calls that attempt to override the
configured player with another name are rejected.
If that player is online but outside the bot's entity-tracking range, gaze tools
and debug-only `get_visible_player` move only `DustRouteBot` near the configured
player and wait for tracking to resume. The observation reports `reacquired=true`
when tracking was missing. A bot already within three horizontal blocks and
three vertical blocks also moves aside, without claiming tracking was reacquired.
The preferred destination is four blocks behind and two blocks above the player;
left and right offsets are tried if it is blocked. The offset follows the player's
horizontal facing, independently of pitch. Each candidate is centered in a block
column and requires ordinary air at both feet and head before the teleport runs.
If all three offsets are unavailable, the bot tries the player's position as a
last resort, allowing overlap to prioritize work in cramped spaces. Tracking and
proximity must still be observed; if that fallback also fails, the tool returns
an error. Player names are validated before commands are issued;
an offline player remains an explicit error. These offsets apply to player
acquisition; block interactions still use their own positions within reach.

stdio is the default transport. A local HTTP client can instead use `/mcp`:

```bash
DUSTROUTE_SERVER_ADDRESS=127.0.0.1:25565 \
  DUSTROUTE_ASSIST_PLAYER=YourMinecraftName \
  DUSTROUTE_BOT_BACKEND=voxrig \
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
directory/file permissions on Unix, and loaded from disk for each use. There is
no in-memory fallback for expired, deleted or unreadable records. Preview and
successful apply/undo save the plan again and renew its TTL; ordinary reads do
not. The TTL checks admission when loading a plan, rather than cancelling an
action already in progress. The stored data contains physical patches and
verification baselines, not API keys.

Native readback uses received packets and supported client reconstruction; it
uses no per-cell confirmation commands and produces no server-confirmed ticks. Missing
cells or incomplete reconstruction fail observation. Region previews, teleport
approach and world mutations still need their corresponding permissions.
Client readback does not freeze the world or prove empty event queues.

Every scan and preview carries the selected dimension, so moving between dimensions invalidates
the operation instead of silently targeting a different world.

Region selection and reverse translation remain read-only. World mutations
require a preview operation ID, `confirm=true`, and
`DUSTROUTE_READ_ONLY=false`.
