# Blueprint block-count reduction

`dustroute_optimize::blueprint_reduction::reduce_blueprint_blocks` and
`test_circuit_change(blueprint.action="optimize")` search for a smaller physical
realization of an explicit behavioral type. The implementation uses the same
fresh contextual verification as Blueprint adoption; it never infers a function
from a classification label.

The first regression reduces a NOT Assembly from **7 actual blocks to 3**, moves
both input and output terminals, removes the old shared decomposition from the
new candidate, and retains the original immutable records. Both counts include
the actual lever used for input, every support and every wire. The proof checks
arbitrary input changes in the single-torch block-effects model. This is an
improvement, not a proof of a globally minimal NOT or a new live-world capture.
Both existing built-in NOT realizations (`torch-top` and `torch-side`) also
reduce from **5 to 3 actual blocks**, including their input lever. Direct-device outputs now allow the torch itself to be observed. In component
scope the NOT body is **2 blocks**, with the lever counted separately. See
[component patterns](blueprint-component-patterns.md) for the boundary, independent
lever type and placement enumeration.

## Optimization domain and preserved requirements

By default, the **complete supplied Assembly** is the search and counting domain.
The explicit `scope.kind: "component"` mode separates body positions and fixed
external equipment; `occupied_blocks` then counts the body and
`total_occupied_blocks` counts the complete physical state.
`target_instance` selects the source interpretation whose physical ports bind
the target type; it does not itself define the cost boundary. Select component scope explicitly
for a body-level search. Surrounding blocks included in that
Assembly participate in physics and cost. Shared physical positions are counted
once, independently of how many Blueprint occurrences claim them. Explicit air
counts zero.

Within the rewritten body, only the selected `RepeatedSettling`, `Periodic` or
`FiniteBurst` requirement is preserved by generated candidates. In component mode,
retained environmental obligations, external routes and boundaries also remain
mandatory; a passing target cannot hide damage to them. Its coordinates, timing and decomposition
are not additional constraints. Ordinary `Signal` and `BlockPattern` meanings
remain unchanged; this search does not reinterpret them as logical functions.

The baseline check explicitly projects the supplied actual state to the selected
type. Its `baseline_scope` is `selected_type_in_complete_actual_assembly`.
It does not certify all old children or other old obligations. Generated
candidates replace those interpretation claims with one newly declared target
binding, and the report lists `prior_interpretations` for review. This is an
explicit new decomposition; the old source and Assembly records remain unchanged.
When an explicit alternative retains children or additional requirements, every
retained claim, including `static_type_bindings`, must pass. Protected component
connections and boundaries cannot be dropped or rerouted by an alternative.
Unretained external endpoints require an explicit parent reconnection proposal
before component search. A parent's pass never overrides a child's result.

Candidate comparison pins the type Revision, dust/torch and implicit spatial law Revisions, execution
profile, fresh-construction condition and electrical solver budget. Actual input
controls and observed terminals must be valid under the selected physical model.
No hidden input device is added. The input-driver checks from
[repeated-settling adoption](repeated-settling-adoption.md) remain mandatory.

## Candidate families and limits

The automatic family enumerates deletion subsets of the actual non-air blocks,
from smaller to larger, with optional relocation of one retained conducting
block beside a retained signal device. Destinations must already be known and
empty in the candidate. Every type port is rebound to supported signal terminals.
An input terminal can be dust or a conducting block; an output can also be a
direct device output, including the torch itself. The generated family
retains the actual input levers at their supplied positions; component mode also
retains every fixed external block and declared environmental occurrence; input **terminal**
positions are free. Deletions and a relocated block's former position are recorded
as explicit air, and the existing known regions are retained. Unsupported
placement and inconsistent terminal aliases cannot become successful candidates.

`alternatives` accepts complete `{blueprint, state, behavior_context}` candidates.
They may rearrange blocks, move controls and ports, or introduce a different
nested structure using catalog Revisions. They use the selected new candidate IDs
and ancestry, retain the target obligation and the supplied observation coverage,
and cannot change the law/profile assumptions. They are freshly checked before
comparison. Source metrics, classification names and previous reports have no
authority. Unknown candidate behavior cannot win.

This is a bounded search, not synthesis of every physically possible circuit.
The automatic family does not add blocks or relocate several blocks together;
explicit alternatives cover broader rewrites. Adding more candidate generators
requires no change to the target contract or adoption rules.
`global_minimality_proven` remains false even when all smaller layouts in the
generated family have been examined. Unsupported physics, undetermined proofs
and unexamined realizations prevent a global minimum claim.

Budgets bound computation rather than circuit behavior. The MCP defaults and
maximums are 4,096 generated layouts, 256 candidate bindings and 30,000 ms total,
including baseline verification. Each candidate also has the existing behavioral
state/transition limits. A preliminary exact check can reject a repeated-settling
candidate when holding an input from the fresh state reaches a complete-state
cycle with an incorrect output. Merely observing a wrong transient or reaching
the screening limit of 256 steps cannot reject it. Passing these initial-hold
cases never certifies reuse: every winning candidate still passes the full
universal contextual check. A result may retain a fully verified smaller candidate
when later search exhausts its budget; an incomplete candidate proof itself
never counts as a pass. `best: null` does not prove that no improvement exists.
Reports echo `budget` and a typed `stop_reason`: `baseline_not_passed`,
`layout_budget`, `binding_budget`, `time_budget`, `smaller_candidate_found` or
`generated_family_exhausted`. An early candidate return is not family exhaustion.
These stable labels replace the former prose strings. See
[search failure diagnostics](search-failure-diagnostics.md).

## Existing MCP workflow

Import the type, source definitions, law Revisions and base Assembly first.
Use unused Blueprint and Assembly IDs for the candidate. A request has this
shape, with IDs and terminal names from the selected records:

```json
{
  "blueprint": {
    "action": "optimize",
    "request": {
      "base_state": "example.base.v1",
      "target_instance": ["root", "child"],
      "target": {
        "behavior_type": "example.not.type.v1",
        "inputs": {"a": "input"},
        "outputs": {"out": "output"}
      },
      "candidate_revision": "example.smaller.v1",
      "candidate_state": "example.smaller.state.v1",
      "behavior_context": {
        "profile": "dustroute.dust-single-torch-block-effects.v1",
        "initial_condition": "fresh_construction",
        "dust_law": "dustroute.law.dust-strength.v1",
        "torch_law": "dustroute.law.torch.java-1-21-11.v1",
        "max_electrical_iterations": 128,
        "input_drivers": [{
          "port_position": {"x": -2, "y": 0, "z": 0},
          "port_kind": "wire",
          "lever_position": {"x": -1, "y": 0, "z": 0}
        }]
      },
      "alternatives": []
    }
  }
}
```

The response uses `dustroute.blueprint-mcp.v1` and returns `result.best` only for
a strictly smaller candidate that passed fresh verification. It includes the
new source definition, actual state, port/driver mappings and diagnostic review.
`catalog_changed`, `writes_minecraft` and `adoption_authorized` are false. Search
alone neither publishes the candidate nor changes any parent reference.

Review the changed ports, removed interpretations and complete physical state.
Then use the candidate in the existing `propose_update` request, explicitly
supplying any new parent definition, connections and candidate Assembly. Keep the
target binding and candidate context. Parent reconnection is a separate proposal,
not an automatic consequence of reducing the isolated realization. The normal
parent, child and shared-occurrence checks all apply to that proposed context.

`show_operation` performs fresh review. The proposal retains its complete data
and context across restart; `invoke_operation` with an explicit adoption decision
revalidates before publishing. The optimization report cannot bypass these gates.
The integration test exercises the component 6-to-2 path (7-to-3 including
equipment) through a real MCP session, persistence and restart. It also checks that the old parent still pins
its original children after the new parent is adopted.

## Verification evidence

| Requirement | Evidence |
|---|---|
| Count supports, wires, controls and shared positions once | Two overlapping source occurrences still count 7 baseline blocks; reduced actual state counts 3 |
| Optimize existing sources without hand-authored replacements | Both built-in NOT layouts reduce from 5 to 3 blocks under the same type and laws |
| Move both terminals without altering the target relation | Input dust at `(-2,0,0)` becomes block power at `(0,0,0)`; output dust at `(1,2,0)` becomes direct torch output at `(1,0,0)`; both pass repeated-settling proof |
| Permit broader rewrites | Supplied alternative relocates all controls/ports and includes a reusable compact Blueprint; its retained child is checked |
| Retain immutable sources and explicit parent adoption | Catalog equality checks, archive round trip and parent-update/MCP restart tests |
| Do not hide a retained child's problem | An alternative retaining a child with missing terminals cannot win |
| Do not change physics or accept unfinished proofs | Changed-law alternative rejected; unsupported profile, zero-time and search-budget cases cannot invent a pass |
| Screening is not type certification or a timing deadline | A 300-step startup is not rejected by the 256-step screen; a model passing fresh held cases but failing reuse still fails universal verification |
| Distinguish improvement from minimality | Every search result reports `global_minimality_proven: false` |

```bash
cargo test -p dustroute-optimize --test blueprint_reduction
cargo test -p dustroute-mcp blueprint_optimization_moves_ports
```

The physical verification profile remains dust and at most one torch, with
complete runtime histories and a conservative verification-only abstraction.
Multi-torch scheduling, restartability requirements, piston laws, entity handling,
new live placement and general merging remain outside this implementation.
