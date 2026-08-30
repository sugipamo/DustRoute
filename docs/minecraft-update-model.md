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

Physical IR must consume observations produced by this engine, but the engine
must not depend on PhysicalScene, Gate IR, or optimizer types.

## Per-block implementation policy

Each supported block gets its own module under `dustroute-minecraft/src/blocks/`.
The central `BlockKind` match remains exhaustive so adding a kind without a
behavior profile fails at compile time. Initial profiles only classify current
properties and update mechanisms. Later revisions add versioned transition
functions and vanilla-derived scenario tests without changing IR APIs.

The first temporal implementation should cover, in order:

1. redstone wire immediate update chains;
2. redstone torch scheduled changes and burnout state;
3. repeater delay, locking, and scheduling;
4. comparator scheduling and side inputs;
5. piston block events, quasi-connectivity, and movement;
6. observer pulses and movable block interactions.

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
