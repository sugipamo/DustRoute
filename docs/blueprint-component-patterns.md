# Component bodies, equipment and physical patterns

A NOT realization can consist of **one conducting block and one redstone torch**.
The lever used to exercise it is a separate realization. Candidate generation
first enumerates physical placements; verification then decides whether each
placement satisfies the explicitly selected behavior. The name NOT is never
used as evidence.

## Body and environment

`BlueprintReductionRequest.scope` has two forms:

- `{"kind":"whole_assembly"}`: compatibility default; every occupied actual
  position participates in the cost and search.
- `{"kind":"component","body_positions":[...],"environment_instances":[...]}`:
  the listed distinct occupied positions initially belong to the body. Other
  occupied positions are fixed external equipment. Actual input levers must be
  external. Environment occurrences are explicit pinned Blueprint inclusions,
  retained and freshly checked in every candidate.

The body may move into already known empty space. It cannot modify or absorb an
external occupied cell. Every new occupied cell must belong to the candidate's
composed body definition, preventing blocks from disappearing from the count.
All component ports belong to the body. Generated source definitions contain the
body layout only; the candidate Assembly retains the full actual environment.
Shared supports can belong to both the body and an environmental interpretation.
Physical positions count once in the complete Assembly.

Component candidates also retain actual routes involving an environment occurrence
or leaving the original body, together with external boundary names and port
references. These contracts are freshly checked against the candidate's complete
state. Removing a body wire shared by an external route therefore fails that
route even if the selected NOT relation still passes. Supplied alternatives
cannot erase or rewrite the retained contracts to obtain a smaller passing result.
Generated body boundary names receive an `optimized.` prefix when necessary to
avoid taking an existing external name.

If a protected route or boundary refers to an occurrence that is not retained,
component search rejects the request and reports that explicit parent
reconnection is required. It does not invent new routes. Use the existing parent
proposal workflow to prepare that change. This restriction on automatic search
does not fix input/output coordinates in the behavioral type.

`baseline_blocks` and `best.occupied_blocks` measure the selected cost domain.
`baseline_total_blocks` and `best.total_occupied_blocks` always measure the entire
Assembly. In the component regression, the body changes from 6 blocks to 2,
while the complete Assembly changes from 7 to 3. The separate lever remains one
actual block; its shared attachment support is already counted in the body.
Both existing built-in NOTs also reduce to a block and torch, plus the external
lever when counting the complete verification setup.

These are contextual results. Excluding equipment from cost does not remove its
physical effects or prove independence from every possible surrounding circuit.
Connecting a candidate to a new parent requires fresh checks of that complete
state. Old parents and source definitions are never modified by this search.

## Direct device outputs and the lever type

`device_output` is an output-only Blueprint port kind for the supported direct
signal producers: torches, levers and redstone blocks. It observes the device's
output without requiring a dust or conducting observation block. `wire` still
requires actual dust, and `block_power` still requires a conducting terminal.
A port label does not prove connectivity: supplied routes are checked against
physical directions. In particular, a torch strongly powers a conductor above
it; a lever powers its attachment support. These directions share the existing
electrical model's target calculation.

The independent definitions in
[`primitives-v2.json`](../crates/dustroute-library/blueprints/primitives-v2.json)
include:

- `dustroute.lever.wall.v2`: one lever, with a support supplied by its environment.
  Its `static_type_bindings` explicitly bind `dustroute.type.lever.v1` to `out`,
  so its actual terminal must be a lever even without any connected consumer.
- `dustroute.lever.wall.v1`: the unchanged older realization, retained for pinned
  references. It has no self identity obligation and is not silently upgraded.
- `dustroute.type.lever.v1`: `{"kind":"block_kind","block_kind":"Lever"}`.
  This checks physical block identity at a producer terminal, independent of its
  current powered state. It does not claim a logical function or timing behavior.
- `dustroute.type.device-output.v1`: the generic direct-device signal interface.

A Blueprint can declare a snapshot obligation on its own terminal:

```json
"static_type_bindings": [{
  "type_revision": "dustroute.type.lever.v1", "port": "out"
}]
```

These bindings accept `Signal`, `BlockKind` and `BlockPattern` contracts. Patterns
use offsets relative to the resolved terminal's local orientation. They inspect
actual Assembly state, not equality with every block in the source layout.
Both powered and unpowered levers satisfy the identity type; a torch or redstone
block does not. Unknown state remains undetermined. Review checks every retained
occurrence, including nested/shared children, and both validation and adoption
enforce these obligations. Behavioral contracts still use `behavior_bindings`.

A consumer can separately require lever identity when appropriate. A NOT input
does **not** require it: compatible other sources may connect. Arbitrary-history
verification currently drives actual external levers as its supported test
arrangement; it does not yet universally stimulate every possible upstream
circuit. This limitation is not encoded as a NOT type requirement.

Static type bindings require at least `dustroute.blueprint-catalog.v8`. Direct-device
ports and block-kind types alone still need v7. Earlier catalogs retain their
minimum schema version and remain readable; relabeling newer features with an
older schema is rejected. The original v7 primitive archive remains available.
Proposal histories containing these bindings need at least `dustroute.blueprint-updates.v2`,
including when only an unadopted candidate has the new obligation and the embedded
catalog still uses v7. Relabeling these histories as v1 is rejected. Older histories
without newer features remain v1 and readable. Physical law requirements raise
these minima to catalog v9 and proposal-history v3; see the
[law requirement contract](blueprint-mcp.md#physical-law-requirements).
The legacy physical-cell adapter rejects obligations and interfaces it cannot
retain. Adopting the stronger lever realization is an explicit revision change;
existing parents continue to reference their previous revision.

## Enumerate placements, then verify state transitions

The existing `test_circuit_change` endpoint accepts
`blueprint.action: "enumerate_layouts"`:

```json
{
  "blueprint": {
    "action": "enumerate_layouts",
    "request": {
      "search": {
        "base_state": "example.base.v1",
        "target_instance": ["root", "child"],
        "target": {
          "behavior_type": "example.not.type.v1",
          "inputs": {"a": "input"},
          "outputs": {"out": "output"}
        },
        "candidate_revision": "example.pair.v1",
        "candidate_state": "example.pair.state.v1",
        "scope": {
          "kind": "component",
          "body_positions": [
            {"x":0,"y":0,"z":0}, {"x":1,"y":0,"z":0}
          ],
          "environment_instances": [{
            "instance":"control", "revision":"dustroute.lever.wall.v2",
            "origin":{"x":-1,"y":0,"z":0}
          }]
        },
        "behavior_context": {
          "profile":"dustroute.dust-single-torch-block-effects.v1",
          "initial_condition":"fresh_construction",
          "dust_law":"dustroute.law.dust-strength.v1",
          "torch_law":"dustroute.law.torch.java-1-21-11.v1",
          "max_electrical_iterations":128,
          "input_drivers":[{
            "port_position":{"x":0,"y":0,"z":0},
            "port_kind":"block_power",
            "lever_position":{"x":-1,"y":0,"z":0}
          }]
        },
        "alternatives":[]
      },
      "support_position":{"x":0,"y":0,"z":0}
    }
  }
}
```

Import the referenced definitions, laws, type and base state first. This example
assumes those source port names and body positions exist in the supplied state.
`support_position` selects an actual conducting body block as the family anchor;
it is not part of the type requirement. The current family requires component
scope and one input/one output. It places one torch on the top or one of the four
horizontal faces, with its input observing the support's block power and its
output observing the torch directly. Candidate IDs receive distinct `.top`,
`.east`, `.west`, `.south`, `.north` suffixes. Existing IDs are never overwritten.

Every candidate retains the same law Revisions, execution profile and declared
fresh initial condition. Histories and scheduled events are kept by the existing
runtime; universal repeated-settling verification includes later input changes
and burnout recovery. Two static ON/OFF snapshots are insufficient evidence.
The type still requires eventual correct output when an input is held, with no
new timing bound or fixed endpoint position.

Each result includes a status and, when constructible, candidate data and its
fresh review. Unsupported construction, collision with fixed equipment, unknown
space, unfinished exploration or incompatible obligations cannot pass. With a
ceiling-mounted external lever, all five placements pass the explicit NOT
relation. With the wall lever on the west face, that face is unavailable to the
torch and is reported as undetermined; the other four can pass. Changing the
requested relation to identity does not certify these patterns.

The layout, binding and elapsed-time budgets are shared across enumeration.
`family_exhausted` means all five placements were attempted, not that every
candidate passed. `global_minimality_proven` is always false. Enumeration does
not publish, reconnect, adopt, or write Minecraft. Select a passing candidate,
prepare an explicit parent-update proposal with the complete new state and
context, then review and explicitly adopt through the existing MCP workflow.
Persistence and adoption re-run verification; a saved report is not authority.

## Checks and scope

Regression tests cover the cost boundary, fixed equipment, shared supports,
external route/boundary preservation and shared-wire damage, self-bound lever
identity in both powered states, rotation and unknown observations, unchanged
legacy references, direct-device directions,
all five physical placements, incompatible relations, archive version checks,
and the real MCP enumeration/optimization/proposal/restart/adoption sequence.
The physical profile remains dust and at most one torch. General upstream
stimulus adapters, multi-torch scheduling, piston laws, entity handling and new
live-world placement remain subsequent work.
