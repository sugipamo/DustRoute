# Minecraft 1.21.11 update-model research

This note records the constraints that DustRoute must preserve before it attempts
timed or zero-tick circuit equivalence. It is a design input, not a claim that the
current simulator already reproduces every vanilla edge case.

## Main conclusion

A zero-tick pulse is not a component with a delay of zero. It is an observable
transition that rises and falls within one game tick because individual updates
inside that tick are processed in an order. Consequently, a simulator that only
computes one fixed point per game tick loses information required by pistons,
observers, and other order-sensitive consumers.

DustRoute therefore needs two levels of time:

- `game_tick`, for externally visible server progression;
- an ordered event sequence within a game tick.

The event sequence must retain its cause and ordering evidence. Treating all
same-tick updates as an unordered set would make direction-dependent and
location-dependent contraptions impossible to validate.

The scheduler foundation now records three coordinates for an event:

- `game_tick`;
- a coarse `phase` (`external`, `neighbor_update`, `scheduled_tick`,
  `block_event`, `block_entity`, or `observation`);
- `sub_tick_order`, the deterministic sequence within that phase.

The phase order is an explicit model boundary and is not, by itself, a claim
that Mineflayer packet order exposes the complete vanilla scheduler. A future
version profile must define the relative order for each event source before it
can be used to prove zero-tick behavior.

The queue preserves insertion order within a phase. A zero-delay child may be
queued in the same phase or a later phase of its parent game tick; attempting
to move back to an already-processed phase returns a structured
`CausalOrderViolation` instead of producing a time trace that runs backwards.
This is a deterministic safety rule for the model, not a substitute for a
version-specific vanilla ordering table.

The current Observer slice follows this boundary: it records the front-face
state before and after each simulated mutation/tick, schedules a pulse for the
next redstone tick, and exposes the back-face output as a strong level-15
source for one redstone tick. Live Mineflayer recordings now preserve packet
order within the observed game tick as `sub_tick_order`, together with an
explicit `event_kind`, `cause`, and `source`. Translation also accepts
server-side OrderedTick causal references for
`dustroute.vanilla-instrumentation.v1` artifacts. Causal IDs remain
source-local and are compared only by their grouping relationship, never as
cross-source scalar IDs. This is ordering/provenance evidence only:
Mineflayer does not expose the internal vanilla scheduler cause, so it is
intentionally not a claim that every same-game-tick or zero-tick interaction is
reproduced.

## Evidence from the 1.21.11 implementation surface

The Fabric Yarn 1.21.11 mappings expose the following vanilla structures:

- [`RedstoneWireBlock`](https://github.com/FabricMC/yarn/blob/1.21.11/mappings/net/minecraft/block/RedstoneWireBlock.mapping)
  owns a redstone controller, accepts a wire orientation during update, and has
  explicit neighbor and offset-neighbor update paths. It also has a separate
  check for the Redstone Experiments feature. Wire behavior must therefore be
  versioned and feature-set aware.
- [`AbstractRedstoneGateBlock`](https://github.com/FabricMC/yarn/blob/1.21.11/mappings/net/minecraft/block/AbstractRedstoneGateBlock.mapping)
  separates input/output calculation, internal update delay, target update, and
  powered-state update. Repeaters and comparators cannot be modeled as a single
  combinational transfer function.
- [`WorldTickScheduler`](https://github.com/FabricMC/yarn/blob/1.21.11/mappings/net/minecraft/world/tick/WorldTickScheduler.mapping)
  collects tickable work into ordered queues before executing it.
- [`OrderedTick`](https://github.com/FabricMC/yarn/blob/1.21.11/mappings/net/minecraft/world/tick/OrderedTick.mapping)
  retains both `triggerTick` and `subTickOrder`. Same-game-tick insertion order is
  part of the state that a faithful model needs to preserve.
- [`ServerWorld`](https://github.com/FabricMC/yarn/blob/1.21.11/mappings/net/minecraft/server/world/ServerWorld.mapping)
  maintains a separate block-event queue.
- [`PistonBlock`](https://github.com/FabricMC/yarn/blob/1.21.11/mappings/net/minecraft/block/PistonBlock.mapping)
  separates the decision to extend, movement attempt, and actual movement. A
  piston is therefore both a redstone consumer and a mechanical/block-event
  producer.

Community technical documentation agrees with the observable consequence: a
zero-tick pulse turns on and off within one game tick, and different consumers
may observe it differently. The [Minecraft Wiki zero-ticking tutorial](https://minecraft.wiki/w/Tutorial:Zero-ticking)
also distinguishes scheduled block ticks, block events, and update-chain-based
ordering. This source is useful for test scenarios; exact behavior should still
be checked against the target server version.

## Required engine boundaries

The Minecraft crate should eventually own:

1. block states and world storage;
2. immediate neighbor-update propagation;
3. the scheduled block-tick queue;
4. the block-event queue;
5. deterministic same-tick ordering metadata;
6. per-block transition rules;
7. a trace of every state transition and its cause.

## Shape and WorldDelta contract

Piston movement is a geometry transition, not a collection of independent
`setblock` calls.  The Minecraft crate now exposes the following boundary:

```text
Shape --WorldDelta--> Shape'
```

`ShapeId` is a content-derived cache key for block placement, orientation,
wire geometry, and piston extension state.  Signal-only fields such as
`powered`, `power_level`, and ordinary `power`/`lit` observations are excluded
from that identity; changing a lever level therefore does not invalidate a
geometry cache.  The ID is only a fast key: an atomic apply still checks every
coordinate's exact `before` block.

`WorldDelta` contains:

- coordinate-level `BlockChange { position, before, after, reason }` entries;
- logical `BlockMove { from, to, block }` relations for mechanical consumers;
- a conservative `RegionSet` dirty neighborhood (including adjacent support,
  wire-rise, and observer-facing positions);
- a version-independent `DeltaCause`, such as a piston extend/retract.

Push chains collapse a coordinate that is both a source and a destination into
one final before/after entry.  This keeps validation atomic while preserving
the complete move list for later incremental graph updates.  Applying a delta
validates the parent shape, all before states, and duplicate coordinates on a
staged world, then commits the clone in one operation.  A failed plan cannot
leave a partially moved chain.

`PistonPlan` is read-only and produces this delta without mutating the source
world.  The supported subset remains deliberately narrow: horizontal
normal/sticky pistons, ordinary exact blocks, and the Java 12-block push limit.
Stable `PistonHead` blocks and discrete `MovingPiston` block-entity metadata
are now explicit in the start/completion deltas. Continuous animation and
collision, quasi-connectivity, BUD, slime/honey, entities, and zero-tick
ordering remain outside the executable contract and must stay `PreviewOnly`.

Live piston planning must also carry a complete static observation boundary.
`PistonPlanningContext` treats an absent block as Air only inside its declared
known region and returns `unknown_space` when the movement ray leaves that
region. The legacy `plan_piston` function is intentionally retained for
synthetic worlds; it must not be used as proof that a partial live scan is
clear. `PhysicsEngine::with_piston_planning_region` carries the same boundary
into the built-in event runner, so a live-derived engine cannot accidentally
fall back to the unchecked planner.

The physics engine records both coordinate transitions and a
`ShapeTransition { from, to, delta, cause }` for each accepted phase.  A piston
Block Event first produces a transient transition to `Extending` or
`Retracting`, installing typed `MovingPiston` carriers, then queues a
completion event whose rebased `WorldDelta` resolves the carriers to stable
ordinary blocks and `PistonHead` state.  This is the seam for a future
`AnalysisState::update(WorldDelta)` path.  The current graph builder may
conservatively rebuild its affected scene; it must not claim incremental
completeness until dirty-region invalidation and stable component remapping are
implemented.

The transition-first ledger is `PhysicsEngine::transition_trace()`. Each
`TransitionRecord` has a monotonic `TransitionId`, full-state endpoints
(`StateId`), geometry endpoints (`ShapeId`), the triggering `EventId`, the
ordered `PhysicsTime`, and an optional `elapsed_from_previous`. The first
accepted transition has no predecessor; later records distinguish a same-tick
edge (`Zero` with its order delta) from a positive game-tick interval. A
successful event that leaves the world unchanged is retained separately in
`PhysicsEngine::event_trace()` as `NoTransition`; it must not be mistaken for
an omitted observation. `step_transition()` executes exactly one accepted
event and returns both records, while `run_until_idle_checked()` is the
compatibility loop over that API. This makes the state-changing edge, rather
than the tick counter, the canonical unit without removing existing tick
callers.

`StateId` includes observed signal state, whereas `ShapeId` intentionally
excludes ordinary signal values for geometry-cache reuse. Neither identity
includes a pending event queue. `PhysicsEngine::checkpoint()` now captures the
queue, logical time, scheduler counters, policy, and trace cursors for exact
in-memory restoration. `PhysicsEngine::execution_state_key()` is the
history-independent comparison view; it includes pending event payload/order
without treating trace-only IDs as physical state.

The redstone-driven piston boundary is exposed separately through
`schedule_redstone_input` and `run_redstone_piston_events`. It accepts one
typed external source edge, applies only the supported direct source mutation,
and emits a horizontal `NeighborUpdate` for each adjacent directionally
connected piston. The neighbor phase re-queries all four direct inputs before
queuing a `PistonExtend` or `PistonRetract` Block Event, so an unrelated
powered input is not lost when another source turns off. Duplicate Block
Events are retained in `EventTrace`; an event that arrives after the piston has
already entered the requested or moving state is a successful `NoTransition`,
preserving evidence without applying the motion twice.

The bounded world-driven propagation MVP is exposed through
`schedule_world_change` and `run_redstone_propagation`. A WorldChange is the
explicit mutation boundary: it is applied as a validated `WorldDelta`, then
the six neighboring positions (and the changed position when it may be a
wire) receive deterministic `NeighborUpdate` events. The supported kernel
reevaluates horizontal redstone-wire levels (15 down to 0 with one level lost
per wire), plus the bounded vertical rise/fall relation over the
`wire_rise_connection` and `strong_power_drives_dust` block traits. Changed
wires also notify existing diagonal offset-neighbor wires, and the queue is
fed back until it reaches a fixed point. A basic horizontal Repeater path is
now also executable: its rear input is sampled on a neighbor update, a
`RepeaterTick { expected_powered }` is scheduled after 1..=4 redstone ticks
(2 game ticks per redstone tick), and only a still-current input may change the
front output. Typed `RedstoneInput` events can use this runner as well, so a
synthetic `Lever -> Wire -> Repeater -> Wire -> Lamp/Piston` path no longer
needs the caller to identify the intermediate repeater or piston action.

The MVP is bounded by `with_piston_planning_region` when a complete observed
region is supplied and by the engine's per-tick microstep budget. Coordinates
outside that region, missing observed wire shape, missing repeater direction or
delay, or missing observed signal state are explicit errors; they are never
inferred as Air or unpowered. The kernel does not claim full Vanilla wire
topology, repeater side-locking, comparator/observer timing, quasi-connectivity,
BUD, vertical piston activation, or continuous mechanical downstream
propagation. Those remain versioned follow-up contracts.

`run_until_idle_checked` treats one event as the unit of failure isolation. If
the handler rejects an event, a delta fails its before-state check, or an
outcome violates the phase contract, the triggering event and its scheduler
order counter are restored at the front of the queue with the original ID;
the previous logical time and World remain unchanged. Events successfully
processed earlier in the same run are not rolled back, so a returned error is
still a committed prefix plus a pending rejected event. Both ledgers expose a
`failed` status for that prefix, and only a drained queue is marked `complete`.
The piston-only runner additionally preflights the whole queue and refuses a
mixed queue before applying any piston event.

## Timing contract for mechanical transitions

The legacy IR fields expressed delays as whole redstone ticks.  That is useful
for repeaters, but it is not a sufficient contract for a piston: a piston has a
start event, a moving interval, and a completion event.  A nominal 1.5-redstone
tick observation must not be rounded to `1` and presented as a verified delay.

The temporal projection therefore also carries a zero-capable
`TransitionDelay`:

- `same_game_tick` means an ordered transition whose `game_tick` is unchanged;
- `exact_game_ticks` represents a measured deterministic interval;
- `game_tick_range` represents a bounded variable interval;
- `unavailable` means that the current observation boundary cannot verify the
  interval.

`PhysicsTime.phase` and `PhysicsTime.sub_tick_order` are the causal order for
same-game-tick changes. This lets a future 0-tick implementation represent a
non-empty transition sequence without pretending that all changes happen at
the same instant. The engine now enforces a per-game-tick microstep budget in
addition to the total event budget. Repeated-state detection and a complete
versioned scheduler profile remain required before a zero-delay feedback loop
can be considered semantically supported.

Events inserted after a later phase in the current game tick are rejected at
execution time with `CausalOrderViolation`; live callers can use
`schedule_external_in_phase_checked` to receive the same failure at insertion
time. The infallible compatibility scheduler may still retain such an event,
but it can never invoke its handler or mutate the World.

For the current piston subset, the plan remains a stable-shape structural
operation and the transition timing is explicitly `unavailable` at the IR
boundary. The physics engine's provisional profile separates a `0..1`
game-tick activation-side range from a `2` game-tick stable block-state
completion interval. The activation range is informational until an upper
scheduler supplies an observed Block Event; it is not silently consumed by
`schedule_piston_action`. The stable interval is not the continuous
moving-piston animation duration. The tracked
`scheduler_1_21_11_observed_piston.json` fixture records one measured start and
stable-completion interval, while the repeater/observer fixture records the
corresponding device pulse edges. These are packet-order regressions only:
start/completion short pulses, re-trigger behavior, and the internal Vanilla
phase order still require independent evidence before a timed or MCP-ready
result can be claimed.

Physical IR must consume observations produced by this engine, but the engine
must not depend on PhysicalScene, Gate IR, or optimizer types.

## Per-block implementation policy

Each supported block gets its own module under `dustroute-minecraft/src/blocks/`.
The central `BlockKind` match remains exhaustive so adding a kind without a
behavior profile fails at compile time. Initial profiles only classify current
properties and update mechanisms. Later revisions add versioned transition
functions and vanilla-derived scenario tests without changing IR APIs.

The remaining full-engine implementation should cover, in order:

1. redstone wire immediate update chains;
2. basic repeater delay and directional scheduling (implemented in the bounded
   world-driven runner);
3. redstone torch scheduled changes and burnout state;
4. repeater side-locking and 0-tick behavior;
5. comparator scheduling and side inputs;
6. piston block events, quasi-connectivity, and movement;
7. same-tick observer event ordering and movable block interactions.

## Validation policy

YouTube and technical-community builds are valuable scenario sources, especially
for discovering counterexamples. They are not sufficient as the sole oracle.
Every adopted scenario should be captured as:

- an input world fixture;
- an action at a known game tick;
- an ordered server-observed block-state trace;
- a simulator trace comparison;
- the exact Minecraft version and enabled feature set.

Until these traces exist, DustRoute should mark circuits containing block-event
or unsupported scheduled behavior as `temporal verification unavailable`, rather
than approving an optimization from a final steady state alone.
