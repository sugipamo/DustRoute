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

The runner discovers the JSON files in `scenarios/` in lexical order (the
current checkout contains 32 scenarios) and fails on the first assertion or
cleanup error. Use a subset while debugging and rerun the full set before
promoting a release.

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

Observer scenarios use a second visible Mineflayer client as the dummy player.
`observer_dummy_transition` checks raw Observer state, a normal lever
activation, and the one-redstone-tick pulse. `observer_chain_dummy_transition`
and `observer_repeater_preview_only` extend that trace through another Observer
or a delayed repeater; they also verify that the MCP transition plan remains
`preview_only` and cannot be invoked automatically. `piston_transition_preview_only`
performs the same safety check for a piston and confirms that no block movement
occurs when invocation is rejected. `piston_motion_trace` directly activates a
normal piston with the visible test player and stores a bounded Java trace,
including block updates and within-game-tick ordering. It is an observation
fixture only. On the pinned 1.21.11 test server it asserts the observed
2-game-tick start-to-head-completion interval and reports the input-to-start
interval separately; those measurements must be promoted to a verified profile
before piston timing can become MCP-ready.

Run these bounded live checks after rebuilding the Rust binary and restarting
the visible bridge so the bridge's within-tick order field is active:

```bash
cargo build -p dustroute-mcp
npm run test:e2e -- observer_dummy_transition observer_chain_dummy_transition observer_repeater_preview_only piston_transition_preview_only
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

### Promoting measured timing

`activate_trace` with `save_artifact: true` also writes an ignored JSON artifact
with relative-to-input game-tick timing and within-tick packet order. After a
human review, promote a trace into the tracked scheduler-observation fixtures:

```bash
npm run promote:scheduler -- \
  ../../../.local/e2e-artifacts/observer_repeater_preview_only-latest.json \
  trace scheduler_1_21_11_observed_repeater_observer \
  "Capture repeater and observer timing on the pinned 1.21.11 server"
```

The command refuses to overwrite an existing fixture. It preserves no absolute
server tick, keeps no-op updates, and marks the internal scheduler phase as
unknown. These observations strengthen delay regression coverage but do not
promote the modelled scheduler profile to a Vanilla-complete implementation.

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
- `activate_trace`: move the dummy player if requested, activate a normal
  player input, and record observed block states for a bounded number of game
  ticks. Events include `game_tick`, `sub_tick_order`, `event_kind`, `cause`,
  `source`, and optional `cause_sequence`; the latter provenance fields are
  packet-order evidence, not a claim about the internal vanilla scheduler
  cause.

`${result.path.0.value}` references pass dynamic operation IDs between steps.
Mutation tools still receive `confirm: true` explicitly; the harness never
weakens MCP preview policy.
Individual expensive read-only steps may set `timeout_ms`; it remains bounded
by the scenario author and does not change the MCP server's search budgets.

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
