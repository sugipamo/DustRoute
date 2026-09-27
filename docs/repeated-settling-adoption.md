# Repeated-settling Blueprint adoption

`RepeatedSettling` requirements now participate in contextual Blueprint review,
promotion and explicit update adoption through the existing MCP tools. A complete
universal abstract proof can satisfy a declared obligation. Importing a binding
or retaining a previous passing report cannot authorize adoption.

The type still specifies only a total Boolean input/output relation and eventual
stable outputs whenever inputs are held constant. It fixes no coordinates,
settling deadline, input rate or internal decomposition. `Signal` and
`BlockPattern` keep their existing connection-only meanings.

## Definition ports and actual controls

A Blueprint's `behavior_bindings` maps every named type input/output to a named
physical port of that realization:

```json
{
  "behavior_type": "example.not.type.v1",
  "inputs": {"a": "input"},
  "outputs": {"out": "output"}
}
```

`inputs` and `outputs` may both have several entries. Their keys must exactly
cover the type names; map order does not replace the relation's vector order.
Ports must exist, have the corresponding direction, and use `wire` or `block_power` for inputs; outputs also support
`device_output` for a direct torch/lever/redstone-block signal. Independent inputs cannot map to the same physical port. Multiple
outputs can observe a shared physical terminal when the declared relation allows
that. Existing physical port checks still apply: a `wire` port denotes actual
dust, not an arbitrary component with a signal.

These are immutable definition data. The separate `behavior_context` supplies
actual levers that control the input terminals in the current Assembly:

```json
{
  "profile": "dustroute.dust-single-torch-block-effects.v1",
  "initial_condition": "fresh_construction",
  "dust_law": "dustroute.law.dust-strength.v1",
  "torch_law": "dustroute.law.torch.java-1-21-11.v1",
  "max_electrical_iterations": 128,
  "input_drivers": [{
    "port_position": {"x": 0, "y": 0, "z": 0},
    "port_kind": "block_power",
    "lever_position": {"x": -1, "y": 0, "z": 0}
  }]
}
```

Positions here are actual Assembly coordinates. They are not type requirements
or implicit Blueprint edits. Rotation and translation resolve definition ports
before matching the driver. Grouping retains geometry and these bindings;
relocating a candidate requires corresponding new context data and fresh checks.
No invisible input device is introduced. Each independent input needs a distinct
actual lever and exactly one matching driver record.

A lever's Boolean setting is insufficient by itself: in every explored electrical
state, including synchronous torch effects, the named input terminal must have
the requested Boolean level. The selected dust law and actual surrounding blocks
determine that level. A remote unrelated lever, obstructed input, ambiguous driver,
unsupported observation or unresolved electrical state yields `undetermined`.
This is a conservative supported control arrangement, not a new requirement that
all possible realizations have instantaneous inputs. General input stimulus
adapters are not implemented.

## Review and adoption

Use `get_circuit_revision` with `blueprint.kind: "assembly"`, `validate: true`
and the context above. Persist it in the `test_circuit_change` Blueprint
`propose_update` request for subsequent `show_operation` review and explicit
`invoke_operation` adoption. There is no new endpoint or Minecraft write.

Every declared occurrence is checked in the complete actual Assembly. The
[history abstraction](abstract-behavior-verification.md) retains exact input,
register and pending-event values while conservatively covering histories. Full
runtime histories remain available for simulation and replay. A pass requires a
closed abstract graph with every held-input cycle satisfying the relation.
Possible abstract counterexamples, execution errors and incomplete searches stay
`undetermined`; they never supply either a pass or an invented concrete failure.

A consumer can require the same type on a connected output through
`required_source_types`. For this multiport contract, review must find a complete
declared producer mapping that covers that output, resolving parent aliases.
It never guesses the other relation terminals from a label or position. Missing
or conflicting mappings remain undetermined. Ordinary connection-type checks
remain independent.

A parent pass never overrides a failed or undetermined descendant. A review may
reuse a result only for an identical type, all resolved input terminals/drivers
and all outputs within that one Assembly/law context. The state, transition and
time budget is shared across the review. Saving/reloading, reviewing a changed
candidate and adoption rerun verification. Existing dependency checks pin type,
source and law definitions. Explicit adoption publishes new immutable values;
existing child and parent references stay unchanged.

## Compatibility and limits

Catalogs with the relation binding form need at least `dustroute.blueprint-catalog.v6`.
Direct-device ports and block-kind requirements need v7; self-bound static
requirements need v8 and do not change the relation contract. See
[component patterns](blueprint-component-patterns.md).
Catalogs without it retain the earlier required version. Versions v1–v5 remain
readable; relabeling a new binding archive as an older version is rejected.
Autonomous bindings keep their JSON shape:
`{"behavior_type":"clock.type.v1","output_port":"pulse"}`.
The new relation form cannot mix with `output_port` or accept unknown fields.
Old contexts omit `input_drivers`, which defaults to an empty list.

The MCP response schema remains v1. Repeated-settling and finite-burst catalogs
use `placement_connections_and_declared_behavioral_obligations`; existing
periodic-only scope and no-context responses remain supported.
`declared_behavior_verified` refers only to the declared obligations within the
selected model. Legacy `behavior_verified` and `live_world_verified` remain false.

The current abstraction supports fixed dust geometry and at most one torch in
the explicit block-effects profile. Fresh construction does not recover hidden
history from a running world. Arbitrary multi-torch scheduling, independent
stimulation of arbitrary internal terminals, piston laws and live placement are
subsequent work. The separate [block-count search](blueprint-block-reduction.md)
now supplies movable-port candidates to this adoption workflow.

## Regression coverage

- Both existing NOT realizations pass after rotation and translation.
- Promotion and archive reload preserve sources and rerun the proof.
- A two-input/two-output wire relation preserves vector order through aliases
  and consumer requirements; shared proof reuse includes every port.
- Missing, duplicate and unrelated drivers, wrong relation/binding shapes and
  exhausted budgets cannot authorize adoption.
- A passing parent retains a child's unsatisfied relation as undetermined.
- The existing MCP session tests import, validate, propose, restart, review and
  adopt a NOT update. Unsupported-profile proposals remain non-adoptable.

```bash
cargo test -p dustroute-translate --test repeated_settling_adoption
cargo test -p dustroute-mcp repeated_settling_blueprint
```
