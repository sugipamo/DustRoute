# Minecraft update and transition model

DustRoute separates observed Minecraft evidence from its executable model.
This document describes repository contracts, not a complete specification of
Vanilla internals. Java 1.21.11 is the current live comparison target.

## World, events and transitions

Power calculation and update scheduling are distinct. The engine retains an
ordered event queue and records state-changing edges:

```text
(state_before, shape_before) -- event / elapsed time / order -->
(state_after, shape_after)
```

`PhysicsEngine::step_transition` processes one event. Successful no-ops remain
`NoTransition` event records; they are not invented state transitions. The
`run_until_idle*` methods drain this interface. A rejected event restores its
queue position, counter and logical time, while marking the accepted trace
prefix failed. A committed prefix is not a complete run.

`TransitionRecord` carries StateId and ShapeId endpoints, EventId, PhysicsTime,
coordinate changes and causal evidence. `EventTrace` includes no-ops and failed
attempts. Exact game ticks, same-tick order and unavailable/ranged timing must
remain distinguishable. Packet ordering is not internal scheduler phase evidence.

## Identity and checkpoints

| Identity | Content |
| --- | --- |
| `StateId` | World content including observed signal state |
| `ShapeId` | Geometry/cache identity; excludes ordinary signal levels |
| `ExecutionStateKey` | History-independent world, pending event payload/order, logical time and scheduler configuration |
| `ExecutionCheckpoint` | Restorable engine state including queues, counters, profile and trace cursors |

A World-only hash is not an execution checkpoint. State held inside a supplied
event-handler closure must be managed separately by its caller.

## Scheduler and compatibility clock

`SchedulerProfile` defines phase order, insertion order and zero-delay child
placement. Profiles are versioned and carry evidence classification. The
deterministic default and `MinecraftJava1_21_11Modelled` profiles remain modeled,
not a claim of fully observed Vanilla scheduling. Block-specific activation,
movement and repeater/observer durations belong to block behavior models.

The translate simulator has its own event-clock boundary through
`RedstoneTickSimulator::step_event` (`step_transition` is an alias).
`advance_tick` remains a compatibility projection. The two-game-tick boundary
and its child events do not establish complete Vanilla sub-tick behavior.
`simulate_transition_trace` is the transition-oriented entry point;
`simulate_behavior_trace` remains a compatibility adapter.

MCP trace responses retain event projections and exact game-tick/order evidence
when available. Consumers must not reconstruct exact timing from rounded
redstone-tick fields. Optimization contracts compare the declared edge/timing
semantics rather than silently substituting a steady-state truth table.

## Geometry changes and pistons

`WorldDelta` applies coordinate before/after changes and logical block moves
atomically to a staged world. Parent shape, exact before states and duplicate
coordinates are checked before commit. Dirty neighborhoods include local
support/wire/observer dependencies; they do not establish a complete incremental
analysis implementation.

A piston start installs transient state and moving carriers. At completion,
local dependencies are revalidated and a fresh delta is generated against the
current world. Independent completions can proceed; conflicting local movement
is rejected. This does not relax the global `WorldDelta` guard.

`PistonPlanningContext` treats missing cells as air only inside its known region.
A movement ray leaving that region is unknown, not free space. The legacy
synthetic-world planner is not live observation proof. Supported mechanical
subsets and regression evidence are in [low-layer validation](piston-low-layer-validation.md).
General placement still requires the shared validation boundary; the exact
1×2 preset has a separate narrow [immutable proof](piston-door-mcp-v1.md).

## Evidence and limits

Use [differential testing](physics-differential-testing.md) for normalized client
observations and [instrumentation](vanilla-instrumentation.md) for the stronger
server-side artifact schema. Capture completeness, no-op evidence, clock origin
and source provenance remain explicit. Missing streams cannot count as matches.

The model does not claim complete QC/BUD, zero-tick, interruption/reversal,
slime/honey, entity collision, arbitrary block entities, or distant update-order
behavior. Unsupported or unavailable categories must remain visible rather
than being silently promoted to executable placement or functional proof.
