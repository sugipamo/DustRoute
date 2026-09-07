# Fixed 3×3 door: live validation

The current `PistonDoorScenario` is a simulator control contract, **not a
deployable lever-controlled Minecraft build**. On Java 1.21.11 the two-sided
18-normal-piston mechanism completed three consecutive open/close cycles when
the test harness injected power directly behind the pistons. The declared
`1 → 3 → 9` wiring could not survive literal placement, and normal lever
activation did not open the door.

This is the first implementation boundary for live operation. It occurs
before the simulator's separate same-tick completion limitation.

The subsequent validation refactor now rejects this fixture before normal
execution. Historical `complete` results below describe the pre-refactor
simulator. The diagnostic explicitly uses `--diagnostic` to retain that model
for comparison. See [the mandatory validation boundary](world-validation-boundary.md).

## Evidence and scope

The diagnostic runs on the existing private test server, using a visible
`dustroutetest` Mineflayer client. It checks an empty bounded region before
building and verifies that the region is empty again after cleanup. It does
not change MCP safety policy or publish a new tool.

There are two independent probes:

1. **Mechanical probe:** place only the nine panel blocks and 18 normal
   pistons at the reference coordinates. Place/remove redstone blocks behind
   the open-side pistons, then behind the close-side pistons. Repeat three
   times without rebuilding the mechanism. Each phase checks all nine panel
   cells, the empty alternate plane, and both sets of retracted pistons.
   Command-injected power is a test actuator, not a lever controller. The
   commands are sequential and do not establish same-tick behavior.
2. **Declared wiring probe:** materialize the CLI's `initial_state` block
   positions and device settings, allowing Minecraft to calculate dust
   connections. Record missing blocks after settling. Add one explicitly
   reported support block beneath the otherwise unsupported lever and place
   that lever again. Activate it normally ON/OFF three times and record all
   panel and piston states after each edge. No control wires are repaired.

The first capture found:

| Check | Result |
| --- | --- |
| Simulator cycle | `complete` |
| Live mechanical cycles | 3/3 open and close, all pistons retracted |
| Literal placement | 230 of 268 expected block names differ |
| Repeater losses | 162/162 became air |
| Dust losses | 68/72 became air |
| Normal lever ON attempts | 0/3 opened the panel |
| Lever OFF observations | Panel remained closed; this is not a successful return from open |
| Cleanup | Entire diagnostic region verified empty |

The tracked summary is
[`piston-door-live-diagnostic.json`](../crates/dustroute-mcp/mineflayer/e2e/fixtures/piston-door-live-diagnostic.json).
Full per-cell states and packet-visible updates are saved in ignored local
artifacts. These are wall-clock-settled observations, not server-instrumented
scheduler traces or timing-profile promotion evidence.

## Why adding supports is not a small repair

The simulator's initial snapshot has 162 repeaters: 114 above air and 48
directly above another repeater. Of the 72 dust cells, 48 are above air, 14
above dust, six above repeaters, and only four above solid blocks. Adding a
solid support at every required position would overwrite 68 existing control
components. This requires a different physical routing layout.

There is also no physical lever-to-pulse circuit in the fixture.
`PhysicsEngine::schedule_lever_pulse_sequence` changes the lever and schedules
high/low events directly at the selected remote root source. Both roots are
represented by `RedstoneBlock { powered: false }` initially. A Java redstone
block has no switchable `powered=false` state; its exported form is simply
`minecraft:redstone_block`. The live probe retains that discrepancy instead of
silently replacing the roots with a new controller.

## Reproduce

Use the private, disposable Java 1.21.11 test server described in the
[E2E README](../crates/dustroute-mcp/mineflayer/e2e/README.md). It must already
be running on `127.0.0.1:25565`, with `dustroutetest` whitelisted and an
operator. The bot bridge and MCP process are not required.

```bash
cargo build -p dustroute-cli
node crates/dustroute-mcp/mineflayer/e2e/piston-door-live.js
```

The script uses the empty region `975..1005, 178..186, 988..1013` and refuses
to overwrite a nonempty or unloaded region. It writes
`.local/e2e-artifacts/piston-door-live-diagnostic-latest.json` and a timestamped
copy, with the Git revision, fixture digest, actual observations, and cleanup
result. An exit code of zero means the diagnostic completed; inspect `status`
to distinguish a working door from an observed contract gap. Infrastructure
or cleanup errors produce a nonzero exit code. An interrupted process may
leave a fixture; a subsequent run refuses to overwrite it.

## Small next task

**Build one physically valid fixed wiring layout for this same mechanism.**
Keep the panel/piston geometry and begin with two explicit external pulse
inputs, one for each side. Give every wire and repeater valid support and
keep the two channels isolated. Require three live open/close cycles without
rebuilding or directly powering individual pistons. Preserve the successful
layout as a coordinate fixture.

Only then add a physical controller that converts one lever's ON/OFF edges
into those two pulses. This avoids combining routing and pulse generation in
one debugging task. Comparing that real layout with the simulator is another
bounded task; if its required timing exposes the known same-tick `WorldDelta`
failure, use the actual failing layout as the regression for that fix.

Observed-door recognition, arbitrary layout inference, MCP execution
promotion, and general piston/scheduler semantics remain separate work.
