# Virtual building design input

The LLM translates an agreed design into `BuildingDesignRequest` data. The public
`test_circuit_change` action `generate_building_design` expands that data into
ordinary immutable Blueprint parts and a candidate Assembly. It freshly reviews
the declared structure, retained equipment behavior and shared construction and
removal. There is no new building simulator or command-string authoring path.

The workflow is: describe intent → author explicit geometry → correct reported
errors → import/propose/review/adopt → observe the target and preview placement →
apply the authorized operation and read back. Offline authoring requires neither
a running Minecraft server nor a live site. `ok: true` means the declared checks
passed under the recorded model/context. It does not mean the design is attractive,
walkable, survival-buildable or already safe to place in an unobserved world.

Errors use the shared [structured review diagnostics](blueprint-review-diagnostics.md).
Inspect the failed requirement and its coordinate, expected/observed state and
available input/time evidence before changing geometry. Undetermined checks
need additional evidence or review budget; they are not proven violations.

## Example: a room with a window and open entrance

```json
{
  "blueprint": {
    "action": "generate_building_design",
    "request": {
      "namespace": "trial.windowed-room",
      "name": "Small room with an east window",
      "known_region": {
        "min": {"x": -1, "y": -1, "z": -1},
        "max": {"x": 5, "y": 4, "z": 5}
      },
      "parts": [
        {
          "name": "shell",
          "shapes": [{
            "kind": "shell", "material": "stone",
            "region": {
              "min": {"x": 0, "y": 0, "z": 0},
              "max": {"x": 4, "y": 3, "z": 4}
            }
          }],
          "cutouts": [
            {"min": {"x": 2, "y": 1, "z": 0}, "max": {"x": 2, "y": 2, "z": 0}},
            {"min": {"x": 4, "y": 1, "z": 2}, "max": {"x": 4, "y": 2, "z": 2}}
          ]
        },
        {
          "name": "window",
          "shapes": [{
            "kind": "blocks", "material": "glass",
            "positions": [{"x": 4, "y": 1, "z": 2}, {"x": 4, "y": 2, "z": 2}]
          }]
        }
      ],
      "spaces": [{
        "name": "room",
        "region": {"min": {"x": 1, "y": 1, "z": 1}, "max": {"x": 3, "y": 2, "z": 3}}
      }]
    }
  }
}
```

This design has 80 unique non-air blocks: 78 shell claims and two glass cells.
`fill` fills an inclusive rectangle; `shell` fills its one-cell-thick six-sided
boundary; `blocks` fills the explicit positions. Combining shapes can describe
an L-shaped floor, partitions or individual details. Shells include floor and
ceiling; subtract them explicitly for a roofless shape.

Cutouts subtract cells from their own part before parts are merged, regardless
of declaration order. They do not remove another part or carve equipment space.
Identical material overlaps are permitted and generate one physical write per
cell. Different material overlaps are errors; no last-writer precedence exists.
Every part keeps its independent exact material contract. Consequently
`result.parts` contains claim counts, and their sum may exceed
`result.unique_blocks`, the physical write count.
`parts` covers caller-authored geometry; equipment cells are included in
`unique_blocks` and `expected`, with their source identified by `component`.

`spaces` creates named exact-Air Blueprint requirements. A room or passage name
is descriptive; the checked property is Air at every declared coordinate.
Other unoccupied cells in `known_region` are also fixed Air. All solids must lie
strictly inside the outer boundary, leaving a guard cell on all six sides.

## Updating an existing design

`test_circuit_change(blueprint.action=generate_building_design_update)` accepts
`base_assembly_revision_id`, `previous` (the earlier `BuildingDesignRequest`),
and `design` (the revised input with a new namespace). Public MCP requires the
base and any selected equipment sources to have exactly one adopted proposal
and freshly passing runtime review. Keep the structured input with the design;
it is returned by generation, but is not recovered from descriptive names.

The earlier input is regenerated for comparison and checked against the exact
Assembly, every occurrence and retained obligation. Mismatched physical state,
spaces, routes, ports or additional custom requirements are refused with
`base_design_mismatch`; this entry does not silently weaken a general Blueprint.
Use an explicit general proposal for a design this input cannot express.

The result uses the ordinary `records`/`request` workflow. Its proposal descends
directly from the selected base Assembly, parent and building definition, with
the ordinary structural/physical `diff`. Equivalent unchanged direct children
keep their earlier immutable revision pins, including equipment's nested
sources. Changed named children descend from their previous definitions; new
or removed children and requirements are visible in the diff. The whole new
candidate is reviewed and its construction/removal simulated again.

For example, submit the earlier room as `previous`, and as `design` change the
namespace to `trial.windowed-room-v2` and the window material to `tinted_glass`.
The diff contains the two changed glass cells; shell and room pins are retained.
Generation publishes nothing. Import, propose, inspect and adopt explicitly.
Saved/restarted proposals still require fresh review for adoption.

Adoption leaves earlier Assemblies and all placed-instance source identities
unchanged. It does not upgrade a built structure or authorize its replacement.
Site editing needs a separately observed and previewed operation. Internal
comparison definitions and artificial empty baselines are not published as
the update's selected base.

## Equipment composition

The optional `component` pins one existing Assembly Revision. It can contain
multiple nested devices with an already declared joint behavior. Public MCP
requires exactly one adopted source proposal with an explicit runtime context
and a fresh passing review. It then rechecks the **combined** building/equipment
world, preserving every source occurrence, type binding, Law and physical route.
Source adoption or a previous isolated pass is not composition evidence.

```json
{
  "name": "door",
  "assembly_revision_id": "reference-door.state.v3",
  "source_anchor": {"x": 0, "y": 5, "z": 0},
  "target_anchor": {"x": 4, "y": 0, "z": 0},
  "rotation": "r270",
  "reserved_space": {
    "min": {"x": 0, "y": 2, "z": -3},
    "max": {"x": 0, "y": 11, "z": 3}
  },
  "exports": [{
    "name": "control",
    "port": {"instance": ["root", "mechanism"], "port": "control"}
  }]
}
```

Coordinates in `reserved_space` and export references belong to the **original
source**, while parts, spaces and `known_region` belong to the **new design**.
The transform maps source_anchor to target_anchor and rotates every physical
block, occurrence, terminal, route and input. The example wrapper origin is
(4,-5,0), and its `door.control` terminal is at (4,6,0). No automatic wiring is
created. Empty/omitted exports preserve the source Assembly's existing boundaries.

Reservation must be within the source's known rectangle and contain all its
non-air cells, declared inputs and exported terminals. The transformed complete
source context must fit the new known rectangle; its transformed reservation
must retain the outer air guard. For this reference door transform, a sufficient
new rectangle is (-1,-5,-2)..(9,8,3). Structure must explicitly avoid/cut out the
reservation. Permanent air spaces may not overlap it: an aperture that closes
belongs to the door's behavioral contract, rather than a permanent-Air room.

Building patterns exclude the declared reservation and remain fixed throughout
each committed microstep of equipment operation. Source requirements still apply
inside it. A too-small reservation or violated retained child blocks the result.
The same Assembly-wrapping and terminal-propagation helper is used by the earlier
`generate_building_with_door` enclosure convenience action.

## Errors, limits and execution

Semantic authoring errors return `ok: false`, an `errors` array and no candidate.
Each error has a `code` and `detail`; geometry conflicts also carry `item` and
`position`. For example, forgetting the window cutout returns `material_conflict`
for `window` at (4,1,2), with the other claiming part in its detail. Correct the
request and regenerate. Unknown fields/materials/shape kinds are rejected by
the typed MCP parameter schema. A physics/proof failure or exhausted review
budget returns `verification_not_established`, not a successful partial design.

Successful results contain `records`, `request`, `context`, `expected`, `parts`,
`spaces`, `unique_blocks`, optional component metadata and `verification`.
Generation never imports, adopts or writes blocks; the archive stays unchanged.
`live_world_verified` is false. Follow the same
[adoption, observed target, preview, readback and recovery path](blueprint-building.md#adoption-and-target-placement)
as ordinary Assemblies. Restart cannot turn saved validation into authority.
An occupied target guard or later world change blocks writes despite offline
success. Current target planning requires the complete region to be observed Air.

Bounds remain deliberate: 256 unique non-air cells, 8192 cells per known
rectangle, 64 parts, 64 named spaces, 256 shapes and 64 cutouts per part, 16384
expanded shape/space claims, and 64 component exports. Namespace IDs use 1..64
lowercase ASCII letters/digits/`._-`; part/space/component names are unique
1..32 lowercase letters/digits/`_-`, excluding `building`, `parent`, `empty`,
`clearance` and `root`. A design title uses 1..256 bytes. All ranges are inclusive
and coordinate transforms check arithmetic overflow. Use a new namespace for
a changed immutable design.

Geometry uses the existing six cube materials. New block physics, terrain
editing, survival inventory/hand placement, entities, fluid/NBT, large durable
jobs, automatic circuit routing and joint proof for separately selected active
Assemblies are outside this entry. It does not waive those limits based on an
LLM's design name or reservation. No running MCP/server deployment or new live
world comparison is part of this implementation.

## Verification evidence

On 2026-09-30, six new model tests and two new public-MCP cases passed. Twelve
existing building/door model tests and four public building lifecycle regressions
also passed. New public cases were repeated with Voxrig enabled. Model checks
include an independent geometry/material oracle, same-material overlap and
unique writes, retained-child failure, named-Air occupancy, resource bounds,
source immutability, nonzero source anchors and adoption across serialization.
The offline MCP fixture covers conflict correction, unchanged generation archive,
source adoption, target occupancy, placement/readback, restart diagnosis and
removal. Its transport snapshots are model-supplied; they are not independent
live physics observations. Formatting and Clippy with warnings denied passed
for the affected libraries/tests; builds/tests were offline, locked and serial.
