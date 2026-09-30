# Small buildings through the shared Blueprint workflow

The first building capability authors a rectangular enclosure with independently
pinned floor, walls and optional flat roof. Its declared structure includes the
opening, empty interior and a one-block air perimeter on all six sides.
There is no separate building physics engine or unchecked bulk-write route.

## Request

Use the ordinary `test_circuit_change` tool:

```json
{
  "blueprint": {
    "action": "generate_building",
    "request": {
      "namespace": "trial.enclosure",
      "width": 5,
      "depth": 5,
      "height": 4,
      "floor_material": "stone",
      "wall_material": "glass",
      "roof_material": "stone",
      "roof": "flat"
    }
  }
}
```

Dimensions are outside dimensions, including the floor at local y=0 and optional
roof at y=height-1. Each dimension is 3..16; the resulting non-air geometry must
also fit the existing 256-block custom-Assembly budget. A flat-roof design needs
height at least 4 to fit a two-high passage. With `roof: "none"`, the last layer
is wall instead, and height 3 is sufficient.

Materials are `stone`, `cobblestone`, `smooth_stone`, `smooth_quartz`, `glass` and
`tinted_glass`; all three material fields default to stone. These are existing
stateless cube capabilities. Unknown materials are rejected during MCP parameter
decoding, before generation. No implicit block
substitution or expanded simulation capability is supplied by the generator.

The default opening is one wide and two high, near the middle of the north
wall at local z=0. Override it with `entrance: {"offset": 1, "width": 2,
"height": 2}`. The opening starts at y=1; it must fit strictly between the two
wall corners and below a roof. For even widths the default opening uses the
lower central x coordinate. The namespace contains 1..64 lowercase ASCII
letters, digits or `._-`; use a new namespace when authoring a different design.
An existing immutable ID is never overwritten.

The response supplies `result.records`, `result.request`, the pinned execution
`context`, exact `expected` geometry, part counts and fresh structural/construction
checks. The 5x5x4 example has 25 floor, 30 wall and 25 roof blocks. Its build and
removal each use 80 ordered steps grouped into three reviewed batches, at most
32 commands each. Batch boundaries follow the shared model's proof of immediate
quiescence, rather than arbitrarily grouping mechanisms.

## Adoption and target placement

1. Import `result.records` using `blueprint.action: "import"`. The common
   `BlueprintRecords` bundle is authoring data, not a review certificate.
2. Remove the generated proposal `id` from `result.request`, then pass the
   remaining request to `blueprint.action: "propose_update"`.
3. Review the returned `operation_id` with `show_operation`. Adopt using
   `invoke_operation` with `confirm: true` and
   `blueprint_decision: {"action": "adopt"}`. Review and adoption rerun the checks,
   including after a process restart. Generation/import/adoption do not change
   Minecraft.
4. Use `new_placement` with the candidate `assembly_revision_id` and
   `assembly_target: {"source_anchor": {"x":0,"y":0,"z":0}, "target_anchor": ...,
   "rotation": "r0"}`. Rotations `r90`, `r180`, `r270` are also supported.
   The target anchor denotes the transformed local floor corner. Obtain target
   coordinates from the user's selected site or physical observation.
5. Preview with `show_operation`, then apply within the authorized scope using
   `invoke_operation(confirm: true)`. Existing write policy, target/version checks,
   complete verified observations and readbacks still apply.

The target region must be completely observed **air**, including the guard layer
below the floor. This milestone can build in an empty volume; it does not clear
terrain, replace foundations or embed a building in existing ground. Java 1.21.11
and observed vanilla feature flags are required by the existing placement route.
Any occupied guard/interior or intervening world change blocks the corresponding
write. Air-clearance obligations remain part of the design during diagnosis.

The retained part patterns check exact native materials. The whole-building
pattern also checks every declared air cell. Input-free structures are reviewed
by observing every committed initialization microstep until the common runtime
has no pending work and an idle input boundary. A nonterminating mechanism,
unprocessed work, unavailable space or exhausted budget is not a structural pass.
An Assembly with controllable inputs still needs its declared behavior reviewed;
this proof does not silently close missing input branches or behavioral types.

## Diagnosis and interrupted work

Placed buildings use the same durable Assembly instance records as circuits.
After restart, `manage_assembly(action: "list" or "get")` finds the saved instance.
Use `diagnose` for a fresh comparison against the pinned design, including human
damage. It reports missing/material/unexpected blocks and the relevant Blueprint
occurrences without writing Minecraft.

For a partial placement or repairable damage, create a **new**
`manage_assembly(action: "plan_reconstruction", instance_id: ...)` operation from
fresh observations, preview it and apply it. The current shared repair strategy
is teardown and rebuild, rather than a minimum one-block patch. Incomplete
observations, unsupported states and material counts beyond the declared
inventory block reconstruction. Identical material added by someone else can
still fit that inventory; ownership is not inferred. Review every affected block,
including unexpected positions, before authorizing the teardown. Configured
placement-size policy applies to the combined removal and rebuilding work.

Progress and uncertain attempts survive restart, but executable plans do not.
A partially submitted batch verifies no prefix of that batch and remains
`needs_inspection`. New diagnosis does not rewrite the old attempt as successful.
There is no automatic replay or resume. Exact verified completion permits a new
`plan_removal` operation or same-process conditional undo.

## Buildings containing a typed piston door

Use `test_circuit_change(blueprint.action="generate_building_with_door")` to
compose an enclosure with an existing, **uniquely adopted** door Assembly. The
source proposal must have an explicit runtime context and freshly pass review.
The selected occurrence must retain an observed `PistonDoor` binding. Its
definitions and completed-operation input contract are not rewritten.

For the existing Bobiloosky reference design, after importing and adopting its
ordinary-door proposal, the request is:

```json
{
  "blueprint": {
    "action": "generate_building_with_door",
    "request": {
      "building": {
        "namespace": "trial.door-house",
        "width": 9, "depth": 3, "height": 8,
        "entrance": {"offset": 3, "width": 3, "height": 3}
      },
      "door": {
        "assembly_revision_id": "reference-door.state.v3",
        "instance": ["root", "mechanism"],
        "behavior_type": "dustroute.type.piston-door-3x3.v1",
        "rotation": "r270",
        "reserved_space": {
          "min": {"x": 0, "y": 2, "z": -3},
          "max": {"x": 0, "y": 11, "z": 3}
        }
      }
    }
  }
}
```

Read the actual immutable Assembly ID from the catalog; IDs in this example
describe the reference fixture, not a preinstalled adoption. The generator is
not tied to that fixture. It reads the selected definition and its typed
aperture, aligns the nine cells with the requested opening, then authors a new
wrapper around the source Assembly and its original children and routes.

The initial supported mount is one block deep in local z=0, strictly between
the north-wall corners and below the roof. The entrance must explicitly be
3x3. `reserved_space` is in **original Assembly coordinates**; it must be a
known, bounded rectangle containing every initial mechanism block and all
selected input/output terminals. The generator removes shell cells only inside
that transformed reservation. Parts cannot be removed entirely. The complete
building plus mechanism stays within 256 non-air cells and 8192 known cells.

The example contains 43 door cells and 125 fixed building cells. The transformed
motion space is x=1..7, y=-3..6, z=0; the aperture is x=3..5, y=1..3, z=0. The
mechanism extends below the floor, so the target must include empty space below
it as well as all other declared clearance. The exact roof above it remains
fixed. This is composition into an empty volume, not insertion into existing
terrain or an existing occupied building.

The body, outer parent and Assembly boundary export `door_control` as an input
and `door_aperture_0_0` through `door_aperture_2_2` as outputs. These are
`BlockState` aliases to the original lever and aperture terminals, not dust
signal outputs or newly routed wires. `result.door` gives transformed positions,
the source IDs, reservation, rotation and `closed_when_powered` polarity. An
outside controller or more accessible switch needs a separate explicit design
and fresh verification; generation does not add one.

The building's exact fixed pattern excludes reserved component cells. Every
other cell, including air, is audited through the complete **combined**
operation graph and every committed microstep. Thus a door moving outside its
reservation, disturbing fixed structure or failing an inherited child condition
cannot pass. Reservation alone certifies nothing, and isolated adoption does
not grant a combined-world pass. Insufficient proof budgets remain undetermined.
Commands during movement remain outside the ordinary-door input contract.

Import, propose, review/adopt and target placement use the same workflow above.
Source adoption does not adopt the generated building. Shared construction,
per-batch readback, durable instances, diagnosis, reconstruction and removal
continue to apply. Diagnosis compares against the generated settled reference;
a normally closed door can differ from an initially open reference, so do not
treat every reported difference as human damage or automatically rebuild it.
There is no new live-state classifier or automatic door-operation tool here.

## Evidence and limits

The model tests check geometry independently, all four rotations/relocations,
missing material and occupied passage/interior/guard, initialization transitions,
budgets and undeclared input behavior. Public MCP tests use an offline transport
fixture for generation, restart/adoption, guarded batched placement, human-damage
diagnosis, reconstruction, removal and partial-submission recovery. The fixture
is not a Minecraft physics oracle. `live_world_verified` is false for generation;
this change does not supply a new live-building comparison or restart the running
server/MCP process.

On 2026-09-30, all nine building model tests passed, including every admitted
material and the exact 256-block boundary. Twenty-two existing construction,
runtime adoption, static-pattern and flying-machine generation regressions also
passed. All three building lifecycle tests passed with Voxrig enabled. Default
features also passed the placement/recovery scenarios and the targeted
input-rejection test. Three existing public
flying-machine generation/import/adoption tests passed with Voxrig enabled.
These are 37 distinct passing tests, separate from live-server evidence.

Formatting passed for library, translate and MCP. Clippy passed with warnings
denied for these packages' libraries and tests in both default and Voxrig-enabled
configurations. Validation used `--offline --locked -j 1`, with one test thread.

The door-composition follow-up also passed 43 distinct checks on 2026-09-30:
39 model regressions (including three composition cases and five existing
ordinary-door cases), and four building lifecycle cases using the Voxrig-enabled
offline MCP transport fixture. The new public case checks unique source adoption,
side-effect-free generation, independent building adoption after restart,
rotation/relocation, 168-step placement and readback, restart diagnosis and
complete removal. Its fixture uses model-derived readbacks and is not a live
Minecraft comparison. Clippy covers the libraries and tests in default and
Voxrig-enabled configurations; builds/tests remain offline, locked and serial.

Arbitrary architecture, wood palettes, stairs and ordinary hinged doors in generated buildings,
larger multi-part jobs, existing terrain/circuit integration, survival inventory,
entities, fluids, falling-block execution and block-entity contents are future
work. See the [building roadmap](blueprint-building-roadmap.md).
