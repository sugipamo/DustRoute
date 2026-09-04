# Vanilla 1.21.11 instrumentation contract

DustRoute's Mineflayer traces are useful client-visible evidence, but they do
not expose the Java server's `OrderedTick` queue. This document defines the
stronger artifact that a server-side hook must produce before DustRoute can
make an internal scheduler or Piston-state claim.

## Current environment boundary

The server-side probe is maintained in the separate companion repository
`/root/DustRoute-minecraft-instrumentation`. It is a Fabric Loom project pinned
to Minecraft `1.21.11`, Yarn `1.21.11+build.6`, Fabric Loader `0.19.5`, and Loom
`1.17.20`. The probe hooks the vanilla scheduler, redstone/Piston state paths,
the filtered `AbstractBlockState.neighborUpdate` execution path, and relevant
server outbound block updates, then emits raw NDJSON; the Rust crate in this
repository remains the normalization, validation, and comparison boundary.

The local probe runs with `online-mode=false`, so it does not require Microsoft
authentication. An authenticated client or an online-mode server is outside
this goal and must not be required for a fixture to pass.

Validate an artifact with:

```console
cargo run -p dustroute-translate --example validate_vanilla_instrumentation -- \
  path/to/vanilla-1.21.11-instrumentation.json
```

The validator rejects packet-only evidence, wrong Minecraft versions, retained
absolute server ticks, unclaimed scheduler phases, backwards execution ticks,
and stable Piston observations without a head state.

## Artifact contents

The Rust schema is `dustroute.vanilla-instrumentation.v1` in
`dustroute_translate::vanilla_instrumentation`.

An accepted artifact contains:

- `instrumentation`: hook method, mapping namespace, build identifier, and
  exact target class/member;
- `clock`: `game_tick` units anchored at `server_input_received`, with absolute
  ticks omitted from fixture identity. Relative ticks are signed so a retained
  bounded pre-roll can be represented as `-2`, `-1`, and so on;
- `input`: activation position and the first server-side redstone and optional
  packet update ticks;
- `ordered_ticks`: execution records with trigger tick, execution tick, priority,
  signed sub-tick value, position, block name, event kind, and an optional phase
  whose evidence must be `observed_internal`; execution ticks are monotonic per
  scheduler stream, while the signed sub-tick value is not a clock;
- `state_events`: coordinate before/after states, including no-ops when the
  hook observes one, their server-side source, and an optional
  `ordered_tick_sequence` causal reference;
- `neighbor_updates`: filtered server-side neighbor callback observations with
  target state, source block, orientation, `notify`, same-tick callback order,
  and an optional `ordered_tick_sequence`. The callback does not expose a
  source position; the target filter and that missing relation remain explicit
  evidence boundaries;
- `piston_states`: stable, moving, and completion snapshots separating body,
  head, moving block, and block-entity presence/extension state. They also
  retain the optional OrderedTick causal reference;
- `completeness`: independent completeness flags for each evidence family.
- `capture`: lifecycle, heartbeat policy, sequence-gap/write counters, and a
  per-stream `complete`/`partial`/`unavailable` declaration. This is the
  authoritative capture-integrity layer; the legacy `completeness` booleans
  remain for compatibility.

The schema intentionally requires `evidence=observed_internal` for production
artifacts. A hand-written or model-derived example must not pass validation as
server evidence.

The bounded offline input/Piston capture is tracked as
`crates/dustroute-translate/tests/fixtures/vanilla_1_21_11_offline_piston_input.json`.
It deliberately marks `ordered_ticks` unavailable because no scheduler callback
fell inside that bounded interval; the separate raw probe capture still records
the scheduler hook outside the scenario window.

`crates/dustroute-translate/tests/fixtures/vanilla_1_21_11_bounded_capture.json`
is a small contract fixture for bounded lifecycle metadata, signed pre-roll
ticks, and partial stream declarations. It is synthetic evidence and does not
change any scheduler or delay profile.

## Recommended server hook

The hook should be built against the exact 1.21.11 server and pinned mappings.
Its output should be produced in this order:

1. Capture the server game tick when the player interaction or block input is
   accepted.
2. Capture the first redstone state mutation and first network block update
   separately; do not use Mineflayer receipt time as the server input origin.
3. Instrument the ordered-tick scheduling/execution boundary and emit the
   trigger tick, execution tick, priority, server sub-tick order, target
   position, and block/event type.
4. Keep the OrderedTick callback context active only around its server callback.
   State and Piston hooks may then attach the callback's artifact sequence as
   `ordered_tick_sequence`; observations outside that callback must leave the
   field unavailable.
5. Observe Piston body/head block states at stable boundaries. During motion,
   record the moving extension state and the PistonBlockEntity fields needed to
   identify the pushed block, direction, extending flag, and source flag.
6. Observe `AbstractBlockState.neighborUpdate` at its server-side execution
   boundary to preserve chained propagation order. Do not infer a source
   coordinate from the callback's source block argument.
7. Normalize promoted evidence relative to the first server input and retain
   the raw capture outside Git for forensic replay. `--keep-absolute-ticks` is
   for inspection only and must not be promoted.

The exact Java member names must come from the chosen 1.21.11 mapping set. Do
not infer them from Mineflayer names or from a different Minecraft release.

## Evidence rules

`OrderedTick` execution order and Mineflayer `blockUpdate` packet order are
different evidence families. The former may support scheduler claims; the
latter supports only client-visible update ordering. A packet order must never
populate `sub_tick_order` in an internal instrumentation artifact.

The Vanilla field is a signed Java `long`; negative values and signed resets
are valid runtime observations. The contract therefore preserves the raw value
instead of coercing it to the simulator's unsigned local order. Equal values
are also valid; callback sequence is retained separately, while only a
backwards `execution_game_tick` within one scheduler stream is rejected.

Likewise, a one-game-tick gap between server activation and a Mineflayer packet
does not justify changing `DelayProfile` until the server-side input and
redstone mutation ticks are captured. A missing phase or missing Piston head is
represented as unavailable or mismatch in transition conformance, not filled
by the selected `SchedulerProfile`.

Bounded raw capture is an evidence window, not a claim that the rest of the
server was idle. The probe can retain a short pre-roll before the first input
and a drain after it; it records `closed_early` if the server stops before the
drain is observed. Heartbeat omission is a transport optimization only. Raw
sequence numbers are allocated before filtering, so sequence gaps and the
declared suppression/eviction/write counters must be reviewed together. A
truncated artifact or writer error is partial evidence and must not be used to
change a scheduler or delay profile.

An `ordered_tick_sequence` is a local reference within the same promoted
artifact. It links a state, neighbor, or Piston observation to the OrderedTick
callback that was active when the server hook emitted it. It is not a game tick,
packet ordinal, or cross-artifact identifier. If the referenced callback is
outside a bounded promotion window, normalization removes the link and records
the causal evidence as unavailable.

## Promotion and review checklist

Before adding a captured artifact to tracked fixtures, verify:

- the server JAR hash and mapping/build ID are recorded;
- the scenario layout and input position match the corresponding E2E fixture;
- ordered tick execution sequences are complete and monotonic per scheduler;
- input receipt, first redstone change, and first packet update are separate;
- stable Piston snapshots include an explicit `piston_head` state;
- moving snapshots identify whether a block entity was present;
- no absolute server tick or player-specific identifier is retained;
- the artifact passes the Rust validator and receives a human-reviewed reason.

For transfer, `scripts/compress_ndjson.py` in the companion repository creates
a lossless gzip copy while preserving the raw NDJSON as the primary forensic
source. The normalized artifact must retain the stream completeness map even
when the raw file is transferred separately.

Until those checks pass, the current DustRoute model and scheduler profiles
remain unchanged.
