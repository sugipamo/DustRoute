# Minecraft differential physics testing

Minecraft Java 1.21.11 is the source of truth for low-level redstone behavior.
DustRoute compares normalized observations rather than treating its simulator as
an oracle.

The Mineflayer E2E runner supports `block_trace` steps. It samples every two
game ticks (one redstone tick) and writes successful opted-in traces under
`.local/e2e-artifacts/`. These files are intentionally outside Git.

Each observation records the relative tick, cell-relative position, physical
block class, dust strength, powered state, and torch lit state. Weak and strong
conductor power are `null` for Minecraft observations because the client
protocol does not expose them. The comparator only compares properties that
both sources can observe.

The external XOR probe captures all four stable input combinations. Compare a
captured case with DustRoute using:

```console
cargo run -p dustroute-translate --example compare_external_xor_trace -- \
  .local/e2e-artifacts/external_library_xor_compatibility_probe-trace01.trace.json \
  0 1
```

The comparison reports the first mismatch in redstone-tick, position, property
order. This is the boundary for adding one physical rule and rerunning both the
semantic probe suite and Rust regressions.

## Promoting scheduler observations

`activate_trace` artifacts contain a second clock that is useful for timing
regressions: the input activation game tick, the relative game tick of every
observed block update, and Mineflayer's packet order within that game tick. The
promotion command removes absolute server ticks and writes a reviewed fixture
plus metadata pair under `crates/dustroute-translate/tests/fixtures/`:

```console
cd crates/dustroute-mcp/mineflayer
npm run promote:scheduler -- \
  ../../../.local/e2e-artifacts/observer_repeater_preview_only-latest.json \
  trace scheduler_1_21_11_observed_repeater_observer \
  "Capture repeater and observer timing on the pinned 1.21.11 server"
```

The promoted fixture records `evidence=observed`, but its scheduler profile
remains `profile_evidence=modelled`. Mineflayer exposes client-visible packet
order, not the internal Vanilla phase or causal queue, so a fixture must never
be used to claim a complete scheduler order. No-op packet observations are
retained. Promotion refuses to overwrite an existing fixture and should be
reviewed before it is committed.

## Comparing observed and modelled transitions

`transition_conformance` projects promoted scheduler observations and the
Minecraft engine's `TransitionTrace` into a shared coordinate-level format.
Callers provide the model trace's activation game tick; absolute transition,
event, state, and shape IDs are deliberately excluded. The comparison checks
relative game tick, same-tick order, position, and before/after block state and
returns `matched`, `mismatch`, or `unavailable` rather than a boolean.

Server-side `dustroute.vanilla-instrumentation.v1` artifacts use
`normalize_vanilla_instrumentation_artifact`. Their state transitions retain an
optional OrderedTick causal group, and their filtered neighbor-update callbacks
retain same-tick order, target/source evidence, and the same optional causal
group. Model transitions retain their source-local scheduler event group. When
the model has no neighbor callback trace, conformance reports that evidence as
`unavailable`; it never treats the missing model stream as a match or mutates a
scheduler profile. Conformance compares only relational cause/order evidence;
it never equates the observed OrderedTick number with a model EventId.

Same-tick order retains its provenance as either observed Mineflayer packet
order or modelled scheduler order. A missing Vanilla scheduler phase remains
`None` and is never inferred from the selected profile. A model transition
with multiple coordinate changes has unavailable coordinate-level order,
because the `BlockChange` vector is not evidence of Vanilla ordering.

Observed no-op packets remain in the normalized observation, but a
`TransitionTrace` cannot prove their model counterpart because it contains
only state-changing edges. Such a comparison is `unavailable`; event-level
conformance requires an `EventTrace`. Likewise, incomplete or failed model
traces cannot produce a complete-match claim. Comparison results are evidence
only and never mutate scheduler or delay profiles.

The current world-driven redstone boundary is intentionally narrower than a
full server capture. `PhysicsEngine::schedule_world_change` records an
explicit external block mutation, and `run_redstone_propagation` propagates
the six-neighbor steady-state subset for horizontal wire levels and lamp state,
with a bounded vertical wire rise/fall path over the imported wire-shape and
block-trait metadata, before handing a powered wire to the existing piston
runner. Changed wires enqueue existing diagonal offset-neighbor wires so the
path can reach a fixed point without claiming complete Vanilla topology. It
also models a single horizontal Repeater rear-input/front-output path: a neighbor update
queues a 1..=4 redstone-tick delayed `RepeaterTick`, measured as two game ticks
per redstone tick, and the tick revalidates the current input before changing
the output. The wire queue is iterated to a fixed point, with the engine's
microstep budget acting as the termination guard. This path is suitable for
regressions such as `Lever -> Wire -> Repeater -> Wire -> Lamp/Piston` and
three-branch vertical wire feeds; it must not be used as evidence for full wire
topology, repeater locking, comparator/observer timing, quasi-connectivity,
BUD, or Vanilla's complete neighbor-update order.
Incomplete observed regions and missing signal/connection properties remain
failed/unavailable rather than being normalized to an unpowered circuit.

### Current 1.21.11 scenario conformance

The integration test `transition_conformance` rebuilds both promoted scenarios
from their E2E layouts and runs the real simulators. The Repeater/Observer
case uses `RedstoneTickSimulator::step_transition`; its compatibility boundary
advances by two game ticks. The Piston case uses `PhysicsEngine` and its actual
`TransitionTrace`. Java `west` facing maps to the simulator's internal `east`
signal direction for Repeaters and Observers, while Piston facing is retained.

The piston regression now drives the model through
`PhysicsEngine::schedule_redstone_input` and
`run_redstone_piston_events`; the adapter removes only the external lever
transition because the promoted fixture treats player activation as its
baseline. The resulting differences are intentionally retained as
diagnostics:

- The Repeater/Observer model changes the first wire immediately at the input
  boundary (`0` modelled versus `1` observed game tick). Later transition
  ticks and states agree, while the observed packet order differs from the
  modelled event order for Repeater/wire and Observer/lamp pairs.
- The Piston model agrees on start tick, completion tick, destination block
  movement, and the typed stable `piston_head` endpoint. Moving-piston
  block-entity evidence is compared through the dedicated piston-state stream;
  coordinate order inside a multi-change completion delta remains
  `unavailable`, and the physics model/profile is not changed by this test.
- Player activation itself is outside the model transition trace. The adapter
  excludes that input coordinate explicitly and records the boundary as
  unavailable instead of silently dropping it.

Packet and scheduler ordinals are not compared as equal scalar IDs. The
comparator matches coordinate transitions first and then compares relative
ordering between same-tick transitions, preventing one order difference from
misaligning every subsequent state comparison.

The XOR reduction identified two missing distinctions:

- A lit torch strongly powers a solid block directly above it. Dust on that
  block can therefore read strength 15.
- Strong power does not chain from one solid conductor into another solid
  conductor. It may activate an adjacent receiver, but the second conductor is
  not a new powered conductor.
- Dust reads an adjacent strongly powered block even when its rendered wire
  shape has no arm in that direction. Wire-to-wire connectivity and receiving
  block power are separate rules.

The server-side Vanilla instrumentation now also records the execution boundary
of filtered `AbstractBlockState.neighborUpdate` callbacks. This is the first
evidence of chained propagation order; the callback's source position and any
unobserved target blocks remain unavailable, and the simulator does not yet
claim a matching neighbor-event trace.

After adding those rules, all four XOR input states compare without a mismatch
for 33 observations per state. Minecraft and DustRoute now agree that the
imported layout's output remains low for every input combination. The cell is
still rejected as XOR; matching the broken behavior is simulator validation,
not component-library promotion.

## Promoting a mismatch into regression evidence

After reviewing an E2E trace, promote it explicitly with a stable name and a
reason. Existing fixtures are never overwritten:

```console
cd crates/dustroute-mcp/mineflayer
npm run promote:differential -- \
  ../../../../.local/e2e-artifacts/example.trace.json \
  repeater_edge_mismatch \
  "Minecraft updates the output one redstone tick later"
```

This creates a tracked normalized trace and metadata pair under
`crates/dustroute-translate/tests/differential/`. Rust tests validate every
promoted pair. The metadata retains the Minecraft version, source artifact,
and the reason it was promoted, so a simulator correction can cite and retain
the original counterexample.
