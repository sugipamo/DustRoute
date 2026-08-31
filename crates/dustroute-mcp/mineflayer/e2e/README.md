# Mineflayer MCP E2E

This harness replaces the human test player with a second visible Mineflayer
client. `DustRouteBot` remains the MCP-controlled assistant; `dustroutetest`
is the default user actor because Minecraft player names are limited to 16
characters. Override it with `DUSTROUTE_E2E_PLAYER`.

The harness connects to an already running Java 1.21.11 superflat test server
and visible bot bridge, starts `dustroute-mcp` directly over stdio with the
debug tool profile, positions the actor, controls its gaze, verifies the gaze
through `get_player_gaze`, and executes JSON scenarios. It does not require the
HTTP wrapper.

## One-time server setup

The server must be an offline/private Java 1.21.11 superflat test server running
on Java 21 with `online-mode=false`, `white-list=true`, and an accepted EULA.
The repository-level and MCP READMEs describe the required Rust and Node.js
toolchains and initial server setup. Its world-generation properties must
include:

```properties
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
level-type=minecraft:flat
generator-settings={"biome":"minecraft:plains","features":false,"lakes":false,"layers":[{"block":"minecraft:bedrock","height":1},{"block":"minecraft:dirt","height":2},{"block":"minecraft:grass_block","height":1}],"structure_overrides":[]}
```

Do not omit `generator-settings`: with Java 1.21.11,
`level-type=minecraft:flat` alone can produce a void world. These settings are
read when the world is first generated, so correct them before creating the
dedicated E2E world.

After first creation, run this once from the server console:

```text
gamerule doMobSpawning false
```

Together with `generate-structures=false`, `features=false`, and an empty
`structure_overrides`, this keeps structures, decoration, and natural mob
spawning out of the deterministic E2E world.

From the server console, allow and grant operator permission to both bots:

```text
whitelist add DustRouteBot
op DustRouteBot
whitelist add dustroutetest
op dustroutetest
```

Because the server uses offline authentication, connect both names once with
their exact spelling and case before finalizing the whitelist. Verify that
their UUIDs agree across the login message, `usercache.json`,
`whitelist.json`, and `ops.json`. A case difference creates a different
offline UUID and can leave a name visibly listed but unable to join.

The actor needs operator permission only to build isolated deterministic test
fixtures with `/fill`, `/setblock`, and `/tp`. Never point the harness at a
production or shared world.

Start the normal visible bridge and build the MCP binary:

```bash
cargo build -p dustroute-mcp
cd crates/dustroute-mcp/mineflayer
npm ci
DUSTROUTE_SERVER_ADDRESS=127.0.0.1:25565 \
  DUSTROUTE_MC_VERSION=1.21.11 npm start
```

In another shell, run all scenarios:

```bash
cd crates/dustroute-mcp/mineflayer
DUSTROUTE_SERVER_ADDRESS=127.0.0.1:25565 \
  DUSTROUTE_BOT_BRIDGE=127.0.0.1:25580 \
  npm run test:e2e
```

Pass scenario names to run a subset:

```bash
npm run test:e2e -- normal_circuit component_limit repair_and_undo
```

The transition scenario deliberately moves `DustRouteBot` out of interaction
range before invocation. It verifies automatic pre-recording approach, normal
Mineflayer lever activation, non-empty live/simulated JSON traces, steady-state
equivalence, explicit strict-trace differences, state restoration, and
continued MCP availability after the run:

```bash
npm run test:e2e -- transition_run_and_restore
```

When the exported semantics Data Pack is installed and enabled in the test
world, collect its player-chat results automatically with:

```bash
DUSTROUTE_E2E_SEMANTICS_FUNCTION=ro_sem:tests \
  DUSTROUTE_E2E_SEMANTICS_ASSERTIONS=23 \
  npm run test:e2e:semantics
```

The collector fails on any `FAIL` message, a missing `DUSTROUTE COMPLETE`, an
unexpected PASS count, disconnect, or timeout. It deliberately reads player
chat because these assertions are not emitted to the server console.

## Scenario contract

Scenario files are ordered JSON documents in `scenarios/`. Supported steps are:

- `command`: issue fixture-building commands as the actor.
- `aim`: teleport the actor, call Mineflayer `lookAt`, and poll MCP until the
  observed targeted block exactly matches.
- `mcp`: call a named MCP tool and save its JSON result.
- `mcp_error`: require a tool-level error and save its stable error envelope.
- `mcp_with_commands`: call a tool while issuing delayed test-world commands;
  this is reserved for deterministic interference and recovery scenarios.
- `assert`: compare a saved result using `equals`, `at_least`, `at_most`, or
  `exists`.
- `wait`: wait a number of Mineflayer physics ticks.

`${result.path.0.value}` references pass dynamic operation IDs between steps.
Mutation tools still receive `confirm: true` explicitly; the harness never
weakens MCP preview policy.

`DUSTROUTE_E2E_TIMEOUT_MS` configures MCP/scenario waits from 1,000 through
600,000 milliseconds. Each scenario declares cleanup commands, and temporary
gaze footings are removed whether the scenario passes or fails. On failure the
harness writes the failed step, saved MCP results, actor pose, and recent MCP
stderr to `.local/e2e-artifacts/<timestamp>-<scenario>.json`; these diagnostics
remain ignored by Git. Set `DUSTROUTE_E2E_VERBOSE=true` to print every saved MCP
response during a run.

Tracked files contain only harness code and deterministic scenario definitions.
The server JAR, world, logs, node_modules, MCP state, and credentials remain
under ignored local directories.
