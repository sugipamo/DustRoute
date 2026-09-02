# Multi-fault repair

DustRoute treats a multi-fault repair as one reversible physical patch assembled from
multiple non-conflicting partial repairs. It does not assume that every nearby fragment
is broken: each step must improve the virtual physical analysis before the next step is
considered.

## Planning contract

1. Capture one immutable `circuit_id` with `test_circuit`.
2. Generate and rank single-fault candidates from that snapshot.
3. Virtually apply the best improving candidate.
4. Rebuild connectivity, liveness, electrical, and temporal evidence.
5. Repeat up to eight times, rejecting repeated block positions and non-improving steps.
6. Return a `multi_fault_repair` only when at least two repairs remain beneficial together.
7. Preview, explicitly confirm, apply, verify, and undo the combined patch as one operation.

The combined confidence is the lowest confidence among its constituent repairs. The MCP
response includes the final virtual impact so a client can explain why the batch ranked
above its individual alternatives.

## Fault matrix

| Fault combination | Automated expectation | Validation |
| --- | --- | --- |
| Two aligned dust breaks | Combine both missing-wire changes | Rust + live Minecraft E2E |
| Dust break + reversed repeater | Combine wire placement and reorientation | Rust |
| Missing support + another improving repair | Eligible when both remain improving | Existing support and composite planner tests |
| Competing changes at one position | Do not combine | Planner position-conflict guard |
| Nearby independent circuit | Do not infer user intent from proximity alone | Existing external-input/short prevention test |
| Partial write or stale world | Reject or roll back the complete operation | Existing MCP verification and rollback path |

Disconnected fragments can still be reported as `awaiting_external_input` when physical
evidence does not prove user intent. A high-ranked multi-fault repair is a previewable
hypothesis, not permission to mutate the world.
