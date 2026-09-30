# Custom piston Assembly construction

This is the standard path for new supported piston Assemblies on Java 1.21.11.
Execution and review use v17 with [slime/honey adhesion](piston-adhesion.md), after
the [device-program migration](data-driven-block-runtime.md), support loss and
shape extensions. Archived acceptance is not reused as a current proof.
Horizontal, upward and downward bodies share one electrical world and queue.
The [roadmap](piston-general-placement-roadmap.md) and
[evidence report](piston-electrical-live-evidence.md) record the implementation
and the isolated server trials separately from broader conformance claims.

Command construction includes requested-state preprocessing and Java's
post-write callback order. Declared OFF observers use explicit `powered=true`
command initialization, which Java resets OFF without an insertion pulse. The
command and expected settled readback are both visible in the placement plan.
The [reference-door trial](reference-door-live-construction.md) verifies all
43 construction steps, two normal cycles, restart and all 43 removal steps.
The [reference-door relocation matrix](reference-door-relocation.md) additionally
verifies all four horizontal rotations at two target anchors, including exact
directional state readback and normal operation at each destination.

For new review/update requests, supply these explicit physical assumptions in
`behavior_context` (coordinates are illustrative):

```json
{
  "piston": {
    "known_region": {
      "min": {"x": -6, "y": -2, "z": -5},
      "max": {"x": 22, "y": 12, "z": 5}
    },
    "input_levers": [{"x": -3, "y": 1, "z": 0}]
  }
}
```

Optional `root_limits` bound computation. This request form chooses fresh
construction and `dustroute.piston-electrical-root-exploration.v17`; it is resolved
before saving the proposal. Saved archives still require their concrete profile
and initial-condition fields. `RuntimeBehaviorContext::fresh_pistons` and
`new_piston_runtime` are the corresponding library and execution entry points.

Supported electrical components include levers, stone buttons, redstone blocks,
dust, repeaters, torches, circuit comparators, waxed bulbs, observers and lamps,
including admitted conduction and piston quasi-connectivity. Passive identities
and supported states are listed in [physical declarations](../crates/dustroute-minecraft/src/physical/passive.rs).
Bodies may be ordinary or sticky in any of the six directions. Slime/honey add
branching block movement; support loss removes admitted attachments.
Entities, inventories, direct piston destruction and imported moving snapshots
without their exact history remain unsupported. This is command construction,
not survival inventory building.
Retracted ordinary/sticky bodies in all six directions can be moved as payloads,
including mixed and adhesive structures within the shared twelve-block limit. Extended bodies
and active carriers block movement. See [movable body evidence](piston-payload-conformance.md).
Behavioral review currently covers declared location-only repeated-settling and ordinary-door
obligations; unsupported bindings/contracts or exhausted budgets are not passes.

An Assembly adopted under the expanded electrical context can request a new
target through the existing public tool:

```json
{
  "assembly_revision_id": "example.assembly.v2",
  "assembly_target": {
    "source_anchor": { "x": 0, "y": 0, "z": 0 },
    "target_anchor": { "x": 32512, "y": 180, "z": 1000 },
    "rotation": "r90"
  }
}
```

Coordinates here are illustrative, not live placement authority. The source
anchor maps to the target anchor after the selected Y rotation. Actual cells,
occurrence origins/rotations, routes, known air and physical input positions
move together. Source revisions, child requirements and aliases keep their
identities. Coordinate-dependent notification order requires a fresh review
at the destination.

`new_placement` checks unique adoption and reruns review. It observes the
assisted player's dimension and the target server's feature flags. The current
contract is Java 1.21.11 with exactly the vanilla feature set; unknown settings
are refused. The complete transformed known region must be observed empty.
Policy limits still apply; the initial construction budget is at most 256 block
records. A new target is independent of the old captured-grounding workflow.

Construction uses supported retracted bodies and stable components. Supports
precede attached devices, and sources are installed after mechanics when their
support dependencies allow it. A declared occupied watched cell also
precedes its observer; internal observer facing points toward its output, so
the opposite side is used for this dependency. A watched Air cell does not
create a placement dependency. If support and observer-front dependencies cannot
be ordered, construction is refused. This precedence rule generates a candidate
order and does not replace runtime validation. Each insertion uses the same
electrical/physical callback runtime, including its actual command initialization.
The resulting settled world must equal
the fresh-world result used by review. Unsupported or non-settling sequences
are rejected before any live write. Export preserves exact admitted material
identity, wire strength and device properties; it does not use generic OFF
defaults as a substitute for saved state.

Use `show_operation`, inspect the proposed stages, then
`invoke_operation(confirm=true)`. Application rechecks adoption, source data,
server settings and the complete empty baseline. Every stage rechecks its
expected predecessor, writes its commands in order, waits for settling, and compares the
entire region with the modeled result. Freshly modeled commands whose synchronous
roots leave no pending work may share a stage, up to 32 writes; queued work keeps
a separate boundary. Removal and installation stages stay separate. Plans expose
`execution_batches` and `undo_execution_batches`. A failed group verifies none
of its intermediate writes. See [batch admission and measurement](construction-batching.md).
Long modeled waits are split into
bridge-supported requests before that readback. Failed transport, incomplete scans or
state differences consume the attempt and stop further writes. There is no
automatic retry or rollback after an uncertain result.

`undo_operation(confirm=true)` requires a verified application and the exact
constructed settled state, including unchanged surrounding air. Return inputs
and mechanisms to that state first. Teardown removes sources/components before
their supports and follows payload positions after retraction. It also verifies
every stage and the final empty region. A changed world cannot be cleared by
presenting an old plan.

Placement plans expire after five minutes and are process-local. Once an attempt
begins, its placed-instance record and stage progress persist separately without
a TTL. `manage_assembly` lists/reads those records, reobserves their world and
creates freshly reviewed conditional removal plans after restart; see
[persistent placed Assemblies](placed-assembly-management.md). Blueprint
adoption and its pinned definitions also persist. Retired directional/direct-only and electrical v1–v16 contexts are rejected; use
explicit new proposals and fresh review under the current execution context.

Implementation responsibilities and retained distinct placement paths are mapped
in [piston code organization](piston-code-organization.md). Module refactoring
does not by itself change the construction protocol or execution contract.
