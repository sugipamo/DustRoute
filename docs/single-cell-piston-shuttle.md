# Single-cell piston shuttle control

This document freezes the first control slice for the non-compact 3×3 piston
door reference. It is intentionally one-dimensional: one normal piston opens
the cell, a second normal piston closes it, and the door block is moved exactly
one block per actuation.

## Control contract

The external input is one persistent Lever. A `LeverPulseSequence` event
represents the generic edge-to-pulse boundary needed by the bounded runner:

```text
off → on
  lever state becomes on
  selected open source: high, then low after a fixed width

on → off
  lever state becomes off
  selected close source: high, then low after a fixed width
```

The pulse sources are ordinary redstone blocks. Each source drives a real
horizontal path:

```text
pulse source → RedstoneWire → Repeater → Piston
```

The event does not schedule a piston action directly. It schedules typed
redstone input edges; the existing wire, repeater, neighbor-update, and piston
Block Event handlers perform the rest of the propagation.

The current fixture uses one source per edge and an eight-game-tick pulse
width. The width is part of the fixture contract rather than a claim about a
Vanilla pulse generator. A future fanout fixture may provide multiple source
positions without changing the event shape.

## Mechanical layout

For the selected cell (`x=0`, `y=1`):

```text
z=-4       z=-3       z=-2       z=-1      z=0       z=1       z=2       z=3       z=4       z=5
open       open       open       open      door      open      close      close      close      close
source  →  wire    →  repeater → piston  →  block  →  empty    piston  ←  repeater ←  wire   ←  source
```

The open piston faces South and the close piston faces North. Both are normal
pistons and both finish retracted. The open-side extension moves the block
from `z=0` to `z=1`; the close-side extension moves it back.

## Trace expectations

The trace must retain the causal path, not only the final world:

```text
LeverPulseSequence
  → RedstoneInput(high)
  → wire NeighborUpdate / state change
  → RepeaterTick(high)
  → piston NeighborUpdate
  → PistonExtend Block Event
  → PistonComplete
  → RedstoneInput(low)
  → RepeaterTick(low)
  → PistonRetract Block Event
  → PistonComplete
```

For the close edge the same shape is used with the close source and piston.
Stable completion requires an empty pending queue, both pistons retracted, and
the door block at the expected plane. Repeating an already stable Lever value
is a retained no-op and must not create another pulse.

## Scope and stop boundary

This goal includes the generic edge-to-pulse event, one-cell fixture, horizontal
wire/repeater propagation, normal-piston movement, trace assertions, and
stable-edge idempotence.

It does not include Observer, Comparator, repeater locking, QC/BUD, zero-tick
behavior, Slime/Honey, interruption/reversal, multi-piston completion rebasing,
or nine-cell fanout. If any of those are required to complete this one-cell
contract, the implementation must stop and report the missing dependency
instead of adding a speculative model.

The executable coordinate fixture is
[`single_cell_piston_shuttle.json`](../crates/dustroute-translate/tests/fixtures/single_cell_piston_shuttle.json),
and its integration tests are
[`single_cell_piston_shuttle.rs`](../crates/dustroute-translate/tests/single_cell_piston_shuttle.rs).
