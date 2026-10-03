# Native client rollout

The seven-stage rollout is authorized: moving-piston restoration, adhesion,
reference callbacks, broader live comparisons, DustRoute observation/interaction,
remaining operation APIs, then replacement of verified bridge paths. Java 1.16.1
and 1.21.11 coexist in Voxrig; DustRoute's physical context remains 1.21.11.
Entities in the circuit simulator remain deferred. A new large prerequisite
outside this scope stops the task for a report.

Voxrig's separate development history is published on
[`codex/dustroute-integration`](https://github.com/sugipamo/Voxrig/tree/codex/dustroute-integration).
The Cargo
`voxrig` feature now uses the unmodified `vendor/voxrig` snapshot pinned to
`784c12dba126f5829cd7a8db8542360cc48434c9`. A clean DustRoute checkout needs no
sibling repository. [Vendor instructions](../vendor/README.md) describe checksum
verification and deliberate updates from tested source commits.
The [pin alignment verification](evidence/voxrig/source-pin-alignment-20261002.json)
checks all 205 files and standalone builds. This pin includes the shared native
observation changes previously held only in DustRoute's vendor copy.
The original operation/recording evidence used `b98785e`; the subsequent
[usability work](native-client-usability.md) adds static outline targeting,
actual MCP process recovery trials and an independent-checkout runtime trial.

## Observation boundary

`BotBridge::connect_voxrig` connects the native adapter.
`scan_client_region` returns a `ClientRegion` with exact native states and
`dustroute.client-observation.v1` / `client_reconstructed` evidence. The original
received cache, connection ID, packet sequence, frame, dimension, origins and
moving carriers remain attached. Frames/revisions are not server ticks.

Incomplete reconstruction, pending chunk recovery, unavailable cells, a wrong
dimension or incomplete coverage rejects the scan. The Rust type has no
Deserialize implementation, so saved data cannot create a fresh capability.
It also cannot construct `ValidatedRegion` or `ServerReadback`. Shared workflows
consume a fresh observation capability whose source is explicit. The native path
does not issue per-cell confirmation commands or silently fall back to Mineflayer.
The adapter does not run DustRoute's simulator to manufacture observations.

Durable attempts retain `ObservationEvidence`: the existing server receipt or a
strict `dustroute.client-readback.v1` receipt with connection, receive sequence,
client frame, revisions, dimension, bounds, captured time and moving flag. Client
receipts have no server tick. Reading saved evidence cannot construct the private
fresh capability. Construction rejects active moving carriers even when their
visible block name looks stationary. Two-sample instance observation also rejects
motion, changed states, reversed clocks and changed connections. Neither quiet
samples nor a successful step proves empty server queues or excludes a later edit.

As of 2026-10-03, Voxrig is the sole live backend and the default Cargo feature.
Status, player/target observation, client waits, lever approach/activation,
command batches, creative physical placement/removal, particle previews and
ordered block recordings use typed Rust calls. Particle and wait results are
Rust records; the MCP facade serializes them for public responses.

`DUSTROUTE_SERVER_ADDRESS`, `DUSTROUTE_BOT_NAME`, `DUSTROUTE_ASSIST_PLAYER` and
policy configuration still apply. Offline authentication and Java 1.21.11 are
required. An explicit `DUSTROUTE_BOT_BACKEND` other than `voxrig` fails startup;
there is no legacy fallback. `DUSTROUTE_BOT_BRIDGE` is removed. Builds without the
native feature can perform offline library work but cannot start the live client.

Former JSON/TCP workflow stubs compile only under `cfg(test)` and identify
`backend=test_transport`. Synthetic server receipts in those tests do not validate
native confirmation. The Node bridge, npm dependencies and executable live actors
are removed. [Old measurements](evidence/legacy-mineflayer/README.md) retain their
original facts and clock. Offline observation parsers remain separately usable.

The [native-only validation](evidence/native-only-validation-20261003.json)
rechecks the current path after retirement of the bridge: standard production
startup without a backend override, status, gaze, lever interaction, placement,
removal, recording and the maximum client wait. Independent server predicates
matched all 648 final cells. A non-OP survival roof completed 115 steps with
3,120 final cells and position checked by a separate bot. All processes exited
normally. The initial actor-view setup failure is retained alongside the retry;
the actor now sets its view through the existing client API. These bounded trials
do not establish server queue emptiness or unrestricted construction.

Native player targets now carry `targeting_geometry=block_outline`, connection and
receive sequence. Audited native static outlines include dust, switches and
gates; fluids are skipped, while unsupported/unavailable/moving geometry rejects
the query. This is received player/client-world evidence, not an interpolated
graphical frame. Voxrig retains its explicitly named collision query for callers
that need it. Native waits identify their client wall-time clock. Neither
waiting nor dispatching commands creates server-confirmed evidence.

Creative physical placement preflights the whole batch's positions, item IDs,
reference adjacency and faces. It requires received teleport positions and
observes each removal/placement. An unexpected block/orientation stops the batch;
the world may already have changed and must be inspected before retry. It does
not claim survival construction or all placement orientations. Command writes
return submission receipts; previews likewise report submission only.

Native update recordings preserve received block-state packets and their packet
order. They do not include every reconstructed piston frame. Chunk replacement,
unload or world change invalidates a recording; overflow reports truncation.
`clock=client_frame20_hz` is explicit. Transition traces use `client_tick` or
`client_redstone_tick`, with no `game_tick` claim. A client width cannot confirm
or violate a game-tick width requirement; comparisons report incompatible clocks.

The `bridge-a` and `bridge-b` manifests in `docs/evidence/voxrig/` retain isolated
native bridge trials: status, players, a prescribed stone collision target,
previews, lever toggle, stone placement/removal and a complete final region.
Independent native functions confirmed all 648 cells, including air. Trial B
also retained 12 ordered block updates with no truncation. The explicit
server-confirmed scan API rejected native client evidence.
Trial C rechecks the current adapter and the maximum 200-tick client wait.
Intentional local waiting and bounded per-step mutation waits are included in
the timeout budget; the default transport timeout no longer expires merely
because the caller requested its supported maximum wait.

`mcp-door-a-20260929.manifest.json` retains the real public MCP lifecycle: import,
fresh adoption, 43-step placement, persisted instance reopening on a new service/
native connection, missing-quartz diagnosis and reviewed reconstruction, ordinary
close/open, conditional removal, then a fresh observation of the saved removed
record. Missing previews and changed baselines were refused for placement and
repair. Each of placed/closed/reopened/removed regions independently matched all
770 server cells. The MCP restarts use new service instances within one process;
they do not claim new OS processes. The additional stationary-carrier refusal was
added after the probe build and has a separate regression test.

`mcp-flight-a-20260929.manifest.json` covers a public generated `honey_nose`
machine (distance 3, rotation r270, mirrored). Its 11-step placement, ordinary
lever launch, declared arrival, diagnosis, reviewed operating-state removal and
saved removed instance all pass over native MCP. The full 1,134-cell arrival
matches the generator's declared endpoint. Independent vanilla functions check
placed/arrived/removed states in two bounded 567-cell subregions each. These are
explicit stable-state comparisons, not an atomic snapshot claim.

The live harness is `crates/dustroute-mcp/examples/voxrig_assembly_probe.rs`.
Build with `cargo build --locked -p dustroute-mcp --features voxrig --examples`,
then run it with `MC_PORT`, a **new** isolated `DUSTROUTE_STATE_DIR`, a new
`TRACE_OUTPUT` JSONL path, and `PROBE_CASE=door` or `flight`. It uses only loopback,
permits mutations in its declared disposable bounds and pauses before setup and
each independent comparison. It holds clients on failure so the operator can
capture and clean the exact failed world. Do not run it on a player world.
Extract a captured `after_client` record for Voxrig's
`scripts/prepare_motion_confirmation.py`; larger regions need disjoint bounded
parts, as retained in the flight evidence. Revoke test OP grants after cleanup.

All seven rollout stages now have implementations and declared-case evidence.
The retained Mineflayer backend is explicitly selectable; deleting it would also
remove currently separate authentication/targeting behavior and is not part of
this verified-path migration. Native APIs reject unsupported observations. Broad
inventory components, online authentication, context-dependent outlines, arbitrary
circuits and the historical 1.16.1 movement discrepancy remain separate limits.

## Native evidence already retained in Voxrig

- Mid-motion joins restore real moving-piston chunk NBT and provenance.
- Slime and honey branch trials each match 770 final native cells.
- Wire/gate support and diagonal wire callbacks match 770 final native cells.
- The reference door completes repeated close/open cycles. Closed, reopened and
  post-reload open regions each match 770 native cells. Unload explicitly makes
  observations unavailable and fresh chunks restore them.
- The initial door run exposed incorrect support for a retracting piston body;
  the failure, fix and six-direction regression are retained.

These comparisons use an isolated vanilla 1.21.11 server and independent command
assertions as test instrumentation. Runtime scans do not use those assertions.
They do not claim arbitrary circuits, server progress equality for every client
frame, survival construction, complete graphical targeting or entity physics.

Observation increment validation: `cargo check --offline -j1 -p dustroute-mcp
--features voxrig` resolved only the new local dependency and required feature
packages; subsequent commands use `--locked`. Eleven bridge/adapter tests pass,
including stale evidence, invalid coverage and native provenance rejection.
All-target feature-enabled Clippy with warnings denied, package formatting and
diff checks pass. Logs are `.local/voxrig-adapter-{check,tests,clippy}.log`.
That earlier increment covered observation only. Subsequent native operation and
MCP workflow evidence is listed above.

## Final validation (2026-09-29)

- MCP library: 111 tests pass, including source-bound observations, construction,
  human-damage diagnosis, interrupted repair, durable removal and moving-carrier
  refusal (`.local/voxrig-final-mcp-tests.log`, one test thread).
- After the final clock-comparison changes: 27 IR tests, 11 scenario tests and
  all 8 MCP transition tests pass (`.local/voxrig-final-{ir,scenario,transition}-tests.log`).
- Feature-enabled IR/translate/MCP all-target Clippy passes with warnings denied;
  the default-feature workspace/all-target check also passes
  (`.local/voxrig-final-{clippy,default-workspace-check}.log`).
- Both native probe examples build with the final timeout budget. Bridge trial C
  passes the 200-frame wait and final independent readback. Door/flight retained
  builds precede the additional carrier refusal and timeout adjustments, as
  recorded in their manifests. No second full MCP-suite pass is claimed after
  the targeted timing changes.
- Voxrig: 114 unit tests/all targets, 1 doctest, all-target Clippy, format,
  pinned generator and package-list checks pass. Its local commit `b98785e`
  remains ready for separate upstream review; no push or PR was made.

Cargo checks were sequential with `--offline --locked -j1`. Live servers were
owned loopback vanilla instances, without a companion MOD; trial fixtures and
temporary operator grants were removed and each server stopped normally.
