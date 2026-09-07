# Observed 3×3 piston-door recognition

`dustroute_translate::recognize_observed_piston_door` is the read-only bridge
from a bounded `MinecraftSnapshot` to a structural description of an existing
two-sided 3×3 piston-door shape.  It is deliberately separate from
`PistonDoorScenario`: recognition does not create a runnable scenario, schedule
piston actions, or mutate a world.

## Recognition boundary

The recognizer accepts a snapshot whose declared bounds contain:

- a 3×3 grid of piston bodies facing one direction;
- a second 3×3 grid three blocks away, facing the opposite direction; and
- at least one ordinary door-material or transient piston-part observation on
  one of the two intervening planes.

The two piston grids and both panel planes must be inside the declared bounds.
Snapshot records outside the bounds, duplicate positions, invalid block state,
missing required geometry, and competing mechanical shapes are rejected with a
typed error.  A missing record inside a valid bounded snapshot has the same
meaning as `Air` in the existing snapshot contract; this is why an empty panel
can be distinguished from a panel that is outside the observed region.

The recognizer does not claim that the observed circuit is controllable.  The
returned `control` object is an evidence summary built from the existing
directional connectivity graph:

| Field | Meaning |
| --- | --- |
| `input_positions` | observed lever, button, pressure plate, or redstone-block sources |
| `wire_positions` / `repeater_positions` | observed control components in the snapshot |
| `piston_input_edges` | direct graph edges whose sink is one of the 18 recognized pistons |
| `reachable_pistons` / `unreachable_pistons` | graph reachability from the observed sources |
| `complete` | all 18 direct inputs are reachable, a source exists, and the mechanical shape does not touch the boundary |
| `unresolved` | reasons the control evidence is incomplete; quasi-connectivity and scheduler order are never inferred |

## JSON contract

Every successful result carries
`schema_version = "dustroute.observed-3x3-piston-door.v1"`.  The top-level
shape is presentation-neutral and can be reused by a future MCP tool or the
CLI without changing the recognition logic:

```json
{
  "schema_version": "dustroute.observed-3x3-piston-door.v1",
  "status": "recognized",
  "kind": "piston_door",
  "size": [3, 3],
  "orientation": {
    "status": "ambiguous",
    "open_direction": "south",
    "close_direction": "north",
    "width_axis": "east",
    "height_axis": "up",
    "alternatives": [
      {"open_piston_origin": {"x": 0, "y": 0, "z": -1}, "open_direction": "south", "close_piston_origin": {"x": 0, "y": 0, "z": 2}},
      {"open_piston_origin": {"x": 0, "y": 0, "z": 2}, "open_direction": "north", "close_piston_origin": {"x": 0, "y": 0, "z": -1}}
    ]
  },
  "state": "closed",
  "cells": [],
  "pistons": [],
  "control": {"complete": false, "unresolved": ["no controllable input source was observed"]},
  "observation": {"required_region_observed": true}
}
```

The example is abbreviated (`cells` and `pistons` are populated in real
output).  `orientation.alternatives` contains all directional interpretations
of the same mechanical shape, including the deterministic canonical entry in
the other orientation fields.  A symmetric two-sided layout therefore remains
recognizable while `orientation.status = "ambiguous"` and `unresolved` make it
explicit that a static snapshot cannot prove which side is semantically the
opening or closing pusher.  The canonical orientation is stable for replay and
serialization only; it is not a Vanilla behavior inference.

## State evidence

Each cell records its two possible panel positions, the observed current block
when it can be located, and one of these conservative states:

| Cell state | Evidence |
| --- | --- |
| `closed` | a door material occupies the closed plane and the open plane is empty |
| `open` | a door material occupies the open plane and the closed plane is empty |
| `transition` | both planes are occupied, or a `piston_head` / `moving_piston` is observed |
| `unknown` | neither plane provides a consistent door occupancy |

The top-level state is `closed`, `open`, `transition`, or `unknown` only after
all nine cell classifications are aggregated.  Missing or invalid piston
`extended` properties do not get fabricated: the affected piston is
`state = "unknown"`, `observation.piston_state_properties_complete` is false,
and the reason is retained in `unresolved`.  Stable occupancy can still be
reported when that separate piston-phase field is unavailable.

`PistonHead` and `MovingPiston` records are retained as
`cells[*].current_block` evidence during a transition.  Their movement,
collision, entity payload, and timing are outside this recognition contract.

## Deliberate limits

This goal stops at observed structural recognition.  It does not:

- infer arbitrary observed wiring into `PistonDoorScenario`;
- simulate an existing door or derive an open/close trace;
- publish an MCP-specific tool or mutate/repair the Minecraft world;
- infer complete Vanilla redstone topology, scheduler phase, QC/BUD, slime or
  honey attachment, entity movement, retrigger, interruption, or same-tick
  multi-piston rebasing; or
- promote the global piston capability beyond the existing partial/preview
  boundary.

If a future analysis requires one of those behaviors, the recognition result is
the evidence boundary and the separate behavior must be made a new goal.
