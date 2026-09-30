# Editable and protected world space

For a completely captured Circuit Revision, or an adopted Assembly grounded in
that capture, `new_placement` accepts `edit_scope` at the original coordinates:

```json
{
  "revision_id": "<captured revision UUID>",
  "edit_scope": {
    "editable": [{
      "min": {"x": 101, "y": 101, "z": 106},
      "max": {"x": 101, "y": 101, "z": 107}
    }],
    "protected": [{
      "min": {"x": 101, "y": 101, "z": 104},
      "max": {"x": 102, "y": 102, "z": 105}
    }]
  }
}
```

The regions are inclusive, in world coordinates, and must lie in the complete
observed work rectangle. There are 1..64 editable regions and at most 64 explicit
protected regions. Editable/protected overlap is refused. Every other observed
cell is protected implicitly, including Air. This is an explicit change boundary,
not ownership inferred from a matching block or an adopted design.

Planning keeps the complete captured baseline and neighboring context. Requested
changes outside editable space are refused before a capability is created.
The shared electrical modification model checks protected blocks at **every
committed microstep**, including initialization, synchronous callbacks and
scheduled events, for both application and undo. A protected observer that pulses
and returns OFF is refused even if the final snapshot matches. Effects in editable
space must still settle to the exact declared target. The common dependency and
observer placement order is retained; no separate building simulator is used.

Supplying scope selects this common physical path even for inert building blocks.
It retains the current differential budget of 1..64 changed positions and 4096
non-air blocks. All native states must be lossless and supported. An omitted
scope retains the earlier whole-observed-rectangle change boundary and is shown
explicitly in the preview; it provides no additional protected subregion claim.

Preview exposes `edit_scope`, steps/undo steps and the model/readback scope.
Show/apply still require repeated stationary observations, target-server checks,
explicit confirmation and complete batch-boundary readback. Execution rebuilds
the proof with the same scope; undo also checks the saved scope and baseline.
The durable edit record retains scope for diagnosis after restart. A missing
scope on old history is unknown, not recovered authorization. No executable
plan or runtime is restored from history. No automatic retry or rollback occurs.

Model protection establishes block-state invariants under the declared empty
queue and supported-block assumptions. Live checks observe batch boundaries,
not every intermediate server callback; external edits and hidden pending work
are not made atomic by this scope. A discrepancy can stop an operation after
some writes. The scope does not prove functional behavior, item/entity outcomes,
or permission outside the observed rectangle.

This entry does not retarget an ungrounded design or update a placed-instance
source identity. Newly authored Assemblies still require a fully empty target
for fresh construction. Design adoption remains a separate immutable decision;
site modifications use fresh capture/preview and preserve earlier source pins.
`edit_scope` is refused on built-in circuits and `assembly_target` construction
instead of being silently ignored.
