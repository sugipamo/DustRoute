# Reference 3×3 piston door

This document freezes the first concrete door mechanism for the 3×3 target. It
is a reference for replay and capability discovery, not a claim that the
current redstone runner already drives a complete door from a lever.

## Selected mechanism

The reference is a deliberately non-compact, two-sided normal-piston shuttle:

```text
closed:  [door block at z=0]
             ↑ south-facing normal piston at z=-1 (open pusher)

open:    [passage at z=0] [door block at z=1]
                                      ↑ north-facing normal piston at z=2
                                      (close pusher)
```

The diagram is per door cell; the 3×3 panel is the Cartesian product of
`x=0..2` and `y=0..2`.

Each cell has two independent pistons:

- the south-facing **open pusher** at `(x, y, -1)` extends, moving the exact
  ordinary block from `z=0` to `z=1`, then retracts as a normal piston;
  - the north-facing **close pusher** at `(x, y, 2)` later extends, moving the
    block from `z=1` back to `z=0`, then retracts as a normal piston.

The candidate control layout places one repeater directly behind each pusher:
`(x, y, -2)` facing south for the open channel and `(x, y, 3)` facing north
for the close channel. Their rear input anchors are the next block outward
(`z=-3` and `z=4`). These coordinates make the output-to-piston relation
explicit; the shared wire/fanout that feeds those anchors remains an unverified
electrical layer.

Normal retraction is intentional: it removes the piston head without pulling,
so the moved block remains one block away. This avoids sticky-piston pull
semantics, slime/honey, piston-to-piston movement, and vertical piston support.
The resulting reference uses 18 normal pistons and nine movable ordinary
blocks. It is large, but it is a mechanically valid baseline for the user's
“first make it move, optimize later” objective.

The coordinate-level fixture is
[`reference_3x3_noncompact_piston_shuttle.json`](../crates/dustroute-translate/tests/fixtures/reference_3x3_noncompact_piston_shuttle.json).

## Stable states

| State | Door blocks | Open pushers | Close pushers | Passage plane |
| --- | --- | --- | --- | --- |
| `closed` | `(x,y,0)` | retracted | retracted | occupied by the panel |
| `open` | `(x,y,1)` | retracted | retracted | `z=0` is clear |

During each movement batch the active nine pistons temporarily expose a
`PistonHead`/`MovingPiston` transition. The fixture only treats the post-
completion state as a stable door state.

## Mechanical replay timeline

The replay uses the current default profile (`0..1` game-tick activation range,
two game ticks from the piston Block Event to stable completion). The current
delta contract keeps a strict parent `ShapeId`; starting nine pistons in one
game tick would make later completion deltas stale. Therefore the executable
fixture serializes the nine cells in each phase, leaving one game tick between
cells. The table records the first scheduled tick and the final completion
tick for each phase:

| Batch | Trigger label | Pusher role | Action | Scheduled tick | Stable completion |
| --- | --- | --- | --- | ---: | ---: |
| `open_extend` | lever-on derived pulse | open | extend | 0 | 26 |
| `open_retract` | delayed return pulse | open | retract | 27 | 53 |
| `close_extend` | lever-off derived pulse | close | extend | 54 | 80 |
| `close_retract` | delayed return pulse | close | retract | 81 | 107 |

Each phase contains nine independent piston actions. A same-tick nine-piston
batch is recorded as `missing` in the fixture because batch rebasing is not
implemented; the serial form is the supported mechanical prefix. The schedule
is a mechanical replay contract; it deliberately does not infer the internal
Vanilla scheduler phase from packet order.

The electrical timeline has four corresponding repeater-output groups. The
extend groups use a one-redstone-tick path and the delayed-return groups use two
redstone ticks. All four are marked `out_of_scope` until a concrete lever edge,
pulse generator, and 18-way wire fanout are observed and implemented.

## Current DustRoute classification

| Segment | Status | Boundary |
| --- | --- | --- |
| Nine horizontal normal extensions, one exact ordinary block each | **supported_serial_only** | `PhysicsEngine`/`PistonPlan` with a complete planning region |
| Nine normal retractions after the open push | **supported_serial_only** | Head removal without a sticky pull |
| Nine horizontal north-facing close extensions | **supported_serial_only** | Same one-block push rule in the opposite direction |
| Nine final normal retractions | **supported_serial_only** | Stable closed panel and head removal |
| Nine independent piston completions in one game tick | **missing** | Strict `WorldDelta` parent-shape validation requires a batch rebase |
| Lever ON/OFF → two timed pulses per side | **out-of-scope** | No pulse sequencer or normal-piston edge generator in the bounded runner |
| One lever → 18 independently delayed piston channels | **missing** | The repeater model is one horizontal rear-input/front-output path, not a verified 18-branch topology |
| Complete Vanilla wiring/order, QC/BUD, moving interruption/reversal, slime/honey, entities | **out-of-scope** | Explicitly excluded by the Goal |

The `control_contract` in the fixture records the electrical prerequisite: 18
fanout channels and a two-redstone-tick delayed return path. Those values are a
design requirement, not a measured Vanilla scheduler claim.

## Stop condition applied

The declared work ends at the first electrical gap. This change adds only the
reference fixture and the bounded serial mechanical replay test. It does **not**
add a multi-piston batch rebase, pulse sequencer, torch inverter, full repeater
fanout, or lever-driven 3×3 E2E.
If a later replay requires one of those capabilities, implementation must stop
and the missing contract must be reported before expanding the Goal.

For context, public build references also illustrate why a real 3×3 door should
not be reduced to “three pistons”: beginner-oriented designs use nine sticky
pistons and direct repeaters, while other flush designs use twelve sticky
pistons and staggered timing. Those pages are candidate-design evidence only;
the coordinate contract above is intentionally independent of their exact
layouts.
