# Transition-first migration

DustRoute now treats a state-changing transition as the primary unit of
temporal evidence. A transition is an ordered edge

```text
(state_before, shape_before) --[event, elapsed, order]-->
(state_after, shape_after)
```

The game/server tick remains an observation coordinate and a compatibility
clock. It is no longer the identity of the behavior being compared.

## Completed migration stages

1. **Contract audit**

   Event processing, state changes, geometry changes, and tick projections are
   separate responsibilities. A successful no-op is evidence (`NoTransition`)
   rather than a missing event. A rejected event is put back at the front of
   the queue with its original ID; a committed prefix is never presented as a
   complete run.

2. **Minecraft transition ledger**

   `dustroute-minecraft` exposes `TransitionId`, `TransitionRecord`,
   `TransitionTrace`, `EventRecord`, and `EventTrace`. A record includes full
   `StateId` endpoints, geometry-only `ShapeId` endpoints, the triggering
   `EventId`, `PhysicsTime`, coordinate changes, logical moves, and a causal
   delta reason. `StateId` includes observed signal state; `ShapeId` remains a
   geometry/cache key.

3. **Single-step engine boundary**

   `PhysicsEngine::step_transition` accepts one event and returns one
   `TransitionStep`. `run_until_idle` and `run_until_idle_checked` are loops
   over this boundary, so existing callers continue to work. `state_id()`,
   `pending_event_count()`, and `processed_events()` make checkpoint and
   diagnostics possible without exposing the mutable queue. A successful
   no-op remains an event-level record and does not become a fabricated
   transition.

4. **Execution checkpoint**

   `PhysicsEngine::checkpoint()` captures the World, pending queue, scheduler
   counters, logical time, piston profile/boundary, and all trace cursors.
   `restore()` reproduces the exact next execution step. The separate
   `execution_state_key()` includes pending event payload/order but omits
   trace-only IDs and history, so it can be used for state reuse without
   confusing a World-only `StateId` with a complete execution state.
   State held only inside a caller-provided event-handler closure is outside
   the checkpoint and must be deterministic or snapshotted by the caller.

5. **Piston start/completion**

   A piston Block Event records the state-only `Extending`/`Retracting` edge,
   then schedules a completion event. Completion records the atomic movement
   delta and its measured game-tick interval. A zero completion interval is
   ordered by phase/sub-tick metadata. The activation-side delay range remains
   informational until an upper scheduler supplies a real activation event.

6. **IR and simulator projection**

   `dustroute-ir::TransitionTrace` projects legacy `BehaviorTrace` events into
   records with an opaque ID, before/after values, elapsed ordering, exact
   game-tick coordinates when available, and a coarse scheduler phase. The
   shared `TraceStatus` distinguishes `in_progress`, `complete`, and `failed`
   prefixes. The conversion back to `BehaviorTrace` is explicit and lossless
   for the current evidence. `dustroute-translate::simulate_transition_trace`
   is the new transition-oriented entry point; `simulate_behavior_trace`
   remains the compatibility adapter.

7. **MCP and optimization surfaces**

   MCP transition responses retain the old `events` array and add a
   transition-first `transitions` array with IDs, before/after values, and
   elapsed same-tick or tick intervals. Each entry also carries exact
   `game_tick`/`phase` evidence when available, and `logical_elapsed` keeps
   game-tick intervals even when the compatibility `redstone_tick` is rounded.
   The response envelope exposes `time_unit`, exact live duration when known,
   and `TraceStatus`. Optimization reports expose
   `original_transitions` and `candidate_transitions`; the new
   `exact_transitions` timing contract compares state-changing edges and their
   sampled times without requiring every intermediate sample to match.

## Time and failure semantics

`TransitionElapsed::Zero`/`same_tick` means that the game tick did not advance;
the order delta is still significant. `ExactGameTicks` (Minecraft) or
`ExactTicks` (IR/MCP) is measured in the trace's declared unit. IR records also
carry `LogicalElapsed` when exact game ticks are available, so a live event at
game tick 103 is not reduced to a redstone-tick bucket. A range or `Unavailable`
value is not an immediate transition and must not satisfy an exact timing
contract. The first transition has no predecessor and therefore has no elapsed
value.

`StateId` intentionally does not include the pending event queue yet. A
checkpoint retains the queue, current logical time, scheduler counters, and
profile together with the world before restoring an execution. The
history-independent `ExecutionStateKey` is the appropriate comparison seam;
`StateId` remains a World-only content hash.

An event step is transactional with respect to the scheduler and World. If a
handler, delta validation, or phase-order check rejects the event, the event
and its sub-tick counter are restored and the previous logical time remains
unchanged. The accepted prefix is retained for diagnostics, while both traces
are marked `failed` so a caller cannot mistake that prefix for a complete run.
After a successful drain, the traces are marked `complete`.

## Compatibility map

| Existing surface | Transition-first role |
| --- | --- |
| `PhysicsEngine::run_until_idle*` | Repeated `step_transition` calls |
| `PhysicsEngine::trace()` | Coordinate-level legacy projection |
| `PhysicsEngine::shape_transitions()` | Geometry/delta compatibility projection |
| `BehaviorTrace` | IR compatibility projection |
| `ScenarioTrace.events` | MCP wire-compatible event/provenance view |
| `MacroTransitionCase.*_outputs` | Sampled compatibility data |
| `MacroTransitionCase::*_transition_edges()` | State-changing edge view |

No existing tick fields are removed in this phase. A hard cut is only safe
after live event provenance, queue checkpointing, and all downstream consumers
can consume transition records directly.

The compatibility `redstone_tick` fields on scenario traces remain display and
comparison coordinates. Live recordings additionally preserve `game_tick` and
phase/order evidence; consumers that need causal timing must use those exact
fields rather than reconstructing them from a rounded redstone tick.

## Deliberately deferred

This migration does not claim formal support for 0-tick circuits, quasi-
connectivity, BUD, piston moving entities, slime/honey interactions, entity
collisions, or a complete vanilla scheduler order. Those require a separate
versioned physics contract and real 1.21.11 regression traces. The current
microstep limit and phase-order check are safety bounds, not a proof of those
behaviors.

## Next hardening gate

The next safe increment is to feed measured activation-delay ranges into the
transition ledger and to add a versioned scheduler profile. Only then should
the translate simulator be rewritten internally to emit transitions directly
rather than adapting its existing tick loop. MCP can then promote
`exact_transitions` from sampled output comparison to a server-backed contract
without changing the public compatibility fields.
