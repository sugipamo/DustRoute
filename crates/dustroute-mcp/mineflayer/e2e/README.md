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
toolchains and initial server setup. From its console, allow and grant operator
permission to both bots:

```text
whitelist add DustRouteBot
op DustRouteBot
whitelist add dustroutetest
op dustroutetest
```

The actor needs operator permission only to build isolated deterministic test
fixtures with `/fill`, `/setblock`, and `/tp`. Never point the harness at a
production or shared world.

Start the normal visible bridge and build the MCP binary:

```bash
cargo build -p dustroute-mcp
cd crates/dustroute-mcp/mineflayer
npm install
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
Mineflayer lever activation, non-empty live/simulated JSON traces, semantic
equivalence, state restoration, and continued MCP availability after the run:

```bash
npm run test:e2e -- transition_run_and_restore
```

## Scenario contract

Scenario files are ordered JSON documents in `scenarios/`. Supported steps are:

- `command`: issue fixture-building commands as the actor.
- `aim`: teleport the actor, call Mineflayer `lookAt`, and poll MCP until the
  observed targeted block exactly matches.
- `mcp`: call a named MCP tool and save its JSON result.
- `mcp_error`: require a tool-level error and save its stable error envelope.
- `assert`: compare a saved result using `equals`, `at_least`, `at_most`, or
  `exists`.
- `wait`: wait a number of Mineflayer physics ticks.

`${result.path.0.value}` references pass dynamic operation IDs between steps.
Mutation tools still receive `confirm: true` explicitly; the harness never
weakens MCP preview policy.

Tracked files contain only harness code and deterministic scenario definitions.
The server JAR, world, logs, node_modules, MCP state, and credentials remain
under ignored local directories.
